// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from lib/geo/point.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;
use gocompat::math;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RelativePoint {
    pub XPercentage: f64,
    pub YPercentage: f64,
}

gocompat::go_ptr!(RelativePointRef, RelativePoint);

pub fn NewRelativePoint(xPercentage: f64, yPercentage: f64) -> RelativePointRef {
    RelativePointRef::new(RelativePoint {
        XPercentage: TruncateDecimals(xPercentage),
        YPercentage: TruncateDecimals(yPercentage),
    })
}

/// Go `geo.Point` (value). `*geo.Point` is [`PointRef`].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    pub X: f64,
    pub Y: f64,
}

gocompat::go_ptr!(PointRef, Point);

/// Go map-key semantics for `map[geo.Point]V`: field-wise float `==`
/// (+0 == -0, NaN never equal), so `Point` can be used as a `gocompat::Map` key.
impl Eq for Point {}
impl std::hash::Hash for Point {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        let k = |f: f64| if f == 0.0 { 0u64 } else { f.to_bits() };
        state.write_u64(k(self.X));
        state.write_u64(k(self.Y));
    }
}

/// `geo.NewPoint(x, y)` → `*Point`
#[inline]
pub fn NewPoint(x: f64, y: f64) -> PointRef {
    PointRef::new(Point { X: x, Y: y })
}

impl Point {
    #[inline]
    pub fn new(x: f64, y: f64) -> Point {
        Point { X: x, Y: y }
    }

    /// Non-nil `p1.Equals(p2)`.
    #[inline]
    pub fn Equals(&self, p2: &Point) -> bool {
        (self.X == p2.X) && (self.Y == p2.Y)
    }

    pub fn Compare(&self, p2: &Point) -> i64 {
        let xCompare = Sign(self.X - p2.X);
        if xCompare == 0 {
            return Sign(self.Y - p2.Y);
        }
        xCompare
    }

    /// `p.Copy()` → new `*Point`
    #[inline]
    pub fn Copy(&self) -> PointRef {
        PointRef::new(Point { X: self.X, Y: self.Y })
    }

    pub fn GetOrientation(&self, pTo: &Point) -> Orientation {
        if self.Y < pTo.Y {
            if self.X < pTo.X {
                return TopLeft;
            }
            if self.X > pTo.X {
                return TopRight;
            }
            return Top;
        }
        if self.Y > pTo.Y {
            if self.X < pTo.X {
                return BottomLeft;
            }
            if self.X > pTo.X {
                return BottomRight;
            }
            return Bottom;
        }
        if self.X < pTo.X {
            return Left;
        }
        if self.X > pTo.X {
            return Right;
        }
        NONE
    }

    pub fn ToString(&self) -> String {
        format!("({}, {})", gocompat::strconv::FormatFloatV(self.X), gocompat::strconv::FormatFloatV(self.Y))
    }

    pub fn DistanceToLine(&self, p1: &Point, p2: &Point) -> f64 {
        let a = self.X - p1.X;
        let b = self.Y - p1.Y;
        let c = p2.X - p1.X;
        let d = p2.Y - p1.Y;

        let dot = (a * c) + (b * d);
        let len_sq = (c * c) + (d * d);

        let mut param = -1.0;
        if len_sq != 0.0 {
            param = dot / len_sq;
        }

        let xx: f64;
        let yy: f64;
        if param < 0.0 {
            xx = p1.X;
            yy = p1.Y;
        } else if param > 1.0 {
            xx = p2.X;
            yy = p2.Y;
        } else {
            xx = p1.X + (param * c);
            yy = p1.Y + (param * d);
        }

        let dx = self.X - xx;
        let dy = self.Y - yy;
        math::Sqrt((dx * dx) + (dy * dy))
    }

    pub fn AddVector(&self, v: &[f64]) -> PointRef {
        self.ToVector().Add(v).ToPoint()
    }

    pub fn VectorTo(&self, endpoint: &Point) -> Vector {
        endpoint.ToVector().Minus(&self.ToVector())
    }

    pub fn FormattedCoordinates(&self) -> String {
        format!("{},{}", math::F64ToInt(self.X), math::F64ToInt(self.Y))
    }

