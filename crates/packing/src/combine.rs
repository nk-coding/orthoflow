// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/packing/combine.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;
use std::rc::Rc;

/// Go `combineEdgeSnapshot`: the edge's restorable state. Only route points
/// and the public edge fields can change while combining; they are captured
/// with their pointer identity.
pub(crate) struct combineEdgeSnapshot {
    from: NodeRef,
    to: NodeRef,
    d2ID: Option<String>,
    points: Vec<geo::PointRef>,
    pointValues: Vec<geo::Point>,
    label: Option<(layoutgraph::LabelRef, layoutgraph::Label)>,
    sourceArrowheadLabel: Option<(layoutgraph::LabelRef, layoutgraph::Label)>,
    targetArrowheadLabel: Option<(layoutgraph::LabelRef, layoutgraph::Label)>,
    fromTableColumnIndex: Option<i64>,
    toTableColumnIndex: Option<i64>,
}

fn snapLabel(l: &Option<layoutgraph::LabelRef>) -> Option<(layoutgraph::LabelRef, layoutgraph::Label)> {
    l.as_ref().map(|l| (l.clone(), l.b().clone()))
}

fn restoreLabel(l: &Option<(layoutgraph::LabelRef, layoutgraph::Label)>) -> Option<layoutgraph::LabelRef> {
    let (r, v) = l.as_ref()?;
    *r.bm() = v.clone();
    Some(r.clone())
}

pub(crate) fn captureCombineEdge(edge: &EdgeRef) -> combineEdgeSnapshot {
    let e = edge.b();
    combineEdgeSnapshot {
        from: e.From.clone(),
        to: e.To.clone(),
        d2ID: e.D2ID.clone(),
        points: e.Points.clone(),
        pointValues: e.Points.iter().map(|p| p.get()).collect(),
        label: snapLabel(&e.Label),
        sourceArrowheadLabel: snapLabel(&e.SourceArrowheadLabel),
        targetArrowheadLabel: snapLabel(&e.TargetArrowheadLabel),
        fromTableColumnIndex: e.FromTableColumnIndex,
        toTableColumnIndex: e.ToTableColumnIndex,
    }
}

impl combineEdgeSnapshot {
    pub(crate) fn restore(&self, edge: &EdgeRef) {
        let label = restoreLabel(&self.label);
        let src = restoreLabel(&self.sourceArrowheadLabel);
        let dst = restoreLabel(&self.targetArrowheadLabel);
        for (p, v) in self.points.iter().zip(&self.pointValues) {
            *p.bm() = *v;
        }
        let mut e = edge.bm();
        e.From = self.from.clone();
        e.To = self.to.clone();
        e.D2ID = self.d2ID.clone();
        e.Points = self.points.clone();
        e.Label = label;
        e.SourceArrowheadLabel = src;
        e.TargetArrowheadLabel = dst;
        e.FromTableColumnIndex = self.fromTableColumnIndex;
        e.ToTableColumnIndex = self.toTableColumnIndex;
    }
}

fn mapPtr<K, V>(m: &layoutgraph::GoMap<K, V>) -> usize {
    Rc::as_ptr(&m.0) as *const () as usize
}

type topologyIdentity = [usize; 8];

fn identityOf(graph: &GraphRef) -> topologyIdentity {
    let g = graph.b();
    [
        mapPtr(&g.Containers),
        mapPtr(&g.Clusters),
        mapPtr(&g.Trees),
        mapPtr(&g.NodeToTree),
        mapPtr(&g.Hubs),
        mapPtr(&g.Sequences),
        mapPtr(&g.Directions),
        mapPtr(&g.CommonUncleSiblings),
    ]
}

