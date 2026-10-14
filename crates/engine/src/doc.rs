// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/engine/doc.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Package engine composes TALA's layout stages into one deterministic layout.
//!
//! Graph ownership belongs to layoutgraph, and individual algorithms live in
//! their domain packages. Engine owns only stage ordering and the internal
//! layout entry point.
