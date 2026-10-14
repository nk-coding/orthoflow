// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/layoutgraph/segment_crossing.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

pub(crate) const scoringCancellationCheckInterval: i64 = 64;

/// CrossingSegment is an oriented line segment used by crossing scoring.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CrossingSegment {
    pub Start: geo::Point,
    pub End: geo::Point,
}

pub(crate) fn scoringCancellationError(ctx: &Ctx, iteration: i64) -> GoResult<()> {
    if iteration % scoringCancellationCheckInterval != 0 {
        return Ok(());
    }
    if let Some(err) = ctx.Err() {
        return Err(wrapf!(err, "EdgeLength: {}", err));
    }
    Ok(())
}

/// CountSegmentCrossings counts intersections between segments while ignoring
/// pairs with equal starts or equal ends. It is the common geometric kernel used
/// by layout cost evaluation and hierarchy rank optimization.
pub fn CountSegmentCrossings(segments: &[CrossingSegment]) -> i64 {
    match countSegmentCrossingsWithCheck(segments, None) {
        Ok(c) => c,
        Err(_) => 0,
    }
}

/// CountSegmentCrossingsContext is CountSegmentCrossings with the layout
/// engine's bounded cancellation checks.
pub fn CountSegmentCrossingsContext(ctx: &Ctx, segments: &[CrossingSegment]) -> GoResult<i64> {
    if let Some(err) = ctx.Err() {
        return Err(wrapf!(err, "EdgeLength: {}", err));
    }
    let check = |iteration: i64| scoringCancellationError(ctx, iteration);
    let crossings = countSegmentCrossingsWithCheck(segments, Some(&check))?;
    if let Some(err) = ctx.Err() {
        return Err(wrapf!(err, "EdgeLength: {}", err));
    }
    Ok(crossings)
}

pub(crate) fn countSegmentCrossingsWithCheck(
    segments: &[CrossingSegment],
    checkCanceled: Option<&dyn Fn(i64) -> GoResult<()>>,
) -> GoResult<i64> {
    let mut crossings: i64 = 0;
    let n = segments.len() as i64;
    let mut i: i64 = 0;
    while i < n - 1 {
        if let Some(check) = checkCanceled {
            check(i)?;
        }
        let iStart = &segments[i as usize].Start;
        let iEnd = &segments[i as usize].End;
        let (mut minX, mut maxX) = (iStart.X, iEnd.X);
        if maxX < minX {
            std::mem::swap(&mut minX, &mut maxX);
        }
        let (mut minY, mut maxY) = (iStart.Y, iEnd.Y);
        if maxY < minY {
            std::mem::swap(&mut minY, &mut maxY);
        }
        let mut j = i + 1;
        while j < n {
            if let Some(check) = checkCanceled {
                check(j - i - 1)?;
            }
            let jStart = &segments[j as usize].Start;
            let jEnd = &segments[j as usize].End;
            j += 1;
            if jStart.X < jEnd.X {
                if jEnd.X < minX {
                    continue;
                }
                if maxX < jStart.X {
                    continue;
                }
            } else {
                if jStart.X < minX {
                    continue;
                }
                if maxX < jEnd.X {
                    continue;
                }
            }
            if jStart.Y < jEnd.Y {
                if jEnd.Y < minY {
                    continue;
                }
                if maxY < jStart.Y {
                    continue;
                }
            } else {
                if jStart.Y < minY {
                    continue;
                }
                if maxY < jEnd.Y {
                    continue;
                }
            }

            if pointValuesEqual(iStart, jStart) || pointValuesEqual(iEnd, jEnd) {
                continue;
            }
            if SegmentsCross(iStart, iEnd, jStart, jEnd) {
                crossings += 1;
            }
        }
        i += 1;
    }
    Ok(crossings)
}

/// SegmentsCross reports whether two finite segments intersect. Parallel and
/// overlapping segments do not count as crossings.
pub fn SegmentsCross(u0: &geo::Point, u1: &geo::Point, v0: &geo::Point, v1: &geo::Point) -> bool {
    let denom = (u1.Y - u0.Y) * (v1.X - v0.X) - (u1.X - u0.X) * (v1.Y - v0.Y);
    if denom == 0.0 {
        return false;
    }
    let s = ((v1.X - v0.X) * (v0.Y - u0.Y) - (v1.Y - v0.Y) * (v0.X - u0.X)) / denom;
    if s < 0.0 || s > 1.0 {
        return false;
    }
    let t = ((u1.X - u0.X) * (v0.Y - u0.Y) - (u1.Y - u0.Y) * (v0.X - u0.X)) / denom;
    t >= 0.0 && t <= 1.0
}
