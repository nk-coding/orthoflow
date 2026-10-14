// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/routing/edge_simplify.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of routing/edge_simplify.go.
#[allow(unused_imports)]
use crate::*;
use gocompat::context::Ctx;
use gocompat::math;
use gocompat::GoResult;
use layoutgraph::{EdgeRef, GraphRef, NodeRef};

// simplifyEdgeRoutes post-processes edge routes to remove unnecessary bending caused by the constraint that routes have to go through OVG nodes
// It identifies unnecessary bends in routes and eliminates them when possible.
pub fn SimplifyEdgeRoutes(ctx: &Ctx, g: &GraphRef) -> GoResult<()> {
    simplifyEdgeRoutesWithLimit(ctx, g, maxRouteStageWorkUnits)
}

pub(crate) fn simplifyEdgeRoutesWithLimit(ctx: &Ctx, g: &GraphRef, workLimit: u64) -> GoResult<()> {
    runAtomicRouteStage(ctx, "SimplifyEdgeRoutes", g, &[], workLimit, |guard| {
        for edge in g.edges() {
            guard.step()?;
            let mut changed = true;
            while changed {
                let newPoints = simplifyPoints(g, &edge, guard)?;
                changed = newPoints.len() < edge.b().Points.len();
                if changed {
                    edge.bm().Points = newPoints;
                }
            }
        }
        Ok(())
    })
}

pub(crate) fn simplifyPoints(g: &GraphRef, edge: &EdgeRef, guard: &routeWorkGuardRef) -> GoResult<Vec<geo::PointRef>> {
    let points = edge.points();

    if points.len() < 5 {
        guard.step()?;
        return Ok(points);
    }

    // Make a copy of the points to avoid modifying the original
    let mut result: Vec<geo::PointRef> = vec![points[0].clone()];
    let mut i = 0usize;

    // We want to find these and just simplify to L-shape
    while i + 4 < points.len() {
        guard.step()?;
        let p1 = *points[i].b();
        let p2 = *points[i + 1].b();
        let p3 = *points[i + 2].b();
        let p4 = *points[i + 3].b();
        let p5 = *points[i + 4].b();

        let (vec1X, vec1Y) = (p2.X - p1.X, p2.Y - p1.Y); // p1→p2
        let (vec2X, vec2Y) = (p3.X - p2.X, p3.Y - p2.Y); // p2→p3
        let (vec3X, vec3Y) = (p4.X - p3.X, p4.Y - p3.Y); // p3→p4
        let (vec4X, vec4Y) = (p5.X - p4.X, p5.Y - p4.Y); // p4→p5

        let isHorizontal1 = isHorizontalSegment(&p1, &p2);
        let isVertical1 = isVerticalSegment(&p1, &p2);
        let isHorizontal2 = isHorizontalSegment(&p2, &p3);
        let isVertical2 = isVerticalSegment(&p2, &p3);
        let isHorizontal3 = isHorizontalSegment(&p3, &p4);
        let isVertical3 = isVerticalSegment(&p3, &p4);
        let isHorizontal4 = isHorizontalSegment(&p4, &p5);
        let isVertical4 = isVerticalSegment(&p4, &p5);

        let isOppositeVerticals = isVertical2 && isVertical4 && ((vec2Y > 0.0 && vec4Y < 0.0) || (vec2Y < 0.0 && vec4Y > 0.0));

        let isSameHorizontals = isHorizontal1 && isHorizontal3 && ((vec1X > 0.0 && vec3X > 0.0) || (vec1X < 0.0 && vec3X < 0.0));

        let isOppositeHorizontals =
            isHorizontal2 && isHorizontal4 && ((vec2X > 0.0 && vec4X < 0.0) || (vec2X < 0.0 && vec4X > 0.0));

        let isSameVerticals = isVertical1 && isVertical3 && ((vec1Y > 0.0 && vec3Y > 0.0) || (vec1Y < 0.0 && vec3Y < 0.0));

        let patternFound = (isOppositeVerticals && isSameHorizontals) || (isOppositeHorizontals && isSameVerticals);

        let mut intersection: Option<geo::Point> = None;
        let mut hasObstruction = false;

        if patternFound {
            // Find the intersection point between the first segment (p1→p2) and fourth segment (p4→p5)
            intersection = findIntersectionPoint(&p1, &p2, &p4, &p5);

            // Both emitted legs must be legal.
            hasObstruction = intersection.is_none();
            if let Some(inter) = intersection {
                let (from, to) = (edge.from(), edge.to());
                if i == 0 && from.ContainsPoint(&p1, 1.0) && !sameRouteDirection(&p1, &p2, &p1, &inter) {
                    hasObstruction = true;
                }
                if i + 4 == points.len() - 1 && to.ContainsPoint(&p5, 1.0) && !sameRouteDirection(&p4, &p5, &inter, &p5) {
                    hasObstruction = true;
                }
                for leg in [[p1, inter], [inter, p5]] {
                    let blocked = lineIntersectsUnrelatedNode(g, edge, &leg[0], &leg[1], guard)?;
                    hasObstruction = hasObstruction || blocked;
                    let blocked = lineIntersectsOtherEdges(g, edge, &leg[0], &leg[1], guard)?;
                    hasObstruction = hasObstruction || blocked;
                }
            }
        }

        let canBeSimplified = patternFound && !hasObstruction;

        if canBeSimplified {
            // For a simplified route, we want: p1 → intersection → p5
            result.push(geo::PointRef::new(intersection.unwrap()));
            result.push(points[i + 4].clone());

            i += 4;

            // Assume 1 simplification per edge
            break;
        } else {
            result.push(points[i + 1].clone());
            i += 1;
        }
    }

    // Add remaining points, making sure we don't go out of bounds
    let remainingStart = i + 1;
    if remainingStart < points.len() {
        for j in remainingStart..points.len() {
            guard.step()?;
            result.push(points[j].clone());
        }
    }

    Ok(result)
}

