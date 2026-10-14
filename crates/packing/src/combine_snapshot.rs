// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/packing/combine_snapshot.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

#[derive(Clone)]
pub(crate) struct combineNodePositionSnapshot {
    pub(crate) node: NodeRef,
    pub(crate) topLeft: pointerSnapshot,
}

pub(crate) fn restoreCombineNodePositions(snapshots: &[combineNodePositionSnapshot]) {
    for snapshot in snapshots {
        let restored = snapshot.topLeft.restore();
        snapshot.node.set_tl(restored);
    }
}

/// snapshotCombineNodePositions captures every position that CombineSubgraphs
/// can translate while preserving each point's exact identity.
pub(crate) fn snapshotCombineNodePositions(ctx: &Ctx, nodes: &[NodeRef]) -> GoResult<Vec<combineNodePositionSnapshot>> {
    let guard = limits::NewWorkGuard(ctx, "CombineSubgraphs", limits::MaxEngineWorkUnits)?;
    let mut seen: Set<NodeRef> = Set::default();
    let mut snapshots: Vec<combineNodePositionSnapshot> = Vec::with_capacity(nodes.len());
    let mut queue: Vec<(NodeRef, Option<GraphRef>)> = Vec::with_capacity(nodes.len());
    let mut enqueue = |node: &NodeRef,
                       graph: Option<GraphRef>,
                       seen: &mut Set<NodeRef>,
                       snapshots: &mut Vec<combineNodePositionSnapshot>,
                       queue: &mut Vec<(NodeRef, Option<GraphRef>)>|
     -> GoResult<()> {
        if seen.contains(node) {
            return Ok(());
        }
        if seen.len() as i64 >= limits::MaxEngineNodes {
            return Err(GoError::new(format!("TALA CombineSubgraphs unique node count exceeds limit {}", limits::MaxEngineNodes)));
        }
        guard.Step()?;
        seen.insert(node.clone());
        snapshots.push(combineNodePositionSnapshot { node: node.clone(), topLeft: snapshotPointer(node.tl().as_ref()) });
        queue.push((node.clone(), graph));
        Ok(())
    };
    for node in nodes {
        enqueue(node, node.graph(), &mut seen, &mut snapshots, &mut queue)?;
    }
    let mut index = 0;
    while index < queue.len() {
        let (node, graph) = queue[index].clone();
        index += 1;
        let Some(graph) = graph else {
            continue;
        };
        if node.IsContainer() {
            let children = graph.b().Containers.at(&Some(node.clone()));
            for child in &children {
                enqueue(child, Some(graph.clone()), &mut seen, &mut snapshots, &mut queue)?;
            }
        }
        if node.IsClusterVessel() {
            let cluster = graph.b().Clusters.get(&node);
            if let Some(cluster) = cluster {
                let cnodes = cluster.b().Nodes.clone();
                for child in &cnodes {
                    enqueue(child, Some(graph.clone()), &mut seen, &mut snapshots, &mut queue)?;
                }
            }
        }
        let sequence = graph.b().Sequences.get(&node);
        if let Some(sequence) = sequence {
            let snodes = sequence.b().Nodes.clone();
            for child in &snodes {
                enqueue(child, Some(graph.clone()), &mut seen, &mut snapshots, &mut queue)?;
            }
        }
    }
    guard.Finish()?;
    Ok(snapshots)
}
