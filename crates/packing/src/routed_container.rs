// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/packing/routed_container.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

pub(crate) type routedContainerSide = u8;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum routedContainerBoxDecision {
    routedContainerDeferToSideConstraints,
    routedContainerKeepOriginalBox,
    routedContainerUseProposedBox,
}
pub(crate) use routedContainerBoxDecision::*;

pub(crate) const routedContainerTop: routedContainerSide = 1 << 0;
pub(crate) const routedContainerLeft: routedContainerSide = 1 << 1;
pub(crate) const routedContainerBottom: routedContainerSide = 1 << 2;
pub(crate) const routedContainerRight: routedContainerSide = 1 << 3;

// D2 renders an edge with this corner radius when no explicit edge style
// overrides it.
pub(crate) const routedContainerDefaultEdgeBorderRadius: f64 = 10.0;
pub(crate) const routedContainerMaxBorderRadiusTextBytes: usize = 64;

/// Plain value view of a `*geo.Box` with a non-nil TopLeft.
#[derive(Clone, Copy, Debug)]
pub(crate) struct rbox {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) w: f64,
    pub(crate) h: f64,
}

impl rbox {
    fn of(b: &geo::Box) -> Option<rbox> {
        let tl = b.TopLeft.as_ref()?;
        let p = *tl.b();
        Some(rbox { x: p.X, y: p.Y, w: b.Width, h: b.Height })
    }
    /// Go `geo.Box.Contains`.
    fn contains(&self, p: &geo::Point) -> bool {
        !(p.X < self.x || self.x + self.w < p.X || p.Y < self.y || self.y + self.h < p.Y)
    }
}

/// binPackCanUseRoutedContainerBox reports whether the box currently proposed
/// on root preserves every affected routed segment and supported root route
/// attachment from original.
pub(crate) fn binPackCanUseRoutedContainerBox(
    graph: &GraphRef,
    root: &NodeRef,
    original: &geo::Box,
    graphIncidentEdges: &[EdgeRef],
    guard: &WorkGuardRef,
) -> GoResult<routedContainerBoxDecision> {
    guard.Step()?;
    if root.graph().as_ref() != Some(graph) {
        return Ok(routedContainerKeepOriginalBox);
    }
    if graphIncidentEdges.is_empty() {
        guard.Finish()?;
        return Ok(routedContainerDeferToSideConstraints);
    }
    if !root.has_tl() || root.b().Shape.is_none() || original.TopLeft.is_none() {
        return Ok(routedContainerKeepOriginalBox);
    }

    let descendants = graph.AllDescendantNodesWithWorkGuard(Some(root), true, guard)?;
    let mut descendantSet: Set<NodeRef> = Set::default();
    for node in &descendants {
        guard.Step()?;
        descendantSet.insert(node.clone());
    }

    let mut affectedEdges: Vec<EdgeRef> = Vec::with_capacity(graphIncidentEdges.len());
    let mut affectedEdgeSet: Set<EdgeRef> = Set::default();
    for edge in graph.edges() {
        guard.Step()?;
        let (from, to) = (edge.from(), edge.to());
        let fromDescendant = descendantSet.contains(&from);
        let toDescendant = descendantSet.contains(&to);
        if from != *root && to != *root && !fromDescendant && !toDescendant {
            continue;
        }
        if affectedEdgeSet.contains(&edge) {
            return Ok(routedContainerKeepOriginalBox);
        }
        affectedEdgeSet.insert(edge.clone());
        affectedEdges.push(edge);
    }
    let mut graphEdges: Set<EdgeRef> = Set::default();
    for edge in graphIncidentEdges {
        if let Err(e) = guard.Step() {
            return Err(e);
        }
        if edge.from() != *root && edge.to() != *root {
            return Ok(routedContainerKeepOriginalBox);
        }
        if graphEdges.contains(edge) {
            return Ok(routedContainerKeepOriginalBox);
        }
        if !affectedEdgeSet.contains(edge) {
            return Ok(routedContainerKeepOriginalBox);
        }
        graphEdges.insert(edge.clone());
    }
    if !root.IsContainer() || !root.CanContain() || !root.shape().IsRectangular() {
        return Ok(routedContainerKeepOriginalBox);
    }
    let (dx, dy) = root.ModifierElementAdjustments();
    if dx != 0.0 || dy != 0.0 {
        return Ok(routedContainerKeepOriginalBox);
    }
    let rootBox = root.boxValue();
    if !routedContainerBoxIsFinite(Some(&rootBox)) || !routedContainerBoxIsFinite(Some(original)) {
        return Ok(routedContainerKeepOriginalBox);
    }
    let proposed = rbox::of(&rootBox).unwrap();
    let orig = rbox::of(original).unwrap();
    if !routedContainerBoxCovers(&orig, &proposed) {
        return Ok(routedContainerKeepOriginalBox);
    }
    if root.fixedTL().is_some() && (proposed.x != orig.x || proposed.y != orig.y) {
        return Ok(routedContainerKeepOriginalBox);
    }

    let mut checkedEndpoint = false;
    let mut rootEdges: Set<EdgeRef> = Set::default();
    for edge in root.edges() {
        guard.Step()?;
        let (from, to, points) = {
            let e = edge.b();
            (e.From.clone(), e.To.clone(), e.Points.clone())
        };
        if from == to || points.len() < 2 {
            return Ok(routedContainerKeepOriginalBox);
        }
        if !graphEdges.contains(&edge) {
            return Ok(routedContainerKeepOriginalBox);
        }
        if rootEdges.contains(&edge) {
            return Ok(routedContainerKeepOriginalBox);
        }
        rootEdges.insert(edge.clone());
        let mut incident = false;
        if from == *root {
            incident = true;
            checkedEndpoint = true;
            guard.Step()?;
            if !routedContainerEndpointKeepsSides(&orig, &proposed, &points[0].get()) {
                return Ok(routedContainerKeepOriginalBox);
            }
        }
        if to == *root {
            incident = true;
            checkedEndpoint = true;
            guard.Step()?;
            if !routedContainerEndpointKeepsSides(&orig, &proposed, &points[points.len() - 1].get()) {
                return Ok(routedContainerKeepOriginalBox);
            }
        }
        if !incident {
            return Ok(routedContainerKeepOriginalBox);
        }
        let adjacent = if from == *root { to } else { from };
        let inside = binPackIsDescendantOf(Some(&adjacent), Some(root), guard)?;
        if inside {
            // A route to a descendant may leave and re-enter the proposed
            // box even while its root endpoint remains attached.
            return Ok(routedContainerKeepOriginalBox);
        }
    }
    if rootEdges.len() != graphEdges.len() || !checkedEndpoint {
        return Ok(routedContainerKeepOriginalBox);
    }
    let routesPreserved = routedContainerRoutesStayInsideShrink(&affectedEdges, &orig, &proposed, guard)?;
    guard.Finish()?;
    if routesPreserved {
        return Ok(routedContainerUseProposedBox);
    }
    Ok(routedContainerKeepOriginalBox)
}

