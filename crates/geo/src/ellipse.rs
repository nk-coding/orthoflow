// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from lib/geo/ellipse.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;
use gocompat::math;

/// Go `geo.Ellipse`.
#[derive(Clone, Debug)]
pub struct Ellipse {
    pub Center: PointRef,
    pub Rx: f64,
    pub Ry: f64,
}

gocompat::go_ptr!(EllipseRef, Ellipse);

/// `geo.NewEllipse` → `*Ellipse`
pub fn NewEllipse(center: PointRef, rx: f64, ry: f64) -> EllipseRef {
    EllipseRef::new(Ellipse { Center: center, Rx: rx, Ry: ry })
}

impl Ellipse {
    pub fn Intersections(&self, segment: &Segment) -> Vec<PointRef> {
        let mut intersections: Vec<PointRef> = Vec::new();
        let a = self.Rx;
        let b = self.Ry;
        let a2 = a * a;
        let b2 = b * b;
        if a <= 0.0 || b <= 0.0 {
            return Vec::new();
        }
        let c0 = self.Center.get();
        let (ss, se) = (segment.Start.get(), segment.End.get());
        let x1 = ss.X - c0.X;
        let y1 = ss.Y - c0.Y;
        let x2 = se.X - c0.X;
        let y2 = se.Y - c0.Y;

        if x1 == x2 {
            let intersection1 = NewPoint(x1, b * math::Sqrt(a2 - x1 * x1) / a);
            let intersection2 = intersection1.Copy();
            intersection2.bm().Y *= -1.0;

            let isPointOnLine = |p: &PointRef| -> bool {
                let py = p.b().Y;
                let mut ps = vec![py, y1, y2];
                gocompat::sort::SortSlice(&mut ps, |a, b| a < b);
                ps[1] == py
            };

            if isPointOnLine(&intersection1) {
                intersections.push(intersection1.clone());
            }
            if intersection2.b().Y != 0.0 && isPointOnLine(&intersection2) {
                intersections.push(intersection2.clone());
            }
            let c = self.Center.get();
            for p in &intersections {
                let mut pm = p.bm();
                pm.X += c.X;
                pm.Y += c.Y;
            }
            return intersections;
        }

        let m = (y2 - y1) / (x2 - x1);
        let c = y1 - m * x1;
        let isPointOnLine = |p: &PointRef| -> bool {
            PrecisionCompare(p.b().DistanceToLine(&Point::new(x1, y1), &Point::new(x2, y2)), 0.0, PRECISION) == 0
        };

        let denom = a2 * m * m + b2;
        let root = math::Sqrt(a2 * b2 * (denom - c * c));

        let intersection1 = NewPoint((-m * c * a2 + root) / denom, (c * b2 + m * root) / denom);
        let intersection2 = NewPoint((-m * c * a2 - root) / denom, (c * b2 - m * root) / denom);

        if isPointOnLine(&intersection1) {
            intersections.push(intersection1.clone());
        }
        if !intersection1.Equals(&intersection2) && isPointOnLine(&intersection2) {
            intersections.push(intersection2.clone());
        }

        let cc = self.Center.get();
        for p in &intersections {
            let mut pm = p.bm();
            pm.X += cc.X;
            pm.Y += cc.Y;
        }
        intersections
    }
}

impl EllipseRef {
    pub fn Intersections(&self, segment: &Segment) -> Vec<PointRef> {
        self.b().Intersections(segment)
    }
}

impl Intersectable for Ellipse {
    fn Intersections(&self, segment: &Segment) -> Vec<PointRef> {
        Ellipse::Intersections(self, segment)
    }
}

impl Intersectable for EllipseRef {
    fn Intersections(&self, segment: &Segment) -> Vec<PointRef> {
        self.b().Intersections(segment)
    }
}
