// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/quality/inspection_geometry.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

fn isRectangular(n: &NodeRef) -> bool {
    n.shape().IsRectangular()
}

/// measureGeometry counts occupied rectangular interiors.
pub(crate) fn measureGeometry(g: &GraphRef, score: &mut Metrics, guard: &WorkGuardRef) -> GoResult<()> {
    let (gnodes, gedges) = {
        let gb = g.b();
        (gb.Nodes.clone(), gb.Edges.clone())
    };
    for (i, node) in gnodes.iter().enumerate() {
        guard.Step()?;
        if node.b().IsInvisible || !isRectangular(node) {
            continue;
        }
        let nb = qbox::node(node);
        for other in &gnodes[i + 1..] {
            guard.Step()?;
            if other.b().IsInvisible || !isRectangular(other) || !boxesOverlapWithPadding(nb, qbox::node(other), 0.0) {
                continue;
            }
            let related = relatedNodes(node, other, guard)?;
            if !related {
                score.NodeOverlaps += 1;
            }
        }
        for edge in &gedges {
            guard.Step()?;
            if edge.b().IsInvisible || edge.b().IsCurve {
                continue;
            }
            let ancestor = endpointAncestor(edge, node, guard)?;
            let (from, to) = (edge.from(), edge.to());
            if ancestor && from != *node && to != *node {
                continue;
            }
            let points: Vec<geo::Point> = edge.b().Points.iter().map(|p| *p.b()).collect();
            for i in 1..points.len() {
                guard.Step()?;
                if i == 1 && from == *node || i == points.len() - 1 && to == *node {
                    continue;
                }
                if segmentEntersBox(nb, &points[i - 1], &points[i]) {
                    score.RouteObstructions += 1;
                    break;
                }
            }
        }
    }
    Ok(())
}

pub(crate) fn relatedNodes(a: &NodeRef, b: &NodeRef, guard: &WorkGuardRef) -> GoResult<bool> {
    let yes = evaluationIsDescendantOf(Some(a), Some(b), guard)?;
    if yes {
        return Ok(yes);
    }
    evaluationIsDescendantOf(Some(b), Some(a), guard)
}

pub(crate) fn endpointAncestor(edge: &EdgeRef, node: &NodeRef, guard: &WorkGuardRef) -> GoResult<bool> {
    let yes = evaluationIsDescendantOf(Some(&edge.from()), Some(node), guard)?;
    if yes {
        return Ok(yes);
    }
    evaluationIsDescendantOf(Some(&edge.to()), Some(node), guard)
}

/// An open-rectangle slab intersection.
pub(crate) fn segmentEntersBox(bx: qbox, a: &geo::Point, b: &geo::Point) -> bool {
    if bx.Width <= 0.0 || bx.Height <= 0.0 || a.X == b.X && a.Y == b.Y {
        return false;
    }
    let (mut low, mut high) = (0.0, 1.0);
    for axis in [[a.X, b.X - a.X, bx.X, bx.X + bx.Width], [a.Y, b.Y - a.Y, bx.Y, bx.Y + bx.Height]] {
        let (start, delta, minValue, maxValue) = (axis[0], axis[1], axis[2], axis[3]);
        if delta == 0.0 {
            if start <= minValue || start >= maxValue {
                return false;
            }
            continue;
        }
        let (mut first, mut last) = ((minValue - start) / delta, (maxValue - start) / delta);
        if first > last {
            std::mem::swap(&mut first, &mut last);
        }
        low = math::Max(low, first);
        high = math::Min(high, last);
        if low >= high {
            return false;
        }
    }
    low < high
}
