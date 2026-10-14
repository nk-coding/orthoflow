// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/layoutgraph/hub_spoke.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

impl GraphRef {
    /// HubOrder returns hub sentinels in stable ID order.
    pub fn HubOrder(&self) -> Vec<NodeRef> {
        let hubs = self.b().Hubs.clone();
        let mut order: Vec<NodeRef> = hubs.keys();
        sortNodesByID(&mut order);
        order
    }
}
