// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from lib/shape/shape_document.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;
use geo::{BoxRef, Intersectable, Point, PointRef};
use gocompat::math;
use std::rc::Rc;

pub struct shapeDocument {
    pub(crate) b: Rc<baseShape>,
}

pub(crate) const docPathHeight: f64 = 18.925;
pub(crate) const docPathInnerBottom: f64 = 14.0;
pub(crate) const docPathBottom: f64 = 16.3;

// Go constant expressions (folded exactly by the Go compiler).
const docPathBottom_over_docPathHeight: f64 = f64::from_bits(0x3feb8fb9a8c015a5); // docPathBottom/docPathHeight
const c12_8_over_docPathHeight: f64 = f64::from_bits(0x3fe5a4b1346add2b); // 12.8/docPathHeight
const c19_8_over_docPathHeight: f64 = f64::from_bits(0x3ff0bd610e8aa70f); // 19.8/docPathHeight
const defaultPadding_docPathInnerBottom_over_docPathHeight: f64 = f64::from_bits(0x403d972a45aa1a61);

pub fn NewDocument(box_: BoxRef) -> ShapeRc {
    Rc::new(shapeDocument { b: baseShape::new(DOCUMENT_TYPE, box_) })
}

pub(crate) fn documentPath(box_: &geo::Box) -> svg::SvgPathContext {
    let mut pc = svg::NewSVGPathContext(box_.TL(), box_.Width, box_.Height);
    let p = pc.Absolute(0.0, docPathBottom_over_docPathHeight);
    pc.StartAt(p);
    pc.L(false, 0.0, 0.0);
    pc.L(false, 1.0, 0.0);
    pc.L(false, 1.0, docPathBottom_over_docPathHeight);
    pc.C(false, 5.0 / 6.0, c12_8_over_docPathHeight, 2.0 / 3.0, c12_8_over_docPathHeight, 1.0 / 2.0, docPathBottom_over_docPathHeight);
    pc.C(false, 1.0 / 3.0, c19_8_over_docPathHeight, 1.0 / 6.0, c19_8_over_docPathHeight, 0.0, docPathBottom_over_docPathHeight);
    pc.Z();
    pc
}

impl Shape for shapeDocument {
    fn base(&self) -> &baseShape {
        &self.b
    }
    fn GetInnerBox(&self) -> BoxRef {
        let bx = self.b.Box.get();
        let height = bx.Height * docPathInnerBottom / docPathHeight;
        geo::NewBox(bx.TL().Copy(), bx.Width, height)
    }
    fn Perimeter(&self) -> Vec<Rc<dyn Intersectable>> {
        documentPath(&self.b.Box.get()).Path
    }
    fn GetSVGPathData(&self) -> Vec<String> {
        vec![documentPath(&self.b.Box.get()).PathData()]
    }
    fn GetPathCommands(&self) -> Option<Vec<Vec<svg::PathCommand>>> {
        Some(pathCommands(&[&documentPath(&self.b.Box.get())]))
    }
    fn GetDimensionsToFit(&self, width: f64, height: f64, paddingX: f64, paddingY: f64) -> (f64, f64) {
        let baseHeight = (height + paddingY) * docPathHeight / docPathInnerBottom;
        (math::Ceil(width + paddingX), math::Ceil(baseHeight))
    }
    fn GetDefaultPadding(&self) -> (f64, f64) {
        (defaultPadding, defaultPadding_docPathInnerBottom_over_docPathHeight)
    }
}
