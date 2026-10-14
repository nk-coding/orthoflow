// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/placement/equidistance.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.
//
// Port of placement/equidistance.go
#[allow(unused_imports)]
use crate::*;
use gocompat::{Ctx, GoResult, Set};
use layoutgraph::{GraphRef, NodeRef};

fn equidistanceLengthOptions() -> placementcost::EdgeLengthOptions {
    placementcost::EdgeLengthOptions {
        EdgeAbductions: None,
        IncludeNodeSizes: true,
        EnforceMinimumGap: false,
        PenalizeDirection: true,
    }
}

// Equidistance centers nodes between aligned neighbors.
//
// A ---------------- B ------ C
// Move B to the midpoint of A and C.
pub fn Equidistance(ctx: &Ctx, g: &GraphRef) -> GoResult<bool> {
    let (ctx, _) = layoutgraph::EnsureTransactionWorkGuard(ctx, "EquidistanceTransactions")?;
    layoutgraph::Validate(&ctx, "Equidistance", g)?;
    let originalPositions = snapshotNodePositionsContext(&ctx, "Equidistance", &g.nodes())?;
    let result = (|| -> GoResult<bool> {
        let reachabilityGuard = limits::NewWorkGuard(&ctx, "EquidistanceReachability", limits::MaxEngineWorkUnits)?;
        let mut movedHorizontally = false;
        let mut movedVertically = false;
        for n in g.nodes() {
            let movedHorizontally2 = equidistanceNodeGuarded(&ctx, &n, g, true, &reachabilityGuard)?;
            let movedVertically2 = equidistanceNodeGuarded(&ctx, &n, g, false, &reachabilityGuard)?;
            if movedHorizontally2 {
                movedHorizontally = true;
            }
            if movedVertically2 {
                movedVertically = true;
            }
        }
        // Try multiple times, because moving one may result in different symmetries for others
        if movedHorizontally {
            let mut movedAgain = false;
            for _ in 0..5 {
                for n in g.nodes() {
                    let m = equidistanceNodeGuarded(&ctx, &n, g, true, &reachabilityGuard)?;
                    if m {
                        movedAgain = true;
                    }
                }
                if !movedAgain {
                    break;
                }
                movedAgain = false;
            }
        }
        if movedVertically {
            let mut movedAgain = false;
            for _ in 0..5 {
                for n in g.nodes() {
                    let m = equidistanceNodeGuarded(&ctx, &n, g, false, &reachabilityGuard)?;
                    if m {
                        movedAgain = true;
                    }
                }
                if !movedAgain {
                    break;
                }
                movedAgain = false;
            }
        }

        reachabilityGuard.Finish()?;
        Ok(movedHorizontally || movedVertically)
    })();
    if result.is_err() {
        restoreNodePositions(&originalPositions);
    }
    result
}

