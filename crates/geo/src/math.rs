// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from lib/geo/math.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;
use gocompat::math;

pub fn EuclideanDistance(x1: f64, y1: f64, x2: f64, y2: f64) -> f64 {
    if x1 == x2 {
        math::Abs(y1 - y2)
    } else if y1 == y2 {
        math::Abs(x1 - x2)
    } else {
        math::Sqrt((x1 - x2) * (x1 - x2) + (y1 - y2) * (y1 - y2))
    }
}

pub fn PrecisionCompare(a: f64, b: f64, e: f64) -> i64 {
    if math::Abs(a - b) < e {
        return 0;
    }
    if a < b {
        return -1;
    }
    1
}

/// `float64(int(v*1000)) / 1000`
pub fn TruncateDecimals(v: f64) -> f64 {
    math::F64ToInt(v * 1000.0) as f64 / 1000.0
}

pub fn Sign(i: f64) -> i64 {
    if i < 0.0 {
        return -1;
    }
    if i > 0.0 {
        return 1;
    }
    0
}