// findIntersectionPoint calculates the intersection point of the lines p1→p2 and p3→p4
pub(crate) fn findIntersectionPoint(p1: &geo::Point, p2: &geo::Point, p3: &geo::Point, p4: &geo::Point) -> Option<geo::Point> {
    if isHorizontalSegment(p1, p2) && isVerticalSegment(p3, p4) {
        return Some(geo::Point { X: p4.X, Y: p1.Y });
    } else if isVerticalSegment(p1, p2) && isHorizontalSegment(p3, p4) {
        return Some(geo::Point { X: p1.X, Y: p4.Y });
    }
    None
}

#[inline]
pub(crate) fn isHorizontalSegment(p1: &geo::Point, p2: &geo::Point) -> bool {
    p1.Y == p2.Y
}

#[inline]
pub(crate) fn isVerticalSegment(p1: &geo::Point, p2: &geo::Point) -> bool {
    p1.X == p2.X
}

// sameRouteDirection preserves the signed departure/approach direction, not
// merely the axis. A zero-length replacement cannot preserve a port approach.
pub(crate) fn sameRouteDirection(a: &geo::Point, b: &geo::Point, c: &geo::Point, d: &geo::Point) -> bool {
    (b.X - a.X) * (d.X - c.X) + (b.Y - a.Y) * (d.Y - c.Y) > 0.0 && (b.X - a.X) * (d.Y - c.Y) == (b.Y - a.Y) * (d.X - c.X)
}

// lineIntersectsUnrelatedNode checks every obstacle's open interior.
pub(crate) fn lineIntersectsUnrelatedNode(
    g: &GraphRef,
    edge: &EdgeRef,
    p1: &geo::Point,
    p2: &geo::Point,
    guard: &routeWorkGuardRef,
) -> GoResult<bool> {
    let (from, to) = (edge.from(), edge.to());
    for node in g.nodes() {
        guard.step()?;
        let Some(tl) = node.tl() else { continue };
        let (w, h) = (node.w(), node.h());
        if w <= 0.0 || h <= 0.0 {
            continue;
        }
        if node != from && node != to && (from.IsDescendantOf(Some(&node)) || to.IsDescendantOf(Some(&node))) {
            continue;
        }
        if orthogonalSegmentEntersBox(&tl.get(), w, h, p1, p2) {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(crate) fn orthogonalSegmentEntersNode(node: &NodeRef, a: &geo::Point, b: &geo::Point) -> bool {
    orthogonalSegmentEntersBox(&node.tlv(), node.w(), node.h(), a, b)
}

/// orthogonalSegmentEntersNode on the node's (or a fake label node's) box.
pub(crate) fn orthogonalSegmentEntersBox(tl: &geo::Point, width: f64, height: f64, a: &geo::Point, b: &geo::Point) -> bool {
    const epsilon: f64 = 1e-6;
    let (left, right) = (tl.X + epsilon, tl.X + width - epsilon);
    let (top, bottom) = (tl.Y + epsilon, tl.Y + height - epsilon);
    if a.Y == b.Y {
        return top < a.Y && a.Y < bottom && math::BuiltinMin(a.X, b.X) < right && math::BuiltinMax(a.X, b.X) > left;
    }
    if a.X == b.X {
        return left < a.X && a.X < right && math::BuiltinMin(a.Y, b.Y) < bottom && math::BuiltinMax(a.Y, b.Y) > top;
    }
    true
}

// lineIntersectsOtherEdges checks if the line from p1 to p2 intersects with any
// other edges in the graph (excluding the edge being simplified)
pub(crate) fn lineIntersectsOtherEdges(
    g: &GraphRef,
    currentEdge: &EdgeRef,
    p1: &geo::Point,
    p2: &geo::Point,
    guard: &routeWorkGuardRef,
) -> GoResult<bool> {
    for edge in g.edges() {
        guard.step()?;
        if edge == *currentEdge {
            continue;
        }
        let points = edge.points();
        if points.len() < 2 {
            continue;
        }
        for i in 0..points.len() - 1 {
            guard.step()?;
            if intersects(p1, p2, &points[i].get(), &points[i + 1].get()) {
                return Ok(true);
            }
        }
    }
    Ok(false)
}
