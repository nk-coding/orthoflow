// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/quality/crossings.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

pub(crate) fn countNonSharedCrossings(edges: &[EdgeRef], guard: &WorkGuardRef) -> GoResult<i64> {
    let mut crossingCount: i64 = 0;
    let pts: Vec<Vec<geo::Point>> = edges.iter().map(|e| e.b().Points.iter().map(|p| *p.b()).collect()).collect();
    for i in 0..edges.len() {
        guard.Step()?;
        for j in i + 1..edges.len() {
            guard.Step()?;
            crossingCount += countEdgeCrossings(&pts[i], &pts[j], guard)?;
        }
    }
    Ok(crossingCount)
}

pub(crate) fn countEdgeCrossings(edge: &[geo::Point], other: &[geo::Point], guard: &WorkGuardRef) -> GoResult<i64> {
    let mut crossingCount: i64 = 0;
    let n = edge.len() as i64;
    let m = other.len() as i64;
    let mut i = 0i64;
    while i < n - 1 {
        guard.Step()?;
        let mut j = 0i64;
        while j < m - 1 {
            guard.Step()?;
            if isNonSharedCrossing(edge, other, i as usize, j as usize) {
                crossingCount += 1;
            }
            j += 1;
        }
        i += 1;
    }
    Ok(crossingCount)
}

pub(crate) fn isNonSharedCrossing(edge: &[geo::Point], other: &[geo::Point], edgeSegment: usize, otherSegment: usize) -> bool {
    if !nonParallelIntersection(&edge[edgeSegment], &edge[edgeSegment + 1], &other[otherSegment], &other[otherSegment + 1]) {
        return false;
    }
    let segmentStartX = edge[edgeSegment].X;
    let segmentStartY = edge[edgeSegment].Y;
    if segmentStartX == edge[edgeSegment + 1].X
        && (segmentStartX == other[otherSegment].X || segmentStartX == other[otherSegment + 1].X)
    {
        return false;
    }
    if segmentStartY == edge[edgeSegment + 1].Y
        && (segmentStartY == other[otherSegment].Y || segmentStartY == other[otherSegment + 1].Y)
    {
        return false;
    }
    true
}

/// orientation returns a positive value for a clockwise turn, a negative value
/// for a counter-clockwise turn, and zero for collinear points.
pub(crate) fn orientation(p: &geo::Point, q: &geo::Point, r: &geo::Point) -> f64 {
    let pqX = q.X - p.X;
    let pqY = q.Y - p.Y;
    let prX = r.X - p.X;
    let prY = r.Y - p.Y;
    pqY * prX - pqX * prY
}

pub(crate) fn equalSigns(a: f64, b: f64) -> bool {
    a > 0.0 && b > 0.0 || a == 0.0 && b == 0.0 || a < 0.0 && b < 0.0
}

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
