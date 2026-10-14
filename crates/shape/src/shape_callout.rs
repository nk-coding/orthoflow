// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from lib/shape/shape_callout.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;
use geo::{BoxRef, Intersectable, Point, PointRef};
use gocompat::math;
use std::rc::Rc;

pub struct shapeCallout {
    pub(crate) b: Rc<baseShape>,
}

pub(crate) const defaultTipWidth: f64 = 30.0;
pub(crate) const defaultTipHeight: f64 = 45.0;

pub fn NewCallout(box_: BoxRef) -> ShapeRc {
    Rc::new(shapeCallout { b: baseShape::new(CALLOUT_TYPE, box_) })
}

pub(crate) fn getTipWidth(box_: &geo::Box) -> f64 {
    let mut tipWidth = defaultTipWidth;
    if box_.Width < tipWidth * 2.0 {
        tipWidth = box_.Width / 2.0;
    }
    tipWidth
}

pub(crate) fn getTipHeight(box_: &geo::Box) -> f64 {
    let mut tipHeight = defaultTipHeight;
    if box_.Height < tipHeight * 2.0 {
        tipHeight = box_.Height / 2.0;
    }
    tipHeight
}

pub(crate) fn calloutPath(box_: &geo::Box) -> svg::SvgPathContext {
    let tipWidth = getTipWidth(box_);
    let tipHeight = getTipHeight(box_);
    let mut pc = svg::NewSVGPathContext(box_.TL(), 1.0, 1.0);
    let p = pc.Absolute(0.0, 0.0);
    pc.StartAt(p);
    pc.V(true, box_.Height - tipHeight);
    pc.H(true, box_.Width / 2.0);
    pc.V(true, tipHeight);
    pc.L(true, tipWidth, -tipHeight);
    pc.H(true, box_.Width / 2.0 - tipWidth);
    pc.V(true, -(box_.Height - tipHeight));
    pc.H(true, -box_.Width);
    pc.Z();
    pc
}

impl Shape for shapeCallout {
    fn base(&self) -> &baseShape {
        &self.b
    }
    fn GetInnerBox(&self) -> BoxRef {
        let bx = self.b.Box.get();
        let tipHeight = getTipHeight(&bx);
        let height = bx.Height - tipHeight;
        geo::NewBox(bx.TL().Copy(), bx.Width, height)
    }
    fn Perimeter(&self) -> Vec<Rc<dyn Intersectable>> {
        calloutPath(&self.b.Box.get()).Path
    }
    fn GetSVGPathData(&self) -> Vec<String> {
        vec![calloutPath(&self.b.Box.get()).PathData()]
    }
    fn GetPathCommands(&self) -> Option<Vec<Vec<svg::PathCommand>>> {
        Some(pathCommands(&[&calloutPath(&self.b.Box.get())]))
    }
    fn GetDimensionsToFit(&self, width: f64, height: f64, paddingX: f64, paddingY: f64) -> (f64, f64) {
        let mut baseHeight = height + paddingY;
        if baseHeight < defaultTipHeight {
            baseHeight *= 2.0;
        } else {
            baseHeight += defaultTipHeight;
        }
        (math::Ceil(width + paddingX), math::Ceil(baseHeight))
    }
    fn GetDefaultPadding(&self) -> (f64, f64) {
        (defaultPadding, defaultPadding / 2.0)
    }
}
