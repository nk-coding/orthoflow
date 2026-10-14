// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/routing/standalone_router.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of routing/standalone_router.go.
#[allow(unused_imports)]
use crate::*;
use gocompat::context::Ctx;
use gocompat::{GoError, GoResult, Set};
use layoutgraph::{EdgeRef, GraphRef, NodeRef};

pub(crate) fn hasCompleteEdgeRoute(edge: &EdgeRef) -> bool {
    // Rust routes cannot hold nil points.
    edge.b().Points.len() >= 2
}

pub fn RouteEdges(ctx: &Ctx, g: &GraphRef, edges: &[EdgeRef]) -> GoResult<()> {
    routeEdgesWithBudgets(ctx, g, edges, defaultOVGBuildLimits(), maxRouteStageWorkUnits)
}

pub(crate) fn routeEdgesWithBudgets(ctx: &Ctx, g: &GraphRef, edges: &[EdgeRef], ovgLimits: ovgBuildLimits, workLimit: u64) -> GoResult<()> {
    if let Some(err) = ctx.Err() {
        return Err(err);
    }
    // Reject caller-owned selections before using their length as an allocation
    // hint. Accepted selections are charged again inside the atomic boundary.
    if edges.len() as i64 > layoutgraph::MaxTopologyReferences {
        return Err(GoError::new(format!(
            "TALA EdgeRouting selected edge references exceed limit {}",
            layoutgraph::MaxTopologyReferences
        )));
    }
    layoutgraph::Validate(ctx, "RouteEdges", g)?;
    if edges.is_empty() {
        return Ok(());
    }
    let (gNodes, gEdges) = (g.nodes(), g.edges());
    if gNodes.is_empty() {
        return Err(GoError::new("cannot route edges on an empty graph"));
    }
    // A valid selection is duplicate-free and contains only graph edges.
    if edges.len() > gEdges.len() {
        return Err(GoError::new(format!(
            "selected edge count {} exceeds graph edge count {}",
            edges.len(),
            gEdges.len()
        )));
    }

    let graphNodes: Set<NodeRef> = gNodes.iter().cloned().collect();
    let mut graphEdges: Set<EdgeRef> = Set::default();
    for edge in &gEdges {
        let (from, to) = (edge.from(), edge.to());
        if !from.has_tl() || !to.has_tl() {
            return Err(GoError::new(format!("graph edge {} has unplaced or missing endpoints", edge.IDValue())));
        }
        if !graphNodes.contains(&from) {
            return Err(GoError::new(format!("graph edge {} source node does not belong to the graph", edge.IDValue())));
        }
        if !graphNodes.contains(&to) {
            return Err(GoError::new(format!("graph edge {} target node does not belong to the graph", edge.IDValue())));
        }
        graphEdges.insert(edge.clone());
    }
    let mut selected: Set<EdgeRef> = Set::default();
    for edge in edges {
        if !graphEdges.contains(edge) {
            return Err(GoError::new(format!("edge {} does not belong to the graph", edge.IDValue())));
        }
        if selected.contains(edge) {
            return Err(GoError::new(format!("edge {} was selected more than once", edge.IDValue())));
        }
        selected.insert(edge.clone());
    }
    for edge in &gEdges {
        if selected.contains(edge) {
            continue;
        }
        if !hasCompleteEdgeRoute(edge) {
            return Err(GoError::new(format!("unselected edge {} has no complete route", edge.IDValue())));
        }
    }

    // Standalone routing is one API transaction and one aggregate post-routing
    // work budget.
    runAtomicRouteStageAfterPreflight(ctx, "EdgeRouting", g, edges, workLimit, |guard| {
        guard.add(g.b().Nodes.len() as u64)?;
        g.ComputeCellSize();

        routeAdditionalEdgesWithLimits(ctx, g, edges, ovgLimits)?;
        guard.step()?;

        for edge in edges {
            if edge.HasTableColumn() {
                guard.step()?;
                continue;
            }
            tryStraightEdgeFallbackGuarded(g, edge, guard)?;
        }

        let mut lockedEdges: Vec<EdgeRef> = Vec::with_capacity(g.b().Edges.len());
        for edge in g.edges() {
            guard.step()?;
            if !selected.contains(&edge) {
                lockedEdges.push(edge);
            }
        }
        let mut regularEdges: Vec<EdgeRef> = Vec::with_capacity(edges.len());
        for edge in edges {
            guard.step()?;
            if isSpecialEdgeForBalancing(g, edge) {
                lockedEdges.push(edge.clone());
                continue;
            }
            regularEdges.push(edge.clone());
        }
        balanceRegularEdgesGuarded(g, &lockedEdges, &regularEdges, guard)?;
        for edge in edges {
            traceToShapeBorderGuarded(Some(edge), guard)?;
        }
        reorderDuplicatesInEdgesGuarded(edges, guard)?;
        guard.step()?;
        labeling::PlaceNewEdges(ctx, g, edges)?;
        guard.finish()
    })
}
