// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/placement/tuning.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of placement/tuning.go
use crate::*;

pub const compactionFactor: f64 = 3.0;
pub const nodePlacementIterations: i64 = 90;
pub const placementPadding: f64 = 1000.0;
pub const jitterThreshold: f64 = 80.0;
pub const signFlipPadding: f64 = 5.0;
pub const largeGapThreshold: f64 = 0.5;
