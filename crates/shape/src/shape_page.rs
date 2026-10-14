// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from lib/shape/shape_page.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;
use geo::{BoxRef, Intersectable, Point, PointRef};
use gocompat::math;
use std::rc::Rc;

pub struct shapePage {
    pub(crate) b: Rc<baseShape>,
}

pub(crate) const pageCornerWidth: f64 = 20.8164;
pub(crate) const pageCornerHeight: f64 = 20.348;
// Go constant expressions (folded exactly)
const three_pageCornerHeight: f64 = f64::from_bits(0x404e85a1cac08312); // 3*pageCornerHeight
const two_pageCornerWidth: f64 = f64::from_bits(0x4044d0ff97247454); // 2*pageCornerWidth
const pageCornerHeight_plus_defaultPadding: f64 = f64::from_bits(0x404e2c8b43958106);

pub fn NewPage(box_: BoxRef) -> ShapeRc {
    Rc::new(shapePage { b: baseShape::new(PAGE_TYPE, box_) })
}

pub(crate) fn pageOuterPath(box_: &geo::Box) -> svg::SvgPathContext {
    let mut pc = svg::NewSVGPathContext(box_.TL(), 1.0, 1.0);
    let p = pc.Absolute(0.5, 0.0);
    pc.StartAt(p);
    pc.H(false, box_.Width - 20.8164);
    pc.C(false, box_.Width - 19.6456, 0.0, box_.Width - 18.521, 0.456297, box_.Width - 17.6811, 1.27202);
    pc.L(false, box_.Width - 1.3647, 17.12);
    pc.C(false, box_.Width - 0.4923, 17.9674, box_.Width, 19.1318, box_.Width, 20.348);
    pc.V(false, box_.Height - 0.5);
    pc.C(false, box_.Width, box_.Height - 0.2239, box_.Width - 0.2239, box_.Height, box_.Width - 0.5, box_.Height);
    pc.H(false, 0.499999);
    pc.C(false, 0.223857, box_.Height, 0.0, box_.Height - 0.2239, 0.0, box_.Height - 0.5);
    pc.V(false, 0.499999);
    pc.C(false, 0.0, 0.223857, 0.223857, 0.0, 0.5, 0.0);
    pc.Z();
    pc
}

pub(crate) fn pageInnerPath(box_: &geo::Box) -> svg::SvgPathContext {
    let mut pc = svg::NewSVGPathContext(box_.TL(), 1.0, 1.0);
    let p = pc.Absolute(box_.Width - 1.08197, box_.Height);
    pc.StartAt(p);
    pc.H(false, 1.08196);
    pc.C(true, -0.64918, 0.0, -1.08196, -0.43287, -1.08196, -1.08219);
    pc.V(false, 1.08219);
    pc.C(true, 0.0, -0.64931, 0.43278, -1.08219, 1.08196, -1.08219);
    pc.H(true, box_.Width - 22.72132);
    pc.C(true, 0.64918, 0.0, 1.08196, 0.43287, 1.08196, 1.08219);
    pc.V(true, 17.09863);
    pc.C(true, 0.0, 1.29863, 0.86557, 2.38082, 2.38032, 2.38082);
    pc.H(false, box_.Width - 1.08197);
    pc.C(true, 0.64918, 0.0, 1.08196, 0.43287, 1.08196, 1.08196);
    pc.V(false, box_.Height - 1.0822);
    pc.C(false, box_.Width - 1.0, box_.Height - 0.43288, box_.Width - 0.43279, box_.Height, box_.Width - 1.08197, box_.Height);
    pc.Z();
    pc
}

impl Shape for shapePage {
    fn base(&self) -> &baseShape {
        &self.b
    }
    fn GetInnerBox(&self) -> BoxRef {
        let bx = self.b.Box.get();
        let mut width = bx.Width;
        if bx.Height < three_pageCornerHeight {
            width -= pageCornerWidth;
        }
        geo::NewBox(bx.TL().Copy(), width, bx.Height)
    }
    fn Perimeter(&self) -> Vec<Rc<dyn Intersectable>> {
        pageOuterPath(&self.b.Box.get()).Path
    }
    fn GetSVGPathData(&self) -> Vec<String> {
        let bx = self.b.Box.get();
        vec![pageOuterPath(&bx).PathData(), pageInnerPath(&bx).PathData()]
    }
    fn GetPathCommands(&self) -> Option<Vec<Vec<svg::PathCommand>>> {
        let bx = self.b.Box.get();
        Some(pathCommands(&[&pageOuterPath(&bx), &pageInnerPath(&bx)]))
    }
    fn GetDimensionsToFit(&self, width: f64, height: f64, paddingX: f64, paddingY: f64) -> (f64, f64) {
        let mut totalWidth = width + paddingX;
        let mut totalHeight = height + paddingY;
        if totalHeight < three_pageCornerHeight {
            totalWidth += pageCornerWidth;
        }
        totalWidth = math::Max(totalWidth, two_pageCornerWidth);
        totalHeight = math::Max(totalHeight, pageCornerHeight);
        (math::Ceil(totalWidth), math::Ceil(totalHeight))
    }
    fn GetDefaultPadding(&self) -> (f64, f64) {
        (defaultPadding, pageCornerHeight_plus_defaultPadding)
    }
}
