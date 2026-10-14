// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/engine/compound_routes.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

/// PreserveCompoundRoutes refines an already-selected compound drawing.
pub fn PreserveCompoundRoutes(ctx: &Ctx, before: &GraphRef, selected: &GraphRef) -> GoResult<GraphRef> {
    let candidate = layoutgraph::Clone(ctx, Some(selected))?;
    let selectedEdges = selected.edges();
    for (index, edge) in candidate.edges().iter().enumerate() {
        let curve = selectedEdges[index].b().IsCurve;
        edge.bm().IsCurve = curve;
    }
    preserveCompoundInteriors(ctx, before, &candidate)?;
    Ok(candidate)
}

fn copyLabel(l: &layoutgraph::LabelRef) -> layoutgraph::LabelRef {
    layoutgraph::LabelRef::new(l.b().clone())
}

pub(crate) fn preserveCompoundInteriors(ctx: &Ctx, before: &GraphRef, after: &GraphRef) -> GoResult<()> {
    let guard = limits::NewWorkGuard(ctx, "CompoundRoutes", limits::MaxEngineWorkUnits)?;
    let beforeNodes = before.nodes();
    for (index, node) in after.nodes().iter().enumerate() {
        let original = &beforeNodes[index];
        let (ol, oi) = {
            let o = original.b();
            (o.Label.clone(), o.Icon.clone())
        };
        if let Some(l) = ol {
            if l.PositionFixed() {
                node.bm().Label = Some(copyLabel(&l));
            }
        }
        if let Some(i) = oi {
            if i.PositionFixed() {
                node.bm().Icon = Some(layoutgraph::IconRef::new(i.b().clone()));
            }
        }
    }
    let beforeEdges = before.edges();
    for (index, edge) in after.edges().iter().enumerate() {
        if let Some(err) = ctx.Err() {
            return Err(err);
        }
        let original = &beforeEdges[index];
        let ol = original.b().Label.clone();
        if let Some(l) = &ol {
            if l.PositionFixed() {
                let pct = original.b().LabelPercentage;
                let mut e = edge.bm();
                e.Label = Some(copyLabel(l));
                e.LabelPercentage = pct;
            }
        }
        let enclosed = compoundEnclosedRoute(original, &guard)?;
        if enclosed {
            let (oldRoot, newRoot) = (compoundRoot(&original.from()), compoundRoot(&edge.from()));
            let (dx, dy) = (newRoot.x() - oldRoot.x(), newRoot.y() - oldRoot.y());
            let opoints = original.points();
            let mut points: Vec<geo::PointRef> = Vec::with_capacity(opoints.len());
            for point in &opoints {
                guard.Step()?;
                let p = *point.b();
                points.push(geo::NewPoint(p.X + dx, p.Y + dy));
            }
            let (curve, pct) = {
                let o = original.b();
                (o.IsCurve, o.LabelPercentage)
            };
            let mut e = edge.bm();
            e.Points = points;
            e.IsCurve = curve;
            e.LabelPercentage = pct;
            if let Some(l) = &ol {
                e.Label = Some(copyLabel(l));
            }
            continue;
        }
    }
    after.ResetPlacementCosts();
    labeling::Place(ctx, after)?;
    placement::Normalize(after);
    guard.Finish()
}

pub(crate) fn compoundRoot(node: &NodeRef) -> NodeRef {
    let mut node = node.clone();
    loop {
        let c = node.b().Container.clone();
        match c {
            Some(c) => node = c,
            None => return node,
        }
    }
}

pub(crate) fn compoundEnclosedRoute(edge: &EdgeRef, guard: &limits::WorkGuardRef) -> GoResult<bool> {
    let (from, to) = (edge.from(), edge.to());
    let root = compoundRoot(&from);
    let points = edge.points();
    if !root.IsContainer() || root != compoundRoot(&to) || points.len() < 2 {
        return Ok(false);
    }
    let (rx, ry, rw, rh) = (root.x(), root.y(), root.w(), root.h());
    let inside = |point: Option<geo::Point>, width: f64, height: f64| -> bool {
        let Some(p) = point else { return false };
        !p.X.is_nan() && !p.Y.is_nan() && p.X >= rx && p.Y >= ry && p.X + width <= rx + rw && p.Y + height <= ry + rh
    };
    for point in &points {
        guard.Step()?;
        if !inside(Some(*point.b()), 0.0, 0.0) {
            return Ok(false);
        }
    }
    let attached = |node: &NodeRef, point: &geo::Point| -> bool {
        node.ContainsPointOnBox(point)
            && (point.X == node.x() || point.X == node.x() + node.w() || point.Y == node.y() || point.Y == node.y() + node.h())
    };
    if !attached(&from, &points[0].b()) || !attached(&to, &points[points.len() - 1].b()) {
        return Ok(false);
    }
    let (lbl, sal, tal, sa, ta) = {
        let e = edge.b();
        (
            e.Label.clone(),
            e.SourceArrowheadLabel.clone(),
            e.TargetArrowheadLabel.clone(),
            e.SourceArrowhead.clone(),
            e.TargetArrowhead.clone(),
        )
    };
    if let Some(l) = &lbl {
        let (pos, w, h) = {
            let l = l.b();
            (l.Position, l.Width, l.Height)
        };
        if pos == label::Unset {
            return Ok(false);
        }
        guard.Add(points.len() as i64)?;
        let tl = edge.LabelTopLeft(pos, w, h).map(|p| *p.b());
        if !inside(tl, w, h) {
            return Ok(false);
        }
    }
    for (i, arrowLabel) in [sal, tal].iter().enumerate() {
        let Some(al) = arrowLabel else { continue };
        guard.Add(points.len() as i64)?;
        let (w, h) = {
            let a = al.b();
            (a.Width, a.Height)
        };
        let point = labelgeom::ArrowheadTopLeft(&points, i == 1, &sa, &ta, w, h);
        let pv = *point.b();
        if !inside(Some(pv), w, h) {
            return Ok(false);
        }
    }
    Ok(true)
}
