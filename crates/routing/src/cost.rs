// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/routing/cost.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of routing/cost.go.
#[allow(unused_imports)]
use crate::*;
use gocompat::math;
use layoutgraph::EdgeRef;

fn edgePointValues(edge: &EdgeRef) -> Vec<geo::Point> {
    edge.b().Points.iter().map(|p| *p.b()).collect()
}

/// estimateRouteCost scores one candidate route against the routes that have
/// already been selected. Crossing semantics belong to routing: intersections
/// at the exact end of an axis-aligned segment are bends off a shared route,
/// not route crossings.
pub(crate) fn estimateRouteCost(edges: &[EdgeRef], edge: &EdgeRef) -> f64 {
    let mut cost = edge.Length();
    let npoints = edge.b().Points.len() as i64;
    cost *= math::Pow(turnPenalty, (npoints - 2) as f64);

    let mut crossingCount: i64 = 0;
    for otherEdge in edges {
        if otherEdge == edge {
            continue;
        }
        crossingCount += countNonSharedCrossings(edge, otherEdge);
    }
    cost += layoutgraph::CrossingCostWeight * crossingCount as f64;
    cost
}

pub(crate) fn countNonSharedCrossings(edge: &EdgeRef, otherEdge: &EdgeRef) -> i64 {
    let ep = edgePointValues(edge);
    let op = edgePointValues(otherEdge);
    let mut crossingCount: i64 = 0;
    let mut edgeSegment = 0usize;
    while edgeSegment + 1 < ep.len() {
        let mut otherSegment = 0usize;
        while otherSegment + 1 < op.len() {
            if isNonSharedCrossingPoints(&ep, &op, edgeSegment, otherSegment) {
                crossingCount += 1;
            }
            otherSegment += 1;
        }
        edgeSegment += 1;
    }
    crossingCount
}

pub(crate) fn isNonSharedCrossing(edge: &EdgeRef, otherEdge: &EdgeRef, edgeSegment: usize, otherSegment: usize) -> bool {
    isNonSharedCrossingPoints(&edgePointValues(edge), &edgePointValues(otherEdge), edgeSegment, otherSegment)
}

fn isNonSharedCrossingPoints(ep: &[geo::Point], op: &[geo::Point], edgeSegment: usize, otherSegment: usize) -> bool {
    if !nonParallelIntersection(&ep[edgeSegment], &ep[edgeSegment + 1], &op[otherSegment], &op[otherSegment + 1]) {
        return false;
    }

    // Skip intersections at the exact end of the segment to avoid counting
    // bends off shared edges.
    let segmentStartX = ep[edgeSegment].X;
    let segmentStartY = ep[edgeSegment].Y;
    if segmentStartX == ep[edgeSegment + 1].X && (segmentStartX == op[otherSegment].X || segmentStartX == op[otherSegment + 1].X) {
        return false;
    }
    if segmentStartY == ep[edgeSegment + 1].Y && (segmentStartY == op[otherSegment].Y || segmentStartY == op[otherSegment + 1].Y) {
        return false;
    }
    true
}

pub(crate) fn equalSigns(first: f64, second: f64) -> bool {
    first > 0.0 && second > 0.0 || first == 0.0 && second == 0.0 || first < 0.0 && second < 0.0
}

/// nonParallelIntersection reports whether segments ab and cd intersect at an
/// angle. Collinear overlap is intentionally not a routing crossing.
pub(crate) fn nonParallelIntersection(a: &geo::Point, b: &geo::Point, c: &geo::Point, d: &geo::Point) -> bool {
    let abcOrientation = orientation(a, b, c);
    let abdOrientation = orientation(a, b, d);
    if equalSigns(abcOrientation, abdOrientation) {
        return false;
    }

    let cdaOrientation = orientation(c, d, a);
    let cdbOrientation = orientation(c, d, b);
    !equalSigns(cdaOrientation, cdbOrientation)
}
