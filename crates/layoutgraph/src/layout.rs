// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/layoutgraph/layout.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

/// EdgeAbduction represents the temporary transfer of an edge from a descendant
/// container to the ancestor child so node placement can operate on containers.
#[derive(Clone)]
pub struct EdgeAbduction {
    pub Edge: EdgeRef,

    pub OriginallyFrom: Option<NodeRef>,
    pub OriginallyTo: Option<NodeRef>,

    pub CurrentFrom: Option<NodeRef>,
    pub CurrentTo: Option<NodeRef>,
}

gocompat::go_ptr!(EdgeAbductionRef, EdgeAbduction);

impl GraphRef {
    /// ClusterRDFSOrder returns cluster vessels in reverse depth-first container order.
    pub fn ClusterRDFSOrder(&self) -> Vec<NodeRef> {
        let mut order: Vec<NodeRef> = Vec::new();

        let mut dfsContainerOrder: Vec<Option<NodeRef>> = self.containerRDFSOrder(None).into_iter().map(Some).collect();
        dfsContainerOrder.push(None);
        let containers = self.b().Containers.clone();
        for container in &dfsContainerOrder {
            for child in containers.at(container) {
                if child.b().isClusterVessel {
                    order.push(child.clone());
                }
            }
        }

        order
    }

    /// ClusterOrder returns cluster vessels in stable ID order.
    pub fn ClusterOrder(&self) -> Vec<NodeRef> {
        let mut order: Vec<NodeRef> = self.b().Clusters.keys();
        sortNodesByID(&mut order);
        order
    }

    /// TreeOrder returns tree sentinels in stable ID order. The root sentinel
    /// is nil (`None`).
    pub fn TreeOrder(&self) -> Vec<Option<NodeRef>> {
        let mut order: Vec<Option<NodeRef>> = self.b().Trees.keys();
        sortOptNodesByID(&mut order);
        order
    }

    /// SequenceOrder returns sequence vessels in stable ID order.
    pub fn SequenceOrder(&self) -> Vec<NodeRef> {
        let mut order: Vec<NodeRef> = self.b().Sequences.keys();
        sortNodesByID(&mut order);
        order
    }

    pub(crate) fn applyEdgeAbductions(&self, edgeAbductions: &[EdgeAbductionRef]) {
        for edgeAbduction in edgeAbductions {
            let ea = edgeAbduction.b().clone();
            if let (Some(of), Some(cf)) = (&ea.OriginallyFrom, &ea.CurrentFrom) {
                if of != cf {
                    ea.Edge.reconnect(cf, false);
                }
            }
            if let (Some(ot), Some(ct)) = (&ea.OriginallyTo, &ea.CurrentTo) {
                if ot != ct {
                    ea.Edge.reconnect(ct, true);
                }
            }
        }
    }

    pub(crate) fn restoreEdgeAbductions(&self, edgeAbductions: &[EdgeAbductionRef]) -> Vec<EdgeAbductionRef> {
        let mut nodes: Set<NodeRef> = Set::default();
        let mut queue: std::collections::VecDeque<NodeRef> = self.nodes().into_iter().collect();
        let (containers, sequences, clusters) = {
            let g = self.b();
            (g.Containers.clone(), g.Sequences.clone(), g.Clusters.clone())
        };
        while let Some(node) = queue.pop_front() {
            nodes.insert(node.clone());
            if node.b().isContainer {
                queue.extend(containers.at(&Some(node.clone())));
            }
            if let Some(s) = sequences.get(&node) {
                queue.extend(s.b().Nodes.clone());
            }
            if node.b().isClusterVessel {
                let c = clusters.get(&node).expect("nil pointer dereference: cluster");
                queue.extend(c.b().Nodes.clone());
            }
        }
        let inNodes = |n: &Option<NodeRef>| -> bool {
            match n {
                Some(n) => nodes.contains(n),
                None => false,
            }
        };
        // Assign edges back to children
        let mut remainingAbductions: Vec<EdgeAbductionRef> = Vec::new();
        for edgeAbduction in edgeAbductions {
            let (of, ot, cf, ct, edge) = {
                let ea = edgeAbduction.b();
                (ea.OriginallyFrom.clone(), ea.OriginallyTo.clone(), ea.CurrentFrom.clone(), ea.CurrentTo.clone(), ea.Edge.clone())
            };
            let mut existsFrom = inNodes(&of);
            if of.is_none() {
                existsFrom = inNodes(&cf);
            }
            let mut existsTo = inNodes(&ot);
            if ot.is_none() {
                existsTo = inNodes(&ct);
            }
            if !existsFrom || !existsTo {
                remainingAbductions.push(edgeAbduction.clone());
                continue;
            }
            if let Some(mut of) = of {
                // If it is a cluster child, assign back to vessel
                if let Some(c) = of.clusterField() {
                    of = c.b().Vessel.clone();
                    edgeAbduction.bm().OriginallyFrom = Some(of.clone());
                }
                // If it is a sequence child, assign back to vessel
                if let Some(s) = of.sequenceField() {
                    of = s.b().Vessel.clone();
                    edgeAbduction.bm().OriginallyFrom = Some(of.clone());
                }

                // If simple cluster abduction, edge didn't change
                if cf.as_ref() != Some(&of) {
                    edge.reconnect(&of, false);
                }
            }
            if let Some(mut ot) = ot {
                if let Some(c) = ot.clusterField() {
                    ot = c.b().Vessel.clone();
                    edgeAbduction.bm().OriginallyTo = Some(ot.clone());
                }
                if let Some(s) = ot.sequenceField() {
                    ot = s.b().Vessel.clone();
                    edgeAbduction.bm().OriginallyTo = Some(ot.clone());
                }
                if ct.as_ref() != Some(&ot) {
                    edge.reconnect(&ot, true);
                }
            }
        }
        remainingAbductions
    }