    pub fn OnOrthogonalSegment(&self, a: &Point, b: &Point) -> bool {
        let p = self;
        if a.X < b.X {
            if p.X < a.X || b.X < p.X {
                return false;
            }
        } else if p.X < b.X || a.X < p.X {
            return false;
        }
        if a.Y < b.Y {
            if p.Y < a.Y || b.Y < p.Y {
                return false;
            }
        } else if p.Y < b.Y || a.Y < p.Y {
            return false;
        }
        true
    }

    #[inline]
    pub fn ToVector(&self) -> Vector {
        vec![self.X, self.Y]
    }

    #[inline]
    pub fn Transpose(&mut self) {
        let (x, y) = (self.X, self.Y);
        self.X = y;
        self.Y = x;
    }

    pub fn Interpolate(&self, b: &Point, t: f64) -> PointRef {
        NewPoint(self.X * (1.0 - t) + b.X * t, self.Y * (1.0 - t) + b.Y * t)
    }

    pub fn TruncateFloat32(&mut self) {
        self.X = self.X as f32 as f64;
        self.Y = self.Y as f32 as f64;
    }

    pub fn TruncateDecimals(&mut self) {
        self.X = TruncateDecimals(self.X);
        self.Y = TruncateDecimals(self.Y);
    }
}

/// Pointer-receiver methods on `*Point`. Arguments are non-nil `*Point`s.
impl PointRef {
    /// Copy of the pointee (`*p`).
    #[inline]
    pub fn get(&self) -> Point {
        *self.b()
    }
    /// `*p = v`
    #[inline]
    pub fn set(&self, v: Point) {
        *self.bm() = v;
    }
    #[inline]
    pub fn X(&self) -> f64 {
        self.b().X
    }
    #[inline]
    pub fn Y(&self) -> f64 {
        self.b().Y
    }
    #[inline]
    pub fn Equals(&self, p2: &PointRef) -> bool {
        self.b().Equals(&p2.b())
    }
    #[inline]
    pub fn Compare(&self, p2: &PointRef) -> i64 {
        self.b().Compare(&p2.b())
    }
    #[inline]
    pub fn Copy(&self) -> PointRef {
        self.b().Copy()
    }
    pub fn GetOrientation(&self, pTo: &PointRef) -> Orientation {
        self.b().GetOrientation(&pTo.b())
    }
    pub fn ToString(&self) -> String {
        self.b().ToString()
    }
    pub fn DistanceToLine(&self, p1: &PointRef, p2: &PointRef) -> f64 {
        self.b().DistanceToLine(&p1.b(), &p2.b())
    }
    pub fn AddVector(&self, v: &[f64]) -> PointRef {
        self.b().AddVector(v)
    }
    pub fn VectorTo(&self, endpoint: &PointRef) -> Vector {
        self.b().VectorTo(&endpoint.b())
    }
    pub fn FormattedCoordinates(&self) -> String {
        self.b().FormattedCoordinates()
    }
    pub fn OnOrthogonalSegment(&self, a: &PointRef, b: &PointRef) -> bool {
        self.b().OnOrthogonalSegment(&a.b(), &b.b())
    }
    pub fn ToVector(&self) -> Vector {
        self.b().ToVector()
    }
    pub fn Transpose(&self) {
        self.bm().Transpose()
    }
    pub fn Interpolate(&self, b: &PointRef, t: f64) -> PointRef {
        self.b().Interpolate(&b.b(), t)
    }
    pub fn TruncateFloat32(&self) {
        self.bm().TruncateFloat32()
    }
    pub fn TruncateDecimals(&self) {
        self.bm().TruncateDecimals()
    }
}

/// nil-aware `p1.Equals(p2)` for possibly-nil `*Point`s.
pub fn PointEquals(p1: Option<&PointRef>, p2: Option<&PointRef>) -> bool {
    match (p1, p2) {
        (None, p2) => p2.is_none(),
        (Some(_), None) => false,
        (Some(a), Some(b)) => a.Equals(b),
    }
}

/// nil-aware `p.ToString()`.
pub fn PointToString(p: Option<&PointRef>) -> String {
    match p {
        None => String::new(),
        Some(p) => p.ToString(),
    }
}

