// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from lib/shape/shape_queue.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;
use geo::{BoxRef, Intersectable, Point, PointRef};
use gocompat::math;
use std::rc::Rc;

pub struct shapeQueue {
    pub(crate) b: Rc<baseShape>,
}

pub fn NewQueue(box_: BoxRef) -> ShapeRc {
    Rc::new(shapeQueue { b: baseShape::new(QUEUE_TYPE, box_) })
}

pub(crate) fn getArcWidth(box_: &geo::Box) -> f64 {
    let mut arcWidth = defaultArcDepth;
    if box_.Width < arcWidth * 2.0 {
        arcWidth = box_.Width / 2.0;
    }
    arcWidth
}

pub(crate) fn queueOuterPath(box_: &geo::Box) -> svg::SvgPathContext {
    let arcWidth = getArcWidth(box_);
    let multiplier = 0.45;
    let mut pc = svg::NewSVGPathContext(box_.TL(), 1.0, 1.0);
    let p = pc.Absolute(arcWidth, 0.0);
    pc.StartAt(p);
    pc.H(true, box_.Width - 2.0 * arcWidth);
    pc.C(false, box_.Width, 0.0, box_.Width, box_.Height * multiplier, box_.Width, box_.Height / 2.0);
    pc.C(false, box_.Width, box_.Height - box_.Height * multiplier, box_.Width, box_.Height, box_.Width - arcWidth, box_.Height);
    pc.H(true, -1.0 * (box_.Width - 2.0 * arcWidth));
    pc.C(false, 0.0, box_.Height, 0.0, box_.Height - box_.Height * multiplier, 0.0, box_.Height / 2.0);
    pc.C(false, 0.0, box_.Height * multiplier, 0.0, 0.0, arcWidth, 0.0);
    pc.Z();
    pc
}

pub(crate) fn queueInnerPath(box_: &geo::Box) -> svg::SvgPathContext {
    let arcWidth = getArcWidth(box_);
    let multiplier = 0.45;
    let mut pc = svg::NewSVGPathContext(box_.TL(), 1.0, 1.0);
    let p = pc.Absolute(box_.Width - arcWidth, 0.0);
    pc.StartAt(p);
    pc.C(false, box_.Width - 2.0 * arcWidth, 0.0, box_.Width - 2.0 * arcWidth, box_.Height * multiplier, box_.Width - 2.0 * arcWidth, box_.Height / 2.0);
    pc.C(false, box_.Width - 2.0 * arcWidth, box_.Height - box_.Height * multiplier, box_.Width - 2.0 * arcWidth, box_.Height, box_.Width - arcWidth, box_.Height);
    pc
}

impl Shape for shapeQueue {
    fn base(&self) -> &baseShape {
        &self.b
    }
    fn GetInnerBox(&self) -> BoxRef {
        let bx = self.b.Box.get();
        let mut width = bx.Width;
        let tl = bx.TL().Copy();
        let arcWidth = getArcWidth(&bx);
        width -= 3.0 * arcWidth;
        tl.bm().X += arcWidth;
        geo::NewBox(tl, width, bx.Height)
    }
    fn Perimeter(&self) -> Vec<Rc<dyn Intersectable>> {
        queueOuterPath(&self.b.Box.get()).Path
    }
    fn GetSVGPathData(&self) -> Vec<String> {
        let bx = self.b.Box.get();
        vec![queueOuterPath(&bx).PathData(), queueInnerPath(&bx).PathData()]
    }
    fn GetPathCommands(&self) -> Option<Vec<Vec<svg::PathCommand>>> {
        let bx = self.b.Box.get();
        Some(pathCommands(&[&queueOuterPath(&bx), &queueInnerPath(&bx)]))
    }
    fn GetDimensionsToFit(&self, width: f64, height: f64, paddingX: f64, paddingY: f64) -> (f64, f64) {
        let totalWidth = 3.0 * defaultArcDepth + width + paddingX;
        (math::Ceil(totalWidth), math::Ceil(height + paddingY))
    }
    fn GetDefaultPadding(&self) -> (f64, f64) {
        (defaultPadding / 2.0, defaultPadding)
    }
}
