// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/routing/cluster_route_guard.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of routing/cluster_route_guard.go.
#[allow(unused_imports)]
use crate::*;
use gocompat::math;
use gocompat::GoResult;
use layoutgraph::{EdgeRef, NodeRef};

pub(crate) fn estimateRouteCostGuarded(edges: &[EdgeRef], edge: &EdgeRef, guard: &dyn workBudget) -> GoResult<f64> {
    let points: Vec<geo::Point> = edge.b().Points.iter().map(|p| *p.b()).collect();
    let mut length = 0.0;
    let mut index = 0usize;
    while index + 1 < points.len() {
        guard.step()?;
        length += geo::EuclideanDistance(points[index].X, points[index].Y, points[index + 1].X, points[index + 1].Y);
        index += 1;
    }
    let mut cost = length * math::Pow(turnPenalty, (points.len() as i64 - 2) as f64);

    let mut crossingCount: i64 = 0;
    for otherEdge in edges {
        guard.step()?;
        if otherEdge == edge {
            continue;
        }
        let other: Vec<geo::Point> = otherEdge.b().Points.iter().map(|p| *p.b()).collect();
        let otherLen = other.len();
        let mut edgeSegment = 0usize;
        while edgeSegment + 1 < points.len() {
            let mut otherSegment = 0usize;
            while otherSegment + 1 < otherLen {
                guard.step()?;
                if nonSharedCrossingValues(&points, &other, edgeSegment, otherSegment) {
                    crossingCount += 1;
                }
                otherSegment += 1;
            }
            edgeSegment += 1;
        }
    }
    cost += layoutgraph::CrossingCostWeight * crossingCount as f64;
    Ok(cost)
}

pub(crate) fn routeIntersectsNodeGuarded(nodes: &[NodeRef], edge: &EdgeRef, guard: &dyn workBudget) -> GoResult<bool> {
    let (from, to, points) = {
        let e = edge.b();
        (e.From.clone(), e.To.clone(), e.Points.clone())
    };
    for otherNode in nodes {
        guard.step()?;
        if from.IsDescendantOf(Some(otherNode)) || to.IsDescendantOf(Some(otherNode)) {
            continue;
        }
        let mut index = 0usize;
        while index + 1 < points.len() {
            guard.step()?;
            if otherNode.PassesThrough(Some(&points[index]), Some(&points[index + 1])) {
                return Ok(true);
            }
            index += 1;
        }
    }
    Ok(false)
}

/// cost.go's isNonSharedCrossing on point values already read from the routes.
pub(crate) fn nonSharedCrossingValues(ep: &[geo::Point], op: &[geo::Point], edgeSegment: usize, otherSegment: usize) -> bool {
    if !nonParallelIntersection(&ep[edgeSegment], &ep[edgeSegment + 1], &op[otherSegment], &op[otherSegment + 1]) {
        return false;
    }
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

/// Point values of a route (Go reads them through the shared pointers).
pub(crate) fn pointValues(points: &[geo::PointRef]) -> Vec<geo::Point> {
    points.iter().map(|p| *p.b()).collect()
}
