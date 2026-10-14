// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/proximity/common_uncle.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

/// CommonUncleSiblings finds siblings that connect to the same node at their
/// parent's level. Placement uses the largest such set as a proximity hint.
pub fn CommonUncleSiblings(graph: &GraphRef) -> layoutgraph::GoMap<NodeRef, layoutgraph::Nodes> {
    let mut uncleToCousins: Map<NodeRef, Vec<NodeRef>> = Map::default();
    let mut common: Map<NodeRef, Vec<NodeRef>> = Map::default();
    let mut orderedUncles: Vec<NodeRef> = Vec::new();
    for container in graph.ContainerRDFSOrderUnbounded(None) {
        let children = graph.containers().at(&Some(container.clone()));
        let containerContainer = container.b().Container.clone();
        for node in &children {
            for edge in node.edges() {
                let adjacent = node.Adjacent(&edge);
                if adjacent.b().Container == containerContainer {
                    if !uncleToCousins.contains_key(&adjacent) {
                        orderedUncles.push(adjacent.clone());
                        uncleToCousins.insert(adjacent.clone(), Vec::new());
                    }
                    let list = uncleToCousins.get_mut(&adjacent).unwrap();
                    if !list.contains(node) {
                        list.push(node.clone());
                    }
                }
            }
        }
        for uncle in &orderedUncles {
            let siblings = uncleToCousins.get(uncle).cloned().unwrap_or_default();
            if siblings.len() < 2 {
                continue;
            }
            for sibling in &siblings {
                let replace = match common.get(sibling) {
                    None => true,
                    Some(existing) => existing.len() < siblings.len(),
                };
                if replace {
                    common.insert(sibling.clone(), siblings.clone());
                }
            }
        }
        uncleToCousins = Map::default();
    }
    layoutgraph::GoMap::from_map(common)
}
