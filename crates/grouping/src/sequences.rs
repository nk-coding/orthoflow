// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/grouping/sequences.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

pub(crate) fn identifySequences(graph: &GraphRef, nodes: &[NodeRef], guard: &WorkGuardRef) -> GoResult<Vec<Vec<NodeRef>>> {
    let graphNodes = graph.nodes();
    let mut activeNodes: Set<NodeRef> = gocompat::NewSetCap(graphNodes.len());
    for node in &graphNodes {
        guard.Step()?;
        activeNodes.insert(node.clone());
    }
    let mut stepNodes: Vec<NodeRef> = Vec::new();
    for node in nodes {
        guard.Step()?;
        if !activeNodes.contains(node) || node.IsContainer() {
            continue;
        }
        let seq = node.b().Sequence.clone();
        if let Some(seq) = seq {
            if !seq.IsActive() {
                continue;
            }
        }
        if node.b().FixedTopLeft.is_some() || !node.IsSequenceStep() {
            continue;
        }
        stepNodes.push(node.clone());
    }
    if stepNodes.len() <= 1 {
        return Ok(Vec::new());
    }

    let mut sequences: Vec<Vec<NodeRef>> = Vec::new();
    let mut steps: Vec<NodeRef> = vec![stepNodes[0].clone()];
    for i in 1..stepNodes.len() {
        guard.Step()?;
        let (previous, current) = (&stepNodes[i - 1], &stepNodes[i]);
        let mut connected = false;
        for edge in previous.edges() {
            guard.Step()?;
            if previous.Adjacent(&edge) == *current {
                connected = true;
                break;
            }
        }
        if connected {
            steps.push(current.clone());
        } else {
            if steps.len() > 1 {
                sequences.push(steps);
            }
            steps = vec![current.clone()];
        }
    }
    if steps.len() > 1 {
        sequences.push(steps);
    }
    Ok(sequences)
}

/// SequenceDefiningEdges returns the IDs of edges consumed when step nodes are
/// replaced by sequence vessels.
pub fn SequenceDefiningEdges(ctx: &Ctx, graph: &GraphRef) -> GoResult<Set<EntityID>> {
    layoutgraph::Validate(ctx, "GetSequenceDefiningEdges", graph)?;
    let guard = limits::NewWorkGuard(ctx, "GetSequenceDefiningEdges", limits::MaxEngineWorkUnits)?;
    let containerOrder = graph.ContainerRDFSOrder(None, &guard)?;
    let mut containerOrder: Vec<Option<NodeRef>> = containerOrder.into_iter().map(Some).collect();
    containerOrder.push(None);
    let mut sequenceEdges: Set<EntityID> = Set::default();
    for container in &containerOrder {
        guard.Step()?;
        let children = graph.containers().at(container);
        let sequences = identifySequences(graph, &children, &guard)?;
        for steps in &sequences {
            for i in 1..steps.len() {
                let mut connection: Option<EdgeRef> = None;
                for edge in steps[i - 1].edges() {
                    guard.Step()?;
                    let (from, to) = (edge.from(), edge.to());
                    if (from == steps[i - 1] && to == steps[i]) || (to == steps[i - 1] && from == steps[i]) {
                        connection = Some(edge.clone());
                        break;
                    }
                }
                let Some(connection) = connection else {
                    return Err(errorf!(
                        "TALA sequence steps {} and {} have no defining edge",
                        steps[i - 1].id(),
                        steps[i].id()
                    ));
                };
                sequenceEdges.insert(connection.id());
            }
        }
    }
    guard.Finish()?;
    Ok(sequenceEdges)
}