/// routedContainerRoutesStayInsideShrink proves that shrinking a routed
/// container does not discard any part of an affected route that was inside the
/// original outer box.
fn routedContainerRoutesStayInsideShrink(edges: &[EdgeRef], original: &rbox, proposed: &rbox, guard: &WorkGuardRef) -> GoResult<bool> {
    for edge in edges {
        guard.Step()?;
        let (points, isCurve) = {
            let e = edge.b();
            (e.Points.iter().map(|p| p.get()).collect::<Vec<geo::Point>>(), e.IsCurve)
        };
        if points.len() < 2 || isCurve {
            return Ok(false);
        }

        for point in &points {
            guard.Step()?;
            if !routedContainerPointIsFinite(point) {
                return Ok(false);
            }
        }
        let roundedCornersPreserved = routedContainerRoundedCornersStayInsideShrink(edge, &points, original, proposed, guard)?;
        if !roundedCornersPreserved {
            return Ok(false);
        }

        for index in 0..points.len() - 1 {
            guard.Step()?;
            let preserved = routedContainerSegmentStaysInsideShrink(original, proposed, &points[index], &points[index + 1], guard)?;
            if !preserved {
                return Ok(false);
            }
        }
    }
    guard.Finish()?;
    Ok(true)
}

/// Go `strconv.ParseFloat(s, 64)` for the decimal and special-value forms.
fn parseFloat(s: &str) -> Option<f64> {
    let t = s.strip_prefix('+').or_else(|| s.strip_prefix('-')).unwrap_or(s);
    let lower = t.to_ascii_lowercase();
    if lower == "inf" || lower == "infinity" || lower == "nan" {
        let v = if lower == "nan" { f64::NAN } else { f64::INFINITY };
        return Some(if s.starts_with('-') { -v } else { v });
    }
    if t.is_empty() || !t.bytes().all(|c| c.is_ascii_digit() || matches!(c, b'.' | b'e' | b'E' | b'+' | b'-')) {
        return None;
    }
    match s.parse::<f64>() {
        // Go reports out-of-range values as an error.
        Ok(v) if v.is_infinite() => None,
        Ok(v) => Some(v),
        Err(_) => None,
    }
}

