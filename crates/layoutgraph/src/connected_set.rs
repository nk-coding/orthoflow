// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/layoutgraph/connected_set.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

/// connectedOwners stores the usual single parent without a slice allocation.
pub(crate) struct connectedOwners {
    pub(crate) first: Map<NodeRef, NodeRef>,
    pub(crate) more: Map<NodeRef, Vec<NodeRef>>,
}

impl connectedOwners {
    pub(crate) fn add(&mut self, child: &NodeRef, parent: &NodeRef) {
        if !self.first.contains_key(child) {
            self.first.insert(child.clone(), parent.clone());
            return;
        }
        self.more.entry(child.clone()).or_default().push(parent.clone());
    }
}

impl NodeRef {
    /// ConnectedNodeSet returns the same reachable set as ConnectedNodes, with no
    /// traversal-order guarantee. Use it only when all members are treated equally;
    /// order-sensitive nearest-node tie breaking must continue using ConnectedNodes.
    pub fn ConnectedNodeSet(&self, excludedNodes: &[NodeRef], graph: &GraphRef) -> Vec<NodeRef> {
        let node = self;
        let (gnodesLen, clusters, containers) = {
            let g = graph.b();
            (g.Nodes.len(), g.Clusters.clone(), g.Containers.clone())
        };
        // Tiny graphs and flat graphs have too little ownership scanning to amortize
        // an index. Keep their existing traversal.
        if gnodesLen < 32 || clusters.len() == 0 && containers.len() <= 1 {
            return node.connectedNodes(excludedNodes, graph);
        }
        let mut excluded: Set<NodeRef> = Set::default();
        for n in excludedNodes {
            excluded.insert(n.clone());
        }
        let mut excludedChildren: Map<NodeRef, bool> = Map::default();
        let mut owners = connectedOwners { first: Map::default(), more: Map::default() };
        for (container, children) in containers.entries() {
            let Some(container) = container else { continue };
            let mut blocked = false;
            for child in &children {
                if excluded.contains(child) {
                    blocked = true;
                    break;
                }
            }
            excludedChildren.insert(container.clone(), blocked);
            if blocked || excluded.contains(&container) {
                continue;
            }
            for child in &children {
                owners.add(child, &container);
            }
        }
        for (vessel, cluster) in clusters.entries() {
            let members = cluster.b().Nodes.clone();
            for member in &members {
                if !member.b().isContainer || excludedChildren.get(member).copied().unwrap_or(false) {
                    continue;
                }
                for child in containers.at(&Some(member.clone())) {
                    owners.add(&child, &vessel);
                }
            }
        }
        let mut nodes: Vec<NodeRef> = Vec::new();
        let mut queue: Vec<NodeRef> = vec![node.clone()];
        let mut seen: Set<NodeRef> = Set::default();
        seen.insert(node.clone());
        let add = |n: &NodeRef, queue: &mut Vec<NodeRef>, seen: &mut Set<NodeRef>| {
            if !seen.contains(n) {
                seen.insert(n.clone());
                queue.push(n.clone());
            }
        };
        let mut head = 0usize;
        while head < queue.len() {
            let current = queue[head].clone();
            head += 1;
            let mut blocked = false;
            for e in excludedNodes {
                if isDescendantOf(Some(&current), Some(e)) || isDescendantOf(Some(e), Some(&current)) && !isDescendantOf(Some(node), Some(&current)) {
                    blocked = true;
                    break;
                }
            }
            if blocked {
                continue;
            }
            nodes.push(current.clone());
            for edge in current.edges() {
                let adjacent = current.adjacent(&edge);
                if !excluded.contains(&adjacent) {
                    add(&adjacent, &mut queue, &mut seen);
                }
            }
            if current.b().isClusterVessel {
                let cluster = clusters.get(&current).expect("nil pointer dereference: cluster");
                let members = cluster.b().Nodes.clone();
                for member in &members {
                    if member.b().isContainer {
                        for child in containers.at(&Some(member.clone())) {
                            add(&child, &mut queue, &mut seen);
                        }
                    }
                }
            }
            if let Some(owner) = owners.first.get(&current).cloned() {
                add(&owner, &mut queue, &mut seen);
            }
            if let Some(more) = owners.more.get(&current).cloned() {
                for owner in &more {
                    add(owner, &mut queue, &mut seen);
                }
            }
            if !excluded.contains(&current) && !excludedChildren.get(&current).copied().unwrap_or(false) {
                for child in containers.at(&Some(current.clone())) {
                    add(&child, &mut queue, &mut seen);
                }
            }
        }
        nodes
    }
}