/// CombineSubgraphs arranges laid-out subgraphs and returns a combined graph
/// view that shares the master graph's topology.
pub fn CombineSubgraphs(ctx: &Ctx, masterGraph: &GraphRef, graphs: &[GraphRef], ancestorObstacles: &[geo::Box]) -> GoResult<GraphRef> {
    layoutgraph::Validate(ctx, "CombineSubgraphs", masterGraph)?;
    if graphs.len() as i64 > limits::MaxEngineNodes {
        return Err(GoError::new(format!(
            "TALA CombineSubgraphs subgraph count {} exceeds limit {}",
            graphs.len(),
            limits::MaxEngineNodes
        )));
    }
    // Identical map identities mean identical graph-owned topology, so that
    // expensive portion only needs one full preflight.
    let mut validatedGraphOwnedTopologies: Vec<topologyIdentity> = vec![identityOf(masterGraph)];
    let mut allGraphs: Vec<GraphRef> = Vec::with_capacity(graphs.len() + 1);
    allGraphs.push(masterGraph.clone());
    for graph in graphs {
        let identity = identityOf(graph);
        if !validatedGraphOwnedTopologies.contains(&identity) {
            layoutgraph::Validate(ctx, "CombineSubgraphs", graph)?;
            validatedGraphOwnedTopologies.push(identity);
        }
        allGraphs.push(graph.clone());
    }
    layoutgraph::ValidateSubgraphCombination(ctx, &allGraphs)?;

    let guard = limits::NewWorkGuard(ctx, "CombineSubgraphs", limits::MaxEngineWorkUnits)?;
    let mut allNodes: Vec<NodeRef> = Vec::new();
    let mut uniqueNodes: Set<NodeRef> = Set::default();
    let mut edgeSnapshots: Map<EdgeRef, combineEdgeSnapshot> = Map::default();
    let mut routePointCapacity: i64 = 0;
    let mut collectGraph = |graph: &GraphRef,
                            allNodes: &mut Vec<NodeRef>,
                            uniqueNodes: &mut Set<NodeRef>,
                            edgeSnapshots: &mut Map<EdgeRef, combineEdgeSnapshot>|
     -> GoResult<()> {
        for node in graph.nodes() {
            guard.Step()?;
            if !uniqueNodes.contains(&node) {
                uniqueNodes.insert(node.clone());
                if uniqueNodes.len() as i64 > limits::MaxEngineNodes {
                    return Err(GoError::new(format!("TALA CombineSubgraphs unique node count exceeds limit {}", limits::MaxEngineNodes)));
                }
                allNodes.push(node);
            }
        }
        for edge in graph.edges() {
            guard.Step()?;
            if !edgeSnapshots.contains_key(&edge) {
                if edgeSnapshots.len() as i64 >= limits::MaxEngineEdges {
                    return Err(GoError::new(format!("TALA CombineSubgraphs unique edge count exceeds limit {}", limits::MaxEngineEdges)));
                }
                // Go checks cap(edge.Points); the port uses the length.
                let capacity = edge.b().Points.len() as i64;
                if capacity > layoutgraph::MaxRoutePoints - routePointCapacity {
                    return Err(GoError::new(format!("TALA CombineSubgraphs route point count exceeds limit {}", layoutgraph::MaxRoutePoints)));
                }
                routePointCapacity += capacity;
                for _ in 0..capacity {
                    guard.Step()?;
                }
                edgeSnapshots.insert(edge.clone(), captureCombineEdge(&edge));
            }
        }
        Ok(())
    };
    collectGraph(masterGraph, &mut allNodes, &mut uniqueNodes, &mut edgeSnapshots)?;
    for graph in graphs {
        collectGraph(graph, &mut allNodes, &mut uniqueNodes, &mut edgeSnapshots)?;
    }
    let nodeSnapshots = snapshotCombineNodePositions(ctx, &allNodes)?;
    let nodeGraphReferences: Vec<(NodeRef, Option<GraphRef>)> = nodeSnapshots.iter().map(|s| (s.node.clone(), s.node.graph())).collect();

    let result = combineGuarded(masterGraph, graphs, ancestorObstacles, &guard);
    if result.is_err() {
        restoreCombineNodePositions(&nodeSnapshots);
        for (node, graph) in &nodeGraphReferences {
            node.bm().Graph = graph.clone();
        }
        for (edge, snapshot) in &edgeSnapshots {
            snapshot.restore(edge);
        }
    }
    result
}

