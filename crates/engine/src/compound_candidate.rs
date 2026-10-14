// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/engine/compound_candidate.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

/// CompoundCandidate gives the selected drawing at most one rigid-container
/// placement trial. It never runs another full seed pipeline. The adapter must
/// validate and compare the completed proposal against the ordinary incumbent.
pub fn CompoundCandidate(ctx: &Ctx, graph: &GraphRef) -> GoResult<GraphRef> {
    if let Some(err) = ctx.Err() {
        return Err(err);
    }
    let roots = graph.containers().at(&None);
    let (nNodes, nEdges) = {
        let g = graph.b();
        (g.Nodes.len(), g.Edges.len())
    };
    if nNodes > 128 || nEdges > 256 || roots.len() < 3 || roots.len() > 64 {
        return Ok(graph.clone());
    }
    let mut detailed = false;
    for node in &roots {
        detailed = detailed || node.IsContainer();
    }
    if !detailed || graph.HasFixedNode() {
        return Ok(graph.clone());
    }
    let candidate = layoutgraph::Clone(ctx, Some(graph))?;
    // This is a deterministic post-selection proposal, independent of seed
    // completion order. Requested seeds still run exactly once each.
    let changed = hierarchy::PlaceCompound(ctx, &candidate, Some(&gocompat::rand::New(gocompat::rand::NewSource(0))))?;
    if !changed {
        return Ok(graph.clone());
    }
    let worseAlignment = compoundCrossAxisDetours(&candidate) > compoundCrossAxisDetours(graph);
    if let Some(err) = ctx.Err() {
        return Err(err);
    }
    if worseAlignment {
        return Ok(graph.clone());
    }
    reroutePlaced(ctx, &candidate)?;
    let (tl, br) = candidate.BoundingBox();
    if let (Some(tl), Some(br)) = (tl, br) {
        let (tl, br) = (*tl.b(), *br.b());
        if br.X - tl.X > maxGraphSize || br.Y - tl.Y > maxGraphSize {
            return Ok(graph.clone());
        }
    }
    Ok(candidate)
}

/// compoundCrossAxisDetours counts external connections whose outer blocks have
/// disjoint projections perpendicular to the flow.
pub(crate) fn compoundCrossAxisDetours(g: &GraphRef) -> i64 {
    let mut rootOf: Map<NodeRef, NodeRef> = Map::default();
    for n in g.nodes() {
        let mut root = n.clone();
        loop {
            let c = root.b().Container.clone();
            match c {
                Some(c) => root = c,
                None => break,
            }
        }
        rootOf.insert(n, root);
    }
    let horizontal = g.Direction(None).IsHorizontal();
    let mut detours = 0;
    for edge in g.edges() {
        let (from, to) = (rootOf[&edge.from()].clone(), rootOf[&edge.to()].clone());
        if from == to {
            continue;
        }
        if horizontal {
            if from.y() + from.h() < to.y() || to.y() + to.h() < from.y() {
                detours += 1;
            }
        } else if from.x() + from.w() < to.x() || to.x() + to.w() < from.x() {
            detours += 1;
        }
    }
    detours
}

pub(crate) fn reroutePlaced(ctx: &Ctx, g: &GraphRef) -> GoResult<()> {
    let mut p = newPipeline(g, 0);
    p.resetFullLayoutRouteState();
    let stages: [StageFn; 9] = [
        crate::pipeline::pipeline::edgeRoutingStage,
        crate::pipeline::pipeline::simplifyEdgeRoutes,
        crate::pipeline::pipeline::swapEdgePorts,
        crate::pipeline::pipeline::straightEdgesFallback,
        crate::pipeline::pipeline::balanceEdgeSegments,
        crate::pipeline::pipeline::fixClusterEdgeBranching,
        crate::pipeline::pipeline::traceEdgesToShapeBorder,
        crate::pipeline::pipeline::reorderDuplicates,
        crate::pipeline::pipeline::placeLabels,
    ];
    for stage in stages {
        if let Some(err) = ctx.Err() {
            return Err(err);
        }
        stage(&mut p, ctx)?;
    }
    // Authored fixed origins have already had prescale padding removed.
    if !g.HasFixedNode() {
        placement::Normalize(g);
    }
    match ctx.Err() {
        Some(err) => Err(err),
        None => Ok(()),
    }
}