/// nil-aware `p.Transpose()`.
pub fn PointTranspose(p: Option<&PointRef>) {
    if let Some(p) = p {
        p.Transpose();
    }
}

/// Go `geo.Points` (`[]*Point`).
pub type Points = Vec<PointRef>;

/// Methods of `geo.Points`.
pub trait PointsExt {
    fn Equals(&self, other: &[PointRef]) -> bool;
    fn GetMedian(&self) -> PointRef;
    fn ToString(&self) -> String;
}

impl PointsExt for [PointRef] {
    /// Note: Go distinguishes nil from empty slices here; callers holding a
    /// possibly-nil `Points` must use [`PointsEqualsOpt`].
    fn Equals(&self, other: &[PointRef]) -> bool {
        // Go builds a map[Point]struct{} of ps and checks membership of each other point;
        // float map keys compare with ==, which a linear scan reproduces exactly.
        let set: Vec<Point> = self.iter().map(|p| p.get()).collect();
        for o in other {
            let o = o.get();
            if !set.iter().any(|p| p.X == o.X && p.Y == o.Y) {
                return false;
            }
        }
        true
    }

    fn GetMedian(&self) -> PointRef {
        let mut xs: Vec<f64> = Vec::new();
        let mut ys: Vec<f64> = Vec::new();
        for p in self {
            let p = p.b();
            xs.push(p.X);
            ys.push(p.Y);
        }
        gocompat::sort::Float64s(&mut xs);
        gocompat::sort::Float64s(&mut ys);

        let middleIndex = xs.len() / 2;
        let mut medianX = xs[middleIndex];
        let mut medianY = ys[middleIndex];
        if xs.len() % 2 == 0 {
            medianX += xs[middleIndex - 1];
            medianX /= 2.0;
            medianY += ys[middleIndex - 1];
            medianY /= 2.0;
        }
        NewPoint(medianX, medianY)
    }

    fn ToString(&self) -> String {
        let strs: Vec<String> = self.iter().map(|p| p.ToString()).collect();
        strs.join(", ")
    }
}

/// nil-aware `Points.Equals`.
pub fn PointsEqualsOpt(ps: Option<&[PointRef]>, other: Option<&[PointRef]>) -> bool {
    match (ps, other) {
        (None, o) => o.is_none(),
        (Some(_), None) => false,
        (Some(a), Some(b)) => a.Equals(b),
    }
}

/// `geo.IntersectionPoint(u0, u1, v0, v1)` (nil when the segments don't intersect).
pub fn IntersectionPoint(u0: &PointRef, u1: &PointRef, v0: &PointRef, v1: &PointRef) -> Option<PointRef> {
    IntersectionPointV(&u0.b(), &u1.b(), &v0.b(), &v1.b()).map(PointRef::new)
}

/// Value form of [`IntersectionPoint`].
pub fn IntersectionPointV(u0: &Point, u1: &Point, v0: &Point, v1: &Point) -> Option<Point> {
    let udx = u1.X - u0.X;
    let vdx = v1.X - v0.X;
    let uvdx = v0.X - u0.X;
    let udy = u1.Y - u0.Y;
    let vdy = v1.Y - v0.Y;
    let uvdy = v0.Y - u0.Y;

    let denom = udy * vdx - udx * vdy;
    if denom == 0.0 {
        return None;
    }
    let s = (vdx * uvdy - vdy * uvdx) / denom;
    let t = (udx * uvdy - udy * uvdx) / denom;

    if s < 0.0 || s > 1.0 || t < 0.0 || t > 1.0 {
        return None;
    }
    Some(Point { X: u0.X + math::Round(s * udx), Y: u0.Y + math::Round(s * udy) })
}

/// `geo.RemovePoints(points, toRemove)`
pub fn RemovePoints(points: &[PointRef], toRemove: &[bool]) -> Points {
    let mut newLen = points.len();
    for &should in toRemove {
        if should {
            newLen -= 1;
        }
    }
    let mut without = Vec::with_capacity(newLen);
    for i in 0..points.len() {
        if toRemove[i] {
            continue;
        }
        without.push(points[i].clone());
    }
    without
}
