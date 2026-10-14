// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from lib/shape/shape_parallelogram.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;
use geo::{BoxRef, Intersectable, Point, PointRef};
use gocompat::math;
use std::rc::Rc;

pub struct shapeParallelogram {
    pub(crate) b: Rc<baseShape>,
}

pub(crate) const parallelWedgeWidth: f64 = 26.0;

pub fn NewParallelogram(box_: BoxRef) -> ShapeRc {
    Rc::new(shapeParallelogram { b: baseShape::new(PARALLELOGRAM_TYPE, box_) })
}

pub(crate) fn parallelogramPath(box_: &geo::Box) -> svg::SvgPathContext {
    let mut wedgeWidth = parallelWedgeWidth;
    if box_.Width <= wedgeWidth {
        wedgeWidth = box_.Width / 2.0;
    }
    let mut pc = svg::NewSVGPathContext(box_.TL(), 1.0, 1.0);
    let p = pc.Absolute(wedgeWidth, 0.0);
    pc.StartAt(p);
    pc.L(false, box_.Width, 0.0);
    pc.L(false, box_.Width - wedgeWidth, box_.Height);
    pc.L(false, 0.0, box_.Height);
    pc.L(false, 0.0, box_.Height);
    pc.Z();
    pc
}

impl Shape for shapeParallelogram {
    fn base(&self) -> &baseShape {
        &self.b
    }
    fn GetInnerBox(&self) -> BoxRef {
        let bx = self.b.Box.get();
        let tl = bx.TL().Copy();
        let width = bx.Width - 2.0 * parallelWedgeWidth;
        tl.bm().X += parallelWedgeWidth;
        geo::NewBox(tl, width, bx.Height)
    }
    fn Perimeter(&self) -> Vec<Rc<dyn Intersectable>> {
        parallelogramPath(&self.b.Box.get()).Path
    }
    fn GetSVGPathData(&self) -> Vec<String> {
        vec![parallelogramPath(&self.b.Box.get()).PathData()]
    }
    fn GetPathCommands(&self) -> Option<Vec<Vec<svg::PathCommand>>> {
        Some(pathCommands(&[&parallelogramPath(&self.b.Box.get())]))
    }
    fn GetDimensionsToFit(&self, width: f64, height: f64, paddingX: f64, paddingY: f64) -> (f64, f64) {
        let totalWidth = width + paddingX + parallelWedgeWidth * 2.0;
        (math::Ceil(totalWidth), math::Ceil(height + paddingY))
    }
}
