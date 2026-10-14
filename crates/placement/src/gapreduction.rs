// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/placement/gapreduction.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.
//
// Port of placement/gapreduction.go
#[allow(unused_imports)]
use crate::*;
use gocompat::math;
use gocompat::{Ctx, GoError, GoResult};
use layoutgraph::{GraphRef, NodeRef, TransactionRef};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

#[derive(Clone)]
pub(crate) struct gapNormalizationOptions {
    pub(crate) axis: layoutAxis,
    pub(crate) direction: traversalDirection,
    pub(crate) costTxn: Option<TransactionRef>,
}

#[derive(Clone)]
pub(crate) struct gapReductionOptions {
    pub(crate) axis: layoutAxis,
    pub(crate) direction: traversalDirection,
    pub(crate) attemptRecoverSymmetry: bool,
    pub(crate) costTxn: Option<TransactionRef>,
}

fn noAbductionLength(penalizeDirection: bool) -> placementcost::EdgeLengthOptions {
    placementcost::EdgeLengthOptions {
        EdgeAbductions: None,
        IncludeNodeSizes: true,
        EnforceMinimumGap: false,
        PenalizeDirection: penalizeDirection,
    }
}

pub(crate) fn gapNormalization(
    ctx: &Ctx,
    nodes: &[NodeRef],
    txn: &TransactionRef,
    g: &GraphRef,
    options: gapNormalizationOptions,
) -> GoResult<bool> {
    if !options.axis.valid() {
        return Err(GoError::new("TALA GapNormalization requires an axis"));
    }
    if !options.direction.valid() {
        return Err(GoError::new("TALA GapNormalization requires a direction"));
    }
    let (ctx, _) = layoutgraph::EnsureTransactionWorkGuard(ctx, "GapNormalizationTransactions")?;
    // We gap reduce by order of most connected nodes to least connected
    // This should achieve the effect of bringing nodes into central areas of the diagram instead of isolated nodes dragging stuff to it
    let mut sortedByConnections: Vec<(i64, layoutgraph::EntityID, NodeRef)> =
        nodes.iter().map(|n| (n.b().Edges.len() as i64, n.id(), n.clone())).collect();
    gocompat::sort::SlicesSortFunc(&mut sortedByConnections, |a, b| {
        let order = b.0.cmp(&a.0);
        if order != std::cmp::Ordering::Equal {
            return order;
        }
        a.1.cmp(&b.1)
    });

    let mut changed = false;

    // whole trees can move towards other nodes but we don't want to move within trees
    for (_, _, node) in &sortedByConnections {
        if g.nodeToTree().has(node) {
            continue;
        }
        if node.b().Hierarchy.is_some() {
            continue;
        }
        let candidateTxn = txn.CloneGeometryContext()?;
        {
            let ctx = ctx.clone();
            let node = node.clone();
            let options = options.clone();
            txn.AddOp(move || {
                reduceGapToNeighbors(
                    &ctx,
                    &node,
                    Some(candidateTxn.clone()),
                    gapReductionOptions {
                        axis: options.axis,
                        direction: options.direction,
                        attemptRecoverSymmetry: true,
                        costTxn: options.costTxn.clone(),
                    },
                )?;
                Ok(())
            });
        }
        match txn.Commit(&ctx) {
            Err(err) => {
                if !layoutgraph::IsCandidateRejection(Some(&err)) {
                    return Err(err);
                }
            }
            Ok(()) => {
                txn.UpdateState()?;
                changed = true;
            }
        }
        txn.Clear();
    }

    Ok(changed)
}

// Assumes ahead is ahead of behind when forwards is true
pub(crate) fn isBetween(node: &NodeRef, behind: &NodeRef, ahead: &NodeRef, isHorizontal: bool, forwards: bool) -> bool {
    let delta = behind.DeltaTo(node, behind.tl().as_ref()) as f64;
    let (mut behind, mut ahead) = (behind.clone(), ahead.clone());
    if isHorizontal {
        if (node.y() + node.h()) < behind.y() - delta {
            return false;
        }
        if node.y() > behind.y() + behind.h() + delta {
            return false;
        }

        if !forwards {
            std::mem::swap(&mut behind, &mut ahead);
        }
        // n is behind behind
        if node.x() + node.w() < behind.x() + behind.w() {
            return false;
        }
        // n is ahead of ahead
        if node.x() > ahead.x() {
            return false;
        }
    } else {
        if (node.x() + node.w()) < behind.x() - delta {
            return false;
        }
        if node.x() > behind.x() + behind.w() + delta {
            return false;
        }

        if !forwards {
            std::mem::swap(&mut behind, &mut ahead);
        }
        if node.y() + node.h() < behind.y() + behind.h() {
            return false;
        }
        if node.y() > ahead.y() {
            return false;
        }
    }
    true
}

