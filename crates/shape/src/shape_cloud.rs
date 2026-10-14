// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from lib/shape/shape_cloud.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;
use geo::{BoxRef, Intersectable, Point, PointRef};
use gocompat::math;
use std::rc::Rc;

use std::cell::Cell;

pub const CLOUD_WIDE_INNER_X: f64 = 0.085;
pub const CLOUD_WIDE_INNER_Y: f64 = 0.409;
pub const CLOUD_WIDE_INNER_WIDTH: f64 = 0.819;
pub const CLOUD_WIDE_INNER_HEIGHT: f64 = 0.548;
/// Go constant (1 + CLOUD_WIDE_INNER_WIDTH/CLOUD_WIDE_INNER_HEIGHT) / 2, folded exactly.
pub const CLOUD_WIDE_ASPECT_BOUNDARY: f64 = f64::from_bits(0x3ff3f4c9cf6a82ce);

pub const CLOUD_TALL_INNER_X: f64 = 0.228;
pub const CLOUD_TALL_INNER_Y: f64 = 0.179;
pub const CLOUD_TALL_INNER_WIDTH: f64 = 0.549;
pub const CLOUD_TALL_INNER_HEIGHT: f64 = 0.820;
/// Go constant (1 + CLOUD_TALL_INNER_WIDTH/CLOUD_TALL_INNER_HEIGHT) / 2, folded exactly.
pub const CLOUD_TALL_ASPECT_BOUNDARY: f64 = f64::from_bits(0x3feab6526b6526b6);

pub const CLOUD_SQUARE_INNER_X: f64 = 0.167;
pub const CLOUD_SQUARE_INNER_Y: f64 = 0.335;
pub const CLOUD_SQUARE_INNER_WIDTH: f64 = 0.663;
pub const CLOUD_SQUARE_INNER_HEIGHT: f64 = 0.663;

pub struct shapeCloud {
    pub(crate) b: Rc<baseShape>,
    /// Go `innerBoxAspectRatio *float64` (shared by copies)
    pub(crate) innerBoxAspectRatio: Rc<Cell<f64>>,
}

pub fn NewCloud(box_: BoxRef) -> ShapeRc {
    Rc::new(shapeCloud { b: baseShape::new(CLOUD_TYPE, box_), innerBoxAspectRatio: Rc::new(Cell::new(0.0)) })
}

pub(crate) fn cloudPath(box_: &geo::Box) -> svg::SvgPathContext {
    let mut pc = svg::NewSVGPathContext(box_.TL(), box_.Width / 834.0, box_.Height / 523.0);
    let p = pc.Absolute(137.833, 182.833);
    pc.StartAt(p);
    pc.C(true, 0.0, 5.556, -5.556, 11.111, -11.111, 11.111);
    pc.C(true, -70.833, 6.944, -126.389, 77.778, -126.389, 163.889);
    pc.C(true, 0.0, 91.667, 62.5, 165.278, 141.667, 165.278);
    pc.H(true, 537.5);
    pc.C(true, 84.723, 0.0, 154.167, -79.167, 154.167, -175.0);
    pc.C(true, 0.0, -91.667, -63.89, -168.056, -144.444, -173.611);
    pc.C(true, -5.556, 0.0, -11.111, -4.167, -12.5, -11.111);
    pc.C(true, -18.056, -93.055, -101.39, -162.5, -198.611, -162.5);
    pc.C(true, -63.889, 0.0, -120.834, 29.167, -156.944, 75.0);
    pc.C(true, -4.167, 5.556, -11.111, 6.945, -15.278, 5.556);
    pc.C(true, -13.889, -5.556, -29.166, -8.333, -45.833, -8.333);
    pc.C(false, 196.167, 71.722, 143.389, 120.333, 137.833, 182.833);
    pc.Z();
    pc
}

