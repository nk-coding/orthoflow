// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/hierarchy/eligibility.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

/// Candidates identifies the graph nodes that may participate in hierarchy discovery.
pub fn Candidates(graph: &GraphRef) -> Set<NodeRef> {
    let mut candidates = Set::default();
    let (nodes, nodeToTree) = {
        let g = graph.b();
        (g.Nodes.clone(), g.NodeToTree.clone())
    };
    for node in &nodes {
        if nodeToTree.has(node) {
            continue;
        }
        if graph.IsTreeSentinel(Some(node)) || node.b().Sequence.is_some() || node.b().FixedTopLeft.is_some() {
            continue;
        }
        if isSimpleCandidate(graph, node) {
            candidates.insert(node.clone());
            continue;
        }
        if node.IsContainer() && shapeCanBeContainer(&*node.shape()) && !node.shape().AspectRatio1() && isEligibleContainer(graph, node) {
            candidates.insert(node.clone());
        }
    }
    candidates
}

pub(crate) fn isSimpleCandidate(graph: &GraphRef, node: &NodeRef) -> bool {
    !(node.IsContainer()
        || graph.IsTreeSentinel(Some(node))
        || node.IsClusterVessel()
        || graph.IsSequenceVessel(node)
        || node.b().Cluster.is_some())
}

/// Hierarchy containers grow only horizontally.
pub(crate) fn shapeCanBeContainer(candidate: &dyn nodeshape::Shape) -> bool {
    let t = candidate.GetType();
    t == "" || t == shape::SQUARE_TYPE || t == shape::PACKAGE_TYPE || t == shape::STORED_DATA_TYPE || t == shape::QUEUE_TYPE || t == shape::STEP_TYPE
}

/// A container can participate only when it has no internal edges and none of
/// its descendants belongs to another specialized layout structure.
pub(crate) fn isEligibleContainer(graph: &GraphRef, root: &NodeRef) -> bool {
    let (children, nodeToTree) = {
        let g = graph.b();
        (g.Containers.at(&Some(root.clone())), g.NodeToTree.clone())
    };
    for child in &children {
        if child.b().FixedTopLeft.is_some() {
            return false;
        }
        if nodeToTree.has(child) {
            return false;
        }
        if graph.IsTreeSentinel(Some(child)) || child.b().Sequence.is_some() {
            return false;
        }
        if !child.IsContainer() && !isSimpleCandidate(graph, child) {
            return false;
        }
        for edge in child.edges() {
            if !isHierarchyStructuralEdge(&edge) {
                continue;
            }
            if child.Adjacent(&edge).IsDescendantOf(Some(root)) {
                return false;
            }
        }
        if child.IsContainer() && !isEligibleContainer(graph, child) {
            return false;
        }
    }
    true
}
