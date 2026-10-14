// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/placement/stages.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.
//
// Port of placement/stages.go
#[allow(unused_imports)]
use crate::*;
use gocompat::math;
use gocompat::{Ctx, GoResult};
use layoutgraph::{GraphRef, NodeRef};
use shape::Shape as _;

pub fn Prescale(graph: &GraphRef) {
    for node in graph.nodes() {
        if node.shape().AspectRatio1() {
            let size = math::Max(node.w(), node.h());
            node.set_w(size);
            node.set_h(size);
        }
        scaleBasedOnEdges(&node);
    }
}

pub(crate) fn scaleBasedOnEdges(node: &NodeRef) {
    {
        let n = node.b();
        if n.FixedTopLeft.is_some() || n.DesiredWidth.is_some() || n.DesiredHeight.is_some() {
            return;
        }
    }
    if node.IsTable() || node.IsClass() || node.b().Edges.is_empty() {
        return;
    }
    let edges = node.edges();
    // map iteration below is order-independent (sum and max)
    let mut edgeCounts: gocompat::Map<NodeRef, i64> = gocompat::NewMapCap(edges.len());
    for edge in &edges {
        let adjacent = node.Adjacent(edge);
        if &adjacent != node {
            *edgeCounts.entry(adjacent).or_insert(0) += 1;
        }
    }
    let mut totalEdges: i64 = 0;
    let mut maxEdgesToAdjacent: i64 = 0;
    for (_, &count) in edgeCounts.iter() {
        totalEdges += count;
        maxEdgesToAdjacent = std::cmp::max(maxEdgesToAdjacent, count);
    }
    let mut sidesForEdges = 4.0;
    if edgeCounts.len() < 4 {
        sidesForEdges = edgeCounts.len() as f64;
    }
    let edgesPerSide = std::cmp::max(maxEdgesToAdjacent, math::F64ToInt((totalEdges as f64 / sidesForEdges).ceil()));
    if edgesPerSide == 1 {
        return;
    }
    let mut minLength = (edgesPerSide + 1) as f64 * placementcost::SideEdgeSpacing;
    if minLength < math::Min(node.w(), node.h()) {
        return;
    }
    let (mut xRatio, mut yRatio) = (1.0, 1.0);
    if node.shape().AspectRatio1() {
        if node.w() < minLength {
            xRatio = minLength / node.w();
            yRatio = minLength / node.h();
            node.set_w(minLength);
            node.set_h(minLength);
        }
    } else {
        if node.w() < minLength {
            xRatio = minLength / node.w();
            node.set_w(minLength);
        }
        if node.h() < minLength {
            yRatio = minLength / node.h();
            node.set_h(minLength);
        }
    }
    let Some(nodeFontSize) = node.b().FontSize else {
        return;
    };
    let minRatio = math::Min(xRatio, yRatio);
    let mut bestRatio = 1.0;
    let mut fontSize = nodeFontSize;
    let mut closestDistance = math::Inf(1);
    for size in talaFontSizes() {
        let fontRatio = size as f64 / nodeFontSize as f64;
        let distance = (fontRatio - minRatio).abs();
        if distance < closestDistance {
            fontSize = size;
            closestDistance = distance;
            bestRatio = fontRatio;
        }
    }
    if bestRatio > minRatio {
        minLength = (minLength * bestRatio / minRatio).ceil();
        if node.w() < minLength {
            node.set_w(minLength);
        }
        if node.h() < minLength {
            node.set_h(minLength);
        }
    }
    node.bm().FontSize = Some(fontSize);
    if let Some(label) = node.label() {
        let mut l = label.bm();
        l.Width = (l.Width * bestRatio).ceil();
        l.Height = (l.Height * bestRatio).ceil();
    }
}

/// talaFontSizes returns the adapter-supported layout font scale by value so
/// callers cannot mutate shared engine state.
pub(crate) fn talaFontSizes() -> [i64; 7] {
    [13, 14, 16, 20, 24, 28, 32]
}

