// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/placement/cluster_optimization.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.
//
// Port of placement/cluster_optimization.go
#[allow(unused_imports)]
use crate::*;
use gocompat::{Ctx, GoError, GoResult};
use layoutgraph::{ClusterArrangement, ClusterRef, GraphRef, NodeRef, NodesMethods, TransactionRef};
use std::cell::Cell;
use std::rc::Rc;

#[derive(Clone, Default)]
pub(crate) struct clusterDesiredArrangementSnapshot {
    pub(crate) cluster: Option<ClusterRef>,
    pub(crate) arrangement: ClusterArrangement,
    pub(crate) set: bool,
}

pub(crate) struct optimizeClustersRollback {
    pub(crate) graph: GraphRef,
    pub(crate) graphState: Option<layoutgraph::GraphStateRef>,
    pub(crate) desiredArrangement: clusterDesiredArrangementSnapshot,
    pub(crate) cellSize: f64,
    pub(crate) costs: Option<layoutgraph::PlacementCostSnapshotRef>,
}

impl optimizeClustersRollback {
    pub(crate) fn recordDesiredArrangement(&mut self, cluster: &ClusterRef, arrangement: &ClusterArrangement) {
        // The first transaction snapshot contains every later cluster's entry value.
        // Only a change made before that snapshot exists needs a separate journal.
        if self.graphState.is_some() || self.desiredArrangement.set || cluster.b().DesiredArrangement == *arrangement {
            return;
        }
        self.desiredArrangement = clusterDesiredArrangementSnapshot {
            cluster: Some(cluster.clone()),
            arrangement: cluster.b().DesiredArrangement.clone(),
            set: true,
        };
    }

    pub(crate) fn recordGraphState(&mut self, txn: &TransactionRef) -> bool {
        if self.graphState.is_some() {
            return false;
        }
        self.graphState = txn.b().PriorGraphState.clone();
        true
    }

    pub(crate) fn capturePlacementCosts(&mut self, guard: &limits::WorkGuardRef) -> GoResult<()> {
        if self.costs.is_some() {
            return Ok(());
        }
        let cacheEntries = self.graph.EdgeLengthCacheEntries();
        if cacheEntries as i64 > layoutgraph::MaxTopologyReferences {
            return Err(GoError::new(format!(
                "TALA OptimizeClustersTransactions edge-length cache entries exceed limit {}",
                layoutgraph::MaxTopologyReferences
            )));
        }
        for _ in 0..cacheEntries {
            guard.Step()?;
        }
        let costs = self.graph.SnapshotPlacementCosts();
        guard.Finish()?;
        self.costs = Some(costs);
        Ok(())
    }

    pub(crate) fn restore(&self) {
        if let Some(state) = &self.graphState {
            layoutgraph::RestoreGraphState(&self.graph, state);
        }
        if self.desiredArrangement.set {
            self.desiredArrangement.cluster.as_ref().expect("nil cluster").bm().DesiredArrangement =
                self.desiredArrangement.arrangement.clone();
        }
        self.graph.bm().CellSize = self.cellSize;
        if let Some(costs) = &self.costs {
            costs.Restore();
        }
    }
}

pub(crate) fn alignConnectedNodes(c: &ClusterRef, horizontally: bool) {
    let externalNodes = clusterExternalConnectedNodes(c);
    if externalNodes.is_empty() {
        return;
    }
    let vessel = c.b().Vessel.clone();

    // align every node center with cluster center
    for n in &externalNodes {
        if n.b().FixedTopLeft.is_some() {
            continue;
        }
        // Don't want it to travel too far, risk of fucking up other things
        if !n.OverlapsAlongDimension(&vessel, !horizontally, true) {
            continue;
        }
        let mut xDelta = 0.0;
        let mut yDelta = 0.0;
        if horizontally {
            xDelta = (vessel.x() + vessel.w() / 2.0 - n.x() - n.w() / 2.0).round();
        } else {
            yDelta = (vessel.y() + vessel.h() / 2.0 - n.y() - n.h() / 2.0).round();
        }
        n.MoveWithChildren(xDelta, yDelta);
    }
}