pub(crate) fn isValidRememberedSequence(
    graph: &GraphRef,
    vessel: &NodeRef,
    sequence: Option<&SequenceRef>,
    activeNodes: &Set<NodeRef>,
    guard: &WorkGuardRef,
) -> GoResult<bool> {
    let Some(sequence) = sequence else { return Ok(false) };
    let (sVessel, sGraph, sNodes, sContainer) = {
        let s = sequence.b();
        (s.Vessel.clone(), s.Graph.clone(), s.Nodes.clone(), s.Container.clone())
    };
    if sequence.IsActive() || sVessel != *vessel || sGraph.as_ref() != Some(graph) || sNodes.len() < 2 {
        return Ok(false);
    }
    if let Some(c) = &sContainer {
        if !activeNodes.contains(c) {
            return Ok(false);
        }
    }
    let containers = graph.containers();
    let Some(children) = containers.get(&sContainer) else {
        return Ok(false);
    };
    let mut childIndex: Map<NodeRef, usize> = gocompat::NewMapCap(children.len());
    let mut duplicateChild: Set<NodeRef> = Set::default();
    for (i, child) in children.iter().enumerate() {
        guard.Step()?;
        if childIndex.contains_key(child) {
            duplicateChild.insert(child.clone());
            continue;
        }
        childIndex.insert(child.clone(), i);
    }
    let mut seen: Set<NodeRef> = gocompat::NewSetCap(sNodes.len());
    let mut previousIndex: i64 = -1;
    for node in &sNodes {
        guard.Step()?;
        if seen.contains(node) {
            return Ok(false);
        }
        seen.insert(node.clone());
        if !activeNodes.contains(node)
            || node.b().Graph.as_ref() != Some(graph)
            || !node.IsSequenceStep()
            || node.b().FixedTopLeft.is_some()
        {
            return Ok(false);
        }
        if node.b().Sequence.as_ref() != Some(sequence) || node.b().Container != sContainer {
            return Ok(false);
        }
        let index = childIndex.get(node).copied();
        let Some(index) = index else { return Ok(false) };
        if duplicateChild.contains(node) {
            return Ok(false);
        }
        if previousIndex >= 0 && index as i64 != previousIndex + 1 {
            return Ok(false);
        }
        previousIndex = index as i64;
    }
    Ok(true)
}

pub(crate) fn clearRememberedSequenceMembership(sequence: Option<&SequenceRef>, guard: &WorkGuardRef) -> GoResult<()> {
    let Some(sequence) = sequence else { return Ok(()) };
    let nodes = sequence.b().Nodes.clone();
    for node in &nodes {
        guard.Step()?;
        if node.b().Sequence.as_ref() == Some(sequence) {
            node.bm().Sequence = None;
        }
    }
    Ok(())
}

/// AddSequences replaces each discovered step run with a temporary vessel.
pub fn AddSequences(ctx: &Ctx, graph: &GraphRef, random: &RandRef) -> GoResult<()> {
    layoutgraph::Validate(ctx, "AddSequences", graph)?;
    let guard = limits::NewWorkGuard(ctx, "AddSequences", limits::MaxEngineWorkUnits)?;
    let state = layoutgraph::NewGraphStateSnapshot(layoutgraph::GraphStateSnapshotOptions {
        CaptureTopology: true,
        CaptureEdgeRoutes: true,
    });
    state.UpdateWithWorkGuard(graph, &guard)?;
    let result = addSequencesInner(graph, random, &guard);
    if result.is_err() {
        layoutgraph::RestoreGraphState(graph, &state);
    }
    result
}

