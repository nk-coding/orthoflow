// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from lib/shape/shape_c4_person.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;
use geo::{BoxRef, Intersectable, Point, PointRef};
use gocompat::math;
use std::rc::Rc;

pub const C4_PERSON_AR_LIMIT: f64 = 1.5;
pub const HEAD_RADIUS_FACTOR: f64 = 0.22;
pub const BODY_TOP_FACTOR: f64 = 0.8;
pub const CORNER_RADIUS_FACTOR: f64 = 0.175;

pub struct shapeC4Person {
    pub(crate) b: Rc<baseShape>,
}

pub fn NewC4Person(box_: BoxRef) -> ShapeRc {
    Rc::new(shapeC4Person { b: baseShape::new(C4_PERSON_TYPE, box_) })
}

#[inline]
fn sqrt2m1() -> f64 {
    // Go: math.Sqrt(2)-1 (math.Sqrt is not a constant; evaluated in float64)
    math::Sqrt(2.0) - 1.0
}

pub(crate) fn bodyPath(box_: &geo::Box) -> svg::SvgPathContext {
    let width = box_.Width;
    let height = box_.Height;
    let mut pc = svg::NewSVGPathContext(box_.TL(), 1.0, 1.0);
    let headRadius = width * HEAD_RADIUS_FACTOR;
    let headCenterY = headRadius;
    let bodyTop = headCenterY + headRadius * BODY_TOP_FACTOR;
    let bodyWidth = width;
    let bodyHeight = height - bodyTop;
    let bodyLeft: i64 = 0;
    let cornerRadius = math::Min(width * CORNER_RADIUS_FACTOR, bodyHeight * 0.25);
    let s = sqrt2m1();
    let p = pc.Absolute(bodyLeft as f64, bodyTop + cornerRadius);
    pc.StartAt(p);
    pc.C(true, 0.0, -4.0 * s / 3.0 * cornerRadius, 4.0 * s / 3.0 * cornerRadius, -cornerRadius, cornerRadius, -cornerRadius);
    pc.H(true, bodyWidth - 2.0 * cornerRadius);
    pc.C(true, 4.0 * s / 3.0 * cornerRadius, 0.0, cornerRadius, 4.0 * s / 3.0 * cornerRadius, cornerRadius, cornerRadius);
    pc.V(true, bodyHeight - 2.0 * cornerRadius);
    pc.C(true, 0.0, 4.0 * s / 3.0 * cornerRadius, -4.0 * s / 3.0 * cornerRadius, cornerRadius, -cornerRadius, cornerRadius);
    pc.H(true, -(bodyWidth - 2.0 * cornerRadius));
    pc.C(true, -4.0 * s / 3.0 * cornerRadius, 0.0, -cornerRadius, -4.0 * s / 3.0 * cornerRadius, -cornerRadius, -cornerRadius);
    pc.Z();
    pc
}

pub(crate) fn headPath(box_: &geo::Box) -> svg::SvgPathContext {
    let width = box_.Width;
    let mut pc = svg::NewSVGPathContext(box_.TL(), 1.0, 1.0);
    let headRadius = width * HEAD_RADIUS_FACTOR;
    let headCenterX = width / 2.0;
    let headCenterY = headRadius;
    let s = sqrt2m1();
    let p = pc.Absolute(headCenterX, headCenterY - headRadius);
    pc.StartAt(p);
    pc.C(
        false,
        headCenterX + headRadius * 4.0 * s / 3.0,
        headCenterY - headRadius,
        headCenterX + headRadius,
        headCenterY - headRadius * 4.0 * s / 3.0,
        headCenterX + headRadius,
        headCenterY,
    );
    pc.C(
        false,
        headCenterX + headRadius,
        headCenterY + headRadius * 4.0 * s / 3.0,
        headCenterX + headRadius * 4.0 * s / 3.0,
        headCenterY + headRadius,
        headCenterX,
        headCenterY + headRadius,
    );
    pc.C(
        false,
        headCenterX - headRadius * 4.0 * s / 3.0,
        headCenterY + headRadius,
        headCenterX - headRadius,
        headCenterY + headRadius * 4.0 * s / 3.0,
        headCenterX - headRadius,
        headCenterY,
    );
    pc.C(
        false,
        headCenterX - headRadius,
        headCenterY - headRadius * 4.0 * s / 3.0,
        headCenterX - headRadius * 4.0 * s / 3.0,
        headCenterY - headRadius,
        headCenterX,
        headCenterY - headRadius,
    );
    pc
}

impl Shape for shapeC4Person {
    fn base(&self) -> &baseShape {
        &self.b
    }
    fn GetInnerBox(&self) -> BoxRef {
        let bx = self.b.Box.get();
        let width = bx.Width;
        let height = bx.Height;
        let headRadius = width * HEAD_RADIUS_FACTOR;
        let headCenterY = headRadius;
        let bodyTop = headCenterY + headRadius * BODY_TOP_FACTOR;
        let horizontalPadding = width * 0.05;
        let verticalPadding = height * 0.03;
        let tl = bx.TL().Copy();
        {
            let mut t = tl.bm();
            t.X += horizontalPadding;
            t.Y += bodyTop + verticalPadding;
        }
        let innerWidth = width - (horizontalPadding * 2.0);
        let innerHeight = height - bodyTop - (verticalPadding * 2.0);
        geo::NewBox(tl, innerWidth, innerHeight)
    }
    fn Perimeter(&self) -> Vec<Rc<dyn Intersectable>> {
        let bx = self.b.Box.get();
        let width = bx.Width;
        let mut bodyPerimeter = bodyPath(&bx).Path;
        let headRadius = width * HEAD_RADIUS_FACTOR;
        let tl = bx.TL().get();
        let headCenterX = tl.X + width / 2.0;
        let headCenterY = tl.Y + headRadius;
        let headCenter = geo::NewPoint(headCenterX, headCenterY);
        let headEllipse = geo::NewEllipse(headCenter, headRadius, headRadius);
        bodyPerimeter.push(Rc::new(headEllipse));
        bodyPerimeter
    }
    fn GetSVGPathData(&self) -> Vec<String> {
        let bx = self.b.Box.get();
        vec![bodyPath(&bx).PathData(), headPath(&bx).PathData()]
    }
    fn GetPathCommands(&self) -> Option<Vec<Vec<svg::PathCommand>>> {
        let bx = self.b.Box.get();
        Some(pathCommands(&[&bodyPath(&bx), &headPath(&bx)]))
    }
    fn GetDimensionsToFit(&self, width: f64, height: f64, paddingX: f64, paddingY: f64) -> (f64, f64) {
        let contentWidth = width + paddingX;
        let contentHeight = height + paddingY;
        let totalWidth = contentWidth / 0.9;
        let headRadius = totalWidth * HEAD_RADIUS_FACTOR;
        let headCenterY = headRadius;
        let bodyTop = headCenterY + headRadius * BODY_TOP_FACTOR;
        let verticalPadding = totalWidth * 0.06;
        let mut totalHeight = contentHeight + bodyTop + verticalPadding;
        let minHeight = totalWidth * 0.95;
        if totalHeight < minHeight {
            totalHeight = minHeight;
        }
        let (totalWidth, totalHeight) = LimitAR(totalWidth, totalHeight, C4_PERSON_AR_LIMIT);
        (math::Ceil(totalWidth), math::Ceil(totalHeight))
    }
    fn GetDefaultPadding(&self) -> (f64, f64) {
        (10.0, defaultPadding)
    }
}
