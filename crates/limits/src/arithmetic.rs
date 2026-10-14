// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/limits/arithmetic.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

pub fn CheckedAddUint64(a: u64, b: u64) -> (u64, bool) {
    if u64::MAX - a < b {
        return (0, false);
    }
    (a + b, true)
}

pub fn CheckedMulUint64(a: u64, b: u64) -> (u64, bool) {
    if a != 0 && u64::MAX / a < b {
        return (0, false);
    }
    (a.wrapping_mul(b), true)
}