    /// Edge abduction turns edges into a container's descendants into edges into
    /// the container's children, so node placement can work on containers.
    ///
    /// NOTE: Go concatenates the cluster and sequence abductions by iterating
    /// the `Clusters`/`Sequences` maps (random order); the port uses the Rust
    /// map order. Matching only depends on that order when several pending
    /// abductions match the same edge.
    pub(crate) fn abductEdges(&self, container: Option<&NodeRef>, toGraph: &GraphRef) -> Vec<EdgeAbductionRef> {
        let mut edgeAbductions: Vec<EdgeAbductionRef> = Vec::new();
        let (clusters, sequences, containers) = {
            let g = self.b();
            (g.Clusters.clone(), g.Sequences.clone(), g.Containers.clone())
        };
        for cluster in clusters.values() {
            edgeAbductions.extend(cluster.b().EdgeAbductions.clone());
        }
        for sequence in sequences.values() {
            edgeAbductions.extend(sequence.b().EdgeAbductions.clone());
        }

        let mut used = vec![false; edgeAbductions.len()];
        let mut containerAbductions: Vec<EdgeAbductionRef> = Vec::new();
        let children = containers.at(&container.cloned());
        // Find the edges which are connected to a descendant at one end and to a child at another end
        for edge in self.edges() {
            let mut fromAChild = false;
            let mut toAChild = false;

            let mut fromAChildDescendant = false;
            let mut toAChildDescendant = false;

            let mut fromChildAncestor: Option<NodeRef> = None;
            let mut toChildAncestor: Option<NodeRef> = None;

            let (ef, et) = (edge.from(), edge.to());
            for child in &children {
                if *child == ef {
                    fromAChild = true;
                } else if isDescendantOf(Some(&ef), Some(child)) {
                    fromAChildDescendant = true;
                    fromChildAncestor = Some(child.clone());
                }
                if *child == et {
                    toAChild = true;
                } else if isDescendantOf(Some(&et), Some(child)) {
                    toAChildDescendant = true;
                    toChildAncestor = Some(child.clone());
                }
            }

            if (fromAChild && (Some(&ef) == toChildAncestor.as_ref())) || (toAChild && (Some(&et) == fromChildAncestor.as_ref())) {
                // This is an intra-container edge (points from a child into a descendant of child, or vice versa)
                containerAbductions.push(EdgeAbductionRef::new(EdgeAbduction {
                    Edge: edge.clone(),
                    OriginallyTo: Some(et.clone()),
                    OriginallyFrom: Some(ef.clone()),
                    CurrentFrom: None,
                    CurrentTo: None,
                }));
                ef.removeEdge(&edge);
                et.removeEdge(&edge);
            } else if fromAChild && toAChild {
                toGraph.AddEdge(&edge);
                for (i, ea) in edgeAbductions.iter().enumerate() {
                    if used[i] {
                        continue;
                    }
                    let ea = ea.b();
                    if ea.CurrentFrom.as_ref() == Some(&ef) && ea.CurrentTo.as_ref() == Some(&et) {
                        let mut containerAbduction = EdgeAbduction {
                            Edge: edge.clone(),
                            CurrentFrom: Some(ef.clone()),
                            CurrentTo: Some(et.clone()),
                            OriginallyFrom: None,
                            OriginallyTo: None,
                        };
                        if ea.OriginallyFrom.is_some() {
                            containerAbduction.OriginallyFrom = ea.OriginallyFrom.clone();
                        } else {
                            containerAbduction.OriginallyTo = ea.OriginallyTo.clone();
                        }
                        used[i] = true;
                        containerAbductions.push(EdgeAbductionRef::new(containerAbduction));
                        break;
                    }
                }
            } else {
                // Else, if the edge is connected at one end to a child and another to a nested child, then temporarily reassign
                if fromAChildDescendant && toAChild {
                    let fca = fromChildAncestor.clone().unwrap();
                    containerAbductions.push(EdgeAbductionRef::new(EdgeAbduction {
                        Edge: edge.clone(),
                        OriginallyFrom: Some(ef.clone()),
                        CurrentFrom: Some(fca.clone()),
                        CurrentTo: Some(et.clone()),
                        OriginallyTo: None,
                    }));
                    edge.reconnect(&fca, false);
                    toGraph.AddEdge(&edge);
                } else if toAChildDescendant && fromAChild {
                    let tca = toChildAncestor.clone().unwrap();
                    containerAbductions.push(EdgeAbductionRef::new(EdgeAbduction {
                        Edge: edge.clone(),
                        OriginallyTo: Some(et.clone()),
                        CurrentTo: Some(tca.clone()),
                        CurrentFrom: Some(ef.clone()),
                        OriginallyFrom: None,
                    }));
                    edge.reconnect(&tca, true);
                    toGraph.AddEdge(&edge);
                } else if fromAChildDescendant && toAChildDescendant && (fromChildAncestor != toChildAncestor) {
                    // Edge can also be from one child descendant to another
                    // But if they are the same ancestor, then leave it be
                    let fca = fromChildAncestor.clone().unwrap();
                    let tca = toChildAncestor.clone().unwrap();
                    containerAbductions.push(EdgeAbductionRef::new(EdgeAbduction {
                        Edge: edge.clone(),
                        OriginallyTo: Some(et.clone()),
                        OriginallyFrom: Some(ef.clone()),
                        CurrentTo: Some(tca.clone()),
                        CurrentFrom: Some(fca.clone()),
                    }));
                    edge.reconnect(&fca, false);
                    edge.reconnect(&tca, true);
                    toGraph.AddEdge(&edge);
                }
            }
        }

        // For every edge abduction, the current or original pointer at to or from can be a vessel
        for containerAbduction in &containerAbductions {
            for (i, ea) in edgeAbductions.iter().enumerate() {
                if used[i] {
                    continue;
                }
                let mut matched = false;
                let ea = ea.b().clone();
                {
                    let ca = containerAbduction.b();
                    if ca.OriginallyFrom.is_none() {
                        // Must have abducted To
                        if ca.CurrentFrom == ea.CurrentFrom && ca.OriginallyTo == ea.CurrentTo {
                            matched = true;
                        }
                    } else if ca.OriginallyTo.is_none() {
                        // Must have abducted From
                        if ca.CurrentTo == ea.CurrentTo && ca.OriginallyFrom == ea.CurrentFrom {
                            matched = true;
                        }
                    } else {
                        // Must have abducted both
                        if ca.OriginallyTo == ea.CurrentTo && ca.OriginallyFrom == ea.CurrentFrom {
                            matched = true;
                        }
                    }
                }
                if matched {
                    used[i] = true;
                    let mut ca = containerAbduction.bm();
                    if ea.OriginallyFrom.is_some() {
                        ca.OriginallyFrom = ea.OriginallyFrom.clone();
                    } else {
                        ca.OriginallyTo = ea.OriginallyTo.clone();
                    }
                    break;
                }
            }
        }

        containerAbductions
    }
}

/// Go `maybeDescendant.isDescendantOf(maybeAncestor)` (both may be nil).
pub(crate) fn isDescendantOf(maybeDescendant: Option<&NodeRef>, maybeAncestor: Option<&NodeRef>) -> bool {
    let mut current = maybeDescendant.cloned();
    loop {
        if nodeEq(maybeAncestor, current.as_ref()) {
            return true;
        }
        let Some(cur) = current else {
            return false;
        };
        let (container, cluster, sequence) = {
            let c = cur.b();
            (c.Container.clone(), c.Cluster.clone(), c.Sequence.clone())
        };
        if let Some(c) = container {
            current = Some(c);
        } else if let Some(cl) = cluster {
            let v = cl.b().Vessel.clone();
            current = Some(v);
        } else if let Some(s) = sequence {
            let v = s.b().Vessel.clone();
            current = Some(v);
        } else {
            // Covers the case of nil ancestor
            return maybeAncestor.is_none();
        }
    }
}

impl GraphRef {
    /// routes is provided when graph is still undergoing edge routing and the edges don't have points locked in yet.
    pub fn ComputeNodeSpacing(&self) {
        for node in self.nodes() {
            node.UpdateSpacing();
        }
    }
}
