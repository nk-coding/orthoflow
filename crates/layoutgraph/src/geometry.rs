// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/layoutgraph/geometry.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

/// nonNilEquals compares two optional points by value.
pub(crate) fn nonNilEquals(first: Option<&geo::PointRef>, second: Option<&geo::PointRef>) -> bool {
    match (first, second) {
        (Some(a), Some(b)) => {
            let av = *a.b();
            let bv = *b.b();
            av.X == bv.X && av.Y == bv.Y
        }
        _ => false,
    }
}

/// nonNilEquals for point values held in place (e.g. `&segments[i].Start`).
#[inline]
pub(crate) fn pointValuesEqual(a: &geo::Point, b: &geo::Point) -> bool {
    a.X == b.X && a.Y == b.Y
}
