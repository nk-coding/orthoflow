// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/proximity/nears.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

/// AssignNears marks otherwise unconnected siblings that share an external
/// neighbor so placement keeps them close together.
pub fn AssignNears(ctx: &Ctx, graph: &GraphRef, root: Option<&NodeRef>, abductions: &[EdgeAbductionRef]) -> GoResult<()> {
    assignNearsWithWorkLimit(ctx, graph, root, abductions, limits::MaxEngineWorkUnits)
}

fn topologyLimitErr() -> GoError {
    errorf!("TALA AssignNears topology references exceed limit {}", layoutgraph::MaxTopologyReferences)
}

pub(crate) fn assignNearsWithWorkLimit(
    ctx: &Ctx,
    graph: &GraphRef,
    root: Option<&NodeRef>,
    abductions: &[EdgeAbductionRef],
    workLimit: i64,
) -> GoResult<()> {
    let guard = limits::NewWorkGuard(ctx, "AssignNears", workLimit)?;
    let mut uncles: Map<NodeRef, Set<NodeRef>> = Map::default();
    let rootChildren = graph.containers().at(&root.cloned());
    for abduction in abductions {
        guard.Step()?;
        let a = abduction.b().clone();
        let originallyFrom = groupVessel(a.OriginallyFrom.as_ref());
        let originallyTo = groupVessel(a.OriginallyTo.as_ref());
        let mut uncle: Option<NodeRef> = None;
        let mut connected: Option<NodeRef> = None;
        for node in &rootChildren {
            guard.Step()?;
            let fromDescendant = isDescendantOf(originallyFrom.as_ref(), Some(node), &guard)?;
            let toDescendant = isDescendantOf(originallyTo.as_ref(), Some(node), &guard)?;
            if fromDescendant {
                uncle = a.CurrentTo.clone();
                connected = Some(node.clone());
                break;
            } else if toDescendant {
                uncle = a.CurrentFrom.clone();
                connected = Some(node.clone());
                break;
            }
        }
        if let (Some(uncle), Some(connected)) = (uncle, connected) {
            uncles.entry(uncle).or_default().insert(connected);
        }
    }

    let mut orderedUncles: Vec<NodeRef> = Vec::with_capacity(uncles.len());
    for uncle in uncles.keys() {
        guard.Step()?;
        orderedUncles.push(uncle.clone());
    }
    layoutgraph::SortNodesByID(&mut orderedUncles);

    let mut replacementNears: Map<NodeRef, Set<NodeRef>> = Map::default();
    let mut nearReferences: i64 = 0;

    for uncle in &orderedUncles {
        let set = &uncles[uncle];
        let mut nodes: Vec<NodeRef> = Vec::with_capacity(set.len());
        for node in set {
            guard.Step()?;
            nodes.push(node.clone());
        }
        layoutgraph::SortNodesByID(&mut nodes);
        if nodes.len() == 1 {
            continue;
        }
        for i in 0..nodes.len() {
            let first = &nodes[i];
            for second in &nodes[i + 1..] {
                guard.Step()?;
                if first.b().Hierarchy.is_some() || second.b().Hierarchy.is_some() {
                    continue;
                }
                // hasConnection
                let mut connected = false;
                for edge in first.edges() {
                    guard.Step()?;
                    if first.Adjacent(&edge) == *second {
                        connected = true;
                        break;
                    }
                }
                if connected {
                    continue;
                }
                for n in [first, second] {
                    if !replacementNears.contains_key(n) {
                        let nears: Vec<NodeRef> = n.b().Nears.iter().cloned().collect();
                        if nears.len() as i64 > layoutgraph::MaxTopologyReferences - nearReferences {
                            return Err(topologyLimitErr());
                        }
                        nearReferences += nears.len() as i64;
                        let mut replacement: Set<NodeRef> = gocompat::NewSetCap(nears.len() + 1);
                        for near in nears {
                            guard.Step()?;
                            replacement.insert(near);
                        }
                        replacementNears.insert(n.clone(), replacement);
                    }
                }
                if !replacementNears[first].contains(second) {
                    if nearReferences >= layoutgraph::MaxTopologyReferences {
                        return Err(topologyLimitErr());
                    }
                    replacementNears.get_mut(first).unwrap().insert(second.clone());
                    nearReferences += 1;
                }
                if !replacementNears[second].contains(first) {
                    if nearReferences >= layoutgraph::MaxTopologyReferences {
                        return Err(topologyLimitErr());
                    }
                    replacementNears.get_mut(second).unwrap().insert(first.clone());
                    nearReferences += 1;
                }
            }
        }
    }
    guard.Finish()?;

    let mut commitOrder: Vec<NodeRef> = replacementNears.keys().cloned().collect();
    layoutgraph::SortNodesByID(&mut commitOrder);
    let mut originals: Vec<(NodeRef, Set<NodeRef>)> = Vec::new();
    for node in &commitOrder {
        let replacement = replacementNears.remove(node).unwrap();
        let original = std::mem::replace(&mut node.bm().Nears, replacement);
        originals.push((node.clone(), original));
        if let Err(err) = guard.Finish() {
            for (n, nears) in originals {
                n.bm().Nears = nears;
            }
            return Err(err);
        }
    }
    Ok(())
}

pub(crate) fn groupVessel(node: Option<&NodeRef>) -> Option<NodeRef> {
    let node = node?;
    let (cluster, sequence) = {
        let n = node.b();
        (n.Cluster.clone(), n.Sequence.clone())
    };
    if let Some(c) = cluster {
        return Some(c.b().Vessel.clone());
    }
    if let Some(s) = sequence {
        return Some(s.b().Vessel.clone());
    }
    Some(node.clone())
}

pub(crate) fn parentOf(node: &NodeRef) -> Option<NodeRef> {
    let (container, cluster, sequence) = {
        let n = node.b();
        (n.Container.clone(), n.Cluster.clone(), n.Sequence.clone())
    };
    if container.is_some() {
        container
    } else if let Some(c) = cluster {
        Some(c.b().Vessel.clone())
    } else if let Some(s) = sequence {
        Some(s.b().Vessel.clone())
    } else {
        None
    }
}

pub(crate) fn isDescendantOf(descendant: Option<&NodeRef>, ancestor: Option<&NodeRef>, guard: &WorkGuardRef) -> GoResult<bool> {
    let mut descendant = descendant.cloned();
    loop {
        guard.Step()?;
        if descendant.as_ref() == ancestor {
            return Ok(true);
        }
        let Some(d) = descendant else { return Ok(false) };
        descendant = parentOf(&d);
    }
}
