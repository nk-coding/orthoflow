// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from the following files of terrastruct/d2, Copyright 2022 Terrastruct Inc.:
//   - lib/geo/ellipse_test.go
//   - lib/geo/point_test.go
//   - lib/geo/segment_test.go
//   - lib/geo/vector_test.go
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;
use gocompat::math;

fn veq(a: &[f64], other: &[f64]) -> bool {
    if a.len() != other.len() {
        return false;
    }
    for i in 0..a.len() {
        if PrecisionCompare(a[i], other[i], PRECISION) != 0 {
            return false;
        }
    }
    true
}

#[test]
fn TestPointDistanceTo() {
    let p1 = NewPoint(0.0, 0.0);
    let p2 = NewPoint(100.0, 0.0);
    let p = NewPoint(50.0, 70.0);
    assert_eq!(p.DistanceToLine(&p1, &p2), 70.0);
}

#[test]
fn TestAddVector() {
    let start = NewPoint(1.5, 5.3);
    let c = NewVector(&[-3.5, -2.3]);
    let p2 = start.AddVector(&c);
    assert!(p2.X() == -2.0 && p2.Y() == 3.0, "{:?}", p2.get());
}

#[test]
fn TestToVector() {
    let p = NewPoint(3.5, 6.7);
    let v = p.ToVector();
    assert_eq!(v, vec![3.5, 6.7]);
}

#[test]
fn TestVectorTo() {
    let p1 = NewPoint(1.5, 5.3);
    let p2 = NewPoint(-2.0, 3.0);
    assert!(veq(&p1.VectorTo(&p2), &[-3.5, -2.3]));
    assert!(veq(&p2.VectorTo(&p1), &[3.5, 2.3]));
}

#[test]
fn TestSegmentIntersections() {
    let s1 = NewSegment(NewPoint(0.0, 0.0), NewPoint(10.0, 10.0));
    let s2 = NewSegment(NewPoint(0.0, 10.0), NewPoint(10.0, 0.0));
    let i = s1.Intersections(&s2.get());
    assert_eq!(i.len(), 1);
    assert!(i[0].Equals(&NewPoint(5.0, 5.0)));
    let s3 = NewSegment(NewPoint(10.0, 10.0), NewPoint(10.0, 0.0));
    let i = s1.Intersections(&s3.get());
    assert_eq!(i.len(), 1);
    assert!(i[0].Equals(&NewPoint(10.0, 10.0)));
    let s4 = NewSegment(NewPoint(0.0, 0.0), NewPoint(0.0, 10.0));
    let i = s1.Intersections(&s4.get());
    assert_eq!(i.len(), 1);
    assert!(i[0].Equals(&NewPoint(0.0, 0.0)));
    let s5 = NewSegment(NewPoint(3.0, 8.0), NewPoint(2.0, 15.0));
    assert_eq!(s1.Intersections(&s5.get()).len(), 0);
}

fn ext(p1: (f64, f64), p2: (f64, f64), m: f64, e2: (f64, f64), e1: (f64, f64)) {
    let p1 = NewPoint(p1.0, p1.1);
    let p2 = NewPoint(p2.0, p2.1);
    let v = p1.VectorTo(&p2).Multiply(m);
    assert_eq!(p1.AddVector(&v).get(), Point::new(e2.0, e2.1));
    let v = p2.VectorTo(&p1).Multiply(m);
    assert_eq!(p2.AddVector(&v).get(), Point::new(e1.0, e1.1));
}

#[test]
fn TestExtendLineSegments() {
    ext((0.0, 0.0), (0.0, 1.0), 2.0, (0.0, 2.0), (0.0, -1.0));
    ext((0.0, 0.0), (1.0, 0.0), 1.5, (1.5, 0.0), (-0.5, 0.0));
    ext((0.0, 0.0), (3.0, 1.0), 2.0, (6.0, 2.0), (-3.0, -1.0));
}

