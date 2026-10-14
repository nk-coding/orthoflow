// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/layoutgraph/id.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

#[allow(unused_imports)]
use crate::*;

/// EntityID identifies a node or edge independently of the host architecture.
///
/// TALA IDs are serialized and participate in deterministic ordering, so their
/// width must not depend on whether the engine is running natively or in WASM.
pub type EntityID = i64;
