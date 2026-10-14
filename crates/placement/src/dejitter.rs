// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/placement/dejitter.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.
//
// Port of placement/dejitter.go
#[allow(unused_imports)]
use crate::*;
use gocompat::{Ctx, GoResult, Map};
use layoutgraph::{EdgeRef, GraphRef, NodeRef};

fn pointsOf(e: &EdgeRef) -> Vec<geo::PointRef> {
    e.points()
}

fn pv(p: &geo::PointRef) -> geo::Point {
    *p.b()
}

// dejitter performs micro-movements on nodes to remove the jittery connections that can be straightened out with slight adjustments in one direction
// E.g. jittery line:
// .             +--------->
// . +-----------+
pub fn Dejitter(ctx: &Ctx, g: &GraphRef) -> GoResult<bool> {
    let (ctx, guard) = layoutgraph::EnsureTransactionWorkGuard(ctx, "DejitterTransactions")?;
    let ctx = &ctx;
    let mut dejittered = false;
    let txn = g.NewRequestTransaction(ctx, layoutgraph::TransactionOptions { AffectContainers: true, ..Default::default() })?;
    if g.b().Nodes.is_empty() {
        return Ok(false);
    }
    // Go: rollback = &layoutgraph.Transaction{Graph: g, PriorGraphState: state}
    let mut rollback: Option<layoutgraph::GraphStateRef> = None;
    let result = (|| -> GoResult<bool> {
        for node in g.nodes() {
            {
                let nb = node.b();
                if nb.Cluster.is_some() {
                    continue;
                }
            }
            // Too much trouble to have to jitter all children too
            if node.IsContainer() {
                continue;
            }
            if g.nodeToTree().has(&node) {
                continue;
            }
            {
                let nb = node.b();
                if nb.Sequence.is_some() {
                    continue;
                }
                if nb.FixedTopLeft.is_some() {
                    continue;
                }
            }

            let prevSymmetry = placementcost::NodeSymmetry(ctx, &node, None)?;
            for edge in node.edges() {
                // No jitter possible with less than 3 segments
                if edge.NumSegments() < 3 {
                    continue;
                }
                if node.Adjacent(&edge).b().Cluster.is_some() {
                    continue;
                }

                let points = pointsOf(&edge);
                let mut pointBeforeBends: Option<geo::PointRef> = None;
                let mut pointOnNode: Option<geo::PointRef> = None;
                let mut lastTwoBends: Vec<geo::PointRef> = Vec::new();
                // Skip the first of each since it's the port
                let (eFrom, eTo) = (edge.from(), edge.to());
                if eFrom == node {
                    lastTwoBends.push(points[1].clone());
                    lastTwoBends.push(points[2].clone());
                    pointBeforeBends = Some(points[3].clone());
                    pointOnNode = Some(points[0].clone());
                } else if eTo == node {
                    let l = points.len();
                    lastTwoBends.push(points[l - 2].clone());
                    lastTwoBends.push(points[l - 3].clone());
                    pointBeforeBends = Some(points[l - 4].clone());
                    pointOnNode = Some(points[l - 1].clone());
                }

                let (b0, b1) = (pv(&lastTwoBends[0]), pv(&lastTwoBends[1]));
                // Only a jitter if the last two bends fall within threshold
                if geo::EuclideanDistance(b0.X, b0.Y, b1.X, b1.Y) > jitterThreshold {
                    continue;
                }

                // A vertical line moves horizontally to the second bend's X to straighten it.
                let isVerticalLine = b0.Y == b1.Y;
                let pbb = pv(pointBeforeBends.as_ref().expect("nil pointer dereference: pointBeforeBends"));
                let pon = pv(pointOnNode.as_ref().expect("nil pointer dereference: pointOnNode"));

                // TODO: support correcting U-turns.
                // Leave them unchanged here because they are not straightforward to collapse safely.
                // A U-turn has both the point before the bends and the point on the node on one side of the bends.
                if isVerticalLine {
                    if (pbb.Y > b0.Y) && (pon.Y > b0.Y) {
                        continue;
                    }
                    if (pbb.Y < b0.Y) && (pon.Y < b0.Y) {
                        continue;
                    }
                } else {
                    if (pbb.X > b0.X) && (pon.X > b0.X) {
                        continue;
                    }
                    if (pbb.X < b0.X) && (pon.X < b0.X) {
                        continue;
                    }
                }

                if isVerticalLine {
                    if b1.X == b0.X {
                        continue;
                    }
                } else if b1.Y == b0.Y {
                    continue;
                }

                // ---- from here on we know it is a jitter line

                // Look for other connected edges which are straight lines
                let mut hasAnotherConnectedStraightEdge = false;
                for otherEdge in node.edges() {
                    if otherEdge == edge {
                        continue;
                    }
                    if otherEdge.NumSegments() > 1 {
                        continue;
                    }
                    let op = pointsOf(&otherEdge);
                    let l = op.len();
                    // If the jittering one is vertical, we only look for other vertical ones
                    if otherEdge.from() == node {
                        let (p0, p1) = (pv(&op[0]), pv(&op[1]));
                        if isVerticalLine && (p0.X == p1.X) {
                            hasAnotherConnectedStraightEdge = true;
                            break;
                        }
                        if !isVerticalLine && (p0.Y == p1.Y) {
                            hasAnotherConnectedStraightEdge = true;
                            break;
                        }
                    } else {
                        let (pl1, pl2) = (pv(&op[l - 1]), pv(&op[l - 2]));
                        if isVerticalLine && (pl1.X == pl2.X) {
                            hasAnotherConnectedStraightEdge = true;
                            break;
                        }
                        if !isVerticalLine && (pl1.Y == pl2.Y) {
                            hasAnotherConnectedStraightEdge = true;
                            break;
                        }
                    }
                }
                // Don't want to move other nodes
                if hasAnotherConnectedStraightEdge {
                    continue;
                }

                // ---- from here on we know there is no adjacent node connected via a tangent straight line

                let delta = if isVerticalLine { (b1.X - b0.X).round() } else { (b1.Y - b0.Y).round() };

                // Sometimes it'll get real close to a sign flip but not yet. We want to prevent these super short edges, so we'll extend delta a little to consider those a sign flip
                let signFlipDelta = if delta < 0.0 { delta - signFlipPadding } else { delta + signFlipPadding };

                // A sign flip is when there's movement on the endpoint but not the bend, and the new endpoint ends up
                // on the other axis of the bend (potentially going into the node)
                let mut signFlip = false;
                // Map of the original edge to its closest two segments
                let mut newSegments: Map<EdgeRef, Vec<geo::PointRef>> = Map::default();
                // Insertion order of newSegments (only an any-match is computed from it).
                let mut newSegmentOrder: Vec<EdgeRef> = Vec::new();
                let mut setSegment = |e: &EdgeRef, seg: Vec<geo::PointRef>, newSegments: &mut Map<EdgeRef, Vec<geo::PointRef>>| {
                    if newSegments.insert(e.clone(), seg).is_none() {
                        newSegmentOrder.push(e.clone());
                    }
                };

                // Note that the edge itself is included in this search when it's opposite direction, as it needs to be checked for new intersections as well
                for otherEdge in node.edges() {
                    let op = pointsOf(&otherEdge);
                    let l = op.len();
                    let fromNode = otherEdge.from() == node;
                    let isOtherEdgeVertical = if fromNode {
                        pv(&op[0]).X == pv(&op[1]).X
                    } else {
                        pv(&op[l - 1]).X == pv(&op[l - 2]).X
                    };

                    // This can happen if they are really close to each other
                    // Diagonal points should be free to move around for the purpose of dejitter
                    let isOtherEdgeDiagonal = (l == 2) && (pv(&op[0]).X != pv(&op[1]).X) && (pv(&op[0]).Y != pv(&op[1]).Y);
                    // The distance will be so small we don't have to worry about new intersections, and sign flips can't happen
                    if isOtherEdgeDiagonal {
                        continue;
                    }

                    // Moving horizontally and edge vertical (move both x distance)
                    if isVerticalLine && isOtherEdgeVertical {
                        let (second, third);
                        if fromNode {
                            second = 1;
                            third = 2;
                            let (p0, p1) = (pv(&op[0]), pv(&op[1]));
                            setSegment(&otherEdge, vec![geo::NewPoint(p0.X + delta, p0.Y), geo::NewPoint(p1.X + delta, p1.Y)], &mut newSegments);
                        } else {
                            second = l - 2;
                            third = l - 3;
                            let (pa, pb) = (pv(&op[l - 1]), pv(&op[l - 2]));
                            setSegment(&otherEdge, vec![geo::NewPoint(pa.X + delta, pa.Y), geo::NewPoint(pb.X + delta, pb.Y)], &mut newSegments);
                        }
                        if otherEdge != edge {
                            let (ps, pt) = (pv(&op[second]), pv(&op[third]));
                            if ((ps.X + signFlipDelta < pt.X) && (ps.X > pt.X)) || ((ps.X + signFlipDelta > pt.X) && (ps.X < pt.X)) {
                                signFlip = true;
                            }
                        }
                    }
                    // Moving horizontally and edge horizontal (move closest x distance)
                    if isVerticalLine && !isOtherEdgeVertical {
                        let (first, second);
                        if fromNode {
                            first = 0;
                            second = 1;
                            let p0 = pv(&op[0]);
                            setSegment(&otherEdge, vec![geo::NewPoint(p0.X + delta, p0.Y), op[1].clone()], &mut newSegments);
                        } else {
                            first = l - 1;
                            second = l - 2;
                            let pa = pv(&op[l - 1]);
                            setSegment(&otherEdge, vec![geo::NewPoint(pa.X + delta, pa.Y), op[l - 2].clone()], &mut newSegments);
                        }
                        let (pf, ps) = (pv(&op[first]), pv(&op[second]));
                        if ((pf.X + signFlipDelta < ps.X) && (pf.X > ps.X)) || ((pf.X + signFlipDelta > ps.X) && (pf.X < ps.X)) {
                            signFlip = true;
                        }
                    }
                    // Moving vertical and edge horizontal (move both y distance)
                    if !isVerticalLine && !isOtherEdgeVertical {
                        let (second, third);
                        if fromNode {
                            second = 1;
                            third = 2;
                            let (p0, p1) = (pv(&op[0]), pv(&op[1]));
                            setSegment(&otherEdge, vec![geo::NewPoint(p0.X, p0.Y + delta), geo::NewPoint(p1.X, p1.Y + delta)], &mut newSegments);
                        } else {
                            second = l - 2;
                            third = l - 3;
                            let (pa, pb) = (pv(&op[l - 1]), pv(&op[l - 2]));
                            setSegment(&otherEdge, vec![geo::NewPoint(pa.X, pa.Y + delta), geo::NewPoint(pb.X, pb.Y + delta)], &mut newSegments);
                        }
                        if otherEdge != edge {
                            let (ps, pt) = (pv(&op[second]), pv(&op[third]));
                            if ((ps.Y + signFlipDelta < pt.Y) && (ps.Y > pt.Y)) || ((ps.Y + signFlipDelta > pt.Y) && (ps.Y < pt.Y)) {
                                signFlip = true;
                            }
                        }
                    }
                    // Moving vertical and edge vertical (move closest y distance)
                    if !isVerticalLine && isOtherEdgeVertical {
                        let (first, second);
                        if fromNode {
                            first = 0;
                            second = 1;
                            let p0 = pv(&op[0]);
                            setSegment(&otherEdge, vec![geo::NewPoint(p0.X, p0.Y + delta), op[1].clone()], &mut newSegments);
                        } else {
                            first = l - 1;
                            second = l - 2;
                            let pa = pv(&op[l - 1]);
                            setSegment(&otherEdge, vec![geo::NewPoint(pa.X, pa.Y + delta), op[l - 2].clone()], &mut newSegments);
                        }
                        let (pf, ps) = (pv(&op[first]), pv(&op[second]));
                        if ((pf.Y + signFlipDelta < ps.Y) && (pf.Y > ps.Y)) || ((pf.Y + signFlipDelta > ps.Y) && (pf.Y < ps.Y)) {
                            signFlip = true;
                        }
                    }

                    if signFlip {
                        break;
                    }
                }

                if signFlip {
                    continue;
                }

                // Go ranges over the newSegments map; only whether any segment
                // intersects matters, so the visiting order is irrelevant.
                let mut intersects = false;
                for segmentEdge in &newSegmentOrder {
                    let newSegment = &newSegments[segmentEdge];
                    let (sFrom, sTo) = (segmentEdge.from(), segmentEdge.to());
                    // Check for intersections on other nodes
                    for otherNode in g.nodes() {
                        if otherNode == node {
                            continue;
                        }
                        // Can intersect with a container if one endpoint is inside it
                        if sFrom.IsDescendantOf(Some(&otherNode)) {
                            continue;
                        }
                        if sTo.IsDescendantOf(Some(&otherNode)) {
                            continue;
                        }
                        // It can touch the node it's connected to
                        if (otherNode != sTo && otherNode != sFrom) && otherNode.PassesThrough(Some(&newSegment[0]), Some(&newSegment[1])) {
                            intersects = true;
                            break;
                        }
                    }
                    if intersects {
                        break;
                    }
                }
                if intersects {
                    continue;
                }

                // ---- from here on we know there is no object intersection introduced by updating all the edges to avoid the jitter, so we can safely shift to dejitter

                let countPassedThrough = |node: &NodeRef| -> i64 {
                    let mut count = 0;
                    for otherEdge in g.edges() {
                        if (otherEdge.from() == *node) || (otherEdge.to() == *node) {
                            continue;
                        }
                        let op = pointsOf(&otherEdge);
                        if op.is_empty() {
                            continue;
                        }
                        for i in 0..op.len() - 1 {
                            if node.PassesThrough(Some(&op[i]), Some(&op[i + 1])) {
                                count += 1;
                            }
                        }
                    }
                    count
                };
                let numEdgesPassedThrough = countPassedThrough(&node);

                if rollback.is_none() {
                    let state = layoutgraph::NewGraphStateSnapshot(layoutgraph::GraphStateSnapshotOptions {
                        CaptureEdgeRoutes: true,
                        ..Default::default()
                    });
                    state.UpdateWithWorkGuard(g, &guard)?;
                    rollback = Some(state);
                }

                {
                    let ctx = ctx.clone();
                    let g = g.clone();
                    let node = node.clone();
                    txn.AddOp(move || -> GoResult<()> {
                        // Tentatively commit the node
                        {
                            let tl = node.tl().expect("nil pointer dereference: TopLeft");
                            let mut p = tl.bm();
                            if isVerticalLine {
                                p.X += delta;
                            } else {
                                p.Y += delta;
                            }
                        }

                        let mut newNumEdgesPassedThrough = 0;
                        for otherEdge in g.edges() {
                            if (otherEdge.from() == node) || (otherEdge.to() == node) {
                                continue;
                            }
                            let op = pointsOf(&otherEdge);
                            if op.is_empty() {
                                continue;
                            }
                            for i in 0..op.len() - 1 {
                                if node.PassesThrough(Some(&op[i]), Some(&op[i + 1])) {
                                    newNumEdgesPassedThrough += 1;
                                }
                            }
                        }

                        let currSymmetry = placementcost::NodeSymmetry(&ctx, &node, None)?;
                        let container = node.containerField();
                        if newNumEdgesPassedThrough > numEdgesPassedThrough {
                            return Err(layoutgraph::ErrNonImprovingCandidate.err());
                        } else if container.is_some() && node.SpillsOutOf(container.as_ref().unwrap()) {
                            return Err(layoutgraph::ErrInvalidCandidate.err());
                        } else if currSymmetry < prevSymmetry {
                            return Err(layoutgraph::ErrNonImprovingCandidate.err());
                        }
                        Ok(())
                    });
                }

                if let Err(err) = txn.Commit(ctx) {
                    txn.Clear();
                    if layoutgraph::IsCandidateRejection(Some(&err)) {
                        continue;
                    }
                    return Err(err);
                }
                txn.UpdateState()?;
                txn.Clear();

                // ---- from here on we know jittering this node does not introduce any more route intersections

                // Commit new segments
                for otherEdge in node.edges() {
                    let op = pointsOf(&otherEdge);
                    let l = op.len();
                    let fromNode = otherEdge.from() == node;
                    let isOtherEdgeVertical = if fromNode {
                        pv(&op[0]).X == pv(&op[1]).X
                    } else {
                        pv(&op[l - 1]).X == pv(&op[l - 2]).X
                    };

                    let isOtherEdgeDiagonal = (l == 2) && (pv(&op[0]).X != pv(&op[1]).X) && (pv(&op[0]).Y != pv(&op[1]).Y);
                    // If it's diagonal, we just need to update the one point on the node that's moved
                    if isOtherEdgeDiagonal {
                        if isVerticalLine {
                            if fromNode {
                                op[0].bm().X += delta;
                            } else {
                                op[l - 1].bm().X += delta;
                            }
                        } else if fromNode {
                            op[0].bm().Y += delta;
                        } else {
                            op[l - 1].bm().Y += delta;
                        }
                    } else {
                        // Moving horizontally and edge vertical (move both x distance)
                        if isVerticalLine && isOtherEdgeVertical {
                            if fromNode {
                                op[0].bm().X += delta;
                                op[1].bm().X += delta;
                            } else {
                                op[l - 1].bm().X += delta;
                                op[l - 2].bm().X += delta;
                            }
                        }
                        // Moving horizontally and edge horizontal (move closest x distance)
                        if isVerticalLine && !isOtherEdgeVertical {
                            if fromNode {
                                op[0].bm().X += delta;
                            } else {
                                op[l - 1].bm().X += delta;
                            }
                        }
                        // Moving vertical and edge horizontal (move both y distance)
                        if !isVerticalLine && !isOtherEdgeVertical {
                            if fromNode {
                                op[0].bm().Y += delta;
                                op[1].bm().Y += delta;
                            } else {
                                op[l - 1].bm().Y += delta;
                                op[l - 2].bm().Y += delta;
                            }
                        }
                        // Moving vertical and edge vertical (move closest y distance)
                        if !isVerticalLine && isOtherEdgeVertical {
                            if fromNode {
                                op[0].bm().Y += delta;
                            } else {
                                op[l - 1].bm().Y += delta;
                            }
                        }
                    }
                }

                // Remove the two defunct points corresponding to the old bends
                {
                    let mut eb = edge.bm();
                    if eb.From == node {
                        // This should remove 1 and 2
                        eb.Points.drain(1..3);
                    } else {
                        let l = eb.Points.len();
                        eb.Points.drain(l - 3..l - 1);
                    }
                }
                dejittered = true;
            }
        }
        if dejittered {
            guard.Finish()?;
        }
        Ok(dejittered)
    })();
    if result.is_err() {
        if let Some(state) = &rollback {
            layoutgraph::RestoreGraphState(g, state);
        }
    }
    result
}
