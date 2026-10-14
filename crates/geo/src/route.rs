// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from lib/geo/route.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;
use gocompat::math;

/// Go `geo.Route` (`[]*Point`).
pub type Route = Vec<PointRef>;

/// Methods of `geo.Route`.
pub trait RouteExt {
    fn Length(&self) -> f64;
    fn GetPointAtDistance(&self, distance: f64) -> (PointRef, i64);
    fn GetBoundingBox(&self) -> (PointRef, PointRef);
}

impl RouteExt for [PointRef] {
    fn Length(&self) -> f64 {
        let route = self;
        let mut l = 0.0;
        let n = route.len() as i64;
        let mut i = 0i64;
        while i < n - 1 {
            let a = route[i as usize].b();
            let b = route[i as usize + 1].b();
            l += EuclideanDistance(a.X, a.Y, b.X, b.Y);
            i += 1;
        }
        l
    }

    fn GetPointAtDistance(&self, distance: f64) -> (PointRef, i64) {
        let route = self;
        let mut remaining = distance;
        let mut curr: Option<PointRef> = None;
        let mut next: Option<PointRef> = None;
        let mut length: f64 = 0.0;
        let n = route.len() as i64;
        let mut i = 0i64;
        while i < n - 1 {
            let c = route[i as usize].clone();
            let nx = route[i as usize + 1].clone();
            {
                let (cb, nb) = (c.b(), nx.b());
                length = EuclideanDistance(cb.X, cb.Y, nb.X, nb.Y);
            }
            curr = Some(c);
            next = Some(nx);
            if remaining <= length {
                let t = remaining / length;
                return (curr.unwrap().Interpolate(next.as_ref().unwrap(), t), i);
            }
            remaining -= length;
            i += 1;
        }
        // Go dereferences nil here when the route has fewer than 2 points (panic).
        let curr = curr.expect("nil pointer dereference");
        let next = next.expect("nil pointer dereference");
        (curr.Interpolate(&next, 1.0 + remaining / length), n - 2)
    }

    fn GetBoundingBox(&self) -> (PointRef, PointRef) {
        let mut minX = math::Inf(1);
        let mut minY = math::Inf(1);
        let mut maxX = math::Inf(-1);
        let mut maxY = math::Inf(-1);
        for p in self {
            let p = p.b();
            if p.X < minX {
                minX = p.X;
            }
            if p.X > maxX {
                maxX = p.X;
            }
            if p.Y < minY {
                minY = p.Y;
            }
            if p.Y > maxY {
                maxY = p.Y;
            }
        }
        (NewPoint(minX, minY), NewPoint(maxX, maxY))
    }
}