/// routedContainerRoundedCornersStayInsideShrink accounts for the renderer's
/// rounded polyline corners.
fn routedContainerRoundedCornersStayInsideShrink(edge: &EdgeRef, points: &[geo::Point], original: &rbox, proposed: &rbox, guard: &WorkGuardRef) -> GoResult<bool> {
    let mut radius = routedContainerDefaultEdgeBorderRadius;
    let br = edge.b().Style.BorderRadius.clone();
    if let Some(br) = br {
        if br.Value.len() > routedContainerMaxBorderRadiusTextBytes {
            return Ok(false);
        }
        match parseFloat(&br.Value) {
            Some(v) => radius = v,
            None => return Ok(false),
        }
    }
    if !routedContainerCoordinateIsFinite(radius) || radius < 0.0 {
        return Ok(false);
    }

    for index in 1..points.len().saturating_sub(1) {
        guard.Step()?;
        let (previous, corner, next) = (points[index - 1], points[index], points[index + 1]);
        let (incomingX, incomingY) = (corner.X - previous.X, corner.Y - previous.Y);
        let (outgoingX, outgoingY) = (next.X - corner.X, next.Y - corner.Y);
        let incomingLength = math::Hypot(incomingX, incomingY);
        let outgoingLength = math::Hypot(outgoingX, outgoingY);
        if !routedContainerCoordinateIsFinite(incomingLength)
            || incomingLength <= 0.0
            || !routedContainerCoordinateIsFinite(outgoingLength)
            || outgoingLength <= 0.0
        {
            return Ok(false);
        }
        if radius == 0.0 {
            continue;
        }
        if incomingLength < radius || outgoingLength / 2.0 < radius {
            // Short corners use a different effective radius and may combine
            // controls across vertices. This proof only admits the normal branch.
            return Ok(false);
        }

        let entry = geo::Point { X: corner.X - incomingX / incomingLength * radius, Y: corner.Y - incomingY / incomingLength * radius };
        let exit = geo::Point { X: corner.X + outgoingX / outgoingLength * radius, Y: corner.Y + outgoingY / outgoingLength * radius };
        if !routedContainerPointIsFinite(&entry)
            || !routedContainerPointIsFinite(&exit)
            || !routedContainerControlHullIsPreserved(original, proposed, &[entry, corner, exit])
        {
            return Ok(false);
        }
    }
    Ok(true)
}

fn routedContainerControlHullIsPreserved(original: &rbox, proposed: &rbox, points: &[geo::Point]) -> bool {
    let mut allInsideProposed = true;
    let (mut allLeft, mut allTop, mut allRight, mut allBottom) = (true, true, true, true);
    let (originalLeft, originalTop) = (original.x, original.y);
    let originalRight = originalLeft + original.w;
    let originalBottom = originalTop + original.h;
    for point in points {
        if !routedContainerPointIsFinite(point) {
            return false;
        }
        allInsideProposed = allInsideProposed && proposed.contains(point);
        allLeft = allLeft && point.X < originalLeft;
        allTop = allTop && point.Y < originalTop;
        allRight = allRight && point.X > originalRight;
        allBottom = allBottom && point.Y > originalBottom;
    }
    allInsideProposed || allLeft || allTop || allRight || allBottom
}

/// routedContainerSegmentStaysInsideShrink clips a straight segment to the
/// closed original rectangle with Liang-Barsky.
fn routedContainerSegmentStaysInsideShrink(original: &rbox, proposed: &rbox, start: &geo::Point, end: &geo::Point, guard: &WorkGuardRef) -> GoResult<bool> {
    if !routedContainerRBoxIsFinite(original) || !routedContainerRBoxIsFinite(proposed) || !routedContainerPointIsFinite(start) || !routedContainerPointIsFinite(end) {
        return Ok(false);
    }
    if start.X == end.X && start.Y == end.Y {
        return Ok(false);
    }
    let (originalEnter, originalExit, originalIntersects, valid) = routedContainerSegmentBoxInterval(original, start, end, guard)?;
    if !valid {
        return Ok(false);
    }
    if !originalIntersects {
        return Ok(true);
    }
    let (proposedEnter, proposedExit, proposedIntersects, valid) = routedContainerSegmentBoxInterval(proposed, start, end, guard)?;
    if !valid {
        return Ok(false);
    }
    Ok(proposedIntersects && proposedEnter <= originalEnter && proposedExit >= originalExit)
}