pub(crate) fn equidistanceNodeGuarded(
    ctx: &Ctx,
    n: &NodeRef,
    g: &GraphRef,
    isHorizontal: bool,
    reachabilityGuard: &limits::WorkGuardRef,
) -> GoResult<bool> {
    if let Some(err) = ctx.Err() {
        return Err(err);
    }
    if n.b().Hierarchy.is_some() {
        return Ok(false);
    }
    if g.nodeToTree().has(n) {
        return Ok(false);
    }
    if g.IsTreeSentinel(Some(n)) {
        return Ok(false);
    }
    if n.b().FixedTopLeft.is_some() {
        return Ok(false);
    }
    let mut n = n.clone();

    let mut nearestBack: Option<NodeRef> = None;
    let mut nearestFront: Option<NodeRef> = None;

    for e in n.edges() {
        let adj = n.Adjacent(&e);
        if n.IsDescendantOf(Some(&adj)) || adj.IsDescendantOf(Some(&n)) {
            return Ok(false);
        }
        if isHorizontal {
            // Left
            if adj.x() + adj.w() < n.x() {
                if nearestBack.as_ref().map_or(true, |nb| adj.x() + adj.w() > nb.x() + nb.w()) {
                    nearestBack = Some(adj.clone());
                }
            }
            // Right
            if adj.x() > n.x() + n.w() {
                if nearestFront.as_ref().map_or(true, |nf| adj.x() < nf.x()) {
                    nearestFront = Some(adj.clone());
                }
            }
        } else {
            // Top
            if adj.y() + adj.h() < n.y() {
                if nearestBack.as_ref().map_or(true, |nb| adj.y() + adj.h() > nb.y() + nb.h()) {
                    nearestBack = Some(adj.clone());
                }
            }
            // Bottom
            if adj.y() > n.y() + n.h() {
                if nearestFront.as_ref().map_or(true, |nf| adj.y() < nf.y()) {
                    nearestFront = Some(adj.clone());
                }
            }
        }
    }

    let (Some(mut nearestBack), Some(mut nearestFront)) = (nearestBack, nearestFront) else {
        return Ok(false);
    };

    let mut otherConnected: Vec<NodeRef> = Vec::new();
    for e in n.edges() {
        let adj = n.Adjacent(&e);
        if adj == nearestBack || adj == nearestFront {
            continue;
        }
        if isHorizontal {
            if !adj.Orientation(&n).IsVertical() {
                continue;
            }
        } else if !adj.Orientation(&n).IsHorizontal() {
            continue;
        }
        if adj.IsDescendantOf(Some(&nearestBack)) || nearestBack.IsDescendantOf(Some(&adj)) {
            continue;
        }
        if adj.IsDescendantOf(Some(&nearestFront)) || nearestFront.IsDescendantOf(Some(&adj)) {
            continue;
        }

        let mut ignore: Set<NodeRef> = Set::default();
        ignore.insert(n.clone());
        ignore.insert(nearestBack.clone());
        ignore.insert(nearestFront.clone());
        let reachable = adj.AllReachableNodesContext(false, true, true, Some(&ignore), reachabilityGuard)?;

        // (Go evaluates a no-op filtering loop over reachable here.)

        for reachableN in reachable {
            if !otherConnected.contains(&reachableN) {
                otherConnected.push(reachableN);
            }
        }
    }

    for c in &otherConnected {
        if c.b().FixedTopLeft.is_some() {
            otherConnected = Vec::new();
            break;
        }
    }

    let ancestorBack = n.NearestSharedAncestor(&nearestBack);
    let ancestorFront = n.NearestSharedAncestor(&nearestFront);

    // We want to be able to move the whole container
    while let Some(oc) = n.OwningContainer() {
        if nearestBack.IsDescendantOf(Some(&oc)) || nearestFront.IsDescendantOf(Some(&oc)) {
            break;
        }
        n = oc;
    }

    // If b is the nearest back, b's container is relevant and should be used,
    // unless the container is not fully behind n.
    while nearestBack.OwningContainer() != ancestorBack {
        let container = nearestBack.OwningContainer().expect("nil pointer dereference: container");
        if isHorizontal {
            if container.x() + container.w() < n.x() {
                nearestBack = container;
            } else {
                break;
            }
        } else if container.y() + container.h() < n.y() {
            nearestBack = container;
        } else {
            break;
        }
    }
    while nearestFront.OwningContainer() != ancestorFront {
        let container = nearestFront.OwningContainer().expect("nil pointer dereference: container");
        if isHorizontal {
            if container.x() > n.x() + n.w() {
                nearestFront = container;
            } else {
                break;
            }
        } else if container.y() > n.y() + n.h() {
            nearestFront = container;
        } else {
            break;
        }
    }

    let originalLength = placementcost::EdgeLength(ctx, g, &equidistanceLengthOptions())?;

    let mut diffX = 0.0;
    let mut diffY = 0.0;
    if isHorizontal {
        let x = ((nearestBack.x() + nearestBack.w() + nearestFront.x()) / 2.0 - n.w() / 2.0).round();
        diffX = x - n.x();
    } else {
        let y = ((nearestBack.y() + nearestBack.h() + nearestFront.y()) / 2.0 - n.h() / 2.0).round();
        diffY = y - n.y();
    }

    let txn = g.NewRequestTransaction(ctx, layoutgraph::TransactionOptions { AffectContainers: true, ..Default::default() })?;

    let mut rolledBack = false;
    let mut soloMoveLength = 0.0;
    let mut connectedMoveLength = 0.0;

    // First try moving just the node by itself
    {
        let n = n.clone();
        txn.AddOp(move || {
            n.MoveWithChildren(diffX, diffY);
            Ok(())
        });
    }
    if let Err(err) = txn.Commit(ctx) {
        if !layoutgraph::IsCandidateRejection(Some(&err)) {
            return Err(err);
        }
        rolledBack = true;
    }
    if !rolledBack {
        let newLength = match placementcost::EdgeLength(ctx, g, &equidistanceLengthOptions()) {
            Ok(v) => v,
            Err(err) => {
                txn.Rollback();
                return Err(err);
            }
        };
        if geo::PrecisionCompare(newLength, originalLength, geo::PRECISION) <= 0 {
            soloMoveLength = newLength;
        }
        txn.Rollback();
    }

    txn.Clear();

    let mut withN = otherConnected.clone();
    withN.push(n.clone());

    // Then try moving with connected
    if !otherConnected.is_empty() {
        rolledBack = false;
        {
            let withN = withN.clone();
            txn.AddOp(move || {
                for n2 in &withN {
                    n2.MoveWithChildren(diffX, diffY);
                }
                Ok(())
            });
        }
        if let Err(err) = txn.Commit(ctx) {
            if !layoutgraph::IsCandidateRejection(Some(&err)) {
                return Err(err);
            }
            rolledBack = true;
        }
        if !rolledBack {
            let newLength = match placementcost::EdgeLength(ctx, g, &equidistanceLengthOptions()) {
                Ok(v) => v,
                Err(err) => {
                    txn.Rollback();
                    return Err(err);
                }
            };
            if geo::PrecisionCompare(newLength, originalLength, geo::PRECISION) <= 0 {
                connectedMoveLength = newLength;
            }
            txn.Rollback();
        }
    }

    txn.Clear();

    // Then use the best global edge length and move it there
    if soloMoveLength == 0.0 && connectedMoveLength == 0.0 {
        return Ok(false);
    }
    {
        let n = n.clone();
        txn.AddOp(move || {
            if soloMoveLength == 0.0 && connectedMoveLength > 0.0 {
                for n2 in &withN {
                    n2.MoveWithChildren(diffX, diffY);
                }
            } else if soloMoveLength > 0.0 && connectedMoveLength == 0.0 {
                n.MoveWithChildren(diffX, diffY);
            } else if geo::PrecisionCompare(soloMoveLength, connectedMoveLength, geo::PRECISION) < 0 {
                n.MoveWithChildren(diffX, diffY);
            } else {
                for n2 in &withN {
                    n2.MoveWithChildren(diffX, diffY);
                }
            }
            Ok(())
        });
    }

    txn.Commit(ctx)?;
    Ok(true)
}