pub fn Prepare(graph: &GraphRef) {
    loops::ComputeOffsets(graph);
    labeling::Initialize(graph);
    graph.ComputeNodeSpacing();
}

pub(crate) struct normalizeGapsRollback {
    pub(crate) graph: GraphRef,
    pub(crate) txn: layoutgraph::TransactionRef,
    pub(crate) graphState: Option<layoutgraph::GraphStateRef>,
    pub(crate) cellSize: f64,
    pub(crate) routingCosts: layoutgraph::RoutingCostState,
}

impl normalizeGapsRollback {
    pub(crate) fn restore(&self) {
        layoutgraph::RestoreGraphState(&self.graph, self.graphState.as_ref().expect("nil graphState"));
        self.graph.bm().CellSize = self.cellSize;
        if !self.txn.RestorePlacementCosts() {
            self.graph.RestoreRoutingCosts(self.routingCosts.clone());
        }
    }
}

/// Go's `graph == nil` check is unreachable through this signature (callers
/// always hold a graph), so the Rust port takes `&GraphRef`.
pub fn Place(ctx: &Ctx, graph: &GraphRef, seed: i64) -> GoResult<()> {
    let priorCommonUncleSiblings = graph.b().CommonUncleSiblings.clone();
    let mut ownership = layoutgraph::NodeGraphOwnershipSnapshot::default();
    let result = (|| -> GoResult<()> {
        let cus = proximity::CommonUncleSiblings(graph);
        graph.bm().CommonUncleSiblings = cus;
        ownership = layoutgraph::SnapshotNodeGraphOwnership(ctx, Some(graph))?;
        let ctx = orientationContext(ctx, graph)?;
        placeNodes(&ctx, graph, None, seed, None, &[])?;
        let gnodes = graph.nodes();
        let mut nodes: Vec<NodeRef> = Vec::with_capacity(gnodes.len());
        for node in &gnodes {
            nodes.push(node.clone());
            nodes.extend(graph.AllDescendantNodes(Some(node), true));
        }
        for node in &nodes {
            node.bm().Graph = Some(graph.clone());
        }
        graph.ComputeCellSize();
        graph.ResetTurnCost();
        Ok(())
    })();
    if result.is_ok() {
        // Successful placement does not retain transient proximity hints.
        graph.bm().CommonUncleSiblings = layoutgraph::GoMap::new();
    } else {
        ownership.Restore();
        graph.bm().CommonUncleSiblings = priorCommonUncleSiblings;
    }
    result
}

pub fn NormalizeGaps(ctx: &Ctx, graph: &GraphRef) -> GoResult<bool> {
    let (ctx, guard) = layoutgraph::EnsureTransactionWorkGuard(ctx, "GapNormalizationTransactions")?;
    let txn = graph.NewRequestTransaction(&ctx, layoutgraph::TransactionOptions { AffectContainers: true, ..Default::default() })?;
    let rollback = normalizeGapsRollback {
        graph: graph.clone(),
        txn: txn.clone(),
        graphState: txn.PreservePriorGraphState(),
        cellSize: graph.b().CellSize,
        routingCosts: graph.RoutingCosts(),
    };
    let result = (|| -> GoResult<bool> {
        let bidirectional = |nodes: &[NodeRef], axis: layoutAxis| -> GoResult<bool> {
            let forward = gapNormalization(
                &ctx,
                nodes,
                &txn,
                graph,
                gapNormalizationOptions { axis, direction: forwardDirection, costTxn: Some(txn.clone()) },
            )?;
            let backward = gapNormalization(
                &ctx,
                nodes,
                &txn,
                graph,
                gapNormalizationOptions { axis, direction: backwardDirection, costTxn: Some(txn.clone()) },
            );
            match backward {
                Ok(b) => Ok(forward || b),
                Err(e) => Err(e),
            }
        };
        let mut changed = false;
        let containers = graph.ContainerRDFSOrder(None, &guard)?;
        for container in &containers {
            let nodes = graph.AllDescendantNodes(Some(container), false);
            for axis in [horizontalAxis, verticalAxis] {
                let current = bidirectional(&nodes, axis)?;
                changed = changed || current;
            }
        }
        for axis in [horizontalAxis, verticalAxis] {
            let gnodes = graph.nodes();
            let current = bidirectional(&gnodes, axis)?;
            changed = changed || current;
        }
        graph.ResetTurnCost();
        graph.SyncSequences();
        graph.SyncClusters();
        guard.Finish()?;
        Ok(changed)
    })();
    if result.is_err() {
        rollback.restore();
    }
    result
}