fn combineGuarded(masterGraph: &GraphRef, graphs: &[GraphRef], ancestorObstacles: &[geo::Box], guard: &WorkGuardRef) -> GoResult<GraphRef> {
    let combined = layoutgraph::NewGraph();
    combined.CopyEntitiesFrom(masterGraph);
    if graphs.is_empty() {
        return Ok(combined);
    }
    let mut graphs: &[GraphRef] = graphs;

    let mut combinedBR = geo::Point { X: f64::NEG_INFINITY, Y: f64::NEG_INFINITY };
    let mut candidatePoints: Vec<geo::Point> = Vec::new();
    let p = |x: f64, y: f64| geo::Point { X: x, Y: y };

    // first subgraph (contains all fixed nodes) is always placed at 0,0
    let firstGraph = graphs[0].clone();
    if firstGraph.HasFixedNode() {
        graphs = &graphs[1..];
        let (_, graphBR) = firstGraph.BoundingBox();
        let graphBR = graphBR.expect("nil pointer dereference").get();

        for n in firstGraph.nodes() {
            guard.Step()?;
            combined.AddNodeUnchecked(&n);
            for child in masterGraph.AllDescendantNodes(Some(&n), true) {
                guard.Step()?;
                combined.AddNodeUnchecked(&child);
            }
        }
        for edge in firstGraph.edges() {
            guard.Step()?;
            combined.AddEdge(&edge);
        }

        combinedBR.X = math::Max(combinedBR.X, graphBR.X);
        combinedBR.Y = math::Max(combinedBR.Y, graphBR.Y);

        candidatePoints.extend([
            p(graphBR.X, 0.0),
            p(graphBR.X, graphBR.Y),
            p(0.0, graphBR.Y),
            p(graphBR.X + subgraphPadding, 0.0),
            p(graphBR.X + subgraphPadding, graphBR.Y),
            p(0.0, graphBR.Y + subgraphPadding),
        ]);
    }

    let mut sortedByArea: Vec<GraphRef> = graphs.to_vec();
    gocompat::sort::SortSlice(&mut sortedByArea, |a, b| a.Area() > b.Area());
    guard.Finish()?;

    let ignoredObstacle = -(layoutgraph::ContainerPadding as f64);
    for graph in &sortedByArea {
        guard.Step()?;
        let (graphTL, graphBR) = graph.BoundingBox();
        let mut graphTL = graphTL.expect("nil pointer dereference").get();
        let mut graphBR = graphBR.expect("nil pointer dereference").get();

        let mut minCost = f64::INFINITY;
        let mut minOverlaps = i64::MAX;
        // place the first graph at (0,0) so that combined's top left is always there
        let mut minCandidatePoint = p(0.0, 0.0);
        let mut minCandidatePointIndex = 0usize;

        let graphNodes = graph.nodes();
        for (i, candidatePoint) in candidatePoints.iter().enumerate() {
            guard.Step()?;
            // points relative to graph top left would now be relative to the candidate point
            let offsetX = candidatePoint.X - graphTL.X;
            let offsetY = candidatePoint.Y - graphTL.Y;

            let mut collides = false;
            let mut ancestorOverlaps: i64 = 0;
            for node in &graphNodes {
                guard.Step()?;
                node.MoveWithChildren(offsetX, offsetY);
                for otherNode in combined.nodes() {
                    guard.Step()?;
                    if node.DoesOverlap(&otherNode) {
                        collides = true;
                        break;
                    }
                }
                if !collides {
                    for o in ancestorObstacles {
                        guard.Step()?;
                        let otl = o.TL().get();
                        if otl.X == ignoredObstacle && otl.Y == ignoredObstacle {
                            continue;
                        }
                        if node.boxValue().Overlaps(o) {
                            ancestorOverlaps += 1;
                        }
                    }
                }
                node.MoveWithChildren(-offsetX, -offsetY);
                if collides {
                    break;
                }
            }
            if collides {
                continue;
            }
            if ancestorOverlaps > minOverlaps {
                continue;
            }

            // The new combined width is only (candidate point + graph width) if that is wider than the existing width (same for height)
            let newWidth = math::Max(combinedBR.X, candidatePoint.X + (graphBR.X - graphTL.X));
            let newHeight = math::Max(combinedBR.Y, candidatePoint.Y + (graphBR.Y - graphTL.Y));
            let cost = newWidth * newHeight + math::Pow(newWidth - newHeight, 2.0) * subgraphSquareDampener;

            if ancestorOverlaps < minOverlaps || (ancestorOverlaps == minOverlaps && cost < minCost) {
                minOverlaps = ancestorOverlaps;
                minCost = cost;
                minCandidatePoint = *candidatePoint;
                minCandidatePointIndex = i;
            }
        }

        if !candidatePoints.is_empty() {
            // Remove the candidate point used
            let last = candidatePoints.len() - 1;
            candidatePoints[minCandidatePointIndex] = candidatePoints[last];
            candidatePoints.truncate(last);
        }

        // Since we are placing the graph at minCandidatePoint,
        // each point relative to the graph top left now needs to be relative to minCandidatePoint
        let offsetX = minCandidatePoint.X - graphTL.X;
        let offsetY = minCandidatePoint.Y - graphTL.Y;

        for n in &graphNodes {
            guard.Step()?;
            n.MoveWithChildren(offsetX, offsetY);
            combined.AddNodeUnchecked(n);
            for child in masterGraph.AllDescendantNodes(Some(n), true) {
                guard.Step()?;
                combined.AddNodeUnchecked(&child);
            }
        }
        for edge in graph.edges() {
            guard.Step()?;
            for point in edge.points() {
                guard.Step()?;
                let mut pb = point.bm();
                pb.X += offsetX;
                pb.Y += offsetY;
            }
            combined.AddEdge(&edge);
        }

        // update graph tl and br to be relative to combined
        graphTL.X += offsetX;
        graphTL.Y += offsetY;
        graphBR.X += offsetX;
        graphBR.Y += offsetY;

        combinedBR.X = math::Max(combinedBR.X, graphBR.X);
        combinedBR.Y = math::Max(combinedBR.Y, graphBR.Y);

        candidatePoints.extend([
            p(graphBR.X, graphTL.Y),
            p(graphBR.X, graphBR.Y),
            p(graphTL.X, graphBR.Y),
            p(graphBR.X + subgraphPadding, graphTL.Y),
            p(graphBR.X + subgraphPadding, graphBR.Y),
            p(graphTL.X, graphBR.Y + subgraphPadding),
        ]);
    }

    guard.Finish()?;
    Ok(combined)
}
