// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/grouping/cluster_discovery.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

pub(crate) const maxEngineNodes: i64 = limits::MaxEngineNodes;

#[derive(Clone, Default)]
pub(crate) struct clusterEdgeSignature {
    pub(crate) from: i64,
    pub(crate) to: i64,
    pub(crate) bidirectional: i64,
    pub(crate) undirected: i64,
    pub(crate) directed: i64,
    pub(crate) fromArrowheads: Set<layoutgraph::Arrowhead>,
    pub(crate) toArrowheads: Set<layoutgraph::Arrowhead>,
}

impl clusterEdgeSignature {
    pub(crate) fn add(&mut self, node: &NodeRef, edge: &EdgeRef) {
        let (from, to) = (edge.from(), edge.to());
        if from == *node {
            self.from += 1;
        }
        if to == *node {
            self.to += 1;
        }
        if edge.IsBidirectional() {
            self.bidirectional += 1;
        } else if edge.IsUndirected() {
            self.undirected += 1;
        } else if edge.IsDirected() {
            self.directed += 1;
        }
        let (sa, ta) = {
            let e = edge.b();
            (e.SourceArrowhead.clone(), e.TargetArrowhead.clone())
        };
        if from == *node {
            self.fromArrowheads.insert(sa);
            self.toArrowheads.insert(ta);
        } else {
            self.fromArrowheads.insert(ta);
            self.toArrowheads.insert(sa);
        }
    }

    pub(crate) fn arrowTypeCount(&self) -> i64 {
        let mut types = 0;
        if self.directed > 0 {
            types += 1;
        }
        if self.bidirectional > 0 {
            types += 1;
        }
        if self.undirected > 0 {
            types += 1;
        }
        types
    }

    pub(crate) fn matches(&self, other: &clusterEdgeSignature) -> bool {
        self.from == other.from
            && self.to == other.to
            && self.bidirectional == other.bidirectional
            && self.undirected == other.undirected
            && self.arrowTypeCount() <= 1
            && other.arrowTypeCount() <= 1
            && arrowheadSetsEqual(&self.fromArrowheads, &other.fromArrowheads)
            && arrowheadSetsEqual(&self.toArrowheads, &other.toArrowheads)
    }
}

pub(crate) fn arrowheadSetsEqual(first: &Set<layoutgraph::Arrowhead>, second: &Set<layoutgraph::Arrowhead>) -> bool {
    if first.len() != second.len() {
        return false;
    }
    first.iter().all(|a| second.contains(a))
}

#[derive(Clone, Default)]
pub(crate) struct clusterDiscoveryInfo {
    pub(crate) neighbors: Vec<Option<NodeRef>>,
    pub(crate) neighborSet: Set<Option<NodeRef>>,
    pub(crate) edges: Vec<EdgeRef>,
    pub(crate) edgeSignature: clusterEdgeSignature,
    pub(crate) estimatedWidth: f64,
    pub(crate) estimatedHeight: f64,
    pub(crate) noClustering: bool,
    pub(crate) toTableColumn: bool,
}

pub(crate) struct clusterDiscoveryIndex {
    pub(crate) infos: Map<NodeRef, clusterDiscoveryInfo>,
    pub(crate) edgeOrder: Map<EdgeRef, i64>,
    pub(crate) edgeNodes: Map<EdgeRef, Vec<NodeRef>>,
    pub(crate) sequenceEdges: Map<SequenceRef, Map<EdgeRef, Option<NodeRef>>>,
}

impl clusterDiscoveryIndex {
    pub(crate) fn sequenceOriginal(&mut self, sequence: &SequenceRef, edge: &EdgeRef, guard: &WorkGuardRef) -> GoResult<Option<NodeRef>> {
        if !self.sequenceEdges.contains_key(sequence) {
            let (abductions, vessel) = {
                let s = sequence.b();
                (s.EdgeAbductions.clone(), s.Vessel.clone())
            };
            let mut byEdge: Map<EdgeRef, Option<NodeRef>> = gocompat::NewMapCap(abductions.len());
            for abduction in &abductions {
                guard.Step()?;
                let a = abduction.b();
                if a.CurrentFrom.as_ref() == Some(&vessel) {
                    byEdge.entry(a.Edge.clone()).or_insert_with(|| a.OriginallyFrom.clone());
                } else if a.CurrentTo.as_ref() == Some(&vessel) {
                    byEdge.entry(a.Edge.clone()).or_insert_with(|| a.OriginallyTo.clone());
                }
            }
            self.sequenceEdges.insert(sequence.clone(), byEdge);
        }
        Ok(self.sequenceEdges.get(sequence).unwrap().get(edge).cloned().flatten())
    }

