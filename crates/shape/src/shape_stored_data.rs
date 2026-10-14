// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from lib/shape/shape_stored_data.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;
use geo::{BoxRef, Intersectable, Point, PointRef};
use gocompat::math;
use std::rc::Rc;

pub struct shapeStoredData {
    pub(crate) b: Rc<baseShape>,
}

pub(crate) const storedDataWedgeWidth: f64 = 15.0;

pub fn NewStoredData(box_: BoxRef) -> ShapeRc {
    Rc::new(shapeStoredData { b: baseShape::new(STORED_DATA_TYPE, box_) })
}

pub(crate) fn storedDataPath(box_: &geo::Box) -> svg::SvgPathContext {
    let mut wedgeWidth = storedDataWedgeWidth;
    let multiplier = 0.27;
    if box_.Width < wedgeWidth * 2.0 {
        wedgeWidth = box_.Width / 2.0;
    }
    let mut pc = svg::NewSVGPathContext(box_.TL(), 1.0, 1.0);
    let p = pc.Absolute(wedgeWidth, 0.0);
    pc.StartAt(p);
    pc.H(true, box_.Width - wedgeWidth);
    pc.C(false, box_.Width - wedgeWidth * multiplier, 0.0, box_.Width - wedgeWidth, box_.Height * multiplier, box_.Width - wedgeWidth, box_.Height / 2.0);
    pc.C(false, box_.Width - wedgeWidth, box_.Height - box_.Height * multiplier, box_.Width - wedgeWidth * multiplier, box_.Height, box_.Width, box_.Height);
    pc.H(true, -(box_.Width - wedgeWidth));
    pc.C(false, wedgeWidth - wedgeWidth * multiplier, box_.Height, 0.0, box_.Height - box_.Height * multiplier, 0.0, box_.Height / 2.0);
    pc.C(false, 0.0, box_.Height * multiplier, wedgeWidth - wedgeWidth * multiplier, 0.0, wedgeWidth, 0.0);
    pc.Z();
    pc
}

impl Shape for shapeStoredData {
    fn base(&self) -> &baseShape {
        &self.b
    }
    fn GetInnerBox(&self) -> BoxRef {
        let bx = self.b.Box.get();
        let mut width = bx.Width;
        let tl = bx.TL().Copy();
        width -= 2.0 * storedDataWedgeWidth;
        tl.bm().X += storedDataWedgeWidth;
        geo::NewBox(tl, width, bx.Height)
    }
    fn Perimeter(&self) -> Vec<Rc<dyn Intersectable>> {
        storedDataPath(&self.b.Box.get()).Path
    }
    fn GetSVGPathData(&self) -> Vec<String> {
        vec![storedDataPath(&self.b.Box.get()).PathData()]
    }
    fn GetPathCommands(&self) -> Option<Vec<Vec<svg::PathCommand>>> {
        Some(pathCommands(&[&storedDataPath(&self.b.Box.get())]))
    }
    fn GetDimensionsToFit(&self, width: f64, height: f64, paddingX: f64, paddingY: f64) -> (f64, f64) {
        let totalWidth = width + paddingX + 2.0 * storedDataWedgeWidth;
        (math::Ceil(totalWidth), math::Ceil(height + paddingY))
    }
    fn GetDefaultPadding(&self) -> (f64, f64) {
        (defaultPadding - 10.0, defaultPadding)
    }
}
