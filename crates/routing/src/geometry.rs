// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/routing/geometry.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of routing/geometry.go.
#[allow(unused_imports)]
use crate::*;
use gocompat::math;

/// Go `segmentIntersectsBox(first, second *geo.Point, box *geo.Box)`. The Go
/// nil checks on the points/box are the caller's business (callers always pass
/// non-nil values); the nil TopLeft check is kept.
pub(crate) fn segmentIntersectsBox(first: &geo::Point, second: &geo::Point, box_: &geo::Box) -> bool {
    let tl = match &box_.TopLeft {
        None => return false,
        Some(tl) => *tl.b(),
    };
    let left = math::Min(tl.X, tl.X + box_.Width);
    let right = math::Max(tl.X, tl.X + box_.Width);
    let top = math::Min(tl.Y, tl.Y + box_.Height);
    let bottom = math::Max(tl.Y, tl.Y + box_.Height);
    if math::Max(first.X, second.X) < left
        || right < math::Min(first.X, second.X)
        || math::Max(first.Y, second.Y) < top
        || bottom < math::Min(first.Y, second.Y)
    {
        return false;
    }
    let contains = |point: &geo::Point| left <= point.X && point.X <= right && top <= point.Y && point.Y <= bottom;
    if contains(first) || contains(second) {
        return true;
    }
    let mut tEnter = 0.0f64;
    let mut tExit = 1.0f64;
    let mut clipAxis = |start: f64, delta: f64, minCoord: f64, maxCoord: f64| -> bool {
        if delta == 0.0 {
            return minCoord <= start && start <= maxCoord;
        }
        let mut one = (minCoord - start) / delta;
        let mut two = (maxCoord - start) / delta;
        if one > two {
            std::mem::swap(&mut one, &mut two);
        }
        tEnter = math::Max(tEnter, one);
        tExit = math::Min(tExit, two);
        tEnter <= tExit
    };
    if !clipAxis(first.X, second.X - first.X, left, right) || !clipAxis(first.Y, second.Y - first.Y, top, bottom) {
        return false;
    }
    tEnter < tExit
}

pub(crate) fn orientation(first: &geo::Point, second: &geo::Point, third: &geo::Point) -> f64 {
    let firstSecondX = second.X - first.X;
    let firstSecondY = second.Y - first.Y;
    let firstThirdX = third.X - first.X;
    let firstThirdY = third.Y - first.Y;
    firstSecondY * firstThirdX - firstSecondX * firstThirdY
}

pub(crate) fn intersects(firstStart: &geo::Point, firstEnd: &geo::Point, secondStart: &geo::Point, secondEnd: &geo::Point) -> bool {
    let secondStartSide = orientation(firstStart, firstEnd, secondStart);
    let secondEndSide = orientation(firstStart, firstEnd, secondEnd);
    let firstStartSide = orientation(secondStart, secondEnd, firstStart);
    let firstEndSide = orientation(secondStart, secondEnd, firstEnd);
    if secondStartSide == 0.0 && secondEndSide == 0.0 && firstStartSide == 0.0 && firstEndSide == 0.0 {
        return closedIntervalsOverlap(firstStart.X, firstEnd.X, secondStart.X, secondEnd.X)
            && closedIntervalsOverlap(firstStart.Y, firstEnd.Y, secondStart.Y, secondEnd.Y);
    }
    straddlesLine(secondStartSide, secondEndSide) && straddlesLine(firstStartSide, firstEndSide)
}

pub(crate) fn straddlesLine(first: f64, second: f64) -> bool {
    first == 0.0 || second == 0.0 || (first < 0.0) != (second < 0.0)
}

pub(crate) fn closedIntervalsOverlap(firstStart: f64, firstEnd: f64, secondStart: f64, secondEnd: f64) -> bool {
    let (firstMin, firstMax) = (math::Min(firstStart, firstEnd), math::Max(firstStart, firstEnd));
    let (secondMin, secondMax) = (math::Min(secondStart, secondEnd), math::Max(secondStart, secondEnd));
    firstMin <= secondMax && secondMin <= firstMax
}
