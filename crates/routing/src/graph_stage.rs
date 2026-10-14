// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/routing/graph_stage.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of routing/graph_stage.go.
#[allow(unused_imports)]
use crate::*;
use gocompat::context::Ctx;
use gocompat::{GoError, GoResult, Set};
use layoutgraph::{GraphRef, NodeRef};
use std::rc::Rc;

// SubgraphRouteObserver observes each completed subgraph route at the exact
// point where its OVG and the graph's current route state agree.
pub trait SubgraphRouteObserver {
    fn SubgraphRouted(&self, ovg: &OVGRef) -> GoResult<()>;
}

// RouteCompletionObserver observes the route-complete transition inside the
// atomic routing stage.
pub trait RouteCompletionObserver {
    fn RoutingCompleted(&self);
}

// GraphRouteOptions describes the route state owned by the pipeline around a
// whole-graph routing pass.
#[derive(Clone, Default)]
pub struct GraphRouteOptions {
    pub ForceReroute: bool,
    pub RoutesPreviouslyCompleted: bool,
    pub Observer: Option<Rc<dyn SubgraphRouteObserver>>,
    pub CompletionObserver: Option<Rc<dyn RouteCompletionObserver>>,
}

// RouteGraph runs the complete graph-routing stage. The returned boolean is
// the updated value of RoutesPreviouslyCompleted; on error the caller must
// retain its prior value.
pub fn RouteGraph(ctx: &Ctx, graph: &GraphRef, options: GraphRouteOptions) -> GoResult<bool> {
    RouteGraphWithWorkLimit(ctx, graph, options, maxEdgeRoutingStageWorkUnits)
}

// RouteGraphWithWorkLimit is RouteGraph with an explicit aggregate budget.
pub fn RouteGraphWithWorkLimit(ctx: &Ctx, graph: &GraphRef, options: GraphRouteOptions, workLimit: u64) -> GoResult<bool> {
    let mut routingComplete = options.RoutesPreviouslyCompleted;
    let guard = newRouteWorkGuard(ctx, "EdgeRouting", workLimit)?;

    // Previously completed routes take an early success path below, but still
    // cross the same topology, geometry, and resource boundary as routes built here.
    layoutgraph::Validate(ctx, "EdgeRouting", graph)?;
    validateRouteStageGeometry(graph, &[], &guard)?;
    if graph.b().Edges.is_empty() {
        guard.finish()?;
        return Ok(routingComplete);
    }

    // Existing routes are authoritative only after this pipeline has completed
    // its first routing pass.
    if !options.ForceReroute && options.RoutesPreviouslyCompleted {
        let edges = graph.edges();
        let mut routedEdges = 0usize;
        for edge in &edges {
            guard.step()?;
            let n = edge.b().Points.len();
            if n == 0 {
                continue;
            }
            if !hasCompleteEdgeRoute(edge) {
                return Err(invariant::Errorf(format!(
                    "edge {} has an incomplete route: expected at least two non-nil points, got {}",
                    edge.IDValue(),
                    n
                )));
            }
            routedEdges += 1;
        }
        if routedEdges == edges.len() {
            routingComplete = true;
            if let Some(o) = &options.CompletionObserver {
                o.RoutingCompleted();
            }
            guard.finish()?;
            return Ok(routingComplete);
        }
        if routedEdges != 0 {
            return Err(invariant::Errorf(format!(
                "graph is partially routed: {} of {} edges have routes",
                routedEdges,
                edges.len()
            )));
        }
    }

    runAtomicRouteStageWithValidatedGeometry(graph, &[], &guard, |guard| {
        let aggregate: workBudgetRef = Rc::new(guard.clone());
        let routingCtx = contextWithRouteAggregateWork(ctx, Some(aggregate));
        let ovgGuard = newOVGBuildGuard(Some(&routingCtx), defaultOVGBuildLimits())?;

        let (split, splitOwnership) = graph.SplitSubgraphsTracked(
            ctx,
            layoutgraph::SplitOptions { IncludeContainers: true, IncludeNears: true, TraverseTrees: false },
            Some(guard as &dyn layoutgraph::SharedWorkStepper),
        );
        let subgraphs = split?;
        // SplitSubgraphs intentionally points reachable nodes at temporary graphs.
        // Restore the exact prior owner on every exit (Go: defer).
        let result = routeSubgraphs(ctx, &routingCtx, graph, &subgraphs, &splitOwnership, &ovgGuard, &options, guard);
        splitOwnership.Restore();
        result?;

        routingComplete = true;
        if let Some(o) = &options.CompletionObserver {
            o.RoutingCompleted();
        }
        Ok(())
    })?;
    Ok(routingComplete)
}

