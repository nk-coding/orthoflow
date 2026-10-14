// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/placement/alignment.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.
//
// Port of placement/alignment.go
#[allow(unused_imports)]
use crate::*;
use gocompat::math;
use gocompat::{Ctx, GoResult};
use layoutgraph::{EdgeAbductionRef, EdgeRef, GraphRef, NodeRef, TransactionRef};
use std::cell::Cell;
use std::rc::Rc;

fn alignLengthOptions(edgeAbductions: &Option<Vec<EdgeAbductionRef>>) -> placementcost::EdgeLengthOptions {
    placementcost::EdgeLengthOptions {
        EdgeAbductions: edgeAbductions.clone(),
        IncludeNodeSizes: true,
        EnforceMinimumGap: false,
        PenalizeDirection: true,
    }
}

// alignAxes tries to align connected nodes that currently differ on both axes.
// If b has more connections, consider its subgraph.
pub(crate) fn alignAxes(ctx: &Ctx, g: &GraphRef, txn: &TransactionRef) -> GoResult<bool> {
    if let Some(err) = ctx.Err() {
        return Err(err);
    }
    let changed = Rc::new(Cell::new(false));

    let fixedNodes = g.FixedNodes();

    // Note: the edge from rootSentinel to tree root is not excluded here since if the root moves,
    // all connected nodes are moved so it should move the whole tree with it which would be okay
    let mut isTreeEdge = g.TreeEdgeMap();
    g.AddIsolatedTreeEdges(&mut isTreeEdge);
    // Tree nodes may be connected nodes when they are containers, so exclude them from the connected-node search.
    let nodeToTree = g.nodeToTree();
    let mut excluded: Vec<NodeRef> = Vec::with_capacity(nodeToTree.len() + fixedNodes.len() + 1);
    for n in g.nodes() {
        if nodeToTree.has(&n) {
            excluded.push(n);
        }
    }
    excluded.extend(fixedNodes.iter().cloned());
    // last node is replaced with e.To/e.From below
    let lastIndex = excluded.len();

    // Go appends each sequence's abductions while ranging over the Sequences
    // map; appending only empty slices leaves the slice nil.
    let mut abductions: Vec<EdgeAbductionRef> = Vec::new();
    for seq in g.sequences().values() {
        abductions.extend(seq.b().EdgeAbductions.iter().cloned());
    }
    let edgeAbductions: Option<Vec<EdgeAbductionRef>> = if abductions.is_empty() { None } else { Some(abductions) };

    // need to compute it here because as we align a single edge, it may end up aligning other edges
    // and it would affect the actual count on the fly
    let mut hasTableColumns = false;
    let initialLength = placementcost::EdgeLength(ctx, g, &alignLengthOptions(&edgeAbductions))?;
    let mut nInitialAlignedTableColumns = 0;
    for e in g.edges() {
        if e.HasTableColumn() {
            hasTableColumns = true;
            if e.IsAxisAligned() {
                nInitialAlignedTableColumns += 1;
            }
        }
    }

    for e in g.edges() {
        if isTreeEdge.contains_key(&e) {
            continue;
        }
        if e.IsAxisAligned() {
            continue;
        }
        let (from, to) = (e.from(), e.to());
        if from.b().Hierarchy.is_some() || to.b().Hierarchy.is_some() {
            continue;
        }
        txn.Clear();
        txn.UpdateState()?;

        let (xDiff, yDiff) = alignmentDeltas(&e, g);

        let mut dxBest = 0.0;
        let mut dyBest = 0.0;
        let mut bestNodes: Vec<NodeRef> = Vec::new();
        let mut bestEdgeLength = placementcost::EdgeLength(ctx, g, &alignLengthOptions(&edgeAbductions))?;
        let containerAlignment = placementcost::ContainerAlignmentCost(ctx, g)?;
        bestEdgeLength += containerAlignment;
        // Try to move e.From
        {
            excluded.truncate(lastIndex);
            excluded.push(to.clone());
            let connectedNodes = from.ConnectedNodeSet(&excluded, g);
            let (edgeLength, dx, dy) = tryMove(ctx, txn, g, &e, &connectedNodes, &edgeAbductions, xDiff, yDiff)?;
            if geo::PrecisionCompare(edgeLength, bestEdgeLength, geo::PRECISION) < 0 {
                bestEdgeLength = edgeLength;
                dxBest = dx;
                dyBest = dy;
                bestNodes = connectedNodes;
            } else if e.HasTableColumn() && geo::PrecisionCompare(edgeLength, bestEdgeLength, geo::PRECISION) == 0 {
                // aligning ports in tables might be a sort of micromovement that doesn't change that much and it results in
                // the new length being equal to the old one at our precision level, but still, the aligned ports look better
                bestEdgeLength = edgeLength;
                dxBest = dx;
                dyBest = dy;
                bestNodes = connectedNodes;
            }
        }

        // Try to move e.To
        {
            excluded.truncate(lastIndex);
            excluded.push(from.clone());
            let connectedNodes = to.ConnectedNodeSet(&excluded, g);
            let (edgeLength, dx, dy) = tryMove(ctx, txn, g, &e, &connectedNodes, &edgeAbductions, -xDiff, -yDiff)?;
            if geo::PrecisionCompare(edgeLength, bestEdgeLength, geo::PRECISION) < 0 {
                dxBest = dx;
                dyBest = dy;
                bestNodes = connectedNodes;
            } else if e.HasTableColumn() && geo::PrecisionCompare(edgeLength, bestEdgeLength, geo::PRECISION) == 0 {
                dxBest = dx;
                dyBest = dy;
                bestNodes = connectedNodes;
            }
        }

        if !bestNodes.is_empty() {
            {
                let changed = changed.clone();
                txn.AddOp(move || {
                    changed.set(true);
                    for n in &bestNodes {
                        n.Translate(dxBest, dyBest);
                    }
                    Ok(())
                });
            }
            txn.Commit(ctx)?;
        }
    }

    let mut changedV = changed.get();
    if hasTableColumns && changedV {
        // column alignment can cause small changes in edge length that that "stay the same"
        // at our precision comparison. To handle it properly, we also count the number of
        // aligned edges
        let mut nAlignedTableColumns = 0;
        for e in g.edges() {
            if e.HasTableColumn() && e.IsAxisAligned() {
                nAlignedTableColumns += 1;
            }
        }
        // Without more aligned columns, accept only a strict edge-length improvement.
        if nAlignedTableColumns <= nInitialAlignedTableColumns {
            let mut edgeLength = placementcost::EdgeLength(ctx, g, &alignLengthOptions(&edgeAbductions))?;
            let containerAlignment = placementcost::ContainerAlignmentCost(ctx, g)?;
            edgeLength += containerAlignment;
            changedV = geo::PrecisionCompare(edgeLength, initialLength, geo::PRECISION) == -1;
        }
    }

    Ok(changedV)
}

