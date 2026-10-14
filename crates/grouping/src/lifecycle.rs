// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/grouping/lifecycle.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

fn filterOut(nodes: &[NodeRef], remove: impl Fn(&NodeRef) -> bool) -> Vec<NodeRef> {
    nodes.iter().filter(|n| !remove(n)).cloned().collect()
}

/// ResetClusters removes inactive vessel metadata so a later layout rediscovers
/// clusters from ordinary nodes and edges.
pub fn ResetClusters(graph: &GraphRef) {
    let clustersMap = graph.clusters();
    if clustersMap.len() == 0 {
        return;
    }
    let bulkFilter = clustersMap.len() > 1;
    let mut singleRetiredVessel: Option<NodeRef> = None;
    let mut retiredVesselCount = 0;
    for (vessel, cluster) in clustersMap.entries() {
        let (abductions, nodes) = {
            let c = cluster.b();
            (c.EdgeAbductions.clone(), c.Nodes.clone())
        };
        for abduction in &abductions {
            let a = abduction.b().clone();
            if let Some(of) = &a.OriginallyFrom {
                a.Edge.Reconnect(of, false);
            }
            if let Some(ot) = &a.OriginallyTo {
                a.Edge.Reconnect(ot, true);
            }
        }
        for node in &nodes {
            if node.b().Cluster.as_ref() == Some(&cluster) {
                node.bm().Cluster = None;
            }
            node.bm().Nears.remove(&vessel);
        }
        let nears: Vec<NodeRef> = vessel.b().Nears.iter().cloned().collect();
        for near in &nears {
            near.bm().Nears.remove(&vessel);
        }
        vessel.bm().Nears = Set::default();
        if bulkFilter {
            retiredVesselCount += 1;
            if retiredVesselCount == 1 {
                singleRetiredVessel = Some(vessel.clone());
            }
        } else {
            let filtered = filterOut(&graph.nodes(), |n| *n == vessel);
            graph.bm().Nodes = filtered;
            let containers = graph.containers();
            for (container, children) in containers.entries() {
                containers.set(container, filterOut(&children, |c| *c == vessel));
            }
        }
        {
            let mut v = vessel.bm();
            v.Container = None;
            v.Graph = None;
        }
        vessel.UnmarkClusterVessel();
    }
    if !bulkFilter || retiredVesselCount == 0 {
        clustersMap.clear();
        return;
    }
    if retiredVesselCount == 1 {
        let single = singleRetiredVessel.unwrap();
        let filtered = filterOut(&graph.nodes(), |n| *n == single);
        graph.bm().Nodes = filtered;
        let containers = graph.containers();
        for (container, children) in containers.entries() {
            containers.set(container, filterOut(&children, |c| *c == single));
        }
        clustersMap.clear();
        return;
    }
    let isRetired = |node: &NodeRef| node.b().Graph.is_none() && clustersMap.has(node);
    let filtered = filterOut(&graph.nodes(), |n| isRetired(n));
    graph.bm().Nodes = filtered;
    let containers = graph.containers();
    for (container, children) in containers.entries() {
        containers.set(container, filterOut(&children, |c| isRetired(c)));
    }
    clustersMap.clear();
}

/// Cleanup restores cluster and sequence members after node placement and
/// retires the temporary vessels while retaining rediscovery metadata.
pub fn Cleanup(graph: &GraphRef) {
    for key in graph.ClusterOrder() {
        let cluster = graph.clusters().get(&key).unwrap();
        cluster.ArrangeClusterNodes();
        let (nodes, container, abductions, vessel) = {
            let c = cluster.b();
            (c.Nodes.clone(), c.Container.clone(), c.EdgeAbductions.clone(), c.Vessel.clone())
        };
        for node in &nodes {
            graph.AddNewNodeToContainer(container.as_ref(), node);
        }
        restoreAbductions(&abductions);
        if !vessel.b().Nears.is_empty() {
            for near in vessel.OrderedNears() {
                near.bm().Nears.remove(&vessel);
                for node in &nodes {
                    near.AddNear(Some(node));
                }
            }
            vessel.bm().Nears = Set::default();
        }
        graph.RemoveNode(&vessel);
        let updated = filterOut(&graph.containers().at(&container), |c| *c == vessel);
        graph.containers().set(container.clone(), updated);
        let mut v = vessel.bm();
        v.Container = None;
        v.Graph = None;
    }

    for vessel in graph.SequenceOrder() {
        let sequence = graph.sequences().get(&vessel).unwrap();
        sequence.ArrangeSteps();
        let (nodes, container, abductions) = {
            let s = sequence.b();
            (s.Nodes.clone(), s.Container.clone(), s.EdgeAbductions.clone())
        };
        for node in &nodes {
            graph.AddNewNodeToContainer(container.as_ref(), node);
        }
        restoreAbductions(&abductions);
        if !vessel.b().Nears.is_empty() {
            for near in vessel.OrderedNears() {
                near.bm().Nears.remove(&vessel);
                for node in &nodes {
                    near.AddNear(Some(node));
                }
            }
            vessel.bm().Nears = Set::default();
        }
        graph.RemoveNode(&vessel);
        let updated = filterOut(&graph.containers().at(&container), |c| *c == vessel);
        graph.containers().set(container.clone(), updated);
        let mut v = vessel.bm();
        v.Container = None;
        v.Graph = None;
    }
    for node in graph.nodes() {
        node.bm().HerdAssignment = None;
    }
}

fn restoreAbductions(abductions: &[EdgeAbductionRef]) {
    for abduction in abductions {
        let a = abduction.b().clone();
        if let Some(of) = &a.OriginallyFrom {
            a.Edge.Reconnect(of, false);
        }
        if let Some(ot) = &a.OriginallyTo {
            a.Edge.Reconnect(ot, true);
        }
    }
}
