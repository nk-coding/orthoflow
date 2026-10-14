// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/placement/swap.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.
//
// Port of placement/swap.go
#[allow(unused_imports)]
use crate::*;
use gocompat::math;
use gocompat::{Ctx, GoResult};
use layoutgraph::{GraphRef, NodeRef};

pub(crate) fn swapPositions(nodeA: &NodeRef, nodeB: &NodeRef) {
    let tmpX = nodeA.x();
    let tmpY = nodeA.y();

    nodeA.MoveWithChildren(nodeB.x() - nodeA.x(), nodeB.y() - nodeA.y());

    nodeB.MoveWithChildren(tmpX - nodeB.x(), tmpY - nodeB.y());
}

pub(crate) fn smartSwapPositions(nodeA: &NodeRef, nodeB: &NodeRef) {
    let tmpX = nodeA.x();
    let tmpY = nodeA.y();

    let o = nodeA.Orientation(nodeB);
    if o == geo::Left {
        nodeA.MoveAbsWithChildren(nodeB.x() + nodeB.w() - nodeA.w(), nodeA.y());
        nodeB.MoveAbsWithChildren(tmpX, nodeB.y());
    } else if o == geo::Right {
        nodeA.MoveAbsWithChildren(nodeB.x(), nodeA.y());
        nodeB.MoveAbsWithChildren(tmpX + nodeA.w() - nodeB.w(), nodeB.y());
    } else if o == geo::Top {
        nodeA.MoveAbsWithChildren(nodeA.x(), nodeB.y() + nodeB.h() - nodeA.h());
        nodeB.MoveAbsWithChildren(nodeB.x(), tmpY);
    } else if o == geo::Bottom {
        nodeA.MoveAbsWithChildren(nodeA.x(), nodeB.y());
        nodeB.MoveAbsWithChildren(nodeB.x(), tmpY + nodeA.h() - nodeB.h());
    } else if o == geo::TopLeft {
        nodeA.MoveAbsWithChildren(nodeB.x() + nodeB.w() - nodeA.w(), nodeB.y() + nodeB.h() - nodeA.h());
        nodeB.MoveAbsWithChildren(tmpX, tmpY);
    } else if o == geo::TopRight {
        nodeA.MoveAbsWithChildren(nodeB.x(), nodeB.y() + nodeB.h() - nodeA.h());
        nodeB.MoveAbsWithChildren(tmpX + nodeA.w() - nodeB.w(), tmpY);
    } else if o == geo::BottomLeft {
        nodeA.MoveAbsWithChildren(nodeB.x() + nodeB.w() - nodeA.w(), nodeB.y());
        nodeB.MoveAbsWithChildren(tmpX, tmpY + nodeA.h() - nodeB.h());
    } else if o == geo::BottomRight {
        nodeA.MoveAbsWithChildren(nodeB.x(), nodeB.y());
        nodeB.MoveAbsWithChildren(tmpX + nodeA.w() - nodeB.w(), tmpY + nodeA.h() - nodeB.h());
    }
}

fn swapLengthOptions() -> placementcost::EdgeLengthOptions {
    placementcost::EdgeLengthOptions {
        EdgeAbductions: None,
        IncludeNodeSizes: true,
        EnforceMinimumGap: false,
        PenalizeDirection: true,
    }
}

