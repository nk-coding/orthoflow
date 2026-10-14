// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from lib/shape/shape_circle.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;
use geo::{BoxRef, Intersectable, Point, PointRef};
use gocompat::math;
use std::rc::Rc;

pub struct shapeCircle {
    pub(crate) b: Rc<baseShape>,
}
pub fn NewCircle(box_: BoxRef) -> ShapeRc {
    Rc::new(shapeCircle { b: baseShape::new(CIRCLE_TYPE, box_) })
}
impl Shape for shapeCircle {
    fn base(&self) -> &baseShape {
        &self.b
    }
    fn GetInnerBox(&self) -> BoxRef {
        let bx = self.b.Box.get();
        let mut width = bx.Width;
        let mut height = bx.Height;
        let insideTL = self.GetInsidePlacement(width, height, 0.0, 0.0);
        let tl = bx.TL().get();
        width -= 2.0 * (insideTL.X - tl.X);
        height -= 2.0 * (insideTL.Y - tl.Y);
        geo::NewBox(PointRef::new(insideTL), width, height)
    }
    fn AspectRatio1(&self) -> bool {
        true
    }
    fn GetDimensionsToFit(&self, width: f64, height: f64, paddingX: f64, paddingY: f64) -> (f64, f64) {
        let length = math::Max(width + paddingX, height + paddingY);
        let diameter = math::Ceil(math::Sqrt2 * length);
        (diameter, diameter)
    }
    fn GetInsidePlacement(&self, _width: f64, _height: f64, paddingX: f64, paddingY: f64) -> Point {
        let gb = self.GetBox().get();
        let r = gb.Width / 2.0;
        let halfLength = r * math::Sqrt2 / 2.0;
        let tl = gb.TL().get();
        Point { X: tl.X + math::Ceil(r - halfLength + paddingX / 2.0), Y: tl.Y + math::Ceil(r - halfLength + paddingY / 2.0) }
    }
    fn Perimeter(&self) -> Vec<Rc<dyn Intersectable>> {
        let bx = self.b.Box.get();
        vec![Rc::new(geo::NewEllipse(bx.Center(), bx.Width / 2.0, bx.Height / 2.0))]
    }
    fn GetDefaultPadding(&self) -> (f64, f64) {
        // Go constant defaultPadding / math.Sqrt2 (exactly folded; differs from runtime division)
        let v = f64::from_bits(0x403c48c6001f0ac0);
        (v, v)
    }
}