// alignmentDeltas computes the X and Y deltas required to center-align the connected nodes.
// Exceptions:
// - for edges between table columns, return the delta to align the columns
// - for sequences, return the delta to align with the sequence step
pub(crate) fn alignmentDeltas(e: &EdgeRef, g: &GraphRef) -> (f64, f64) {
    let (eFrom, eTo) = (e.from(), e.to());
    if e.HasTableColumn() {
        let (fromPort, toPort, hasFromPort, hasToPort, _) = e.FacingTablePortValues(None, None);
        if hasFromPort && hasToPort {
            return (0.0, toPort.Y - fromPort.Y);
        } else if hasFromPort {
            return (0.0, (eTo.y() + eTo.h() / 2.0) - fromPort.Y);
        } else if hasToPort {
            return (0.0, toPort.Y - (eFrom.y() + eFrom.h() / 2.0));
        } else {
            // Align tables to the left to avoid issues that can happen if we center align the tables
            return (eTo.x() - eFrom.x(), 0.0);
        }
    }
    let sequences = g.sequences();
    let mut from = eFrom.clone();
    if let Some(seq) = sequences.get(&from) {
        from = seq.AbductedNodeByEdge(e).expect("nil pointer dereference: abducted node");
    }
    let mut to = eTo.clone();
    if let Some(seq) = sequences.get(&to) {
        to = seq.AbductedNodeByEdge(e).expect("nil pointer dereference: abducted node");
    }

    let mut yDiff = ((to.y() + to.h() / 2.0) - (from.y() + from.h() / 2.0)).round();
    let mut xDiff = ((to.x() + to.w() / 2.0) - (from.x() + from.w() / 2.0)).round();

    let o = eFrom.Orientation(&eTo);
    if o.IsHorizontal() {
        xDiff = 0.0;
    } else if o.IsVertical() {
        yDiff = 0.0;
    } else {
        let (xDiff2, yDiff2) = eFrom.OrthogonalDistanceTo(&eTo);
        if xDiff2 < layoutgraph::ConnectedNodeGap as f64 {
            yDiff = 0.0;
        }
        if yDiff2 < layoutgraph::ConnectedNodeGap as f64 {
            xDiff = 0.0;
        }
    }
    (xDiff, yDiff)
}

