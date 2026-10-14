// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/routing/shortcut_routes.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of routing/shortcut_routes.go.
#[allow(unused_imports)]
use crate::*;
use gocompat::context::Ctx;
use gocompat::math;
use gocompat::GoResult;
use layoutgraph::{EdgeRef, GraphRef};

pub(crate) const maxShortcutInput: usize = 256;
pub(crate) const maxShortcutRoute: usize = 32;
pub(crate) const shortcutEpsilon: f64 = 1e-6;

// ShortcutEdgeRoutes is a bounded bend-reduction postpass with fixed boxes and
// ports.
pub fn ShortcutEdgeRoutes(ctx: &Ctx, g: &GraphRef) -> GoResult<()> {
    match shortcutRoutesWithLimit(ctx, g, maxRouteStageWorkUnits) {
        Err(err) if err.Is(&errRouteStageWorkLimit) => Ok(()), // The atomic stage has already restored the entire drawing.
        r => r,
    }
}

pub(crate) fn shortcutInputTooLarge(g: &GraphRef) -> bool {
    let (nodes, edges) = {
        let gb = g.b();
        (gb.Nodes.len(), gb.Edges.clone())
    };
    if nodes > maxShortcutInput || edges.len() > maxShortcutInput {
        return true;
    }
    let mut remaining = maxShortcutInput;
    for e in &edges {
        let n = e.b().Points.len();
        if n > remaining {
            return true;
        }
        remaining -= n;
    }
    false
}

pub(crate) fn shortcutRoutesWithLimit(ctx: &Ctx, g: &GraphRef, limit: u64) -> GoResult<()> {
    if let Some(err) = ctx.Err() {
        return Err(err);
    }
    if shortcutInputTooLarge(g) {
        return match ctx.Err() {
            Some(err) => Err(err),
            None => Ok(()),
        };
    }
    runAtomicRouteStage(ctx, "ShortcutEdgeRoutes", g, &[], limit, |guard| shortcutRoutesGuarded(g, guard))
}

/// A Go `[]*geo.Point` route whose candidate points are either shared with the
/// edge (unchanged) or fresh elbows; only values are ever read.
type pts = Vec<geo::PointRef>;

pub(crate) fn shortcutRoutesGuarded(g: &GraphRef, guard: &routeWorkGuardRef) -> GoResult<()> {
    let mut boxes: Vec<geo::Box> = Vec::new();
    let mut labelsReady = false;
    let mut inventoryChecked = false;
    let ctx = guard.ctx();
    let nodeToTree = g.b().NodeToTree.clone();
    for e in g.edges() {
        guard.step()?;
        let fromTree = nodeToTree.has(&e.from());
        let toTree = nodeToTree.has(&e.to());
        let points = e.points();
        let skip = {
            let eb = e.b();
            eb.IsInvisible
                || eb.IsCurve
                || eb.From == eb.To
                || fromTree
                || toTree
                || eb.Label.is_some()
                || eb.SourceArrowheadLabel.is_some()
                || eb.TargetArrowheadLabel.is_some()
        };
        if skip || points.len() < 5 || points.len() > maxShortcutRoute || !shortcutOrthogonal(&points) {
            continue;
        }
        // A crossing or contact with a curved route cannot be certified from
        // its control polygon. Leave these drawings to the existing router.
        let mut unsupported = false;
        for other in g.edges() {
            guard.step()?;
            unsupported = unsupported || other.b().IsCurve || !shortcutOrthogonal(&other.points());
        }
        if unsupported {
            return Ok(());
        }
        if !labelsReady {
            boxes = shortcutLabelBoxes(g, guard)?;
            labelsReady = true;
        }
        let mut best: pts = points.clone();
        let mut bestLength = shortcutLength(&best);
        let beforeBends = shortcutBends(&best);
        let mut bestBends = shortcutBends(&best);
        let mut start = 0usize;
        while start + 3 < points.len() {
            let mut end = start + 3;
            while end < points.len() {
                for horizontal in [true, false] {
                    guard.add(points.len() as u64)?;
                    let candidate = shortcutCandidate(&points, start, end, horizontal);
                    let length = shortcutLength(&candidate);
                    let bends = shortcutBends(&candidate);
                    if bends >= beforeBends
                        || bends > bestBends
                        || length > shortcutLength(&points) + shortcutEpsilon
                        || (bends == bestBends && length >= bestLength - shortcutEpsilon)
                    {
                        continue;
                    }
                    let safe = shortcutCandidateSafe(g, &e, &candidate, &boxes, guard)?;
                    if safe {
                        best = candidate;
                        bestLength = length;
                        bestBends = bends;
                    }
                }
                end += 1;
            }
            start += 1;
        }
        if bestBends == beforeBends {
            continue;
        }
        if !inventoryChecked {
            // Direct graph slices define the modeled obstacles.
            let snapshot = captureRouteMutations(Some(g), &[], guard)?;
            let closed = channelInventoryIsClosed(g, &snapshot, guard)?;
            if !closed {
                return Ok(());
            }
            inventoryChecked = true;
        }
        // The local guards are per node/label/edge pair; inspection is a final
        // aggregate backstop, paid only for a geometrically viable shortcut.
        let before = quality::Inspect(&ctx, Some(g))?;
        let original = e.points();
        e.bm().Points = best;
        let after = quality::Inspect(&ctx, Some(g))?;
        if after.RouteObstructions > before.RouteObstructions
            || after.Crossings > before.Crossings
            || after.TextOcclusions > before.TextOcclusions
            || after.RouteLength > before.RouteLength + shortcutEpsilon
        {
            e.bm().Points = original;
        }
    }
    Ok(())
}

