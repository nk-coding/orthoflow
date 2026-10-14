// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/graphbounds/doc.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Package graphbounds computes work-accounted bounds for mutable TALA nodes.
//!
//! It owns the exact node, outside-label, icon, loop, modifier, and fixed-origin
//! geometry shared by packing and routing. Placement policy and graph-wide edge
//! bounds remain with their respective layout domains.
#[allow(unused_imports)]
use crate::*;