#[test]
fn TestVectorOps() {
    let a = NewVector(&[1.0, 2.0]);
    let b = NewVector(&[3.0, 4.0]);
    assert!(veq(&a.Add(&b), &[4.0, 6.0]));
    assert!(veq(&a.Minus(&b), &[-2.0, -2.0]));
    assert!(veq(&a.Multiply(3.0), &[3.0, 6.0]));
    assert_eq!(NewVector(&[3.0, 4.0]).Length(), 5.0);
    assert!(veq(&NewVectorFromProperties(3.0, math::Pi / 3.0), &[2.59807621135, 1.5]));
    assert!(veq(&NewVectorFromProperties(3.0, -math::Pi / 3.0), &[-2.59807621135, 1.5]));
    assert!(veq(&NewVectorFromProperties(3.0, math::Pi * 2.0 / 3.0), &[2.59807621135, -1.5]));
    assert!(veq(&NewVectorFromProperties(3.0, -math::Pi * 2.0 / 3.0), &[-2.59807621135, -1.5]));
    assert!(veq(&NewVector(&[3.0, 4.0]).Unit(), &[3.0 / 5.0, 4.0 / 5.0]));
    assert_eq!(PrecisionCompare(NewVector(&[3.0, 4.0]).AddLength(8.0).Length(), 13.0, PRECISION), 0);
    let v = NewVector(&[3.789, -0.731]);
    let p = v.ToPoint();
    assert_eq!(p.X(), v[0]);
    assert_eq!(p.Y(), v[1]);
}

fn seg(a: (f64, f64), b: (f64, f64)) -> Segment {
    Segment::new(NewPoint(a.0, a.1), NewPoint(b.0, b.1))
}

#[test]
fn TestEllipseLineIntersections() {
    let e = NewEllipse(NewPoint(0.0, 0.0), 11.0, 11.0);
    let i = e.Intersections(&seg((0.0, 20.0), (0.0, -20.0)));
    assert!(i.len() == 2 && i[0].Equals(&NewPoint(0.0, 11.0)) && i[1].Equals(&NewPoint(0.0, -11.0)));
    assert_eq!(e.Intersections(&seg((0.0, 2.0), (0.0, -2.0))).len(), 0);
    assert_eq!(e.Intersections(&seg((2.0, 2.0), (5.0, 5.0))).len(), 0);
    let i = e.Intersections(&seg((2.0, 2.0), (50.0, 50.0)));
    // Go: x := math.Sqrt2 / 2 * 11 (constant-folded exactly)
    let x = f64::from_bits(0x401f1cd9cceef239);
    // runtime evaluation differs by 1 ulp from Go's exact constant folding:
    assert_eq!((math::Sqrt2 / 2.0 * 11.0).to_bits(), 0x401f1cd9cceef23a);
    assert!(i.len() == 1 && i[0].Equals(&NewPoint(x, x)), "{:?}", i.iter().map(|p| p.get()).collect::<Vec<_>>());
    let e = NewEllipse(NewPoint(100.0, 200.0), 21.0, 21.0);
    assert_eq!(e.Intersections(&seg((0.0, 0.0), (100.0, 150.0))).len(), 0);
    assert_eq!(e.Intersections(&seg((50.0, 150.0), (200.0, 250.0))).len(), 2);
    assert_eq!(e.Intersections(&seg((0.0, 221.0), (200.0, 221.0))).len(), 1);
    assert_eq!(e.Intersections(&seg((121.0, 100.0), (121.0, 300.0))).len(), 1);
    // Go constant 2/math.Sqrt2 folds to Sqrt2 exactly (runtime division differs)
    let r = math::Sqrt2;
    assert!(NewEllipse(NewPoint(1.0, 1.0), r, r).Intersections(&seg((1.0, 3.0), (3.0, 1.0))).len() != 0);
}

#[test]
fn TestBoxAliasing() {
    let tl = NewPoint(1.0, 2.0);
    let b = Box::new(tl.clone(), 10.0, 20.0);
    let b2 = b.clone();
    tl.bm().X = 5.0;
    assert_eq!(b2.TL().X(), 5.0);
    assert_eq!(b.Center().get(), Point::new(10.0, 12.0));
    let br = NewBox(tl.clone(), 1.0, 1.0);
    let c = br.Copy();
    tl.bm().X = 7.0;
    assert_eq!(c.TL().X(), 5.0);
    assert!(NewBox(None, 1.0, 1.0).TopLeft().is_none());
}
