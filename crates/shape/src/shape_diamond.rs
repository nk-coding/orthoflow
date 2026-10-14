// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from lib/shape/shape_diamond.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;
use geo::{BoxRef, Intersectable, Point, PointRef};
use gocompat::math;
use std::rc::Rc;

pub struct shapeDiamond {
    pub(crate) b: Rc<baseShape>,
}

pub fn NewDiamond(box_: BoxRef) -> ShapeRc {
    Rc::new(shapeDiamond { b: baseShape::new(DIAMOND_TYPE, box_) })
}

pub(crate) fn diamondPath(box_: &geo::Box) -> svg::SvgPathContext {
    let mut pc = svg::NewSVGPathContext(box_.TL(), box_.Width / 77.0, box_.Height / 76.9);
    let p = pc.Absolute(38.5, 76.9);
    pc.StartAt(p);
    pc.C(true, -0.3, 0.0, -0.5, -0.1, -0.7, -0.3);
    pc.L(false, 0.3, 39.2);
    pc.C(true, -0.4, -0.4, -0.4, -1.0, 0.0, -1.4);
    pc.L(false, 37.8, 0.3);
    pc.C(true, 0.4, -0.4, 1.0, -0.4, 1.4, 0.0);
    pc.L(true, 37.5, 37.5);
    pc.C(true, 0.4, 0.4, 0.4, 1.0, 0.0, 1.4);
    pc.L(false, 39.2, 76.6);
    pc.C(false, 39.0, 76.8, 38.8, 76.9, 38.5, 76.9);
    pc.Z();
    pc
}

impl Shape for shapeDiamond {
    fn base(&self) -> &baseShape {
        &self.b
    }
    fn GetInnerBox(&self) -> BoxRef {
        let bx = self.b.Box.get();
        let mut width = bx.Width;
        let mut height = bx.Height;
        let tl = bx.TL().Copy();
        {
            let mut t = tl.bm();
            t.X += width / 4.0;
            t.Y += height / 4.0;
        }
        width /= 2.0;
        height /= 2.0;
        geo::NewBox(tl, width, height)
    }
    fn Perimeter(&self) -> Vec<Rc<dyn Intersectable>> {
        diamondPath(&self.b.Box.get()).Path
    }
    fn GetSVGPathData(&self) -> Vec<String> {
        vec![diamondPath(&self.b.Box.get()).PathData()]
    }
    fn GetPathCommands(&self) -> Option<Vec<Vec<svg::PathCommand>>> {
        Some(pathCommands(&[&diamondPath(&self.b.Box.get())]))
    }
    fn GetDimensionsToFit(&self, width: f64, height: f64, paddingX: f64, paddingY: f64) -> (f64, f64) {
        let totalWidth = 2.0 * (width + paddingX);
        let totalHeight = 2.0 * (height + paddingY);
        (math::Ceil(totalWidth), math::Ceil(totalHeight))
    }
    fn GetDefaultPadding(&self) -> (f64, f64) {
        (defaultPadding / 4.0, defaultPadding / 2.0)
    }
}
