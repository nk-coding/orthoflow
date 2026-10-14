// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from lib/shape/shape_hexagon.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;
use geo::{BoxRef, Intersectable, Point, PointRef};
use gocompat::math;
use std::rc::Rc;

pub struct shapeHexagon {
    pub(crate) b: Rc<baseShape>,
}

pub fn NewHexagon(box_: BoxRef) -> ShapeRc {
    Rc::new(shapeHexagon { b: baseShape::new(HEXAGON_TYPE, box_) })
}

pub(crate) fn hexagonPath(box_: &geo::Box) -> svg::SvgPathContext {
    // Go constant 43.6 / 87.3 (folded exactly; differs from runtime division)
    let halfYFactor = f64::from_bits(0x3fdff69dc3979b20);
    let mut pc = svg::NewSVGPathContext(box_.TL(), box_.Width, box_.Height);
    let p = pc.Absolute(0.25, 0.0);
    pc.StartAt(p);
    pc.L(false, 0.0, halfYFactor);
    pc.L(false, 0.25, 1.0);
    pc.L(false, 0.75, 1.0);
    pc.L(false, 1.0, halfYFactor);
    pc.L(false, 0.75, 0.0);
    pc.Z();
    pc
}

impl Shape for shapeHexagon {
    fn base(&self) -> &baseShape {
        &self.b
    }
    fn GetInnerBox(&self) -> BoxRef {
        let bx = self.b.Box.get();
        let mut width = bx.Width;
        let mut height = bx.Height;
        let tl = bx.TL().Copy();
        tl.bm().X += width / 6.0;
        width /= 1.5;
        tl.bm().Y += height / 6.0;
        height /= 1.5;
        geo::NewBox(tl, width, height)
    }
    fn Perimeter(&self) -> Vec<Rc<dyn Intersectable>> {
        hexagonPath(&self.b.Box.get()).Path
    }
    fn GetSVGPathData(&self) -> Vec<String> {
        vec![hexagonPath(&self.b.Box.get()).PathData()]
    }
    fn GetPathCommands(&self) -> Option<Vec<Vec<svg::PathCommand>>> {
        Some(pathCommands(&[&hexagonPath(&self.b.Box.get())]))
    }
    fn GetDimensionsToFit(&self, width: f64, height: f64, paddingX: f64, paddingY: f64) -> (f64, f64) {
        let totalWidth = 1.5 * (width + paddingX);
        let totalHeight = 1.5 * (height + paddingY);
        (math::Ceil(totalWidth), math::Ceil(totalHeight))
    }
    fn GetDefaultPadding(&self) -> (f64, f64) {
        (defaultPadding / 2.0, defaultPadding / 2.0)
    }
}