fn addSequencesInner(graph: &GraphRef, random: &RandRef, guard: &WorkGuardRef) -> GoResult<()> {
    let mut remembered: Map<Option<NodeRef>, Vec<Vec<NodeRef>>> = Map::default();
    let mut rememberedIDByFirstStep: Map<NodeRef, EntityID> = Map::default();
    let mut reservedNodeIDs: Set<EntityID> = Set::default();
    let graphNodes = graph.nodes();
    let mut activeNodes: Set<NodeRef> = gocompat::NewSetCap(graphNodes.len());
    for node in &graphNodes {
        guard.Step()?;
        activeNodes.insert(node.clone());
        reservedNodeIDs.insert(node.id());
    }
    for (vessel, cluster) in graph.clusters().entries() {
        guard.Step()?;
        reservedNodeIDs.insert(vessel.id());
        let nodes = cluster.b().Nodes.clone();
        for node in &nodes {
            guard.Step()?;
            reservedNodeIDs.insert(node.id());
        }
    }
    for (vessel, sequence) in graph.sequences().entries() {
        guard.Step()?;
        reservedNodeIDs.insert(vessel.id());
        let nodes = sequence.b().Nodes.clone();
        for node in &nodes {
            guard.Step()?;
            reservedNodeIDs.insert(node.id());
        }
    }
    for vessel in graph.SequenceOrder() {
        guard.Step()?;
        let sequence = graph.sequences().get(&vessel);
        let valid = isValidRememberedSequence(graph, &vessel, sequence.as_ref(), &activeNodes, guard)?;
        if valid {
            let sequence = sequence.unwrap();
            let (container, nodes) = {
                let s = sequence.b();
                (s.Container.clone(), s.Nodes.clone())
            };
            rememberedIDByFirstStep.insert(nodes[0].clone(), vessel.id());
            remembered.entry(container).or_default().push(nodes);
        } else {
            clearRememberedSequenceMembership(sequence.as_ref(), guard)?;
        }
    }
    graph.bm().Sequences = layoutgraph::GoMap::new();

    let containerOrder = graph.ContainerRDFSOrder(None, guard)?;
    let mut order: Vec<Option<NodeRef>> = containerOrder.into_iter().map(Some).collect();
    order.push(None);
    for container in &order {
        guard.Step()?;
        let children = graph.containers().at(container);
        let mut groups = identifySequences(graph, &children, guard)?;
        if let Some(r) = remembered.get(container) {
            groups.extend(r.iter().cloned());
        }
        let children = graph.containers().at(container);
        let mut containerIndex: Map<NodeRef, i64> = gocompat::NewMapCap(children.len());
        for (i, node) in children.iter().enumerate() {
            guard.Step()?;
            containerIndex.insert(node.clone(), i as i64);
        }
        gocompat::sort::SlicesSortStableFunc(&mut groups, |a: &Vec<NodeRef>, b: &Vec<NodeRef>| {
            let ia = containerIndex.get(&a[0]).copied().unwrap_or(0);
            let ib = containerIndex.get(&b[0]).copied().unwrap_or(0);
            gocompat::sort::cmpCompare(ia, ib)
        });

        let mut generatedIDs: Vec<EntityID> = vec![0; groups.len()];
        let mut slotByID: Map<EntityID, usize> = gocompat::NewMapCap(groups.len());
        for i in 0..groups.len() {
            guard.Step()?;
            generatedIDs[i] = random.bm().Int63();
            slotByID.insert(generatedIDs[i], i);
        }
        let mut orderedGroups: Vec<Option<Vec<NodeRef>>> = vec![None; groups.len()];
        let mut usedGroups: Vec<bool> = vec![false; groups.len()];
        for (i, steps) in groups.iter().enumerate() {
            guard.Step()?;
            if let Some(rememberedID) = rememberedIDByFirstStep.get(&steps[0]) {
                if let Some(&slot) = slotByID.get(rememberedID) {
                    if orderedGroups[slot].is_none() {
                        orderedGroups[slot] = Some(steps.clone());
                        usedGroups[i] = true;
                    }
                }
            }
        }
        let mut nextGroup = 0usize;
        for i in 0..orderedGroups.len() {
            guard.Step()?;
            if orderedGroups[i].is_some() {
                continue;
            }
            while nextGroup < groups.len() && usedGroups[nextGroup] {
                guard.Step()?;
                nextGroup += 1;
            }
            orderedGroups[i] = Some(groups[nextGroup].clone());
            usedGroups[nextGroup] = true;
        }

        for (i, steps) in orderedGroups.into_iter().enumerate() {
            guard.Step()?;
            let steps = steps.unwrap();
            let mut id = generatedIDs[i];
            if let Some(&rememberedID) = rememberedIDByFirstStep.get(&steps[0]) {
                id = rememberedID;
            } else {
                id = nextAvailableNodeID(graph, id, &reservedNodeIDs);
                reservedNodeIDs.insert(id);
            }
            addSequence(graph, &buildSequence(&steps, graph, container.as_ref(), id));
        }
    }
    guard.Finish()?;
    Ok(())
}

pub(crate) fn buildSequence(steps: &[NodeRef], graph: &GraphRef, container: Option<&NodeRef>, id: EntityID) -> SequenceRef {
    let mut maxHeight = 0.0f64;
    for node in steps {
        if node.w() <= shape::STEP_WEDGE_WIDTH {
            node.set_w(2.0 * shape::STEP_WEDGE_WIDTH);
        }
        maxHeight = math::Max(maxHeight, node.h());
    }
    for node in steps {
        node.set_h(maxHeight);
    }
    for i in 1..steps.len() {
        if let Some(edge) = steps[i - 1].ConnectionTo(&steps[i]) {
            graph.Disconnect(&edge);
        }
    }
    let sequence = SequenceRef::new(layoutgraph::Sequence {
        Vessel: layoutgraph::NewNode(id, 0.0, 0.0),
        Nodes: steps.to_vec(),
        Graph: Some(graph.clone()),
        EdgeAbductions: Vec::new(),
        Container: container.cloned(),
    });
    sequence.SyncGeometry();
    abductSequenceEdges(&sequence);
    sequence.PlaceVessel();
    sequence
}

