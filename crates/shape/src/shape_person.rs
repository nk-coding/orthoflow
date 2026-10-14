// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from lib/shape/shape_person.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;
use geo::{BoxRef, Intersectable, Point, PointRef};
use gocompat::math;
use std::rc::Rc;

pub struct shapePerson {
    pub(crate) b: Rc<baseShape>,
}

pub fn NewPerson(box_: BoxRef) -> ShapeRc {
    Rc::new(shapePerson { b: baseShape::new(PERSON_TYPE, box_) })
}

pub const PERSON_AR_LIMIT: f64 = 1.5;
/// Go constant 20.2 / 68.3
pub(crate) const personShoulderWidthFactor: f64 = f64::from_bits(0x3fd2eda24bb6892f);
// Go constant (1 - 2*personShoulderWidthFactor)
const one_minus_two_personShoulderWidthFactor: f64 = f64::from_bits(0x3fda24bb6892eda2);

pub(crate) fn personPath(box_: &geo::Box) -> svg::SvgPathContext {
    let mut pc = svg::NewSVGPathContext(box_.TL(), box_.Width / 68.3, box_.Height / 77.4);
    let p = pc.Absolute(68.3, 77.4);
    pc.StartAt(p);
    pc.H(false, 0.0);
    pc.V(true, -1.1);
    pc.C(true, 0.0, -13.2, 7.5, -25.1, 19.3, -30.8);
    pc.C(false, 12.8, 40.9, 8.9, 33.4, 8.9, 25.2);
    pc.C(false, 8.9, 11.3, 20.2, 0.0, 34.1, 0.0);
    pc.C(true, 13.9, 0.0, 25.2, 11.3, 25.2, 25.2);
    pc.C(true, 0.0, 8.2, -3.8, 15.6, -10.4, 20.4);
    pc.C(true, 11.8, 5.7, 19.3, 17.6, 19.3, 30.8);
    pc.V(true, 1.0);
    pc.H(false, 68.3);
    pc.Z();
    pc
}

impl Shape for shapePerson {
    fn base(&self) -> &baseShape {
        &self.b
    }
    fn GetInnerBox(&self) -> BoxRef {
        let bx = self.b.Box.get();
        let mut width = bx.Width;
        let tl = bx.TL().Copy();
        let shoulderWidth = personShoulderWidthFactor * width;
        tl.bm().X += shoulderWidth;
        width -= shoulderWidth * 2.0;
        geo::NewBox(tl, width, bx.Height)
    }
    fn Perimeter(&self) -> Vec<Rc<dyn Intersectable>> {
        personPath(&self.b.Box.get()).Path
    }
    fn GetSVGPathData(&self) -> Vec<String> {
        vec![personPath(&self.b.Box.get()).PathData()]
    }
    fn GetPathCommands(&self) -> Option<Vec<Vec<svg::PathCommand>>> {
        Some(pathCommands(&[&personPath(&self.b.Box.get())]))
    }
    fn GetDimensionsToFit(&self, width: f64, height: f64, paddingX: f64, paddingY: f64) -> (f64, f64) {
        let mut totalWidth = width + paddingX;
        let shoulderWidth = totalWidth * personShoulderWidthFactor / one_minus_two_personShoulderWidthFactor;
        totalWidth += 2.0 * shoulderWidth;
        let totalHeight = height + paddingY;
        let (totalWidth, totalHeight) = LimitAR(totalWidth, totalHeight, PERSON_AR_LIMIT);
        (math::Ceil(totalWidth), math::Ceil(totalHeight))
    }
    fn GetDefaultPadding(&self) -> (f64, f64) {
        (10.0, defaultPadding)
    }
}