pub(crate) fn tryMove(
    ctx: &Ctx,
    txn: &TransactionRef,
    g: &GraphRef,
    e: &EdgeRef,
    connectedNodes: &[NodeRef],
    edgeAbductions: &Option<Vec<EdgeAbductionRef>>,
    xDiff: f64,
    yDiff: f64,
) -> GoResult<(f64, f64, f64)> {
    let mut bestEdgeLength = math::Inf(1);
    let mut bestAttempt = geo::Point::default();
    let mut attempts: Vec<geo::Point> = Vec::with_capacity(2);

    if geo::PrecisionCompare(yDiff, 0.0, geo::PRECISION) != 0 {
        // e.From tries to slide up or down to e.To
        attempts.push(geo::Point { X: 0.0, Y: yDiff });
    }

    if geo::PrecisionCompare(xDiff, 0.0, geo::PRECISION) != 0 {
        // e.From tries to slide left or right to e.To
        attempts.push(geo::Point { X: xDiff, Y: 0.0 });
    }

    for attempt in attempts {
        {
            let g = g.clone();
            let e = e.clone();
            let connectedNodes = connectedNodes.to_vec();
            txn.AddOp(move || {
                if !attemptShift(&g, &e, &connectedNodes, attempt.X, attempt.Y) {
                    return Err(layoutgraph::ErrInvalidCandidate.err());
                }
                Ok(())
            });
        }

        match txn.Commit(ctx) {
            Ok(()) => {
                let mut edgeLength = match placementcost::EdgeLength(ctx, g, &alignLengthOptions(edgeAbductions)) {
                    Ok(v) => v,
                    Err(scoreErr) => {
                        txn.Rollback();
                        txn.Clear();
                        return Err(scoreErr);
                    }
                };
                let containerAlignment = match placementcost::ContainerAlignmentCost(ctx, g) {
                    Ok(v) => v,
                    Err(scoreErr) => {
                        txn.Rollback();
                        txn.Clear();
                        return Err(scoreErr);
                    }
                };
                edgeLength += containerAlignment;
                if geo::PrecisionCompare(edgeLength, bestEdgeLength, geo::PRECISION) < 1 {
                    bestEdgeLength = edgeLength;
                    bestAttempt = attempt;
                }
            }
            Err(err) => {
                if !layoutgraph::IsCandidateRejection(Some(&err)) {
                    return Err(err);
                }
            }
        }
        txn.Clear();
        txn.Rollback();
    }

    Ok((bestEdgeLength, bestAttempt.X, bestAttempt.Y))
}

// Returns true if no obvious problems with the shift
pub(crate) fn attemptShift(g: &GraphRef, aligningEdge: &EdgeRef, nodes: &[NodeRef], x: f64, y: f64) -> bool {
    for n in nodes {
        n.Translate(x, y);
    }

    if !withinMaxSize(g) {
        return false;
    }

    // Shifting is only good if it results in a direct line. It'd be undesirable if, after shifting, there are nodes in between obscuring the alignment
    // This includes any connected nodes
    // Note that this function already excludes ancestor containers by default
    if intersectsOtherNode(g, &aligningEdge.from(), &aligningEdge.to()) {
        return false;
    }

    true
}