// Assumes otherNode is ahead of node when forwards is true
pub(crate) fn nearestBetween(
    nodes: &[NodeRef],
    behind: &NodeRef,
    ahead: &NodeRef,
    inContainer: Option<&NodeRef>,
    isHorizontal: bool,
    forwards: bool,
) -> Option<NodeRef> {
    let mut nearest: Option<NodeRef> = None;
    for n in nodes {
        if n == behind || n == ahead {
            continue;
        }
        if n.OwningContainer().as_ref() != inContainer {
            continue;
        }
        if !isBetween(n, behind, ahead, isHorizontal, forwards) {
            continue;
        }
        let Some(nr) = nearest.clone() else {
            nearest = Some(n.clone());
            continue;
        };
        if isHorizontal {
            if forwards {
                if n.x() < nr.x() {
                    nearest = Some(n.clone());
                }
            } else if (n.x() + n.w()) > (nr.x() + nr.w()) {
                nearest = Some(n.clone());
            }
        } else if forwards {
            if n.y() < nr.y() {
                nearest = Some(n.clone());
            }
        } else if (n.y() + n.h()) > (nr.y() + nr.h()) {
            nearest = Some(n.clone());
        }
    }

    nearest
}

pub(crate) fn nearestConnectedAhead(node: &NodeRef, isHorizontal: bool, forwards: bool) -> Option<NodeRef> {
    let mut nearest: Option<NodeRef> = None;

    for e in node.edges() {
        let adj = node.Adjacent(&e);
        if isHorizontal {
            if forwards {
                // if adj's left is behind node's right its not ahead
                if adj.x() < (node.x() + node.w()) {
                    continue;
                }
                if nearest.as_ref().map_or(true, |nr| adj.x() < nr.x()) {
                    nearest = Some(adj);
                }
            } else {
                // if adj's right is ahead of node's left its not behind
                if (adj.x() + adj.w()) > node.x() {
                    continue;
                }
                if nearest.as_ref().map_or(true, |nr| adj.x() + adj.w() > nr.x() + nr.w()) {
                    nearest = Some(adj);
                }
            }
        } else if forwards {
            if adj.y() < (node.y() + node.h()) {
                continue;
            }
            if nearest.as_ref().map_or(true, |nr| adj.y() < nr.y()) {
                nearest = Some(adj);
            }
        } else {
            if adj.y() + adj.h() > node.y() {
                continue;
            }
            if nearest.as_ref().map_or(true, |nr| (adj.y() + adj.h()) > (nr.y() + nr.h())) {
                nearest = Some(adj);
            }
        }
    }

    nearest
}

