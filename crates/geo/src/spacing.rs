// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from lib/geo/spacing.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Spacing {
    pub Top: f64,
    pub Bottom: f64,
    pub Left: f64,
    pub Right: f64,
}
