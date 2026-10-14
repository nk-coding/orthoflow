// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from lib/shape/shape_oval.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;
use geo::{BoxRef, Intersectable, Point, PointRef};
use gocompat::math;
use std::rc::Rc;

pub const OVAL_AR_LIMIT: f64 = 3.0;

pub struct shapeOval {
    pub(crate) b: Rc<baseShape>,
}
pub fn NewOval(box_: BoxRef) -> ShapeRc {
    Rc::new(shapeOval { b: baseShape::new(OVAL_TYPE, box_) })
}
impl Shape for shapeOval {
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
    fn GetDimensionsToFit(&self, width: f64, height: f64, paddingX: f64, paddingY: f64) -> (f64, f64) {
        let theta = math::Atan2(height, width) as f32 as f64;
        let paddedWidth = width + paddingX * math::Cos(theta);
        let paddedHeight = height + paddingY * math::Sin(theta);
        let (totalWidth, totalHeight) = (math::Ceil(math::Sqrt2 * paddedWidth), math::Ceil(math::Sqrt2 * paddedHeight));
        LimitAR(totalWidth, totalHeight, OVAL_AR_LIMIT)
    }
    fn GetInsidePlacement(&self, _width: f64, _height: f64, paddingX: f64, paddingY: f64) -> Point {
        let bx = self.b.Box.get();
        let rx = bx.Width / 2.0;
        let ry = bx.Height / 2.0;
        let theta = math::Atan2(ry, rx) as f32 as f64;
        let sin = math::Sin(theta);
        let cos = math::Cos(theta);
        let r = rx * ry / math::Sqrt(math::Pow(rx * sin, 2.0) + math::Pow(ry * cos, 2.0));
        let tl = bx.TL().get();
        Point::new(tl.X + math::Ceil(rx - cos * (r - paddingX / 2.0)), tl.Y + math::Ceil(ry - sin * (r - paddingY / 2.0)))
    }
    fn Perimeter(&self) -> Vec<Rc<dyn Intersectable>> {
        let bx = self.b.Box.get();
        vec![Rc::new(geo::NewEllipse(bx.Center(), bx.Width / 2.0, bx.Height / 2.0))]
    }
}