pub(crate) fn shortcutOrthogonal(points: &[geo::PointRef]) -> bool {
    let v = pointValues(points);
    shortcutOrthogonalValues(&v)
}

fn shortcutOrthogonalValues(points: &[geo::Point]) -> bool {
    for (i, p) in points.iter().enumerate() {
        if p.X.is_nan() || p.Y.is_nan() || p.X.is_infinite() || p.Y.is_infinite() {
            return false;
        }
        if i > 0 && (*p == points[i - 1] || (p.X != points[i - 1].X && p.Y != points[i - 1].Y)) {
            return false;
        }
        if i > 1
            && !sameRouteDirection(&points[i - 2], &points[i - 1], &points[i - 1], p)
            && (points[i - 2].X == p.X || points[i - 2].Y == p.Y)
        {
            return false; // A U-turn is not a harmless collinear vertex.
        }
    }
    points.len() >= 2
}

pub(crate) fn shortcutLength(points: &[geo::PointRef]) -> f64 {
    let mut length = 0.0;
    for i in 1..points.len() {
        let (a, b) = (points[i - 1].get(), points[i].get());
        length += (b.X - a.X).abs() + (b.Y - a.Y).abs();
    }
    length
}

pub(crate) fn shortcutBends(points: &[geo::PointRef]) -> i64 {
    let mut bends = 0;
    for i in 2..points.len() {
        let (a, b, c) = (points[i - 2].get(), points[i - 1].get(), points[i].get());
        if (b.X - a.X) * (c.Y - b.Y) != (b.Y - a.Y) * (c.X - b.X) {
            bends += 1;
        }
    }
    bends
}

pub(crate) fn shortcutCandidate(points: &[geo::PointRef], start: usize, end: usize, horizontal: bool) -> pts {
    let mut elbow = geo::NewPoint(points[end].X(), points[start].Y());
    if !horizontal {
        elbow = geo::NewPoint(points[start].X(), points[end].Y());
    }
    let mut raw: pts = points[..start + 1].to_vec();
    raw.push(elbow);
    raw.extend_from_slice(&points[end..]);
    let mut result: pts = Vec::with_capacity(raw.len());
    for p in &raw {
        if !result.is_empty() && result[result.len() - 1].get() == p.get() {
            continue;
        }
        while result.len() >= 2
            && sameRouteDirection(
                &result[result.len() - 2].get(),
                &result[result.len() - 1].get(),
                &result[result.len() - 1].get(),
                &p.get(),
            )
        {
            result.pop();
        }
        result.push(p.clone());
    }
    result
}

