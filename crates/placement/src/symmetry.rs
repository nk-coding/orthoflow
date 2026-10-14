// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/placement/symmetry.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.
//
// Port of placement/symmetry.go
#[allow(unused_imports)]
use crate::*;
use gocompat::math;
use gocompat::{Ctx, GoResult, Set};
use layoutgraph::{GraphRef, NodeRef, NodesMethods};

// BalanceSymmetry centers simple nodes among neighbors aligned on one side.
pub fn BalanceSymmetry(ctx: &Ctx, g: &GraphRef) -> GoResult<()> {
    let (ctx, guard) = layoutgraph::EnsureTransactionWorkGuard(ctx, "BalanceSymmetryTransactions")?;
    if g.b().Nodes.is_empty() {
        return Ok(());
    }
    let mut rollbackState: Option<layoutgraph::GraphStateRef> = None;
    let result = (|| -> GoResult<()> {
        // Gap normalization takes care of cases where the adjacents are on opposite ends
        // So the only case we care about is when all the adjacents are aligned on one end
        // And we just nudge to be in the middle of the alignment
        for n in g.nodes() {
            if !isSimple(g, &n) {
                continue;
            }
            let edges = n.edges();
            if edges.len() < 2 {
                continue;
            }
            let mut sameSide = true;
            // Go collects these into a map and ranges over it; every later use
            // (AxisScore, area checks, Orientation of two aligned members,
            // bounding-box center) is independent of the member order.
            let mut adjacentsM: Set<NodeRef> = Set::default();
            let mut adjacentsOrder: Vec<NodeRef> = Vec::new();
            for i in 0..edges.len() - 1 {
                let e1 = &edges[i];
                let e2 = &edges[i + 1];
                {
                    let e = e1.b();
                    if e.FromTableColumnIndex.is_some() || e.ToTableColumnIndex.is_some() {
                        continue;
                    }
                }
                {
                    let e = e2.b();
                    if e.FromTableColumnIndex.is_some() || e.ToTableColumnIndex.is_some() {
                        continue;
                    }
                }
                let adj1 = n.Adjacent(e1);
                let adj2 = n.Adjacent(e2);
                let o1 = n.Orientation(&adj1);
                let o2 = n.Orientation(&adj2);
                if !o1.SameSide(o2) {
                    sameSide = false;
                    break;
                }
                if adjacentsM.insert(adj1.clone()) {
                    adjacentsOrder.push(adj1);
                }
                if adjacentsM.insert(adj2.clone()) {
                    adjacentsOrder.push(adj2);
                }
            }
            if !sameSide {
                continue;
            }
            if adjacentsM.len() < 2 {
                continue;
            }
            let adjacents: Vec<NodeRef> = adjacentsOrder;
            if placementcost::AxisScore(&adjacents) != 1.0 {
                continue;
            }
            // Fails to look symmetrical if uneven adjacents
            let mut maxArea = math::Inf(-1);
            for adj in &adjacents {
                maxArea = math::Max(maxArea, adj.w() * adj.h());
            }
            let mut uneven = false;
            for adj in &adjacents {
                if adj.w() * adj.h() < (0.5 * maxArea) {
                    uneven = true;
                    break;
                }
            }
            if uneven {
                continue;
            }
            let mut isHorizontal = true;
            if adjacents[0].Orientation(&adjacents[1]).IsVertical() {
                isHorizontal = false;
            }
            let txn = g.NewRequestTransactionWithWorkGuard(&ctx, Some(&guard), layoutgraph::TransactionOptions { AffectContainers: true, ..Default::default() })?;
            if rollbackState.is_none() {
                rollbackState = txn.b().PriorGraphState.clone();
            }
            {
                let n = n.clone();
                txn.AddOp(move || {
                    let center = *adjacents.Center().b();
                    let nCenter = *n.Center().b();
                    if isHorizontal {
                        n.MoveWithChildren((center.X - nCenter.X).floor(), 0.0);
                    } else {
                        n.MoveWithChildren(0.0, (center.Y - nCenter.Y).floor());
                    }
                    Ok(())
                });
            }
            if let Err(err) = txn.Commit(&ctx) {
                if !layoutgraph::IsCandidateRejection(Some(&err)) {
                    return Err(err);
                }
            }
        }
        Ok(())
    })();
    if result.is_err() {
        if let Some(state) = &rollbackState {
            layoutgraph::RestoreGraphState(g, state);
        }
    }
    result
}

// isSimple reports whether node participates only as a regular placement node,
// rather than as or inside one of the engine's structured graph forms.
pub(crate) fn isSimple(graph: &GraphRef, node: &NodeRef) -> bool {
    !(node.IsContainer()
        || graph.IsTreeSentinel(Some(node))
        || node.IsClusterVessel()
        || graph.IsSequenceVessel(node)
        || node.b().Cluster.is_some()
        || node.b().Hierarchy.is_some())
}
