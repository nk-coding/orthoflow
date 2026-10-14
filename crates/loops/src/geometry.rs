// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/loops/geometry.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

pub(crate) fn nonNilEquals(first: Option<&geo::Point>, second: Option<&geo::Point>) -> bool {
    match (first, second) {
        (Some(a), Some(b)) => *a == *b,
        _ => false,
    }
}