pub fn Align(ctx: &Ctx, graph: &GraphRef) -> GoResult<()> {
    let (ctx, _) = layoutgraph::EnsureTransactionWorkGuard(ctx, "AlignAxesTransactions")?;
    if graph.b().CellSize == 0.0 {
        graph.ComputeCellSize();
    }
    let txn = graph.NewRequestTransaction(&ctx, layoutgraph::TransactionOptions { AffectContainers: true, ..Default::default() })?;
    for _ in 0..100 {
        let changed = alignAxes(&ctx, graph, &txn)?;
        if !changed {
            break;
        }
    }
    Ok(())
}

pub fn Swap(ctx: &Ctx, graph: &GraphRef) -> GoResult<()> {
    let (ctx, _) = layoutgraph::EnsureTransactionWorkGuard(ctx, "SwapStuffTransactions")?;
    graph.ComputeCellSize();
    for _ in 0..4 {
        let gnodes = graph.nodes();
        let changed = swapOptimize(&ctx, &gnodes, graph)?;
        if !changed {
            break;
        }
    }
    let gnodes = graph.nodes();
    direct(&ctx, graph, &gnodes, None, directOptions { checkEdgeLength: true })
}

pub fn TransposeAll(ctx: &Ctx, graph: &GraphRef) -> GoResult<()> {
    let (ctx, _) = layoutgraph::EnsureTransactionWorkGuard(ctx, "TransposeStageTransactions")?;
    graph.ComputeCellSize();
    for node in graph.nodes() {
        transpose(&ctx, graph, &node, None)?;
    }
    Ok(())
}

pub fn Normalize(graph: &GraphRef) {
    let (mut minX, mut minY) = (math::Inf(1), math::Inf(1));
    if graph.HasFixedNode() {
        minX = placementPadding;
        minY = placementPadding;
    } else {
        for node in graph.nodes() {
            minX = math::Min(minX, node.x());
            minY = math::Min(minY, node.y());
        }
        for edge in graph.edges() {
            for point in edge.points() {
                let p = *point.b();
                minX = math::Min(minX, p.X.floor());
                minY = math::Min(minY, p.Y.floor());
            }
            let label = edge.b().Label.clone();
            if let Some(label) = label {
                let (pos, w, h) = {
                    let l = label.b();
                    (l.Position, l.Width, l.Height)
                };
                let topLeft = edge.LabelTopLeft(pos, w, h).expect("nil pointer dereference: label top left");
                let tl = *topLeft.b();
                minX = math::Min(minX, tl.X.floor());
                minY = math::Min(minY, tl.Y.floor());
            }
        }
    }
    for node in graph.nodes() {
        let tl = node.tl().expect("nil pointer dereference: node.TopLeft");
        let mut p = tl.bm();
        p.X -= minX;
        p.Y -= minY;
    }
    for edge in graph.edges() {
        for point in edge.points() {
            let mut p = point.bm();
            p.X -= minX;
            p.Y -= minY;
        }
    }
}

pub fn Pad(graph: &GraphRef) {
    for node in graph.nodes() {
        let tl = node.tl().expect("nil pointer dereference: node.TopLeft");
        let mut p = tl.bm();
        p.X += placementPadding;
        p.Y += placementPadding;
    }
}
