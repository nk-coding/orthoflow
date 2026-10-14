// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/layoutgraph/node_graph_ownership.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

pub(crate) const nodeGraphOwnershipSnapshotLocation: &str = "NodeGraphOwnershipSnapshot";

/// NodeGraphOwnershipSnapshot is a read-only capture of every pre-existing node
/// owner reachable from a graph's runtime topology.
#[derive(Clone, Default)]
pub struct NodeGraphOwnershipSnapshot {
    pub(crate) owners: Vec<nodeGraphOwner>,
}

#[derive(Clone)]
pub(crate) struct nodeGraphOwner {
    pub(crate) node: NodeRef,
    pub(crate) graph: Option<GraphRef>,
}

impl NodeGraphOwnershipSnapshot {
    /// Restore restores every graph pointer captured by the snapshot.
    pub fn Restore(&self) {
        for owner in &self.owners {
            owner.node.bm().Graph = owner.graph.clone();
        }
    }
}

/// SnapshotNodeGraphOwnership captures every node reachable through the graph's
/// runtime topology, including nodes referenced only by grouping, tree,
/// proximity, hub, or hierarchy state. It does not mutate the graph.
pub fn SnapshotNodeGraphOwnership(ctx: &Ctx, graph: Option<&GraphRef>) -> GoResult<NodeGraphOwnershipSnapshot> {
    let Some(graph) = graph else {
        return Err(invariant::New("node graph ownership snapshot requires a graph"));
    };
    let guard = limits::NewWorkGuard(ctx, nodeGraphOwnershipSnapshotLocation, maxEngineWorkUnits)?;
    let objects = collectRuntimeObjectsContext(graph, &guard, "ownership runtime")?;
    let mut owners: Vec<nodeGraphOwner> = Vec::with_capacity(objects.nodeOrder.len());
    for node in &objects.nodeOrder {
        guard.Step()?;
        owners.push(nodeGraphOwner { node: node.clone(), graph: node.graph() });
    }
    guard.Finish()?;
    Ok(NodeGraphOwnershipSnapshot { owners })
}