// reduceGapToNeighbors attempts to reduce the gap between this node and its neighbors in front of it
// returns true if reduced, false if no changes. If true, also returns the reduced global edge length
// (see gapreduction.go for the detailed scenarios).
pub(crate) fn reduceGapToNeighbors(
    ctx: &Ctx,
    node: &NodeRef,
    txn: Option<TransactionRef>,
    options: gapReductionOptions,
) -> GoResult<(bool, f64)> {
    if let Some(err) = ctx.Err() {
        return Err(err);
    }
    if !options.axis.valid() {
        return Err(GoError::new("TALA gap reduction requires an axis"));
    }
    if !options.direction.valid() {
        return Err(GoError::new("TALA gap reduction requires a direction"));
    }
    let mut txn = txn;
    let mut node = node.clone();
    let isHorizontal = options.axis.isHorizontal();
    let forwards = options.direction.isForward();
    let attemptRecoverSymmetry = options.attemptRecoverSymmetry;
    let graph = node.g();
    // Needed for calculating partial symmetry
    if graph.b().CellSize == 0.0 {
        graph.ComputeCellSize();
    }

    let Some(mut nearestAhead) = nearestConnectedAhead(&node, isHorizontal, forwards) else {
        return Ok((false, 0.0));
    };
    // if nearest ahead is fixed or is within a fixed node, we can't pull it closer
    {
        let mut c = nearestAhead.containerField();
        while let Some(cn) = c {
            if cn.b().FixedTopLeft.is_some() {
                return Ok((false, 0.0));
            }
            c = cn.containerField();
        }
    }
    let mut excluded: Vec<NodeRef> = vec![node.clone()];
    let nodeHerd = node.b().HerdAssignment.clone();
    if let Some(nodeHerd) = nodeHerd {
        // when normalizing the gap, siblings in the same herd orientation are excluded
        // so moving the connected nodes does not break the herd alignment
        let siblings = graph.containers().at(&node.containerField());
        for sibling in siblings {
            let siblingHerd = sibling.b().HerdAssignment.clone();
            if sibling == node || siblingHerd.is_none() || nearestAhead.IsDescendantOf(Some(&sibling)) {
                continue;
            }
            if siblingHerd.unwrap().b().Orientation == nodeHerd.b().Orientation {
                excluded.push(sibling);
            }
        }
    }

    let sharedContainer = node.NearestSharedAncestor(&nearestAhead);
    let nearestBetweenNode = nearestBetween(
        &nearestAhead.ConnectedNodes(&excluded, &graph),
        &node,
        &nearestAhead,
        sharedContainer.as_ref(),
        isHorizontal,
        forwards,
    );

    // we don't want fixed nodes preventing us from getting the actual nearest between (could be a fixed node's container)
    // but we only want to move connected nodes up until we reach a fixed node
    excluded.extend(graph.FixedNodes());
    let connectedNodes = nearestAhead.ConnectedNodeSet(&excluded, &graph);

    if let Some(nb) = nearestBetweenNode {
        nearestAhead = nb;
    }

    if let Some(costTxn) = &options.costTxn {
        costTxn.CapturePlacementCosts("GapNormalizationTransactions")?;
    }
    let oldEdgeLength = placementcost::EdgeLength(ctx, &graph, &noAbductionLength(false))?;
    let mut newEdgeLength = oldEdgeLength;

    let mut nodesInBetween: Vec<NodeRef> = Vec::new();
    for n in graph.nodes() {
        if n == node || n == nearestAhead {
            continue;
        }
        let isAConnectedNode = connectedNodes.contains(&n);
        if isAConnectedNode {
            continue;
        }
        if forwards {
            if n.IsBlocked(&node, &nearestAhead, true, isHorizontal) {
                nodesInBetween.push(n);
            }
        } else if n.IsBlocked(&nearestAhead, &node, true, isHorizontal) {
            nodesInBetween.push(n);
        }
    }

    let backwardTxn: Rc<RefCell<Option<TransactionRef>>> = Rc::new(RefCell::new(None));
    // We try pulling the nearest ahead to any nodes in between
    let mut candidates = vec![node.clone()];
    candidates.extend(nodesInBetween.iter().cloned());
    for candidateNode in candidates {
        // if nearest ahead is less nested, we only want move it to the candidate's ancestor in the same container
        // Note: we should always reach nearestAhead's Container, since it should be the outermost connected node
        let mut candidateNode = Some(candidateNode);
        let nearestAheadOwner = nearestAhead.OwningContainer();
        while let Some(cn) = candidateNode.clone() {
            if cn.OwningContainer() == nearestAheadOwner {
                break;
            }
            candidateNode = cn.OwningContainer();
            if candidateNode.is_none() {
                break;
            }
        }
        let Some(candidateNode) = candidateNode else {
            continue;
        };
        let gapSize;
        if isHorizontal {
            if forwards {
                gapSize = nearestAhead.x() - (candidateNode.x() + candidateNode.w());
            } else {
                gapSize = candidateNode.x() - (nearestAhead.x() + nearestAhead.w());
            }
        } else if forwards {
            gapSize = nearestAhead.y() - (candidateNode.y() + candidateNode.h());
        } else {
            gapSize = candidateNode.y() - (nearestAhead.y() + nearestAhead.h());
        }

        if gapSize <= largeGapThreshold * graph.b().CellSize {
            continue;
        }

        // TODO 150 seems a bit large for ideal gap size
        let mut delta = placementcost::IdealGapSize - gapSize;
        if !forwards {
            delta = -delta;
        }
        if delta == 0.0 {
            continue;
        }

        let candidateEdgeLength = Rc::new(Cell::new(0.0f64));
        match &txn {
            None => {
                txn = Some(graph.NewRequestTransaction(ctx, layoutgraph::TransactionOptions { AffectContainers: true, ..Default::default() })?);
            }
            Some(t) => {
                t.Clear();
                t.UpdateState()?;
            }
        }
        let t = txn.clone().unwrap();
        {
            let ctx = ctx.clone();
            let connectedNodes = connectedNodes.clone();
            let graph = graph.clone();
            let node = node.clone();
            let candidateEdgeLength = candidateEdgeLength.clone();
            let backwardTxn = backwardTxn.clone();
            let txnInner = t.clone();
            let options = options.clone();
            t.AddOp(move || -> GoResult<()> {
                for n in &connectedNodes {
                    if isHorizontal {
                        n.Translate(delta, 0.0);
                    } else {
                        n.Translate(0.0, delta);
                    }
                }
                graph.SyncClusters();
                graph.SyncSequences();
                candidateEdgeLength.set(placementcost::EdgeLength(&ctx, &graph, &noAbductionLength(false))?);

                // Before we commit to anything, we try to "recover symmetry" by running gap reduction on the opposite side
                // There are 3 scenarios:
                // 1. The gap reduction without backwards run is best
                // 2. The gap reduction plus backwards run is best
                // 3. No gap reduction is best
                // There is a fourth, which is that gap reduction backwards is best, but that's checked in outer
                if attemptRecoverSymmetry {
                    if backwardTxn.borrow().is_none() {
                        let cloned = txnInner.CloneGeometryContext()?;
                        *backwardTxn.borrow_mut() = Some(cloned);
                    }
                    let bt = backwardTxn.borrow().clone().unwrap();
                    bt.Clear();
                    bt.UpdateState()?;
                    let mirroredTxn = bt.CloneGeometryContext()?;
                    {
                        let ctx = ctx.clone();
                        let node = node.clone();
                        let options = options.clone();
                        let candidateEdgeLength = candidateEdgeLength.clone();
                        bt.AddOp(move || -> GoResult<()> {
                            let (moved, mirroredEdgeLength) = reduceGapToNeighbors(
                                &ctx,
                                &node,
                                Some(mirroredTxn.clone()),
                                gapReductionOptions {
                                    axis: options.axis,
                                    direction: options.direction.opposite(),
                                    attemptRecoverSymmetry: false,
                                    costTxn: options.costTxn.clone(),
                                },
                            )?;

                            if moved && (mirroredEdgeLength < candidateEdgeLength.get() && mirroredEdgeLength < oldEdgeLength) {
                                return Ok(());
                            }
                            Err(layoutgraph::ErrNonImprovingCandidate.err())
                        });
                    }
                    match bt.Commit(&ctx) {
                        Err(err) => {
                            if !layoutgraph::IsCandidateRejection(Some(&err)) {
                                return Err(err);
                            }
                        }
                        Ok(()) => {
                            // If the backward commit was successful, it means it was optimal
                            return Ok(());
                        }
                    }
                }

                if candidateEdgeLength.get() >= oldEdgeLength {
                    return Err(layoutgraph::ErrNonImprovingCandidate.err());
                }

                Ok(())
            });
        }
        match t.Commit(ctx) {
            Err(err) => {
                t.Clear();
                // Continue searching after an invalid candidate. A non-improving
                // candidate cannot get better by searching farther forward.
                if err.Is(&layoutgraph::ErrNonImprovingCandidate) {
                    break;
                }
                if !err.Is(&layoutgraph::ErrInvalidCandidate) {
                    return Err(err);
                }
            }
            Ok(()) => {
                newEdgeLength = candidateEdgeLength.get();
                t.Clear();
                t.UpdateState()?;
                break;
            }
        }
    }
    // Break the backward transaction's op references (no Go counterpart; Rc hygiene).
    if let Some(bt) = backwardTxn.borrow().as_ref() {
        bt.Clear();
    }

    // try moving non-fixed node to the side of its container

    let nodeContainer = node.containerField();
    if nodeContainer.is_some() && nodeContainer != nearestAhead.containerField() && node.b().FixedTopLeft.is_none() {
        while node.OwningContainer() != nearestAhead.OwningContainer() {
            // Go reads node.Graph afresh for the (possibly reassigned) node.
            let graph = node.g();
            let container = node.containerField().expect("nil pointer dereference: node.Container");
            let innerBox = container.InnerBox();
            let ib = *innerBox.TL().b();

            let gapSize;
            if isHorizontal {
                if forwards {
                    gapSize = ib.X + innerBox.Width - (node.x() + node.w());
                } else {
                    gapSize = node.x() - ib.X;
                }
            } else if forwards {
                gapSize = ib.Y + innerBox.Height - (node.y() + node.h());
            } else {
                gapSize = node.y() - ib.Y;
            }

            let padding = graph.ContainerPadding(Some(&container), true);
            let mut delta = gapSize;
            if isHorizontal {
                if forwards {
                    delta -= padding.Right();
                } else {
                    delta -= padding.Left();
                }
            } else if forwards {
                delta -= padding.Bottom();
            } else {
                delta -= padding.Top();
            }
            if !forwards {
                delta = -delta;
            }
            if gapSize > largeGapThreshold * graph.b().CellSize && delta != 0.0 {
                match &txn {
                    None => {
                        txn = Some(graph.NewRequestTransaction(ctx, layoutgraph::TransactionOptions { AffectContainers: true, ..Default::default() })?);
                    }
                    Some(t) => {
                        t.Clear();
                        t.UpdateState()?;
                    }
                }
                let t = txn.clone().unwrap();
                {
                    let node = node.clone();
                    t.AddOp(move || {
                        if isHorizontal {
                            node.MoveWithChildren(delta, 0.0);
                        } else {
                            node.MoveWithChildren(0.0, delta);
                        }
                        Ok(())
                    });
                }
                let mut rolledBack = false;
                match t.Commit(ctx) {
                    Err(err) => {
                        if !layoutgraph::IsCandidateRejection(Some(&err)) {
                            return Err(err);
                        }
                        rolledBack = true;
                    }
                    Ok(()) => {
                        let movedEdgeLength = match placementcost::EdgeLength(ctx, &graph, &noAbductionLength(false)) {
                            Ok(v) => v,
                            Err(err) => {
                                t.Rollback();
                                return Err(err);
                            }
                        };

                        if geo::PrecisionCompare(movedEdgeLength, newEdgeLength, geo::PRECISION) >= 0 {
                            t.Rollback();
                            rolledBack = true;
                        } else {
                            newEdgeLength = movedEdgeLength;
                        }
                    }
                }
                if rolledBack {
                    t.Clear();
                    // If moving to the side of the container obstructs something, move PADDING away
                    // from the next thing in that axis instead.
                    let mut leastDistance = math::Inf(1);
                    let siblings = graph.containers().at(&node.OwningContainer());
                    for otherNode in &siblings {
                        if &node == otherNode {
                            continue;
                        }
                        if isHorizontal {
                            if forwards {
                                let x = otherNode.x() - placementcost::IdealGapSize;
                                if x > node.x() + node.w() {
                                    if x - (node.x() + node.w()) < leastDistance {
                                        leastDistance = x - (node.x() + node.w());
                                    }
                                }
                            } else {
                                let x = otherNode.x() + otherNode.w() + placementcost::IdealGapSize;
                                if x < node.x() {
                                    if node.x() - x < leastDistance {
                                        leastDistance = node.x() - x;
                                    }
                                }
                            }
                        } else if forwards {
                            let y = otherNode.y() - placementcost::IdealGapSize;
                            if y > node.y() + node.h() {
                                if y - (node.y() + node.h()) < leastDistance {
                                    leastDistance = y - (node.y() + node.h());
                                }
                            }
                        } else {
                            let y = otherNode.y() + otherNode.h() + placementcost::IdealGapSize;
                            if y < node.y() {
                                if node.y() - y < leastDistance {
                                    leastDistance = node.y() - y;
                                }
                            }
                        }
                    }
                    if !math::IsInf(leastDistance, 1) {
                        delta = leastDistance;
                        if !forwards {
                            delta = -delta;
                        }
                        {
                            let node = node.clone();
                            t.AddOp(move || {
                                if isHorizontal {
                                    node.MoveWithChildren(delta, 0.0);
                                } else {
                                    node.MoveWithChildren(0.0, delta);
                                }
                                Ok(())
                            });
                        }
                        match t.Commit(ctx) {
                            Err(err) => {
                                if !layoutgraph::IsCandidateRejection(Some(&err)) {
                                    return Err(err);
                                }
                            }
                            Ok(()) => {
                                let movedEdgeLength = match placementcost::EdgeLength(ctx, &graph, &noAbductionLength(false)) {
                                    Ok(v) => v,
                                    Err(err) => {
                                        t.Rollback();
                                        return Err(err);
                                    }
                                };
                                if geo::PrecisionCompare(movedEdgeLength, newEdgeLength, geo::PRECISION) >= 0 {
                                    t.Rollback();
                                } else {
                                    newEdgeLength = movedEdgeLength;
                                    t.Clear();
                                    t.UpdateState()?;
                                }
                            }
                        }
                    }
                }
            }
            node = node.OwningContainer().expect("nil pointer dereference: node.OwningContainer()");
            if node.containerField().is_none() {
                break;
            }
        }
    }
    Ok((newEdgeLength != oldEdgeLength, newEdgeLength))
}