pub(crate) fn shortcutCandidateSafe(
    g: &GraphRef,
    e: &EdgeRef,
    points: &[geo::PointRef],
    labels: &[geo::Box],
    guard: &routeWorkGuardRef,
) -> GoResult<bool> {
    let ePointRefs = e.points();
    if !shortcutOrthogonal(points)
        || shortcutBends(points) >= shortcutBends(&ePointRefs)
        || shortcutLength(points) > shortcutLength(&ePointRefs) + shortcutEpsilon
    {
        return Ok(false);
    }
    let pv = pointValues(points);
    let ev = pointValues(&ePointRefs);
    let safe = changedRouteIsClear(g, e, &pv, &ev, true, guard)?;
    if !safe {
        return Ok(safe);
    }
    for i in 1..pv.len() {
        let (a, b) = (pv[i - 1], pv[i]);
        let length = (b.X - a.X).abs() + (b.Y - a.Y).abs();
        let mut minimum = segmentSpacingBuffer;
        let mut unchanged = false;
        for j in 1..ev.len() {
            guard.step()?;
            unchanged = unchanged || (a == ev[j - 1] && b == ev[j]);
        }
        if i == 1 {
            minimum = math::Min(minimum, shortcutLength(&ePointRefs[..2]));
        }
        if i == pv.len() - 1 {
            minimum = math::Min(minimum, shortcutLength(&ePointRefs[ePointRefs.len() - 2..]));
        }
        if !unchanged && length + shortcutEpsilon < minimum {
            return Ok(false);
        }
        for j in i + 2..pv.len() {
            guard.step()?;
            if shortcutSegmentsMeet(&a, &b, &pv[j - 1], &pv[j]) {
                return Ok(false);
            }
        }
        if unchanged {
            continue;
        }
        for box_ in labels {
            guard.step()?;
            let tl = box_.TopLeft.as_ref().expect("nil pointer dereference").get();
            if orthogonalSegmentEntersBox(&tl, box_.Width, box_.Height, &a, &b) {
                return Ok(false);
            }
        }
        let safe = shortcutWallClearance(g, &ev, &a, &b, guard)?;
        if !safe {
            return Ok(safe);
        }
        let safe = shortcutParallelClearance(g, e, &ev, &a, &b, guard)?;
        if !safe {
            return Ok(safe);
        }
    }
    for other in g.edges() {
        if other == *e {
            continue;
        }
        let safe = shortcutContactsPreserved(&ev, &pv, &pointValues(&other.b().Points), guard)?;
        if !safe {
            return Ok(safe);
        }
    }
    Ok(true)
}

// Compare actual intersection geometry rather than segment indices.
pub(crate) fn shortcutContactsPreserved(
    before: &[geo::Point],
    after: &[geo::Point],
    other: &[geo::Point],
    guard: &routeWorkGuardRef,
) -> GoResult<bool> {
    for i in 1..after.len() {
        for j in 1..other.len() {
            guard.step()?;
            let (a, b, c, d) = (after[i - 1], after[i], other[j - 1], other[j]);
            if !shortcutSegmentsMeet(&a, &b, &c, &d) {
                continue;
            }
            if (a.X == b.X) != (c.X == d.X) {
                let mut p = geo::Point { X: a.X, Y: c.Y };
                if a.Y == b.Y {
                    p = geo::Point { X: c.X, Y: a.Y };
                }
                let mut covered = false;
                for k in 1..before.len() {
                    guard.step()?;
                    covered = covered || shortcutPointOnSegment(&p, &before[k - 1], &before[k]);
                }
                let (oldH, oldV) = shortcutThroughRays(before, &p, guard)?;
                let (newH, newV) = shortcutThroughRays(after, &p, guard)?;
                let (otherH, otherV) = shortcutThroughRays(other, &p, guard)?;
                let newCrossing = (newH && otherV) || (newV && otherH);
                let oldCrossing = (oldH && otherV) || (oldV && otherH);
                if !covered || (newCrossing && !oldCrossing) {
                    return Ok(false);
                }
                continue;
            }
            let horizontal = a.Y == b.Y;
            let mut position = a.X;
            if horizontal {
                position = a.Y;
            }
            let lo = math::Max(
                math::Min(channelCoordinate(&a, horizontal), channelCoordinate(&b, horizontal)),
                math::Min(channelCoordinate(&c, horizontal), channelCoordinate(&d, horizontal)),
            );
            let hi = math::Min(
                math::Max(channelCoordinate(&a, horizontal), channelCoordinate(&b, horizontal)),
                math::Max(channelCoordinate(&c, horizontal), channelCoordinate(&d, horizontal)),
            );
            // Extend covered prefix without sorting or altering the old route.
            let mut cursor = lo;
            loop {
                let mut next = cursor;
                let mut containsPoint = false;
                for k in 1..before.len() {
                    guard.step()?;
                    let (u, v) = (before[k - 1], before[k]);
                    if (horizontal && (u.Y != position || v.Y != position)) || (!horizontal && (u.X != position || v.X != position)) {
                        continue;
                    }
                    let left = math::Min(channelCoordinate(&u, horizontal), channelCoordinate(&v, horizontal));
                    let right = math::Max(channelCoordinate(&u, horizontal), channelCoordinate(&v, horizontal));
                    if left <= cursor + shortcutEpsilon && right >= cursor - shortcutEpsilon {
                        containsPoint = true;
                        next = math::Max(next, right);
                    }
                }
                if containsPoint && next >= hi - shortcutEpsilon {
                    break;
                }
                if next <= cursor + shortcutEpsilon {
                    return Ok(false);
                }
                cursor = next;
            }
        }
    }
    Ok(true)
}

