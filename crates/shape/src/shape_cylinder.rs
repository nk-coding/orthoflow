// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from lib/shape/shape_cylinder.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;
use geo::{BoxRef, Intersectable, Point, PointRef};
use gocompat::math;
use std::rc::Rc;

pub struct shapeCylinder {
    pub(crate) b: Rc<baseShape>,
}

pub(crate) const defaultArcDepth: f64 = 24.0;

pub fn NewCylinder(box_: BoxRef) -> ShapeRc {
    Rc::new(shapeCylinder { b: baseShape::new(CYLINDER_TYPE, box_) })
}

pub(crate) fn getArcHeight(box_: &geo::Box) -> f64 {
    let mut arcHeight = defaultArcDepth;
    if box_.Height < arcHeight * 2.0 {
        arcHeight = box_.Height / 2.0;
    }
    arcHeight
}

pub(crate) fn cylinderOuterPath(box_: &geo::Box) -> svg::SvgPathContext {
    let arcHeight = getArcHeight(box_);
    let multiplier = 0.45;
    let mut pc = svg::NewSVGPathContext(box_.TL(), 1.0, 1.0);
    let p = pc.Absolute(0.0, arcHeight);
    pc.StartAt(p);
    pc.C(false, 0.0, 0.0, box_.Width * multiplier, 0.0, box_.Width / 2.0, 0.0);
    pc.C(false, box_.Width - box_.Width * multiplier, 0.0, box_.Width, 0.0, box_.Width, arcHeight);
    pc.V(true, box_.Height - arcHeight * 2.0);
    pc.C(false, box_.Width, box_.Height, box_.Width - box_.Width * multiplier, box_.Height, box_.Width / 2.0, box_.Height);
    pc.C(false, box_.Width * multiplier, box_.Height, 0.0, box_.Height, 0.0, box_.Height - arcHeight);
    pc.V(true, -(box_.Height - arcHeight * 2.0));
    pc.Z();
    pc
}

pub(crate) fn cylinderInnerPath(box_: &geo::Box) -> svg::SvgPathContext {
    let arcHeight = getArcHeight(box_);
    let multiplier = 0.45;
    let mut pc = svg::NewSVGPathContext(box_.TL(), 1.0, 1.0);
    let p = pc.Absolute(0.0, arcHeight);
    pc.StartAt(p);
    pc.C(false, 0.0, arcHeight * 2.0, box_.Width * multiplier, arcHeight * 2.0, box_.Width / 2.0, arcHeight * 2.0);
    pc.C(false, box_.Width - box_.Width * multiplier, arcHeight * 2.0, box_.Width, arcHeight * 2.0, box_.Width, arcHeight);
    pc
}

impl Shape for shapeCylinder {
    fn base(&self) -> &baseShape {
        &self.b
    }
    fn GetInnerBox(&self) -> BoxRef {
        let bx = self.b.Box.get();
        let mut height = bx.Height;
        let tl = bx.TL().Copy();
        let arc = getArcHeight(&bx);
        height -= 3.0 * arc;
        tl.bm().Y += 2.0 * arc;
        geo::NewBox(tl, bx.Width, height)
    }
    fn Perimeter(&self) -> Vec<Rc<dyn Intersectable>> {
        cylinderOuterPath(&self.b.Box.get()).Path
    }
    fn GetSVGPathData(&self) -> Vec<String> {
        let bx = self.b.Box.get();
        vec![cylinderOuterPath(&bx).PathData(), cylinderInnerPath(&bx).PathData()]
    }
    fn GetPathCommands(&self) -> Option<Vec<Vec<svg::PathCommand>>> {
        let bx = self.b.Box.get();
        Some(pathCommands(&[&cylinderOuterPath(&bx), &cylinderInnerPath(&bx)]))
    }
    fn GetDimensionsToFit(&self, width: f64, height: f64, paddingX: f64, paddingY: f64) -> (f64, f64) {
        let totalHeight = height + paddingY + 3.0 * defaultArcDepth;
        (math::Ceil(width + paddingX), math::Ceil(totalHeight))
    }
    fn GetDefaultPadding(&self) -> (f64, f64) {
        (defaultPadding, defaultPadding / 2.0)
    }
}
