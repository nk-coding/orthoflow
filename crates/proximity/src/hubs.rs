// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/proximity/hubs.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

/// AddHubs discovers nodes that have both a leaf spoke and another connection
/// within the same containing layout group.
pub fn AddHubs(ctx: &Ctx, graph: &GraphRef) -> GoResult<()> {
    layoutgraph::Validate(ctx, "AddHubs", graph)?;
    let guard = limits::NewWorkGuard(ctx, "AddHubs", limits::MaxEngineWorkUnits)?;
    let hubs: layoutgraph::GoMap<NodeRef, Vec<NodeRef>> = layoutgraph::GoMap::new();
    for node in graph.nodes() {
        guard.Step()?;
        let mut hasConnected = false;
        let mut spokes: Vec<NodeRef> = Vec::new();
        let owning = node.OwningContainer();
        for edge in node.edges() {
            guard.Step()?;
            let adjacent = node.Adjacent(&edge);
            if adjacent.OwningContainer() != owning {
                continue;
            }
            if adjacent.b().Edges.len() == 1 {
                spokes.push(adjacent);
            } else {
                hasConnected = true;
            }
        }
        if hasConnected && !spokes.is_empty() {
            hubs.set(node.clone(), spokes);
        }
    }
    guard.Finish()?;
    graph.bm().Hubs = hubs;
    Ok(())
}
