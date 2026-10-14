// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/placement/transpose.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.
//
// Port of placement/transpose.go
#[allow(unused_imports)]
use crate::*;
use gocompat::math;
use gocompat::{Ctx, GoResult, Set};
use layoutgraph::{EdgeAbductionRef, GraphRef, NodeRef};

// C is the current node:
//
// X
// |
// E -- A
// |    |
// D    C
//      |
//      B
//
// # We can tranpose C-B around A to get
//
// X
// |
// E -- A -- C -- B
// |
// D
//
// Which is more symmetrical
pub(crate) fn transpose(ctx: &Ctx, g: &GraphRef, node: &NodeRef, edgeAbductions: Option<&[EdgeAbductionRef]>) -> GoResult<bool> {
    let (ctx, _) = layoutgraph::EnsureTransactionWorkGuard(ctx, "TransposeTransactions")?;
    if node.b().Hierarchy.is_some() {
        return Ok(false);
    }
    if g.nodeToTree().has(node) {
        return Ok(false);
    }
    if g.IsTreeSentinel(Some(node)) {
        return Ok(false);
    }
    if node.b().FixedTopLeft.is_some() {
        return Ok(false);
    }
    // Maybe re-enable when I see a use case
    if let Some(abductions) = edgeAbductions {
        for e in abductions {
            let eb = e.b();
            if eb.OriginallyFrom.as_ref() == Some(node) || eb.OriginallyTo.as_ref() == Some(node) {
                return Ok(false);
            }
            if eb.CurrentFrom.as_ref() == Some(node) || eb.CurrentTo.as_ref() == Some(node) {
                return Ok(false);
            }
        }
    }
    let nodeEdges = node.edges();
    if nodeEdges.len() != 2 && nodeEdges.len() != 1 {
        return Ok(false);
    }
    let reachabilityGuard = limits::NewWorkGuard(&ctx, "TransposeReachability", limits::MaxEngineWorkUnits)?;
    let reachableFrom = |start: &NodeRef, includeContainers: bool, ignore: &[Option<NodeRef>]| -> GoResult<Vec<NodeRef>> {
        // A nil key in Go's ignore map never matches a node.
        let ignoreSet: Set<NodeRef> = ignore.iter().flatten().cloned().collect();
        start.AllReachableNodesContext(includeContainers, false, true, Some(&ignoreSet), &reachabilityGuard)
    };

    // Need to account for the case whereby the container should be the one moving

    let transposeNodes: Vec<NodeRef>;
    let centerNode: NodeRef;
    if nodeEdges.len() == 1 {
        let nodeA = node.Adjacent(&nodeEdges[0]);
        if nodeA.IsDescendantOf(Some(node)) || node.IsDescendantOf(Some(&nodeA)) {
            return Ok(false);
        }
        if node.Orientation(&nodeA).IsDiagonal() {
            return Ok(false);
        }
        let ancestor = node.NearestSharedAncestor(&nodeA);

        centerNode = nodeA.clone();
        if edgeAbductions.is_none() {
            let mut curr = node.clone();
            while curr.OwningContainer() != ancestor {
                curr = curr.OwningContainer().expect("nil pointer dereference: OwningContainer");
            }

            let tn = reachableFrom(&curr, false, &[Some(nodeA.clone())])?;
            if tn.iter().any(|n| nodeA.IsDescendantOf(Some(n))) {
                return Ok(false);
            }
            transposeNodes = tn;
        } else {
            transposeNodes = vec![node.clone()];
        }
    } else {
        let nodeA = node.Adjacent(&nodeEdges[0]);
        let nodeB = node.Adjacent(&nodeEdges[1]);

        if nodeA.IsDescendantOf(Some(node)) || node.IsDescendantOf(Some(&nodeA)) {
            return Ok(false);
        }
        if nodeB.IsDescendantOf(Some(node)) || node.IsDescendantOf(Some(&nodeB)) {
            return Ok(false);
        }

        let ancestorA = node.NearestSharedAncestor(&nodeA);
        let ancestorB = node.NearestSharedAncestor(&nodeB);

        let connectedToA = reachableFrom(&nodeA, true, &[Some(node.clone()), ancestorA.clone()])?;
        if connectedToA.contains(&nodeB) {
            return Ok(false);
        }

        let nodeAOrientation = node.Orientation(&nodeA);
        let nodeBOrientation = node.Orientation(&nodeB);

        if nodeAOrientation.IsDiagonal() || nodeBOrientation.IsDiagonal() {
            return Ok(false);
        }

        let reachableNodesA = reachableFrom(&nodeA, false, &[Some(node.clone()), ancestorA.clone()])?;
        let reachableNodesB = reachableFrom(&nodeB, false, &[Some(node.clone()), ancestorB.clone()])?;

        // Rotate the side with less nodes around the side with more nodes
        if reachableNodesA.len() >= reachableNodesB.len() {
            if edgeAbductions.is_none() {
                let mut curr = node.clone();
                while curr.OwningContainer() != ancestorB {
                    curr = curr.OwningContainer().expect("nil pointer dereference: OwningContainer");
                }

                let tn = reachableFrom(&curr, false, &[Some(nodeA.clone())])?;
                if tn.iter().any(|n| nodeA.IsDescendantOf(Some(n))) {
                    return Ok(false);
                }
                transposeNodes = tn;
            } else {
                let mut tn = reachableNodesB;
                tn.push(node.clone());
                transposeNodes = tn;
            }

            centerNode = nodeA;
        } else {
            if edgeAbductions.is_none() {
                let mut curr = node.clone();
                while curr.OwningContainer() != ancestorA {
                    curr = curr.OwningContainer().expect("nil pointer dereference: OwningContainer");
                }

                let tn = reachableFrom(&curr, false, &[Some(nodeB.clone())])?;
                if tn.iter().any(|n| nodeB.IsDescendantOf(Some(n))) {
                    return Ok(false);
                }
                transposeNodes = tn;
            } else {
                let mut tn = reachableNodesA;
                tn.push(node.clone());
                transposeNodes = tn;
            }
            centerNode = nodeB;
        }
    }
    reachabilityGuard.Finish()?;

    for n in &transposeNodes {
        if n.FixedOrigin().is_some() {
            return Ok(false);
        }
    }

    let elo = || placementcost::EdgeLengthOptions {
        EdgeAbductions: edgeAbductions.map(|a| a.to_vec()),
        IncludeNodeSizes: true,
        EnforceMinimumGap: false,
        PenalizeDirection: true,
    };
    let calcLength = || -> GoResult<f64> {
        if edgeAbductions.is_none() {
            return placementcost::EdgeLength(&ctx, g, &elo());
        }
        let mut sum = placementcost::NodeEdgeLength(&ctx, node, &elo())?;
        for e in node.edges() {
            let length = placementcost::NodeEdgeLength(&ctx, &node.Adjacent(&e), &elo())?;
            sum += length;
        }
        Ok(sum)
    };

    let mut bestLength = calcLength()?;
    let mut bestRotations: i64 = -1;

    let txn = g.NewRequestTransaction(&ctx, layoutgraph::TransactionOptions { AffectContainers: true, ..Default::default() })?;

    let round = edgeAbductions.is_some();
    let addRotationOp = |times: i64| {
        let ctx = ctx.clone();
        let g = g.clone();
        let transposeNodes = transposeNodes.clone();
        let centerNode = centerNode.clone();
        txn.AddOp(move || -> GoResult<()> {
            for n in &transposeNodes {
                rotateAround(n, &g, &centerNode, times, round);
                if n.IsClusterVessel() {
                    let cluster = g.clusters().get(n).expect("nil pointer dereference: cluster");
                    optimizeCluster(&ctx, &cluster, true)?;
                }
            }
            Ok(())
        });
    };

    // Rotate all around
    for i in 0..3i64 {
        addRotationOp(i + 1);

        match txn.Commit(&ctx) {
            Ok(()) => {
                let length = match calcLength() {
                    Ok(v) => v,
                    Err(scoreErr) => {
                        txn.Rollback();
                        txn.Clear();
                        return Err(scoreErr);
                    }
                };
                if geo::PrecisionCompare(length, bestLength, geo::PRECISION) < 0 {
                    bestLength = length;
                    bestRotations = i + 1;
                }
            }
            Err(err) => {
                if !layoutgraph::IsCandidateRejection(Some(&err)) {
                    return Err(err);
                }
            }
        }

        txn.Rollback();
        txn.Clear();
    }

    if bestRotations != -1 {
        addRotationOp(bestRotations);
        txn.Commit(&ctx)?;
        return Ok(true);
    }

    Ok(false)
}

