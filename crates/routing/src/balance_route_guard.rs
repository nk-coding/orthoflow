// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/routing/balance_route_guard.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of routing/balance_route_guard.go.
#[allow(unused_imports)]
use crate::*;
use gocompat::math;
use gocompat::{GoResult, Set};
use layoutgraph::{EdgeRef, EdgeSegmentRef, NodeRef};

pub(crate) fn removeDuplicatePointsGuarded(points: &[geo::PointRef], guard: &routeWorkGuardRef) -> GoResult<Vec<geo::PointRef>> {
    if points.len() <= 1 {
        guard.step()?;
        return Ok(points.to_vec());
    }

    let mut result: Vec<geo::PointRef> = Vec::with_capacity(points.len());
    result.push(points[0].clone());
    for index in 1..points.len() {
        guard.step()?;
        let previous = points[index - 1].get();
        let current = points[index].get();
        if previous.X != current.X || previous.Y != current.Y {
            result.push(points[index].clone());
        }
    }

    if result.len() == 1 {
        let secondPoint = result[0].Copy();
        secondPoint.bm().X += 1.0;
        result.push(secondPoint);
    }
    Ok(result)
}

pub(crate) fn nodeSegmentsGuarded(nodes: &[NodeRef], isHorizontal: bool, guard: &routeWorkGuardRef) -> GoResult<Vec<geo::Segment>> {
    let mut out: Vec<geo::Segment> = Vec::with_capacity(nodes.len() * 2);
    for node in nodes {
        guard.step()?;
        let tl = node.tl().expect("nil pointer dereference");
        let t = tl.get();
        let (w, h) = (node.w(), node.h());
        if isHorizontal {
            out.push(geo::Segment { Start: tl.clone(), End: geo::NewPoint(t.X + w, t.Y) });
            out.push(geo::Segment { Start: geo::NewPoint(t.X, t.Y + h), End: geo::NewPoint(t.X + w, t.Y + h) });
        } else {
            out.push(geo::Segment { Start: tl.clone(), End: geo::NewPoint(t.X, t.Y + h) });
            out.push(geo::Segment { Start: geo::NewPoint(t.X + w, t.Y), End: geo::NewPoint(t.X + w, t.Y + h) });
        }
    }
    Ok(out)
}

pub(crate) fn edgeSegmentsGuarded(edges: &[EdgeRef], isHorizontal: bool, guard: &routeWorkGuardRef) -> GoResult<Vec<EdgeSegmentRef>> {
    let mut out: Vec<EdgeSegmentRef> = Vec::new();
    for edge in edges {
        guard.step()?;
        let points = edge.points();
        let mut index = 0usize;
        while index + 1 < points.len() {
            guard.step()?;
            let (a, b) = (points[index].get(), points[index + 1].get());
            let mut se: Option<(&geo::PointRef, &geo::PointRef)> = None;
            if isHorizontal {
                if a.Y == b.Y {
                    if a.X < b.X {
                        se = Some((&points[index], &points[index + 1]));
                    } else {
                        se = Some((&points[index + 1], &points[index]));
                    }
                }
            } else if a.X == b.X {
                if a.Y < b.Y {
                    se = Some((&points[index], &points[index + 1]));
                } else {
                    se = Some((&points[index + 1], &points[index]));
                }
            }
            if let Some((start, end)) = se {
                out.push(layoutgraph::NewEdgeSegment(start, end, edge));
            }
            index += 1;
        }
    }
    Ok(out)
}