pub(crate) fn shortcutPointOnSegment(p: &geo::Point, a: &geo::Point, b: &geo::Point) -> bool {
    ((a.X == b.X && p.X == a.X) || (a.Y == b.Y && p.Y == a.Y))
        && p.X >= math::Min(a.X, b.X) - shortcutEpsilon
        && p.X <= math::Max(a.X, b.X) + shortcutEpsilon
        && p.Y >= math::Min(a.Y, b.Y) - shortcutEpsilon
        && p.Y <= math::Max(a.Y, b.Y) + shortcutEpsilon
}

// For orthogonal segments, overlapping closed bounding boxes mean contact.
pub(crate) fn shortcutSegmentsMeet(a: &geo::Point, b: &geo::Point, c: &geo::Point, d: &geo::Point) -> bool {
    math::Max(math::Min(a.X, b.X), math::Min(c.X, d.X)) <= math::Min(math::Max(a.X, b.X), math::Max(c.X, d.X))
        && math::Max(math::Min(a.Y, b.Y), math::Min(c.Y, d.Y)) <= math::Min(math::Max(a.Y, b.Y), math::Max(c.Y, d.Y))
}

// Classify the whole route at a contact.
pub(crate) fn shortcutThroughRays(points: &[geo::Point], p: &geo::Point, guard: &routeWorkGuardRef) -> GoResult<(bool, bool)> {
    let (mut left, mut right, mut above, mut below) = (false, false, false, false);
    for i in 1..points.len() {
        guard.step()?;
        let (a, b) = (points[i - 1], points[i]);
        if !shortcutPointOnSegment(p, &a, &b) {
            continue;
        }
        if a.Y == b.Y {
            left = left || math::Min(a.X, b.X) < p.X;
            right = right || math::Max(a.X, b.X) > p.X;
        } else {
            above = above || math::Min(a.Y, b.Y) < p.Y;
            below = below || math::Max(a.Y, b.Y) > p.Y;
        }
    }
    Ok((left && right, above && below))
}