pub(crate) fn rotateAround(n: &NodeRef, g: &GraphRef, centerNode: &NodeRef, times: i64, round: bool) {
    for _ in 0..times {
        let nodeCenter = *n.Center().b();
        let (cx, cy, cw, ch) = (centerNode.x(), centerNode.y(), centerNode.w(), centerNode.h());

        // Translate node center to origin
        let translatedX = nodeCenter.X - cx - cw / 2.0;
        let translatedY = nodeCenter.Y - cy - ch / 2.0;

        // Rotate 90 degrees counterclockwise
        let rotatedX = -translatedY;
        let rotatedY = translatedX;

        // Translate back
        let newCenterX = rotatedX + cx + cw / 2.0;
        let newCenterY = rotatedY + cy + ch / 2.0;

        // Calculate new top-left position
        let mut newTopLeftX = (newCenterX - n.w() / 2.0).round();
        let mut newTopLeftY = (newCenterY - n.h() / 2.0).round();

        if round {
            let cellSize = g.b().CellSize;
            if math::F64ToInt(newTopLeftX) % math::F64ToInt(cellSize) != 0 {
                newTopLeftX = roundToNearestCellSize(newTopLeftX, cellSize);
            }
            if math::F64ToInt(newTopLeftY) % math::F64ToInt(cellSize) != 0 {
                newTopLeftY = roundToNearestCellSize(newTopLeftY, cellSize);
            }
        }

        n.MoveAbsWithChildren(newTopLeftX, newTopLeftY);
    }
}
