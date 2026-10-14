// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from lib/shape/shape_package.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;
use geo::{BoxRef, Intersectable, Point, PointRef};
use gocompat::math;
use std::rc::Rc;

pub struct shapePackage {
    pub(crate) b: Rc<baseShape>,
}

pub(crate) const packageTopMinHeight: f64 = 34.0;
pub(crate) const packageTopMaxHeight: f64 = 55.0;
pub(crate) const packageTopMinWidth: f64 = 50.0;
pub(crate) const packageTopMaxWidth: f64 = 150.0;
pub(crate) const packageHorizontalScalar: f64 = 0.5;
pub(crate) const packageVerticalScalar: f64 = 0.2;
// Go constant (1. - packageVerticalScalar)
const one_minus_packageVerticalScalar: f64 = f64::from_bits(0x3fe999999999999a);

pub fn NewPackage(box_: BoxRef) -> ShapeRc {
    Rc::new(shapePackage { b: baseShape::new(PACKAGE_TYPE, box_) })
}

pub(crate) fn getTopDimensions(box_: &geo::Box) -> (f64, f64) {
    let mut width = box_.Width * packageHorizontalScalar;
    if box_.Width >= 2.0 * packageTopMinWidth {
        width = math::Min(packageTopMaxWidth, math::Max(packageTopMinWidth, width));
    }
    let height = math::Min(packageTopMaxHeight, box_.Height * packageVerticalScalar);
    (width, height)
}

pub(crate) fn packagePath(box_: &geo::Box) -> svg::SvgPathContext {
    let (topWidth, topHeight) = getTopDimensions(box_);
    let mut pc = svg::NewSVGPathContext(box_.TL(), 1.0, 1.0);
    let p = pc.Absolute(0.0, 0.0);
    pc.StartAt(p);
    pc.L(false, topWidth, 0.0);
    pc.L(false, topWidth, topHeight);
    pc.L(false, box_.Width, topHeight);
    pc.L(false, box_.Width, box_.Height);
    pc.L(false, 0.0, box_.Height);
    pc.Z();
    pc
}

impl Shape for shapePackage {
    fn base(&self) -> &baseShape {
        &self.b
    }
    fn GetInnerBox(&self) -> BoxRef {
        let bx = self.b.Box.get();
        let tl = bx.TL().Copy();
        let mut height = bx.Height;
        let (_, topHeight) = getTopDimensions(&bx);
        tl.bm().Y += topHeight;
        height -= topHeight;
        geo::NewBox(tl, bx.Width, height)
    }
    fn Perimeter(&self) -> Vec<Rc<dyn Intersectable>> {
        packagePath(&self.b.Box.get()).Path
    }
    fn GetSVGPathData(&self) -> Vec<String> {
        vec![packagePath(&self.b.Box.get()).PathData()]
    }
    fn GetPathCommands(&self) -> Option<Vec<Vec<svg::PathCommand>>> {
        Some(pathCommands(&[&packagePath(&self.b.Box.get())]))
    }
    fn GetDimensionsToFit(&self, width: f64, height: f64, paddingX: f64, paddingY: f64) -> (f64, f64) {
        let innerHeight = height + paddingY;
        let topHeight = innerHeight * packageVerticalScalar / one_minus_packageVerticalScalar;
        let totalHeight = innerHeight + math::Min(topHeight, packageTopMaxHeight);
        (math::Ceil(width + paddingX), math::Ceil(totalHeight))
    }
    fn GetDefaultPadding(&self) -> (f64, f64) {
        (defaultPadding, 32.0) // Go: .8 * defaultPadding (constant)
    }
}
