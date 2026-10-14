// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/placement/initialize.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of placement/initialize.go
use crate::*;
use gocompat::Ctx;
use gocompat::{errorf, math, wrapf, GoResult};
use layoutgraph::{GraphRef, NodeRef, NodesMethods};

pub fn initializeNodes(ctx: &Ctx, g: &GraphRef) -> GoResult<()> {
    layoutgraph::Validate(ctx, "InitializeNodes", g)?;
    let nodes = g.b().Nodes.clone();
    if nodes.is_empty() {
        return Ok(());
    }
    let originalPositions = snapshotNodePositionsContext(ctx, "InitializeNodes", &nodes)?;
    let result = initializeNodesInner(ctx, g, &nodes);
    if result.is_err() {
        restoreNodePositions(&originalPositions);
    }
    result
}

fn initializeNodesInner(ctx: &Ctx, g: &GraphRef, gNodes: &[NodeRef]) -> GoResult<()> {
    let reachabilityGuard = limits::NewWorkGuard(ctx, "InitializeNodesReachability", limits::MaxEngineWorkUnits)?;
    let sizelessFactor = g.b().CellSize * compactionFactor;

    let init = |node: &NodeRef| -> GoResult<()> {
        if let Some(err) = ctx.Err() {
            return Err(wrapf!(err, "InitializeNodes: {}", err));
        }
        if node.has_tl() {
            return Ok(());
        }

        let mut leastDistance = math::Inf(1);
        let mut leastDistancePoint: Option<geo::PointRef> = node.tl();

        let sizeless = newSizelessOptimizer(ctx, g, None)?;
        let candidates = nodeCandidatePositions(ctx, node, g, &sizeless)?;
        let opts = edgeLengthOptions(None, false);
        for p in &candidates {
            let (_, isOccupied) = occupied(g, &p.b());
            if isOccupied {
                continue;
            }

            node.set_tl(Some(p.clone()));

            let edgeLength = match placementcost::NodeEdgeLength(ctx, node, &opts) {
                Ok(v) => v,
                Err(err) => {
                    node.set_tl(None);
                    return Err(err);
                }
            };

            if edgeLength < leastDistance {
                leastDistance = edgeLength;
                leastDistancePoint = Some(p.clone());
            }

            node.set_tl(None);
        }

        node.set_tl(leastDistancePoint);
        Ok(())
    };

    // the first subgraph has all the fixed node subgraphs (each may be disconnected)
    if g.HasFixedNode() {
        let fixedNodes = g.FixedNodes();
        let mut nodes: Vec<NodeRef> = Vec::with_capacity(fixedNodes.len());

        for fnode in &fixedNodes {
            let reachable = fnode.AllReachableNodesContext(false, true, true, None, &reachabilityGuard)?;
            for n in &reachable {
                n.set_tl(None);
            }
            nodes.extend(reachable);
        }

        for fnode in &fixedNodes {
            let ftl = *fnode.fixedTL().expect("nil pointer dereference: FixedTopLeft").b();
            fnode.set_tl(Some(geo::NewPoint(
                math::Ceil(ftl.X / sizelessFactor),
                math::Ceil(ftl.Y / sizelessFactor),
            )));
        }

        for n in &nodes {
            init(n)?;
        }
    } else {
        let bfsOrder = gNodes[0].AllReachableNodesContext(false, true, true, None, &reachabilityGuard)?;

        for n in &bfsOrder[1..] {
            n.set_tl(None);
        }
        let count = gNodes.len() as f64;
        gNodes[0].set_tl(Some(geo::NewPoint(count, count)));

        for node in &bfsOrder[1..] {
            init(node)?;
        }
    }

    let nodes = g.b().Nodes.clone();
    for node in &nodes {
        if !node.has_tl() {
            return Err(errorf!("missed initializing node {}", node.DebugID()));
        }
    }

    reachabilityGuard.Finish()?;
    Ok(())
}

pub fn nodeCandidatePositions(ctx: &Ctx, node: &NodeRef, g: &GraphRef, opt: &sizelessOptimizer) -> GoResult<Vec<geo::PointRef>> {
    let (floatMedianX, floatMedianY) = medianToNeighbors(node, false, &[]);
    let medianX = math::Floor(floatMedianX);
    let medianY = math::Floor(floatMedianY);
    let d = opt.FindClosestUnoccupiedDistance(ctx, node, &geo::Point { X: medianX, Y: medianY })?;
    let mut topLeftX = medianX - d - 2.0;
    let mut topLeftY = medianY - d - 2.0;
    let bottomRightX = medianX + d + 2.0;
    let bottomRightY = medianY + d + 2.0;
    if g.HasFixedNode() {
        topLeftX = math::BuiltinMax(topLeftX, 0.0);
        topLeftY = math::BuiltinMax(topLeftY, 0.0);
    }

    let mut positions: Vec<geo::PointRef> = Vec::new();
    if node.IsMajorityTarget() {
        // if it's, mainly, a target node, give preference to place it to bottom-right
        let mut x = bottomRightX;
        while x >= topLeftX {
            let mut y = bottomRightY;
            while y >= topLeftY {
                positions.push(geo::NewPoint(x, y));
                y -= 1.0;
            }
            x -= 1.0;
        }
    } else {
        let mut x = topLeftX;
        while x <= bottomRightX {
            let mut y = topLeftY;
            while y <= bottomRightY {
                positions.push(geo::NewPoint(x, y));
                y += 1.0;
            }
            x += 1.0;
        }
    }

    Ok(positions)
}
