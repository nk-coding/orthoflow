// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/routing/balance_crossings.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of routing/balance_crossings.go.
#[allow(unused_imports)]
use crate::*;
use gocompat::math;
use gocompat::{GoResult, Map};
use layoutgraph::{EdgeSegmentRef, GraphRef};

// balanceReversalRemovesCrossings permits an otherwise rejected reversal only
// when it removes crossings without increasing them for any affected route
// pair.
//
// Proposed point copies also move adjacent legs and respect pointer aliases.
// The graph stays untouched until the caller accepts the entire batch.
pub(crate) fn balanceReversalRemovesCrossings(
    g: &GraphRef,
    batch: &[EdgeSegmentRef],
    proposed: &[f64],
    isHorizontal: bool,
    guard: &routeWorkGuardRef,
) -> GoResult<bool> {
    // Go map[*geo.Point]*geo.Point; candidates are values (never aliased).
    let mut pointCopies: Map<geo::PointRef, geo::Point> = Map::default();
    for (i, segment) in batch.iter().enumerate() {
        guard.step()?;
        let (start, end) = {
            let s = segment.b();
            (s.Segment.Start.clone(), s.Segment.End.clone())
        };
        for point in [start, end] {
            guard.step()?;
            let mut candidate = point.get();
            if isHorizontal {
                candidate.X = proposed[i];
            } else {
                candidate.Y = proposed[i];
            }
            if let Some(previous) = pointCopies.get(&point) {
                if *previous != candidate {
                    return Ok(false);
                }
            }
            pointCopies.insert(point, candidate);
        }
    }

    let edges = g.edges();
    guard.add(edges.len() as u64)?;
    let mut candidates: Vec<Option<Vec<geo::Point>>> = vec![None; edges.len()];
    let mut affected: Vec<usize> = Vec::new();
    for (i, edge) in edges.iter().enumerate() {
        guard.step()?;
        let points = edge.points();
        let mut changed = false;
        for point in &points {
            guard.step()?;
            if let Some(candidate) = pointCopies.get(point) {
                if *candidate != point.get() {
                    changed = true;
                }
            }
        }
        if !changed {
            continue;
        }
        if edge.b().IsCurve {
            return Ok(false);
        }
        guard.add(points.len() as u64)?;
        let mut candidate: Vec<geo::Point> = vec![geo::Point::default(); points.len()];
        for (j, point) in points.iter().enumerate() {
            guard.step()?;
            let pv = point.get();
            candidate[j] = pv;
            if let Some(replacement) = pointCopies.get(point) {
                candidate[j] = *replacement;
            }
            if j > 0 {
                let (before, after) = (points[j - 1].get(), candidate[j - 1]);
                if before.X != pv.X && before.Y != pv.Y || after.X != candidate[j].X && after.Y != candidate[j].Y {
                    return Ok(false);
                }
            }
        }
        candidates[i] = Some(candidate);
        affected.push(i);
    }

    let values: Vec<Vec<geo::Point>> = edges.iter().map(|e| pointValues(&e.b().Points)).collect();
    let mut improved = false;
    for &i in &affected {
        guard.step()?;
        let candidateI = candidates[i].as_ref().unwrap();
        for (j, other) in edges.iter().enumerate() {
            guard.step()?;
            if i == j || candidates[j].is_some() && j < i {
                continue;
            }
            // A curve's control polygon cannot prove its rendered crossings.
            if other.b().IsCurve {
                return Ok(false);
            }
            let otherCandidate = candidates[j].as_ref().unwrap_or(&values[j]);
            let (mut before, mut after) = (0i64, 0i64);
            let ei = &values[i];
            let oj = &values[j];
            let mut a = 0usize;
            while a + 1 < ei.len() {
                guard.step()?;
                let mut b = 0usize;
                while b + 1 < oj.len() {
                    guard.step()?;
                    if nonSharedCrossingValues(ei, oj, a, b) {
                        before += 1;
                    }
                    if nonSharedCrossingValues(candidateI, otherCandidate, a, b) {
                        after += 1;
                    }
                    // Dragged adjacent legs must not gain a new shared run.
                    let oldOverlap = balanceCollinearOverlap(&ei[a], &ei[a + 1], &oj[b], &oj[b + 1]);
                    let newOverlap =
                        balanceCollinearOverlap(&candidateI[a], &candidateI[a + 1], &otherCandidate[b], &otherCandidate[b + 1]);
                    if oldOverlap == 0.0 && newOverlap > 0.0 {
                        return Ok(false);
                    }
                    b += 1;
                }
                a += 1;
            }
            if after > before {
                return Ok(false);
            }
            improved = improved || after < before;
        }
    }
    Ok(improved)
}

pub(crate) fn balanceCollinearOverlap(a: &geo::Point, b: &geo::Point, c: &geo::Point, d: &geo::Point) -> f64 {
    if a.X == b.X && c.X == d.X && a.X == c.X {
        return math::Max(
            0.0,
            math::Min(math::Max(a.Y, b.Y), math::Max(c.Y, d.Y)) - math::Max(math::Min(a.Y, b.Y), math::Min(c.Y, d.Y)),
        );
    }
    if a.Y == b.Y && c.Y == d.Y && a.Y == c.Y {
        return math::Max(
            0.0,
            math::Min(math::Max(a.X, b.X), math::Max(c.X, d.X)) - math::Max(math::Min(a.X, b.X), math::Min(c.X, d.X)),
        );
    }
    0.0
}
