// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from lib/geo/bezier.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;
use gocompat::math;

pub const PRECISION: f64 = 0.0001;

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct bezierPoint {
    pub(crate) X: f64,
    pub(crate) Y: f64,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct bezierControlPoint {
    pub(crate) Point: bezierPoint,
    pub(crate) Control: bezierPoint,
}

pub(crate) type bezierCurveImpl = Vec<bezierControlPoint>;

pub(crate) fn newBezierCurveImpl(cp: &[bezierPoint]) -> bezierCurveImpl {
    if cp.is_empty() {
        return Vec::new();
    }
    let mut c: bezierCurveImpl = vec![bezierControlPoint::default(); cp.len()];
    for (i, p) in cp.iter().enumerate() {
        c[i].Point = *p;
    }
    let mut w: f64 = 0.0;
    let n = c.len();
    for i in 0..n {
        let p = c[i];
        match i {
            0 => w = 1.0,
            1 => w = n as f64 - 1.0,
            _ => w *= (n - i) as f64 / i as f64,
        }
        c[i].Control.X = p.Point.X * w;
        c[i].Control.Y = p.Point.Y * w;
    }
    c
}

/// Note: Go's `pointAt` has a value receiver but writes through the shared backing
/// array (GO-ALIAS), so the curve's `Point` fields are mutated; reproduced here.
pub(crate) fn pointAt(c: &mut bezierCurveImpl, t: f64) -> bezierPoint {
    c[0].Point = c[0].Control;
    let mut u = t;
    for i in 1..c.len() {
        let p = c[i];
        c[i].Point = bezierPoint { X: p.Control.X * u, Y: p.Control.Y * u };
        u *= t;
    }
    let t1 = 1.0 - t;
    let mut tt = t1;
    let mut p = c[c.len() - 1].Point;
    let mut i = c.len() as i64 - 2;
    while i >= 0 {
        p.X += c[i as usize].Point.X * tt;
        p.Y += c[i as usize].Point.Y * tt;
        tt *= t1;
        i -= 1;
    }
    p
}

/// Go `geo.BezierCurve`.
#[derive(Clone, Debug)]
pub struct BezierCurve {
    pub(crate) curve: std::rc::Rc<std::cell::RefCell<bezierCurveImpl>>,
    pub(crate) points: Vec<PointRef>,
}

/// `geo.NewBezierCurve(points)` → `*BezierCurve`
pub fn NewBezierCurve(points: &[PointRef]) -> BezierCurve {
    let localPoints: Vec<bezierPoint> = points
        .iter()
        .map(|p| {
            let p = p.b();
            bezierPoint { X: p.X, Y: p.Y }
        })
        .collect();
    let localCurve = newBezierCurveImpl(&localPoints);
    BezierCurve { curve: std::rc::Rc::new(std::cell::RefCell::new(localCurve)), points: points.to_vec() }
}

impl BezierCurve {
    pub fn Intersections(&self, segment: &Segment) -> Vec<PointRef> {
        let p: Vec<Point> = self.points[..4].iter().map(|p| p.get()).collect();
        let (s, e) = (segment.Start.get(), segment.End.get());
        ComputeIntersections(&[p[0].X, p[1].X, p[2].X, p[3].X], &[p[0].Y, p[1].Y, p[2].Y, p[3].Y], &[s.X, e.X], &[s.Y, e.Y])
    }

    pub fn At(&self, point: f64) -> PointRef {
        let curvePoint = pointAt(&mut self.curve.borrow_mut(), point);
        NewPoint(curvePoint.X, curvePoint.Y)
    }
}

impl Intersectable for BezierCurve {
    fn Intersections(&self, segment: &Segment) -> Vec<PointRef> {
        BezierCurve::Intersections(self, segment)
    }
}

pub fn ComputeIntersections(px: &[f64], py: &[f64], lx: &[f64], ly: &[f64]) -> Vec<PointRef> {
    let mut out = Vec::new();

    let A = ly[1] - ly[0];
    let B = lx[0] - lx[1];
    let C = lx[0] * (ly[0] - ly[1]) + ly[0] * (lx[1] - lx[0]);

    let bx = bezierCoeffs(px[0], px[1], px[2], px[3]);
    let by = bezierCoeffs(py[0], py[1], py[2], py[3]);

    let mut P = vec![0.0f64; 4];
    P[0] = A * bx[0] + B * by[0];
    P[1] = A * bx[1] + B * by[1];
    P[2] = A * bx[2] + B * by[2];
    P[3] = A * bx[3] + B * by[3] + C;

    let r = cubicRoots(&P);

    for i in 0..3 {
        let t = r[i];
        let point = Point {
            X: bx[0] * t * t * t + bx[1] * t * t + bx[2] * t + bx[3],
            Y: by[0] * t * t * t + by[1] * t * t + by[2] * t + by[3],
        };
        let s: f64;
        if (lx[1] - lx[0]) != 0.0 {
            s = (point.X - lx[0]) / (lx[1] - lx[0]);
        } else {
            s = (point.Y - ly[0]) / (ly[1] - ly[0]);
        }
        let tLT0 = PrecisionCompare(t, 0.0, PRECISION) < 0;
        let tGT1 = PrecisionCompare(t, 1.0, PRECISION) > 0;
        let sLT0 = PrecisionCompare(s, 0.0, PRECISION) < 0;
        let sGT1 = PrecisionCompare(s, 1.0, PRECISION) > 0;
        if !(tLT0 || tGT1 || sLT0 || sGT1) {
            out.push(PointRef::new(point));
        }
    }
    out
}

pub(crate) fn cubicRoots(P: &[f64]) -> Vec<f64> {
    if PrecisionCompare(P[0], 0.0, PRECISION) == 0 {
        if PrecisionCompare(P[1], 0.0, PRECISION) == 0 {
            let mut t = vec![0.0f64; 3];
            t[0] = -1.0 * (P[3] / P[2]);
            t[1] = -1.0;
            t[2] = -1.0;
            for i in 0..1 {
                let tiLT0 = PrecisionCompare(t[i], 0.0, PRECISION) < 0;
                let tiGT1 = PrecisionCompare(t[i], 1.0, PRECISION) > 0;
                if tiLT0 || tiGT1 {
                    t[i] = -1.0;
                }
            }
            return sortSpecial(t);
        }

        let mut DQ = math::Pow(P[2], 2.0) - 4.0 * P[1] * P[3];
        if PrecisionCompare(DQ, 0.0, PRECISION) >= 0 {
            DQ = math::Sqrt(DQ);
            let mut t = vec![0.0f64; 3];
            t[0] = -1.0 * ((DQ + P[2]) / (2.0 * P[1]));
            t[1] = (DQ - P[2]) / (2.0 * P[1]);
            t[2] = -1.0;
            // Go returns on the first loop iteration.
            let i = 0;
            let tiLT0 = PrecisionCompare(t[i], 0.0, PRECISION) < 0;
            let tiGT1 = PrecisionCompare(t[i], 1.0, PRECISION) > 0;
            if tiLT0 || tiGT1 {
                t[i] = -1.0;
            }
            return sortSpecial(t);
        }
    }

    let a = P[0];
    let b = P[1];
    let c = P[2];
    let d = P[3];

    let A = b / a;
    let B = c / a;
    let C = d / a;

    let Q = (3.0 * B - math::Pow(A, 2.0)) / 9.0;
    let R = (9.0 * A * B - 27.0 * C - 2.0 * math::Pow(A, 3.0)) / 54.0;
    let D = math::Pow(Q, 3.0) + math::Pow(R, 2.0);

    let mut t = vec![0.0f64; 3];
    let Im: f64;

    if PrecisionCompare(D, 0.0, PRECISION) >= 0 {
        let S = sgn(R + math::Sqrt(D)) * math::Pow(math::Abs(R + math::Sqrt(D)), 1.0 / 3.0);
        let T = sgn(R - math::Sqrt(D)) * math::Pow(math::Abs(R - math::Sqrt(D)), 1.0 / 3.0);

        t[0] = -A / 3.0 + (S + T);
        t[1] = -A / 3.0 - (S + T) / 2.0;
        t[2] = -A / 3.0 - (S + T) / 2.0;
        Im = math::Abs(math::Sqrt(3.0) * (S - T) / 2.0);

        if PrecisionCompare(Im, 0.0, PRECISION) != 0 {
            t[1] = -1.0;
            t[2] = -1.0;
        }
    } else {
        let th = math::Acos(R / math::Sqrt(-math::Pow(Q, 3.0)));
        t[0] = 2.0 * math::Sqrt(-Q) * math::Cos(th / 3.0) - A / 3.0;
        t[1] = 2.0 * math::Sqrt(-Q) * math::Cos((th + 2.0 * math::Pi) / 3.0) - A / 3.0;
        t[2] = 2.0 * math::Sqrt(-Q) * math::Cos((th + 4.0 * math::Pi) / 3.0) - A / 3.0;
        Im = 0.0;
    }
    let _ = Im;

    for i in 0..3 {
        let tiLT0 = PrecisionCompare(t[i], 0.0, PRECISION) < 0;
        let tiGT1 = PrecisionCompare(t[i], 1.0, PRECISION) > 0;
        if tiLT0 || tiGT1 {
            t[i] = -1.0;
        }
    }
    sortSpecial(t)
}

pub(crate) fn sortSpecial(mut a: Vec<f64>) -> Vec<f64> {
    loop {
        let mut flip = false;
        let n = a.len() as i64;
        let mut i = 0i64;
        while i < n - 1 {
            let iu = i as usize;
            let ai1GTEQ0 = PrecisionCompare(a[iu + 1], 0.0, PRECISION) >= 0;
            let aiGTai1 = PrecisionCompare(a[iu], a[iu + 1], PRECISION) > 0;
            let aiLT0 = PrecisionCompare(a[iu], 0.0, PRECISION) < 0;
            if (ai1GTEQ0 && aiGTai1) || (aiLT0 && ai1GTEQ0) {
                flip = true;
                a.swap(iu, iu + 1);
            }
            i += 1;
        }
        if !flip {
            break;
        }
    }
    a
}

pub(crate) fn sgn(x: f64) -> f64 {
    if x < 0.0 {
        return -1.0;
    }
    1.0
}

pub(crate) fn bezierCoeffs(P0: f64, P1: f64, P2: f64, P3: f64) -> Vec<f64> {
    let mut Z = vec![0.0f64; 4];
    Z[0] = -P0 + 3.0 * P1 + -3.0 * P2 + P3;
    Z[1] = 3.0 * P0 - 6.0 * P1 + 3.0 * P2;
    Z[2] = -3.0 * P0 + 3.0 * P1;
    Z[3] = P0;
    Z
}
