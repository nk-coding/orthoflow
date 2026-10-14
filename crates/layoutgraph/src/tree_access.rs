// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/layoutgraph/tree_access.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

impl NodeRef {
    pub fn IsClusterVessel(&self) -> bool {
        self.b().isClusterVessel
    }

    /// FixedOrigin returns the fixed-position origin inherited through a node's
    /// descendants. Tree placement uses it to preserve D2 position constraints.
    pub fn FixedOrigin(&self) -> Option<geo::PointRef> {
        self.fixedOrigin()
    }

    /// AddIncidentEdgeUnchecked restores an already graph-owned edge to a node's
    /// incidence list. Tree preprocessing validates ownership before calling it.
    pub fn AddIncidentEdgeUnchecked(&self, edge: &EdgeRef) {
        self.addEdge(edge);
    }
}

impl EdgeRef {
    pub fn ArrowheadTo(&self, node: &NodeRef) -> Arrowhead {
        self.arrowheadTo(node)
    }

    pub fn HasArrowTo(&self, node: &NodeRef) -> bool {
        self.hasArrowTo(node)
    }
}

impl GraphRef {
    /// NewRequestTransaction creates a request-scoped placement transaction. Tree
    /// trials share the same transaction accounting and rollback semantics as the
    /// rest of the engine.
    pub fn NewRequestTransaction(&self, ctx: &Ctx, options: TransactionOptions) -> GoResult<TransactionRef> {
        self.newRequestTransaction(ctx, options)
    }

    /// buildNodeToTreeGuarded derives the auxiliary node index while preserving
    /// graphjson's established validation order and shared decode work budget.
    pub(crate) fn buildNodeToTreeGuarded(&self, guard: &dyn workStepper) -> GoResult<()> {
        let trees = self.b().Trees.clone();
        let nodeToTree = buildNodeToTree(&trees, guard)?;
        self.bm().NodeToTree = GoMap::from_map(nodeToTree);
        Ok(())
    }
}

pub(crate) fn buildNodeToTree(trees: &GoMap<Option<NodeRef>, Vec<TreeRef>>, guard: &dyn workStepper) -> GoResult<Map<NodeRef, TreeRef>> {
    let mut nodeToTree: Map<NodeRef, TreeRef> = Map::default();
    let mut order: Vec<Option<NodeRef>> = Vec::with_capacity(trees.len());
    for rootSentinel in trees.keys() {
        guard.Step()?;
        order.push(rootSentinel);
    }
    sortOptNodesByID(&mut order);
    guard.Finish()?;
    for rootSentinel in &order {
        guard.Step()?;
        for root in trees.at(rootSentinel) {
            let mut queue: Vec<TreeRef> = vec![root.clone()];
            let mut seen: Set<TreeRef> = Set::default();
            let mut index = 0usize;
            while index < queue.len() {
                guard.Step()?;
                let current = queue[index].clone();
                if seen.contains(&current) {
                    return Err(invariant::New("tree preprocessing encountered a repeated tree node"));
                }
                seen.insert(current.clone());
                let (node, children) = {
                    let c = current.b();
                    (c.Node.clone(), c.Children.clone())
                };
                nodeToTree.insert(node, current.clone());
                for child in children {
                    guard.Step()?;
                    queue.push(child);
                }
                index += 1;
            }
        }
    }
    guard.Finish()?;
    Ok(nodeToTree)
}

impl GraphStateRef {
    pub fn TracksNode(&self, node: &NodeRef) -> bool {
        self.b().nodes.contains_key(node)
    }
}