// routeSegmentBounds is the cancellable equivalent of geo.Segment.GetBounds.
pub(crate) fn routeSegmentBounds(
    segment: &geo::Segment,
    segments: &[geo::Segment],
    buffer: f64,
    guard: &routeWorkGuardRef,
) -> GoResult<(f64, f64)> {
    let mut ceil = math::Inf(1);
    let mut floor = math::Inf(-1);
    let (ss, se) = (segment.Start.get(), segment.End.get());
    if ss.X == se.X && ss.Y == se.Y {
        guard.step()?;
        return Ok((floor, ceil));
    }
    let isHorizontal = ss.X == se.X;
    for otherSegment in segments {
        guard.step()?;
        let (os, oe) = (otherSegment.Start.get(), otherSegment.End.get());
        if isHorizontal {
            if oe.Y < ss.Y - buffer {
                continue;
            }
            if os.Y > se.Y + buffer {
                continue;
            }
            if os.X <= ss.X {
                floor = math::Max(floor, os.X);
            }
            if os.X > ss.X {
                ceil = math::Min(ceil, os.X);
            }
        } else {
            if oe.X < ss.X - buffer {
                continue;
            }
            if os.X > se.X + buffer {
                continue;
            }
            if os.Y <= ss.Y {
                floor = math::Max(floor, os.Y);
            }
            if os.Y > ss.Y {
                ceil = math::Min(ceil, os.Y);
            }
        }
    }
    Ok((floor, ceil))
}

pub(crate) fn evenlyDistributeGuarded(floor: f64, ceil: f64, count: usize, guard: &routeWorkGuardRef) -> GoResult<Vec<f64>> {
    if count == 0 || floor >= ceil {
        guard.step()?;
        return Ok(Vec::new());
    }
    let increment = ((ceil - floor) / (count + 1) as f64).floor();
    if increment <= 0.0 {
        guard.step()?;
        return Ok(Vec::new());
    }
    let mut out = Vec::with_capacity(count);
    for index in 1..=count {
        guard.step()?;
        out.push(floor + index as f64 * increment);
    }
    Ok(out)
}

pub(crate) type balanceOrderStatus = i64;

pub(crate) const balanceOrderPreserved: balanceOrderStatus = 0;
pub(crate) const balanceOrderReversed: balanceOrderStatus = 1;
pub(crate) const balanceOrderContactChanged: balanceOrderStatus = 2;

// checkBalanceOrder checks a batch against parallel routes that
// belong to another movement range.
pub(crate) fn checkBalanceOrder(
    batch: &[EdgeSegmentRef],
    batchSet: &Set<EdgeSegmentRef>,
    segments: &[EdgeSegmentRef],
    proposed: &[f64],
    isHorizontal: bool,
    guard: &routeWorkGuardRef,
) -> GoResult<balanceOrderStatus> {
    let mut moving = false;
    for (i, segment) in batch.iter().enumerate() {
        guard.step()?;
        let s = segment.b();
        let old = if isHorizontal { s.Segment.Start.X() } else { s.Segment.Start.Y() };
        if proposed[i] != old {
            moving = true;
            break;
        }
    }
    if !moving {
        return Ok(balanceOrderPreserved);
    }
    let mut status = balanceOrderPreserved;
    for other in segments {
        guard.step()?;
        if batchSet.contains(other) {
            continue;
        }
        let (oStart, oEnd, oOwner) = {
            let o = other.b();
            (o.Segment.Start.get(), o.Segment.End.get(), o.Owner())
        };
        for (i, segment) in batch.iter().enumerate() {
            guard.step()?;
            let (sStart, sEnd, sOwner) = {
                let s = segment.b();
                (s.Segment.Start.get(), s.Segment.End.get(), s.Owner())
            };
            // Segments of one route may meet while eliminating an elbow.
            if sOwner == oOwner {
                continue;
            }
            let (mut old, mut position) = (sStart.Y, oStart.Y);
            let (mut start, mut end) = (sStart.X, sEnd.X);
            let (mut otherStart, mut otherEnd) = (oStart.X, oEnd.X);
            if isHorizontal {
                old = sStart.X;
                position = oStart.X;
                start = sStart.Y;
                end = sEnd.Y;
                otherStart = oStart.Y;
                otherEnd = oEnd.Y;
            }
            if proposed[i] == old {
                continue;
            }
            // Closed intervals matter.
            if math::Max(start, end) < math::Min(otherStart, otherEnd) || math::Max(otherStart, otherEnd) < math::Min(start, end) {
                continue;
            }
            if old == position || proposed[i] == position {
                return Ok(balanceOrderContactChanged);
            }
            if (old < position) != (proposed[i] < position) {
                status = balanceOrderReversed;
                // Keep checking: a later relation may prohibit fallback.
            }
        }
    }
    Ok(status)
}
