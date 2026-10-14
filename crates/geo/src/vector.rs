// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from lib/geo/vector.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;
use gocompat::math;

/// Go `geo.Vector` (`[]float64`).
pub type Vector = Vec<f64>;

/// `geo.NewVector(components...)`
pub fn NewVector(components: &[f64]) -> Vector {
    components.to_vec()
}

pub fn NewVectorFromProperties(length: f64, angleInRadians: f64) -> Vector {
    vec![length * math::Sin(angleInRadians), length * math::Cos(angleInRadians)]
}

/// Methods of `geo.Vector`.
pub trait VectorExt {
    fn AddLength(&self, length: f64) -> Vector;
    fn Add(&self, b: &[f64]) -> Vector;
    fn Minus(&self, b: &[f64]) -> Vector;
    fn Multiply(&self, v: f64) -> Vector;
    fn Length(&self) -> f64;
    fn Unit(&self) -> Vector;
    fn ToPoint(&self) -> PointRef;
    fn Radians(&self) -> f64;
    fn Degrees(&self) -> f64;
    fn Reverse(&self) -> Vector;
}

impl VectorExt for [f64] {
    fn AddLength(&self, length: f64) -> Vector {
        self.Unit().Multiply(self.Length() + length)
    }
    fn Add(&self, b: &[f64]) -> Vector {
        let mut c = Vec::new();
        for i in 0..self.len() {
            c.push(self[i] + b[i]);
        }
        c
    }
    fn Minus(&self, b: &[f64]) -> Vector {
        let mut c = Vec::new();
        for i in 0..self.len() {
            c.push(self[i] - b[i]);
        }
        c
    }
    fn Multiply(&self, v: f64) -> Vector {
        let mut c = Vec::new();
        for i in 0..self.len() {
            c.push(self[i] * v);
        }
        c
    }
    fn Length(&self) -> f64 {
        let mut sum = 0.0;
        for comp in self {
            sum += comp * comp;
        }
        math::Sqrt(sum)
    }
    fn Unit(&self) -> Vector {
        self.Multiply(1.0 / self.Length())
    }
    fn ToPoint(&self) -> PointRef {
        NewPoint(self[0], self[1])
    }
    fn Radians(&self) -> f64 {
        math::Atan2(self[1], self[0]) as f32 as f64
    }
    fn Degrees(&self) -> f64 {
        self.Radians() * 180.0 / math::Pi
    }
    fn Reverse(&self) -> Vector {
        self.Multiply(-1.0)
    }
}

pub(crate) fn getNormalVector(x1: f64, y1: f64, x2: f64, y2: f64) -> (f64, f64) {
    (y1 - y2, x2 - x1)
}

pub fn GetUnitNormalVector(x1: f64, y1: f64, x2: f64, y2: f64) -> (f64, f64) {
    let (normalX, normalY) = getNormalVector(x1, y1, x2, y2);
    let length = EuclideanDistance(x1, y1, x2, y2);
    (normalX / length, normalY / length)
}