impl Shape for shapeCloud {
    fn base(&self) -> &baseShape {
        &self.b
    }
    fn GetInnerBox(&self) -> BoxRef {
        let ar = self.innerBoxAspectRatio.get();
        if ar != 0.0 {
            self.GetInnerBoxForContent(ar, 1.0).unwrap()
        } else {
            let bx = self.b.Box.get();
            self.GetInnerBoxForContent(bx.Width, bx.Height).unwrap()
        }
    }
    fn GetInnerBoxForContent(&self, width: f64, height: f64) -> Option<BoxRef> {
        let insideTL = self.GetInsidePlacement(width, height, 0.0, 0.0);
        let aspectRatio = width / height;
        let bx = self.b.Box.get();
        let (mut width, mut height) = (bx.Width, bx.Height);
        if aspectRatio > CLOUD_WIDE_ASPECT_BOUNDARY {
            width *= CLOUD_WIDE_INNER_WIDTH;
            height *= CLOUD_WIDE_INNER_HEIGHT;
        } else if aspectRatio < CLOUD_TALL_ASPECT_BOUNDARY {
            width *= CLOUD_TALL_INNER_WIDTH;
            height *= CLOUD_TALL_INNER_HEIGHT;
        } else {
            width *= CLOUD_SQUARE_INNER_WIDTH;
            height *= CLOUD_SQUARE_INNER_HEIGHT;
        }
        Some(geo::NewBox(PointRef::new(insideTL), width, height))
    }
    fn SetInnerBoxAspectRatio(&self, aspectRatio: f64) {
        self.innerBoxAspectRatio.set(aspectRatio);
    }
    fn GetDimensionsToFit(&self, width: f64, height: f64, paddingX: f64, paddingY: f64) -> (f64, f64) {
        let width = width + paddingX;
        let height = height + paddingY;
        let aspectRatio = width / height;
        if aspectRatio > CLOUD_WIDE_ASPECT_BOUNDARY {
            (math::Ceil(width / CLOUD_WIDE_INNER_WIDTH), math::Ceil(height / CLOUD_WIDE_INNER_HEIGHT))
        } else if aspectRatio < CLOUD_TALL_ASPECT_BOUNDARY {
            (math::Ceil(width / CLOUD_TALL_INNER_WIDTH), math::Ceil(height / CLOUD_TALL_INNER_HEIGHT))
        } else {
            (math::Ceil(width / CLOUD_SQUARE_INNER_WIDTH), math::Ceil(height / CLOUD_SQUARE_INNER_HEIGHT))
        }
    }
    fn GetInsidePlacement(&self, width: f64, height: f64, paddingX: f64, paddingY: f64) -> Point {
        let r = self.b.Box.get();
        let tl = r.TL().get();
        let width = width + paddingX;
        let height = height + paddingY;
        let aspectRatio = width / height;
        if aspectRatio > CLOUD_WIDE_ASPECT_BOUNDARY {
            Point::new(tl.X + math::Ceil(r.Width * CLOUD_WIDE_INNER_X + paddingX / 2.0), tl.Y + math::Ceil(r.Height * CLOUD_WIDE_INNER_Y + paddingY / 2.0))
        } else if aspectRatio < CLOUD_TALL_ASPECT_BOUNDARY {
            Point::new(tl.X + math::Ceil(r.Width * CLOUD_TALL_INNER_X + paddingX / 2.0), tl.Y + math::Ceil(r.Height * CLOUD_TALL_INNER_Y + paddingY / 2.0))
        } else {
            Point::new(tl.X + math::Ceil(r.Width * CLOUD_SQUARE_INNER_X + paddingX / 2.0), tl.Y + math::Ceil(r.Height * CLOUD_SQUARE_INNER_Y + paddingY / 2.0))
        }
    }
    fn Perimeter(&self) -> Vec<Rc<dyn Intersectable>> {
        cloudPath(&self.b.Box.get()).Path
    }
    fn GetSVGPathData(&self) -> Vec<String> {
        vec![cloudPath(&self.b.Box.get()).PathData()]
    }
    fn GetPathCommands(&self) -> Option<Vec<Vec<svg::PathCommand>>> {
        Some(pathCommands(&[&cloudPath(&self.b.Box.get())]))
    }
    fn GetDefaultPadding(&self) -> (f64, f64) {
        (defaultPadding, defaultPadding / 2.0)
    }
}