    pub(crate) fn refreshNeighbors(&mut self, g: &GraphRef, node: &NodeRef, guard: &WorkGuardRef) -> GoResult<()> {
        if !self.infos.contains_key(node) {
            return Err(errorf!("TALA AddClusters cannot find node discovery index"));
        }
        let edges = node.edges();
        let mut neighbors: Vec<Option<NodeRef>> = Vec::with_capacity(edges.len());
        let mut neighborSet: Set<Option<NodeRef>> = gocompat::NewSetCap(edges.len());
        let sequences = g.sequences();
        for edge in &edges {
            guard.Step()?;
            let mut adjacent = Some(node.Adjacent(edge));
            if let Some(sequence) = sequences.get(adjacent.as_ref().unwrap()) {
                adjacent = self.sequenceOriginal(&sequence, edge, guard)?;
            }
            if !neighborSet.contains(&adjacent) {
                neighborSet.insert(adjacent.clone());
                neighbors.push(adjacent);
            }
        }
        let info = self.infos.get_mut(node).unwrap();
        info.neighbors = neighbors;
        info.neighborSet = neighborSet;
        guard.Finish()
    }

    pub(crate) fn refreshAfterClusterAbduction(
        &mut self,
        g: &GraphRef,
        cluster: &ClusterRef,
        edges: &[EdgeRef],
        guard: &WorkGuardRef,
    ) -> GoResult<()> {
        let vessel = cluster.b().Vessel.clone();
        let mut affected: Vec<NodeRef> = Vec::with_capacity(edges.len());
        let mut seen: Set<NodeRef> = gocompat::NewSetCap(edges.len());
        for edge in edges {
            let nodes = self.edgeNodes.get(edge).cloned().unwrap_or_default();
            for node in &nodes {
                guard.Step()?;
                if *node == vessel || !self.infos.contains_key(node) {
                    continue;
                }
                if seen.contains(node) {
                    continue;
                }
                seen.insert(node.clone());
                affected.push(node.clone());
            }
        }
        for node in &affected {
            self.refreshNeighbors(g, node, guard)?;
        }
        guard.Finish()
    }
}

pub(crate) fn clusterIsDescendantOfGuarded(
    descendant: Option<&NodeRef>,
    ancestor: Option<&NodeRef>,
    guard: &WorkGuardRef,
) -> GoResult<bool> {
    let mut seen: Set<NodeRef> = Set::default();
    let mut descendant = descendant.cloned();
    loop {
        guard.Step()?;
        if descendant.as_ref() == ancestor {
            return Ok(true);
        }
        let Some(d) = descendant.clone() else { return Ok(false) };
        if seen.contains(&d) {
            return Err(errorf!("TALA AddClusters found a cycle in node ancestry"));
        }
        if seen.len() as i64 >= maxEngineNodes {
            return Err(errorf!("TALA AddClusters ancestry exceeds node limit"));
        }
        seen.insert(d.clone());
        let (container, cluster, sequence) = {
            let n = d.b();
            (n.Container.clone(), n.Cluster.clone(), n.Sequence.clone())
        };
        if container.is_some() {
            descendant = container;
        } else if let Some(c) = cluster {
            descendant = Some(c.b().Vessel.clone());
        } else if let Some(s) = sequence {
            descendant = Some(s.b().Vessel.clone());
        } else {
            return Ok(ancestor.is_none());
        }
    }
}

pub(crate) fn clusterHasLeakyEdgeGuarded(g: &GraphRef, node: &NodeRef, guard: &WorkGuardRef) -> GoResult<bool> {
    if !node.IsContainer() {
        return Ok(false);
    }
    let descendants = g.AllDescendantNodesWithWorkGuard(Some(node), true, guard)?;
    for descendant in &descendants {
        guard.Step()?;
        for edge in descendant.edges() {
            guard.Step()?;
            let adjacent = descendant.Adjacent(&edge);
            let inside = clusterIsDescendantOfGuarded(Some(&adjacent), Some(node), guard)?;
            if !inside {
                return Ok(true);
            }
        }
    }
    guard.Finish()?;
    Ok(false)
}

