// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/trees/traversal.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

/// Descendants returns tree descendants in the breadth-first order used when
/// restored tree nodes and edges are copied back into an owning graph.
pub fn Descendants(root: Option<&TreeRef>) -> Vec<TreeRef> {
    let Some(root) = root else { return Vec::new() };
    let mut descendants: Vec<TreeRef> = Vec::new();
    let mut queue: std::collections::VecDeque<TreeRef> = std::collections::VecDeque::new();
    queue.push_back(root.clone());
    while let Some(current) = queue.pop_front() {
        let children = current.b().Children.clone();
        for child in children {
            descendants.push(child.clone());
            queue.push_back(child);
        }
    }
    descendants
}
