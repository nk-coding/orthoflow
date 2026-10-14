// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/layoutgraph/neighbor_requirements.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

#[allow(unused_imports)]
use crate::*;

/// LongDistanceNeighborRequirements records the space a node needs to expose
/// its parallel edges to a distant neighbor.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LongDistanceNeighborRequirements {
    pub EdgeCount: i64,
    pub MaxWidth: i64,
    pub MaxHeight: i64,
}