fn routedContainerSegmentBoxInterval(bx: &rbox, start: &geo::Point, end: &geo::Point, guard: &WorkGuardRef) -> GoResult<(f64, f64, bool, bool)> {
    let (dx, dy) = (end.X - start.X, end.Y - start.Y);
    if !routedContainerCoordinateIsFinite(dx) || !routedContainerCoordinateIsFinite(dy) {
        return Ok((0.0, 0.0, false, false));
    }

    let (left, top) = (bx.x, bx.y);
    let (right, bottom) = (left + bx.w, top + bx.h);
    let constraints: [[f64; 2]; 4] = [[-dx, start.X - left], [dx, right - start.X], [-dy, start.Y - top], [dy, bottom - start.Y]];
    let (mut tEnter, mut tExit) = (0.0, 1.0);
    for constraint in &constraints {
        guard.Step()?;
        let (p, q) = (constraint[0], constraint[1]);
        if p == 0.0 {
            if q < 0.0 {
                return Ok((0.0, 0.0, false, true));
            }
            continue;
        }
        let ratio = q / p;
        if !routedContainerCoordinateIsFinite(ratio) {
            return Ok((0.0, 0.0, false, false));
        }
        if p < 0.0 {
            if ratio > tExit {
                return Ok((0.0, 0.0, false, true));
            }
            if ratio > tEnter {
                tEnter = ratio;
            }
        } else {
            if ratio < tEnter {
                return Ok((0.0, 0.0, false, true));
            }
            if ratio < tExit {
                tExit = ratio;
            }
        }
    }
    Ok((tEnter, tExit, true, true))
}

fn routedContainerEndpointKeepsSides(original: &rbox, proposed: &rbox, point: &geo::Point) -> bool {
    let originalSides = routedContainerPointSides(original, point);
    if originalSides == 0 {
        return false;
    }
    let (originalLeft, originalTop) = (original.x, original.y);
    let originalRight = originalLeft + original.w;
    let originalBottom = originalTop + original.h;
    let (proposedLeft, proposedTop) = (proposed.x, proposed.y);
    let proposedRight = proposedLeft + proposed.w;
    let proposedBottom = proposedTop + proposed.h;

    if originalSides & routedContainerTop != 0 && (proposedTop != originalTop || proposedLeft != originalLeft || proposedRight != originalRight) {
        return false;
    }
    if originalSides & routedContainerLeft != 0 && (proposedLeft != originalLeft || proposedTop != originalTop || proposedBottom != originalBottom) {
        return false;
    }
    if originalSides & routedContainerBottom != 0 && (proposedBottom != originalBottom || proposedLeft != originalLeft || proposedRight != originalRight) {
        return false;
    }
    if originalSides & routedContainerRight != 0 && (proposedRight != originalRight || proposedTop != originalTop || proposedBottom != originalBottom) {
        return false;
    }
    true
}

fn routedContainerPointSides(bx: &rbox, point: &geo::Point) -> routedContainerSide {
    if !routedContainerRBoxIsFinite(bx) || !routedContainerPointIsFinite(point) {
        return 0;
    }
    let (left, top) = (bx.x, bx.y);
    let (right, bottom) = (left + bx.w, top + bx.h);
    let mut sides: routedContainerSide = 0;
    if point.Y == top && point.X >= left && point.X <= right {
        sides |= routedContainerTop;
    }
    if point.X == left && point.Y >= top && point.Y <= bottom {
        sides |= routedContainerLeft;
    }
    if point.Y == bottom && point.X >= left && point.X <= right {
        sides |= routedContainerBottom;
    }
    if point.X == right && point.Y >= top && point.Y <= bottom {
        sides |= routedContainerRight;
    }
    sides
}

fn routedContainerPointIsFinite(point: &geo::Point) -> bool {
    routedContainerCoordinateIsFinite(point.X) && routedContainerCoordinateIsFinite(point.Y)
}

fn routedContainerBoxCovers(outer: &rbox, inner: &rbox) -> bool {
    let outerRight = outer.x + outer.w;
    let outerBottom = outer.y + outer.h;
    let innerRight = inner.x + inner.w;
    let innerBottom = inner.y + inner.h;
    inner.x >= outer.x && inner.y >= outer.y && innerRight <= outerRight && innerBottom <= outerBottom
}

fn routedContainerBoxIsFinite(bx: Option<&geo::Box>) -> bool {
    match bx.and_then(rbox::of) {
        Some(b) => routedContainerRBoxIsFinite(&b),
        None => false,
    }
}

fn routedContainerRBoxIsFinite(b: &rbox) -> bool {
    b.w > 0.0
        && b.h > 0.0
        && routedContainerCoordinateIsFinite(b.x)
        && routedContainerCoordinateIsFinite(b.y)
        && routedContainerCoordinateIsFinite(b.w)
        && routedContainerCoordinateIsFinite(b.h)
        && routedContainerCoordinateIsFinite(b.x + b.w)
        && routedContainerCoordinateIsFinite(b.y + b.h)
}

fn routedContainerCoordinateIsFinite(value: f64) -> bool {
    !value.is_nan() && !value.is_infinite()
}
