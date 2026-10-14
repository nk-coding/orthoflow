// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from lib/shape/shape_step.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;
use geo::{BoxRef, Intersectable, Point, PointRef};
use gocompat::math;
use std::rc::Rc;

pub struct shapeStep {
    pub(crate) b: Rc<baseShape>,
}

pub fn NewStep(box_: BoxRef) -> ShapeRc {
    Rc::new(shapeStep { b: baseShape::new(STEP_TYPE, box_) })
}

pub const STEP_WEDGE_WIDTH: f64 = 35.0;

pub(crate) fn stepPath(box_: &geo::Box) -> svg::SvgPathContext {
    let mut wedgeWidth = STEP_WEDGE_WIDTH;
    if box_.Width <= wedgeWidth {
        wedgeWidth = box_.Width / 2.0;
    }
    let mut pc = svg::NewSVGPathContext(box_.TL(), 1.0, 1.0);
    let p = pc.Absolute(0.0, 0.0);
    pc.StartAt(p);
    pc.L(false, box_.Width - wedgeWidth, 0.0);
    pc.L(false, box_.Width, box_.Height / 2.0);
    pc.L(false, box_.Width - wedgeWidth, box_.Height);
    pc.L(false, 0.0, box_.Height);
    pc.L(false, wedgeWidth, box_.Height / 2.0);
    pc.Z();
    pc
}

impl Shape for shapeStep {
    fn base(&self) -> &baseShape {
        &self.b
    }
    fn GetInnerBox(&self) -> BoxRef {
        let bx = self.b.Box.get();
        let mut width = bx.Width;
        let tl = bx.TL().Copy();
        width -= 2.0 * STEP_WEDGE_WIDTH;
        tl.bm().X += STEP_WEDGE_WIDTH;
        geo::NewBox(tl, width, bx.Height)
    }
    fn Perimeter(&self) -> Vec<Rc<dyn Intersectable>> {
        stepPath(&self.b.Box.get()).Path
    }
    fn GetSVGPathData(&self) -> Vec<String> {
        vec![stepPath(&self.b.Box.get()).PathData()]
    }
    fn GetPathCommands(&self) -> Option<Vec<Vec<svg::PathCommand>>> {
        Some(pathCommands(&[&stepPath(&self.b.Box.get())]))
    }
    fn GetDimensionsToFit(&self, width: f64, height: f64, paddingX: f64, paddingY: f64) -> (f64, f64) {
        let totalWidth = width + paddingX + 2.0 * STEP_WEDGE_WIDTH;
        (math::Ceil(totalWidth), math::Ceil(height + paddingY))
    }
    fn GetDefaultPadding(&self) -> (f64, f64) {
        (defaultPadding / 4.0, defaultPadding + STEP_WEDGE_WIDTH)
    }
}