pub(crate) fn alignVessel(c: &ClusterRef, horizontally: bool) {
    let externalNodes = clusterExternalConnectedNodes(c);
    if externalNodes.is_empty() {
        return;
    }

    let mut avgExternalX = 0.0;
    let mut avgExternalY = 0.0;
    for n in &externalNodes {
        avgExternalX += n.Center().b().X;
        avgExternalY += n.Center().b().Y;
    }
    avgExternalX /= externalNodes.len() as f64;
    avgExternalY /= externalNodes.len() as f64;

    let vessel = c.b().Vessel.clone();
    let mut xDelta = 0.0;
    let mut yDelta = 0.0;
    // align cluster center with avg external node center
    if horizontally {
        xDelta = avgExternalX - vessel.w() / 2.0 - vessel.x();
    } else {
        yDelta = avgExternalY - vessel.h() / 2.0 - vessel.y();
    }

    let tl = vessel.tl().expect("nil pointer dereference: vessel.TopLeft");
    let mut p = tl.bm();
    p.X += xDelta.round();
    p.Y += yDelta.round();
}

// A cluster is considered optimized when
// - its orientation is perpendicular to its edges
// - it's aligned symmetrically to its connected nodes
// If we cannot rotate to the right orientation, then give up (wrongly oriented + symmetrical looks worse than nonsymmetrical)
// If we are able to rotate to the right orientation (or we already are), then try to align
// If it's diagonal, use the one that matches the aspect ratio
pub(crate) fn optimizeCluster(ctx: &Ctx, c: &ClusterRef, onlyFlip: bool) -> GoResult<bool> {
    optimizeClusterWithRollback(ctx, c, onlyFlip, None)
}

fn arrangementAlign(c: &ClusterRef, f: fn(&ClusterRef, bool)) {
    let arrangement = c.b().Arrangement.clone();
    if arrangement == layoutgraph::Column {
        f(c, false);
    } else if arrangement == layoutgraph::Row {
        f(c, true);
    }
}

