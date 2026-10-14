// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/routing/doc.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of routing/doc.go.
//!
//! Package routing builds orthogonal visibility graphs, searches them for edge
//! routes, and applies the routing post-processing stages.
#[allow(unused_imports)]
use crate::*;

// Ported Go unit tests for routing half b (coordination + post-processing).
#[cfg(test)]
#[path = "tests_b/mod.rs"]
mod tests_b;
