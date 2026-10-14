// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/routing/filter.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of routing/filter.go.
#[allow(unused_imports)]
use crate::*;
use layoutgraph::{EdgeRef, NodeRef, Nodes};

/// filterEdgeAncestors removes container ancestors of either endpoint from the
/// obstacle set used to route an edge. The endpoints themselves remain so
/// arrowhead-label placement can still account for their boxes.
pub(crate) fn filterEdgeAncestors(edge: &EdgeRef, nodes: &[NodeRef]) -> Nodes {
    let (from, to) = {
        let e = edge.b();
        (e.From.clone(), e.To.clone())
    };
    let mut nonAncestors: Nodes = Vec::new();
    for node in nodes {
        if *node != from && *node != to && (from.IsDescendantOf(Some(node)) || to.IsDescendantOf(Some(node))) {
            continue;
        }
        nonAncestors.push(node.clone());
    }
    nonAncestors
}