pub(crate) fn optimizeClusterWithRollback(
    ctx: &Ctx,
    c: &ClusterRef,
    onlyFlip: bool,
    mut rollback: Option<&mut optimizeClustersRollback>,
) -> GoResult<bool> {
    let (ctx, guard) = layoutgraph::EnsureTransactionWorkGuard(ctx, "OptimizeClusterTransactions")?;
    let mut horizontalCount = 0;
    let mut verticalCount = 0;
    let mut changed = false;
    let vessel = c.b().Vessel.clone();
    for externalNode in clusterExternalConnectedNodes(c) {
        let orientation = vessel.Orientation(&externalNode);
        if orientation == geo::NONE {
            continue;
        }

        if orientation == geo::Left || orientation == geo::Right {
            horizontalCount += 1;
        } else if orientation == geo::Top || orientation == geo::Bottom {
            verticalCount += 1;
        } else {
            // Diagonal
            let xDistance = (externalNode.Center().b().X - vessel.Center().b().X).abs();
            let yDistance = (externalNode.Center().b().Y - vessel.Center().b().Y).abs();
            if xDistance > yDistance {
                horizontalCount += 1;
            } else if yDistance > xDistance {
                verticalCount += 1;
            }
        }
    }

    let mut desiredArrangement = c.b().DesiredArrangement.clone();
    if verticalCount > horizontalCount {
        desiredArrangement = layoutgraph::Row;
    } else if horizontalCount > verticalCount {
        desiredArrangement = layoutgraph::Column;
    }
    if let Some(rb) = rollback.as_deref_mut() {
        rb.recordDesiredArrangement(c, &desiredArrangement);
    }
    c.bm().DesiredArrangement = desiredArrangement.clone();

    let cgraph = c.b().Graph.clone().expect("nil pointer dereference: cluster.Graph");
    let mut txn: Option<TransactionRef> = None;
    let mut ownsRollbackState = false;
    if c.b().Arrangement != desiredArrangement {
        let t = cgraph.NewRequestTransaction(&ctx, layoutgraph::TransactionOptions { AffectContainers: true, ..Default::default() })?;
        txn = Some(t.clone());
        if let Some(rb) = rollback.as_deref_mut() {
            ownsRollbackState = rb.recordGraphState(&t);
        }
        // first try flipping around center, if that fails try flipping around top left
        let tryCenter = Rc::new(Cell::new(true));
        {
            let c = c.clone();
            let tryCenter = tryCenter.clone();
            let txnRef = t.clone();
            t.AddOp(move || -> GoResult<()> {
                let arrangement = c.b().Arrangement.clone();
                if arrangement == layoutgraph::Column {
                    c.bm().Arrangement = layoutgraph::Row;
                } else if arrangement == layoutgraph::Row {
                    c.bm().Arrangement = layoutgraph::Column;
                }
                let padding = grouping::PaddingBetween(&c, true);
                c.bm().Padding = padding;
                c.SyncGeometry();

                let vessel = c.b().Vessel.clone();
                if tryCenter.get() {
                    // sync from column to row (top left is fixed), then centering
                    let arrangement = c.b().Arrangement.clone();
                    if arrangement == layoutgraph::Column {
                        let (_, originalHeight) = txnRef.OriginalDimensions(&vessel);
                        let heightDelta = originalHeight - vessel.h();
                        vessel.MoveWithChildren(0.0, (heightDelta / 2.0).round());
                    } else if arrangement == layoutgraph::Row {
                        let (originalWidth, _) = txnRef.OriginalDimensions(&vessel);
                        let widthDelta = originalWidth - vessel.w();
                        vessel.MoveWithChildren((widthDelta / 2.0).round(), 0.0);
                    }
                }

                let container = c.b().Container.clone();
                if let Some(container) = container {
                    if container.has_tl() {
                        let cg = c.b().Graph.clone().expect("nil pointer dereference: cluster.Graph");
                        let children = cg.containers().at(&Some(container.clone()));
                        let (childrenTL, childrenBR) = children.FixedBoundingBox();
                        let padding = cg.ContainerPadding(Some(&container), false);
                        container.FitToBoundingBox(
                            &childrenTL.expect("nil children bounding box"),
                            &childrenBR.expect("nil children bounding box"),
                            padding,
                        );
                        container.PositionContainerChildren(true);
                    }
                }
                Ok(())
            });
        }
        if let Err(err) = t.Commit(&ctx) {
            if !layoutgraph::IsCandidateRejection(Some(&err)) {
                t.Clear();
                return Err(err);
            }
            tryCenter.set(false);
            if let Err(err) = t.Commit(&ctx) {
                // Rust-only: drop the op (it holds a handle to `t`) so the Rc cycle is freed.
                t.Clear();
                if !layoutgraph::IsCandidateRejection(Some(&err)) {
                    return Err(err);
                }
                return Ok(changed);
            }
        }
        t.UpdateState()?;
        t.Clear();
        changed = true;
    }

    if !onlyFlip && c.b().Arrangement == desiredArrangement {
        // There's two ways alignment can happen, vessel moves or external nodes move
        // First try vessel move
        if txn.is_none() {
            let t = cgraph.NewRequestTransaction(&ctx, layoutgraph::TransactionOptions { AffectContainers: true, ..Default::default() })?;
            if let Some(rb) = rollback.as_deref_mut() {
                ownsRollbackState = rb.recordGraphState(&t);
            }
            txn = Some(t);
        }
        let mut t = txn.clone().unwrap();
        if let Some(rb) = rollback.as_deref_mut() {
            rb.capturePlacementCosts(&guard)?;
        }
        let elo = || placementcost::EdgeLengthOptions {
            EdgeAbductions: None,
            IncludeNodeSizes: true,
            EnforceMinimumGap: false,
            PenalizeDirection: true,
        };
        let mut bestLength = placementcost::EdgeLength(&ctx, &cgraph, &elo())?;
        let mut moveVessel = false;
        let mut moveNodes = false;

        {
            let c = c.clone();
            t.AddOp(move || {
                arrangementAlign(&c, alignVessel);
                Ok(())
            });
        }
        match t.Commit(&ctx) {
            Ok(()) => {
                let length = match placementcost::EdgeLength(&ctx, &cgraph, &elo()) {
                    Ok(l) => l,
                    Err(scoreErr) => {
                        t.Rollback();
                        t.Clear();
                        return Err(scoreErr);
                    }
                };
                if geo::PrecisionCompare(length, bestLength, geo::PRECISION) < 0 {
                    bestLength = length;
                    moveVessel = true;
                }
            }
            Err(err) => {
                if !layoutgraph::IsCandidateRejection(Some(&err)) {
                    return Err(err);
                }
            }
        }
        t.Rollback();
        t.Clear();

        {
            let c = c.clone();
            t.AddOp(move || {
                arrangementAlign(&c, alignConnectedNodes);
                Ok(())
            });
        }
        match t.Commit(&ctx) {
            Ok(()) => {
                let length = match placementcost::EdgeLength(&ctx, &cgraph, &elo()) {
                    Ok(l) => l,
                    Err(scoreErr) => {
                        t.Rollback();
                        t.Clear();
                        return Err(scoreErr);
                    }
                };
                if geo::PrecisionCompare(length, bestLength, geo::PRECISION) < 0 {
                    moveNodes = true;
                }
            }
            Err(err) => {
                if !layoutgraph::IsCandidateRejection(Some(&err)) {
                    return Err(err);
                }
            }
        }

        t.Rollback();
        t.Clear();

        if moveNodes {
            {
                let c = c.clone();
                t.AddOp(move || {
                    arrangementAlign(&c, alignConnectedNodes);
                    Ok(())
                });
            }
            t.Commit(&ctx)?;
            changed = true;
        } else if moveVessel {
            {
                let c = c.clone();
                t.AddOp(move || {
                    arrangementAlign(&c, alignVessel);
                    Ok(())
                });
            }
            t.Commit(&ctx)?;
            changed = true;
        }

        if moveNodes || moveVessel {
            // after flipping we may need to pull connected nodes closer
            t.Clear();
            if ownsRollbackState {
                // The first transaction's original graph state is the stage rollback
                // point. Detach before a second refresh could recycle that state as
                // transaction scratch.
                t = t.CloneGeometryContext()?;
            }
            t.UpdateState()?;
            {
                let ctx = ctx.clone();
                let c = c.clone();
                t.AddOp(move || {
                    let (vessel, arrangement) = {
                        let cb = c.b();
                        (cb.Vessel.clone(), cb.Arrangement.clone())
                    };
                    reduceGapToNeighbors(
                        &ctx,
                        &vessel,
                        None,
                        gapReductionOptions {
                            axis: axisForArrangement(&arrangement),
                            direction: forwardDirection,
                            attemptRecoverSymmetry: true,
                            costTxn: None,
                        },
                    )?;
                    Ok(())
                });
            }
            if let Err(err) = t.Commit(&ctx) {
                if !layoutgraph::IsCandidateRejection(Some(&err)) {
                    return Err(err);
                }
            }
        }
    }

    Ok(changed)
}

// make clusters horizontal if cluster connections are above or below cluster
pub fn OptimizeClusters(ctx: &Ctx, g: &GraphRef) -> GoResult<bool> {
    let (ctx, _) = layoutgraph::EnsureTransactionWorkGuard(ctx, "OptimizeClustersTransactions")?;
    let order = g.ClusterRDFSOrder();
    if order.is_empty() {
        return Ok(false);
    }
    optimizeClustersAtomic(&ctx, g, &order)
}

pub(crate) fn optimizeClustersAtomic(ctx: &Ctx, g: &GraphRef, order: &[NodeRef]) -> GoResult<bool> {
    let mut rollback = optimizeClustersRollback {
        graph: g.clone(),
        graphState: None,
        desiredArrangement: Default::default(),
        cellSize: g.b().CellSize,
        costs: None,
    };
    let mut changed = false;
    for vessel in order {
        let cluster = g.clusters().get(vessel).expect("nil pointer dereference: cluster");
        match optimizeClusterWithRollback(ctx, &cluster, false, Some(&mut rollback)) {
            Ok(changed2) => {
                if changed2 {
                    changed = true;
                }
            }
            Err(err) => {
                rollback.restore();
                return Err(err);
            }
        }
    }
    Ok(changed)
}