pub(crate) fn addSequence(graph: &GraphRef, sequence: &SequenceRef) {
    let (container, vessel, nodes) = {
        let s = sequence.b();
        (s.Container.clone(), s.Vessel.clone(), s.Nodes.clone())
    };
    graph.AddNewNodeToContainer(container.as_ref(), &vessel);
    for node in &nodes {
        node.bm().Sequence = Some(sequence.clone());
    }
    let mut updatedContainerNodes: Vec<NodeRef> = Vec::new();
    for child in graph.containers().at(&container) {
        if child.b().Sequence.as_ref() != Some(sequence) {
            updatedContainerNodes.push(child.clone());
        }
    }
    graph.containers().set(container.clone(), updatedContainerNodes);
    for node in &nodes {
        graph.RemoveNode(node);
        node.bm().Container = None;
    }
    graph.sequences().set(vessel, sequence.clone());
}

pub(crate) fn abductSequenceEdges(sequence: &SequenceRef) {
    let (nodes, graph, vessel) = {
        let s = sequence.b();
        (s.Nodes.clone(), s.Graph.clone().unwrap(), s.Vessel.clone())
    };
    let mut isStep: Set<NodeRef> = Set::default();
    for node in &nodes {
        isStep.insert(node.clone());
    }
    let mut abductions: Vec<EdgeAbductionRef> = Vec::new();
    for edge in graph.edges() {
        let fromIsStep = isStep.contains(&edge.from());
        let toIsStep = isStep.contains(&edge.to());
        if fromIsStep && toIsStep {
            continue;
        }
        if fromIsStep {
            abductions.push(EdgeAbductionRef::new(EdgeAbduction {
                Edge: edge.clone(),
                OriginallyFrom: Some(edge.from()),
                OriginallyTo: None,
                CurrentFrom: Some(vessel.clone()),
                CurrentTo: Some(edge.to()),
            }));
            edge.Reconnect(&vessel, false);
        }
        if toIsStep {
            abductions.push(EdgeAbductionRef::new(EdgeAbduction {
                Edge: edge.clone(),
                OriginallyFrom: None,
                OriginallyTo: Some(edge.to()),
                CurrentFrom: Some(edge.from()),
                CurrentTo: Some(vessel.clone()),
            }));
            edge.Reconnect(&vessel, true);
        }
    }
    sequence.bm().EdgeAbductions = abductions;
}

pub(crate) fn nextAvailableNodeID(graph: &GraphRef, mut candidate: EntityID, unavailable: &Set<EntityID>) -> EntityID {
    loop {
        if !unavailable.contains(&candidate) && !hasNodeID(graph, candidate) {
            return candidate;
        }
        if candidate == i64::MAX {
            candidate = 0;
        } else {
            candidate += 1;
        }
    }
}

pub(crate) fn hasNodeID(graph: &GraphRef, id: EntityID) -> bool {
    let hasID = |node: &NodeRef| node.id() == id;
    if graph.nodes().iter().any(|n| hasID(n)) {
        return true;
    }
    for (vessel, cluster) in graph.clusters().entries() {
        if hasID(&vessel) || cluster.b().Nodes.iter().any(|n| hasID(n)) {
            return true;
        }
    }
    for (vessel, sequence) in graph.sequences().entries() {
        if hasID(&vessel) || sequence.b().Nodes.iter().any(|n| hasID(n)) {
            return true;
        }
    }
    fn treeHasID(tree: &TreeRef, id: EntityID) -> bool {
        let t = tree.b();
        t.Node.id() == id || t.Children.iter().any(|c| treeHasID(c, id))
    }
    for (sentinel, roots) in graph.trees().entries() {
        if sentinel.as_ref().map_or(false, |s| hasID(s)) || roots.iter().any(|r| treeHasID(r, id)) {
            return true;
        }
    }
    false
}
