// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/layoutgraph/doc.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Package layoutgraph defines the mutable state used by TALA's layout
//! algorithms.
//!
//! The package is deliberately independent of the algorithms that mutate this
//! state. That dependency direction lets placement, routing, and other layout
//! domains share one graph model without depending on the engine pipeline.
#[allow(unused_imports)]
use crate::*;
