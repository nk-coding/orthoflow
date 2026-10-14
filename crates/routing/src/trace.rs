// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/routing/trace.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of routing/trace.go.
#[allow(unused_imports)]
use crate::*;
use gocompat::context::Ctx;
use gocompat::GoResult;
use layoutgraph::{EdgeRef, GraphRef};

// TraceEdgesToShapeBorder trims every route endpoint to the rendered shape.
pub fn TraceEdgesToShapeBorder(ctx: &Ctx, graph: &GraphRef) -> GoResult<()> {
    traceEdgesToShapeBorderWithWorkLimit(ctx, graph, maxRouteStageWorkUnits)
}

pub(crate) fn traceEdgesToShapeBorderWithWorkLimit(ctx: &Ctx, graph: &GraphRef, workLimit: u64) -> GoResult<()> {
    runAtomicRouteStage(ctx, "TraceEdgesToShapeBorder", graph, &[], workLimit, |guard| {
        for edge in graph.edges() {
            traceToShapeBorderGuarded(Some(&edge), guard)?;
        }
        Ok(())
    })
}

pub(crate) fn traceToShapeBorder(edge: &EdgeRef) {
    if edge.b().Points.len() < 2 {
        return;
    }
    let (fromNode, toNode) = (edge.from(), edge.to());
    // (pointer, original value) of each node TopLeft that is moved temporarily.
    let mut originalFrom: Option<(geo::PointRef, geo::Point)> = None;
    let mut originalTo: Option<(geo::PointRef, geo::Point)> = None;

    // If an edge passes through a 3D/multiple modifier, use the offset box for
    // tracing to the rendered border.
    let (dx, dy) = fromNode.ModifierElementAdjustments();
    if dx != 0.0 || dy != 0.0 {
        let points = edge.points();
        let start = points[0].clone();
        let tl = fromNode.tl().expect("nil pointer dereference");
        let (fw, fh) = (fromNode.w(), fromNode.h());
        if start.X() > tl.X() + dx && start.Y() < tl.Y() + fh - dy {
            let originalFromTopLeft = *tl.b();
            originalFrom = Some((tl.clone(), originalFromTopLeft));
            let orientation = fromNode.PortOrientation(&start.get());

            // If connected to the top or right side, move the segment back
            // before tracing to the border.
            let next = points[1].clone();
            if orientation == geo::Right || start.X() == originalFromTopLeft.X + fw {
                tl.bm().X += dx;
                let topRight = tl.AddVector(&[fw, 0.0]);
                let bottomRight = topRight.AddVector(&[0.0, fh]);
                if let Some(newStart) = geo::IntersectionPoint(&start, &next, &topRight, &bottomRight) {
                    let ns = *newStart.b();
                    let mut s = start.bm();
                    s.X = ns.X;
                    s.Y = ns.Y;
                }
            } else if orientation == geo::Top || start.Y() == originalFromTopLeft.Y {
                tl.bm().Y -= dy;
                let topRight = tl.AddVector(&[fw, 0.0]);
                if let Some(newStart) = geo::IntersectionPoint(&start, &next, &tl, &topRight) {
                    let ns = *newStart.b();
                    let mut s = start.bm();
                    s.X = ns.X;
                    s.Y = ns.Y;
                }
            }
        }
    }
    let (dx, dy) = toNode.ModifierElementAdjustments();
    if dx != 0.0 || dy != 0.0 {
        let points = edge.points();
        let end = points[points.len() - 1].clone();
        let tl = toNode.tl().expect("nil pointer dereference");
        let (tw, th) = (toNode.w(), toNode.h());
        if end.X() > tl.X() + dx && end.Y() < tl.Y() + th - dy {
            let originalToTopLeft = *tl.b();
            originalTo = Some((tl.clone(), originalToTopLeft));
            let orientation = toNode.PortOrientation(&end.get());

            let previous = points[points.len() - 2].clone();
            if orientation == geo::Right || end.X() == originalToTopLeft.X + tw {
                tl.bm().X += dx;
                let topRight = tl.AddVector(&[tw, 0.0]);
                let bottomRight = topRight.AddVector(&[0.0, th]);
                if let Some(newEnd) = geo::IntersectionPoint(&end, &previous, &topRight, &bottomRight) {
                    let ne = *newEnd.b();
                    let mut e = end.bm();
                    e.X = ne.X;
                    e.Y = ne.Y;
                }
            } else if orientation == geo::Top || end.Y() == originalToTopLeft.Y {
                tl.bm().Y -= dy;
                let topRight = tl.AddVector(&[tw, 0.0]);
                if let Some(newEnd) = geo::IntersectionPoint(&end, &previous, &tl, &topRight) {
                    let ne = *newEnd.b();
                    let mut e = end.bm();
                    e.X = ne.X;
                    e.Y = ne.Y;
                }
            }
        }
    }

    let fromShape = fromNode.shape();
    let (p0, p1) = {
        let e = edge.b();
        (e.Points[0].clone(), e.Points[1].clone())
    };
    let borderPoint = shape::TraceToShapeBorder(&*fromShape, &p0, &p1);
    edge.bm().Points[0] = borderPoint;
    let toShape = toNode.shape();
    let (pl, pl2) = {
        let e = edge.b();
        let n = e.Points.len();
        (e.Points[n - 1].clone(), e.Points[n - 2].clone())
    };
    let borderPoint = shape::TraceToShapeBorder(&*toShape, &pl, &pl2);
    {
        let mut e = edge.bm();
        let n = e.Points.len();
        e.Points[n - 1] = borderPoint;
    }

    // Go's deferred restore, in reverse mutation order in case both endpoints
    // share a node.
    if let Some((pointer, value)) = originalTo {
        pointer.set(value);
        toNode.set_tl(Some(pointer));
    }
    if let Some((pointer, value)) = originalFrom {
        pointer.set(value);
        fromNode.set_tl(Some(pointer));
    }
}
