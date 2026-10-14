// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from lib/shape/shape_real_square.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;
use geo::{BoxRef, Intersectable, Point, PointRef};
use gocompat::math;
use std::rc::Rc;

pub struct shapeRealSquare {
    pub(crate) b: Rc<baseShape>,
}
pub fn NewRealSquare(box_: BoxRef) -> ShapeRc {
    Rc::new(shapeRealSquare { b: baseShape::new(REAL_SQUARE_TYPE, box_) })
}
impl Shape for shapeRealSquare {
    fn base(&self) -> &baseShape {
        &self.b
    }
    fn AspectRatio1(&self) -> bool {
        true
    }
    fn IsRectangular(&self) -> bool {
        true
    }
    fn GetDimensionsToFit(&self, width: f64, height: f64, paddingX: f64, paddingY: f64) -> (f64, f64) {
        let sideLength = math::Ceil(math::Max(width + paddingX, height + paddingY));
        (sideLength, sideLength)
    }
}