#[allow(clippy::too_many_arguments)]
fn routeSubgraphs(
    ctx: &Ctx,
    routingCtx: &Ctx,
    graph: &GraphRef,
    subgraphs: &[GraphRef],
    splitOwnership: &layoutgraph::NodeGraphOwnershipJournal,
    ovgGuard: &ovgBuildGuardRef,
    options: &GraphRouteOptions,
    guard: &routeWorkGuardRef,
) -> GoResult<()> {
    let _ = ctx;
    guard.add(subgraphs.len() as u64)?;
    for subgraph in subgraphs {
        guard.step()?;
        if subgraph.b().Edges.is_empty() {
            continue;
        }
        let subNodes = subgraph.nodes();
        guard.add(subNodes.len() as u64)?;
        let (topLeft, bottomRight) = routeStageGraphBoundingBox(subgraph, guard)?;
        let (mut tl, mut br) = (topLeft.get(), bottomRight.get());
        tl.X -= overshootAmount;
        tl.Y -= overshootAmount;
        br.X += overshootAmount;
        br.Y += overshootAmount;
        let mut nodesInBoundingBox: Vec<NodeRef> = Vec::new();
        for otherSubgraph in subgraphs {
            guard.step()?;
            if otherSubgraph == subgraph {
                continue;
            }
            for node in otherSubgraph.nodes() {
                guard.step()?;
                if node.IsWithinBounds(&tl, &br) {
                    // If another node not in this subgraph overlaps one of its
                    // nodes, it may sit on the center or block a port into the
                    // center and therefore is not an obstruction.
                    let mut overlapsSubgraph = false;
                    for subgraphNode in &subNodes {
                        guard.step()?;
                        if node.DoesOverlapExact(subgraphNode) {
                            overlapsSubgraph = true;
                            break;
                        }
                    }
                    if !overlapsSubgraph {
                        nodesInBoundingBox.push(node);
                    }
                }
            }
        }

        guard.add(subNodes.len() as u64 + subgraph.b().Edges.len() as u64)?;
        subgraph.ComputeCellSize();
        let ovg = routeEdgesWithResourceGuards(routingCtx, subgraph, &nodesInBoundingBox, ovgGuard, maxRouteSearchWorkUnits)?;
        // Observe cancellation or an exhausted aggregate budget after routing
        // has mutated edges and before optional engine-owned serialization.
        guard.step()?;
        if let Some(observer) = &options.Observer {
            observer.SubgraphRouted(&ovg)?;
        }
    }

    // SplitSubgraphs points members at their temporary graph. Restore every
    // member and descendant once.
    let mut seenNodes: Set<NodeRef> = Set::default();
    let mut nodesToRestore: Vec<NodeRef> = Vec::with_capacity(graph.b().Nodes.len());
    let mut enqueue = |node: &NodeRef, nodesToRestore: &mut Vec<NodeRef>| -> GoResult<()> {
        guard.step()?;
        if seenNodes.contains(node) {
            return Ok(());
        }
        seenNodes.insert(node.clone());
        nodesToRestore.push(node.clone());
        Ok(())
    };
    for subgraph in subgraphs {
        for node in subgraph.nodes() {
            enqueue(&node, &mut nodesToRestore)?;
        }
    }
    let (containers, clusters, sequences) = {
        let gb = graph.b();
        (gb.Containers.clone(), gb.Clusters.clone(), gb.Sequences.clone())
    };
    let mut index = 0usize;
    while index < nodesToRestore.len() {
        let node = nodesToRestore[index].clone();
        index += 1;
        let (originalGraph, captured) = splitOwnership.OriginalGraph(&node);
        if captured {
            node.bm().Graph = originalGraph;
        } else {
            node.bm().Graph = Some(graph.clone());
        }
        if node.IsContainer() {
            for child in containers.get(&Some(node.clone())).unwrap_or_default() {
                enqueue(&child, &mut nodesToRestore)?;
            }
        }
        if node.IsClusterVessel() {
            if let Some(cluster) = clusters.get(&node) {
                let nodes = cluster.b().Nodes.clone();
                for child in &nodes {
                    enqueue(child, &mut nodesToRestore)?;
                }
            }
        }
        if let Some(sequence) = sequences.get(&node) {
            let nodes = sequence.b().Nodes.clone();
            for child in &nodes {
                enqueue(child, &mut nodesToRestore)?;
            }
        }
    }
    Ok(())
}