pub(crate) fn buildClusterDiscoveryIndex(
    g: &GraphRef,
    containerOrder: &[NodeRef],
    guard: &WorkGuardRef,
) -> GoResult<clusterDiscoveryIndex> {
    let mut orderedNodes: Vec<NodeRef> = Vec::new();
    let mut seenNodes: Set<NodeRef> = Set::default();
    let mut addNode = |node: &NodeRef| -> GoResult<()> {
        guard.Step()?;
        if seenNodes.contains(node) {
            return Ok(());
        }
        if seenNodes.len() as i64 >= maxEngineNodes {
            return Err(errorf!("TALA AddClusters unique node count exceeds limit {}", maxEngineNodes));
        }
        seenNodes.insert(node.clone());
        orderedNodes.push(node.clone());
        Ok(())
    };
    let containers = g.containers();
    for container in containerOrder {
        addNode(container)?;
        for child in containers.at(&Some(container.clone())) {
            addNode(&child)?;
        }
    }
    for child in containers.at(&None) {
        addNode(&child)?;
    }

    let mut index = clusterDiscoveryIndex {
        infos: gocompat::NewMapCap(orderedNodes.len()),
        edgeOrder: Map::default(),
        edgeNodes: Map::default(),
        sequenceEdges: Map::default(),
    };
    for node in &orderedNodes {
        index.infos.insert(
            node.clone(),
            clusterDiscoveryInfo { estimatedWidth: node.w(), estimatedHeight: node.h(), ..Default::default() },
        );
    }

    for node in &orderedNodes {
        let mut hasLoop = false;
        let mut toTable = false;
        for edge in node.edges() {
            guard.Step()?;
            index.edgeNodes.entry(edge.clone()).or_default().push(node.clone());
            if edge.IsLoop() {
                hasLoop = true;
            }
            let e = edge.b();
            if e.FromTableColumnIndex.is_some() || e.ToTableColumnIndex.is_some() {
                toTable = true;
            }
        }
        if toTable {
            index.infos.get_mut(node).unwrap().toTableColumn = true;
        }
        index.refreshNeighbors(g, node, guard)?;
        let leaky = clusterHasLeakyEdgeGuarded(g, node, guard)?;
        let noClustering = g.IsTreeSentinel(Some(node))
            || node.IsClusterVessel()
            || g.IsSequenceVessel(node)
            || node.IsTable()
            || node.b().Hierarchy.is_some()
            || node.b().FixedTopLeft.is_some()
            || leaky
            || hasLoop;
        index.infos.get_mut(node).unwrap().noClustering = noClustering;
    }

    for (edgeIndex, edge) in g.edges().iter().enumerate() {
        guard.Step()?;
        index.edgeOrder.entry(edge.clone()).or_insert(edgeIndex as i64);
        let (from, to) = (edge.from(), edge.to());
        if let Some(info) = index.infos.get_mut(&from) {
            info.edges.push(edge.clone());
            info.edgeSignature.add(&from, edge);
        }
        if to != from {
            if let Some(info) = index.infos.get_mut(&to) {
                info.edges.push(edge.clone());
                info.edgeSignature.add(&to, edge);
            }
        }
    }

    guard.Finish()?;
    Ok(index)
}

pub(crate) fn clusterIncidentEdges(
    cluster: &ClusterRef,
    infos: &Map<NodeRef, clusterDiscoveryInfo>,
    edgeOrder: &Map<EdgeRef, i64>,
    guard: &WorkGuardRef,
) -> GoResult<Vec<EdgeRef>> {
    let mut unique: Set<EdgeRef> = Set::default();
    let mut edges: Vec<EdgeRef> = Vec::new();
    let nodes = cluster.b().Nodes.clone();
    for node in &nodes {
        guard.Step()?;
        let Some(info) = infos.get(node) else {
            return Err(errorf!("TALA AddClusters cannot find cluster member index"));
        };
        for edge in &info.edges {
            guard.Step()?;
            if unique.insert(edge.clone()) {
                edges.push(edge.clone());
            }
        }
    }
    let mut width = 1usize;
    while width < edges.len() {
        for _ in 0..edges.len() {
            guard.Step()?;
        }
        if width > edges.len() / 2 {
            break;
        }
        width *= 2;
    }
    // Go collects the edges from a map (random order); the keys are distinct, so
    // the sorted result is unique.
    gocompat::sort::SlicesSortFunc(&mut edges, |a: &EdgeRef, b: &EdgeRef| {
        gocompat::sort::cmpCompare(edgeOrder.get(a).copied().unwrap_or(0), edgeOrder.get(b).copied().unwrap_or(0))
    });
    guard.Finish()?;
    Ok(edges)
}
