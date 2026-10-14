// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/hierarchy/orientation.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

/// IsHorizontal reports the orientation shared by hierarchy placement and
/// hierarchy-aware routing.
pub fn IsHorizontal(nodes: &[NodeRef]) -> bool {
    if nodes.is_empty() {
        return false;
    }
    if nodes[0].ContainerDirection().IsHorizontal() {
        return true;
    }
    for node in nodes {
        if node.IsTable() {
            return true;
        }
    }
    false
}