// A new parallel leg must retain min(previous gap,40) to every projected node
// wall, including container walls.
pub(crate) fn shortcutWallClearance(
    g: &GraphRef,
    before: &[geo::Point],
    a: &geo::Point,
    b: &geo::Point,
    guard: &routeWorkGuardRef,
) -> GoResult<bool> {
    let horizontal = a.Y == b.Y;
    let position = channelCoordinate(a, !horizontal);
    let lo = math::Min(channelCoordinate(a, horizontal), channelCoordinate(b, horizontal));
    let hi = math::Max(channelCoordinate(a, horizontal), channelCoordinate(b, horizontal));
    for n in g.nodes() {
        guard.step()?;
        let tl = n.tlv();
        let (w, h) = (n.w(), n.h());
        let nlo = channelCoordinate(&tl, horizontal);
        let (mut nhi, mut wall) = (nlo + h, tl.X);
        let mut walls = [wall, wall + w];
        if horizontal {
            nhi = nlo + w;
            wall = tl.Y;
            walls = [wall, wall + h];
        }
        if math::Min(hi, nhi) - math::Max(lo, nlo) <= shortcutEpsilon {
            continue;
        }
        for wall in walls {
            let gap = (position - wall).abs();
            if gap >= segmentSpacingBuffer - shortcutEpsilon {
                continue;
            }
            let mut minimum = segmentSpacingBuffer;
            for j in 1..before.len() {
                guard.step()?;
                let (u, v) = (before[j - 1], before[j]);
                let oldPosition = channelCoordinate(&u, !horizontal);
                if oldPosition != channelCoordinate(&v, !horizontal) || (oldPosition - wall) * (position - wall) < 0.0 {
                    continue;
                }
                let oldLo = math::Min(channelCoordinate(&u, horizontal), channelCoordinate(&v, horizontal));
                let oldHi = math::Max(channelCoordinate(&u, horizontal), channelCoordinate(&v, horizontal));
                if math::Min(oldHi, nhi) - math::Max(oldLo, nlo) > shortcutEpsilon {
                    minimum = math::Min(minimum, (oldPosition - wall).abs());
                }
            }
            if gap + shortcutEpsilon < minimum {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

// Preserve the channel pass's readable spacing.
pub(crate) fn shortcutParallelClearance(
    g: &GraphRef,
    e: &EdgeRef,
    ePoints: &[geo::Point],
    a: &geo::Point,
    b: &geo::Point,
    guard: &routeWorkGuardRef,
) -> GoResult<bool> {
    let horizontal = a.Y == b.Y;
    let position = channelCoordinate(a, !horizontal);
    let lo = math::Min(channelCoordinate(a, horizontal), channelCoordinate(b, horizontal));
    let hi = math::Max(channelCoordinate(a, horizontal), channelCoordinate(b, horizontal));
    for other in g.edges() {
        if other == *e {
            continue;
        }
        let op = pointValues(&other.b().Points);
        for j in 1..op.len() {
            guard.step()?;
            let (c, d) = (op[j - 1], op[j]);
            let wall = channelCoordinate(&c, !horizontal);
            if wall != channelCoordinate(&d, !horizontal) {
                continue;
            }
            let otherLo = math::Min(channelCoordinate(&c, horizontal), channelCoordinate(&d, horizontal));
            let otherHi = math::Max(channelCoordinate(&c, horizontal), channelCoordinate(&d, horizontal));
            if math::Min(hi, otherHi) - math::Max(lo, otherLo) <= shortcutEpsilon {
                continue;
            }
            let gap = (position - wall).abs();
            if gap <= shortcutEpsilon || gap >= segmentSpacingBuffer - shortcutEpsilon {
                continue;
            }
            let mut minimum = segmentSpacingBuffer;
            for k in 1..ePoints.len() {
                guard.step()?;
                let (u, v) = (ePoints[k - 1], ePoints[k]);
                let oldPosition = channelCoordinate(&u, !horizontal);
                if oldPosition != channelCoordinate(&v, !horizontal) {
                    continue;
                }
                let oldLo = math::Min(channelCoordinate(&u, horizontal), channelCoordinate(&v, horizontal));
                let oldHi = math::Max(channelCoordinate(&u, horizontal), channelCoordinate(&v, horizontal));
                let oldGap = (oldPosition - wall).abs();
                if oldGap > shortcutEpsilon && math::Min(oldHi, otherHi) - math::Max(oldLo, otherLo) > shortcutEpsilon {
                    minimum = math::Min(minimum, oldGap);
                }
            }
            if gap + shortcutEpsilon < minimum {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

pub(crate) fn shortcutLabelBoxes(g: &GraphRef, guard: &routeWorkGuardRef) -> GoResult<Vec<geo::Box>> {
    let mut boxes: Vec<geo::Box> = Vec::new();
    let nodes = g.nodes();
    for n in &nodes {
        guard.add(1 + 4 * nodes.len() as u64)?;
        if n.b().IsInvisible {
            continue;
        }
        if let Some(l) = n.label() {
            let (pos, w, h) = {
                let lb = l.b();
                (lb.Position, lb.Width, lb.Height)
            };
            if pos != label::Unset {
                boxes.push(geo::Box { TopLeft: Some(n.LabelTopLeft(pos, w, h)), Width: w, Height: h });
            }
        }
        if let Some(icon) = n.icon() {
            if !n.IsImage() {
                let pos = icon.b().Position;
                let size = n.IconSize(pos);
                boxes.push(geo::Box { TopLeft: Some(n.LabelTopLeft(pos, size, size)), Width: size, Height: size });
            }
        }
    }
    for e in g.edges() {
        let points = e.points();
        guard.add(1 + 3 * points.len() as u64)?;
        if e.b().IsInvisible || points.len() < 2 {
            continue;
        }
        let (lbl, hasSource, hasTarget) = {
            let eb = e.b();
            (eb.Label.clone(), eb.SourceArrowheadLabel.is_some(), eb.TargetArrowheadLabel.is_some())
        };
        if let Some(l) = lbl {
            let (pos, w, h) = {
                let lb = l.b();
                (lb.Position, lb.Width, lb.Height)
            };
            if pos != label::Unset {
                boxes.push(geo::Box { TopLeft: e.LabelTopLeft(pos, w, h), Width: w, Height: h });
            }
        }
        if hasSource {
            boxes.push(labeling::PositionArrowheadLabel(&e, false, &points).expect("nil pointer dereference").Box);
        }
        if hasTarget {
            boxes.push(labeling::PositionArrowheadLabel(&e, true, &points).expect("nil pointer dereference").Box);
        }
    }
    Ok(boxes)
}
