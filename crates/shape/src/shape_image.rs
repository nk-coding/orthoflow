// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from lib/shape/shape_image.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;
use geo::{BoxRef, Intersectable, Point, PointRef};
use gocompat::math;
use std::rc::Rc;

pub struct shapeImage {
    pub(crate) b: Rc<baseShape>,
}
pub fn NewImage(box_: BoxRef) -> ShapeRc {
    Rc::new(shapeImage { b: baseShape::new(IMAGE_TYPE, box_) })
}
impl Shape for shapeImage {
    fn base(&self) -> &baseShape {
        &self.b
    }
    fn IsRectangular(&self) -> bool {
        true
    }
    fn GetDefaultPadding(&self) -> (f64, f64) {
        (0.0, 0.0)
    }
}
