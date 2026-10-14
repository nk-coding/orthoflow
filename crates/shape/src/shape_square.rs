// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from lib/shape/shape_square.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;
use geo::{BoxRef, Intersectable, Point, PointRef};
use gocompat::math;
use std::rc::Rc;

pub struct shapeSquare {
    pub(crate) b: Rc<baseShape>,
}
pub fn NewSquare(box_: BoxRef) -> ShapeRc {
    Rc::new(shapeSquare { b: baseShape::new(SQUARE_TYPE, box_) })
}
impl Shape for shapeSquare {
    fn base(&self) -> &baseShape {
        &self.b
    }
    fn IsRectangular(&self) -> bool {
        true
    }
}
