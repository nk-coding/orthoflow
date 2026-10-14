// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/layoutgraph/topology_api.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

#[allow(unused_imports)]
use crate::*;

/// MaxTopologyDepth is the maximum supported depth for layout ownership and
/// ancestry relations.
pub const MaxTopologyDepth: i64 = maxEngineTopologyDepth;