// swapOptimize swaps node positions when doing so lowers placement cost.
pub(crate) fn swapOptimize(ctx: &Ctx, nodes: &[NodeRef], g: &GraphRef) -> GoResult<bool> {
    let (ctx, _) = layoutgraph::EnsureTransactionWorkGuard(ctx, "SwapOptimizeTransactions")?;
    let ctx = &ctx;
    let symmetryCost = g.b().CellSize;
    let mut swapMade = false;

    let measure = |n: &NodeRef| -> GoResult<f64> {
        let mut l = placementcost::NodeEdgeLength(ctx, n, &swapLengthOptions())?;
        let columnCrossingCost = placementcost::ColumnCrossingCost(ctx, n, None)?;
        let symmetry = placementcost::NodeSymmetry(ctx, n, None)?;
        l += columnCrossingCost;
        l -= symmetry * symmetryCost * n.b().Edges.len() as f64;
        Ok(l)
    };

    let txn = g.NewRequestTransaction(ctx, layoutgraph::TransactionOptions { AffectContainers: true, ..Default::default() })?;
    for node in nodes {
        if g.nodeToTree().has(node) {
            continue;
        }
        if node.b().Hierarchy.is_some() {
            continue;
        }
        if node.b().FixedTopLeft.is_some() {
            continue;
        }
        if node.b().Edges.is_empty() && !node.HasLeakyEdge() {
            continue;
        }

        let mut bestSmartSwapped = false;
        let mut bestSwapCandidate: Option<NodeRef> = None;
        let mut bestSwapEdgeLength = math::Inf(1);

        let currentGlobalL = placementcost::EdgeLength(ctx, g, &swapLengthOptions())?;
        let currentL1 = measure(node)?;
        let currentCrossings = placementcost::GraphEdgeCrossings(ctx, g)?;

        let siblings = g.containers().at(&node.containerField());
        for swapCandidate in &siblings {
            if swapCandidate == node {
                continue;
            }
            if g.nodeToTree().has(swapCandidate) {
                continue;
            }
            if swapCandidate.b().Hierarchy.is_some() {
                continue;
            }
            if swapCandidate.b().FixedTopLeft.is_some() {
                continue;
            }

            {
                let (a, b) = (node.clone(), swapCandidate.clone());
                txn.AddOp(move || {
                    swapPositions(&a, &b);
                    Ok(())
                });
            }

            let mut swappedL1 = 0.0;
            let mut swappedGlobalL = 0.0;
            let mut swappedCrossings: i64 = 0;
            match txn.Commit(ctx) {
                Ok(()) => {
                    let r = (|| -> GoResult<()> {
                        swappedL1 = measure(node)?;
                        swappedGlobalL = placementcost::EdgeLength(ctx, g, &swapLengthOptions())?;
                        swappedCrossings = placementcost::GraphEdgeCrossings(ctx, g)?;
                        Ok(())
                    })();
                    if let Err(err) = r {
                        txn.Rollback();
                        txn.Clear();
                        return Err(err);
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

            {
                let (a, b) = (node.clone(), swapCandidate.clone());
                txn.AddOp(move || {
                    smartSwapPositions(&a, &b);
                    Ok(())
                });
            }

            let mut smartSwappedL1 = 0.0;
            let mut smartSwappedGlobalL = 0.0;
            let mut smartSwappedCrossings: i64 = 0;
            match txn.Commit(ctx) {
                Ok(()) => {
                    let r = (|| -> GoResult<()> {
                        smartSwappedL1 = measure(node)?;
                        smartSwappedGlobalL = placementcost::EdgeLength(ctx, g, &swapLengthOptions())?;
                        smartSwappedCrossings = placementcost::GraphEdgeCrossings(ctx, g)?;
                        Ok(())
                    })();
                    if let Err(err) = r {
                        txn.Rollback();
                        txn.Clear();
                        return Err(err);
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

            let pc = |a: f64, b: f64| geo::PrecisionCompare(a, b, geo::PRECISION);
            if swappedGlobalL == 0.0 && smartSwappedGlobalL == 0.0 {
                continue;
            }
            if swappedL1 != 0.0 && smartSwappedL1 == 0.0 {
                if swappedCrossings <= currentCrossings && pc(swappedL1, currentL1) < 0 && pc(swappedGlobalL, currentGlobalL) < 0 && pc(swappedGlobalL, bestSwapEdgeLength) < 0 {
                    bestSwapEdgeLength = swappedGlobalL;
                    bestSmartSwapped = false;
                    bestSwapCandidate = Some(swapCandidate.clone());
                }
            } else if smartSwappedL1 != 0.0 && swappedL1 == 0.0 {
                if smartSwappedCrossings <= currentCrossings && pc(smartSwappedL1, currentL1) < 0 && pc(smartSwappedGlobalL, currentGlobalL) < 0 && pc(smartSwappedGlobalL, bestSwapEdgeLength) < 0 {
                    bestSwapEdgeLength = smartSwappedGlobalL;
                    bestSmartSwapped = true;
                    bestSwapCandidate = Some(swapCandidate.clone());
                }
            } else if swappedL1 != 0.0 && smartSwappedL1 != 0.0 {
                if pc(smartSwappedL1, swappedL1) < 0 {
                    if smartSwappedCrossings <= currentCrossings && pc(smartSwappedL1, currentL1) < 0 && pc(smartSwappedGlobalL, currentGlobalL) < 0 && pc(smartSwappedGlobalL, bestSwapEdgeLength) < 0 {
                        bestSwapEdgeLength = smartSwappedGlobalL;
                        bestSmartSwapped = true;
                        bestSwapCandidate = Some(swapCandidate.clone());
                    }
                } else if swappedCrossings <= currentCrossings && pc(swappedL1, currentL1) < 0 && pc(swappedGlobalL, currentGlobalL) < 0 && pc(swappedGlobalL, bestSwapEdgeLength) < 0 {
                    bestSwapEdgeLength = swappedGlobalL;
                    bestSmartSwapped = false;
                    bestSwapCandidate = Some(swapCandidate.clone());
                }
            } else {
                // They can both be 0 if the node is just a container, in which case we compare globals
                if swappedGlobalL != 0.0 && smartSwappedGlobalL == 0.0 {
                    if swappedCrossings <= currentCrossings && pc(swappedGlobalL, currentGlobalL) < 0 && pc(swappedGlobalL, bestSwapEdgeLength) < 0 {
                        bestSwapEdgeLength = swappedGlobalL;
                        bestSmartSwapped = false;
                        bestSwapCandidate = Some(swapCandidate.clone());
                    }
                } else if swappedGlobalL == 0.0 && smartSwappedGlobalL != 0.0 {
                    if smartSwappedCrossings <= currentCrossings && pc(smartSwappedGlobalL, currentGlobalL) < 0 && pc(smartSwappedGlobalL, bestSwapEdgeLength) < 0 {
                        bestSwapEdgeLength = smartSwappedGlobalL;
                        bestSmartSwapped = true;
                        bestSwapCandidate = Some(swapCandidate.clone());
                    }
                } else if pc(swappedGlobalL, smartSwappedGlobalL) < 0 {
                    if swappedCrossings <= currentCrossings && pc(swappedGlobalL, currentGlobalL) < 0 && pc(swappedGlobalL, bestSwapEdgeLength) < 0 {
                        bestSwapEdgeLength = swappedGlobalL;
                        bestSmartSwapped = false;
                        bestSwapCandidate = Some(swapCandidate.clone());
                    }
                } else if smartSwappedCrossings <= currentCrossings && pc(smartSwappedGlobalL, currentGlobalL) < 0 && pc(smartSwappedGlobalL, bestSwapEdgeLength) < 0 {
                    bestSwapEdgeLength = smartSwappedGlobalL;
                    bestSmartSwapped = true;
                    bestSwapCandidate = Some(swapCandidate.clone());
                }
            }
        }

        if let Some(best) = bestSwapCandidate {
            swapMade = true;
            {
                let a = node.clone();
                txn.AddOp(move || {
                    if bestSmartSwapped {
                        smartSwapPositions(&a, &best);
                    } else {
                        swapPositions(&a, &best);
                    }
                    Ok(())
                });
            }
            txn.Commit(ctx)?;
            txn.Clear();
            txn.UpdateState()?;
        }
    }

    Ok(swapMade)
}
