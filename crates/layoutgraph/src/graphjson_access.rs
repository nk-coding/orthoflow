// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/layoutgraph/graphjson_access.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

// These limits are exported for sibling graph codecs. They are part of the
// engine's resource-safety contract, not configurable layout policy.
pub const MaxSerializationTopologyReferences: i64 = maxEngineTopologyReferences;
pub const MaxSerializationRoutePoints: i64 = maxEngineRoutePoints;
pub const MaxSerializationPreflightWork: i64 = maxEnginePreflightWork;

/// ValidateForSerialization performs the layout graph's complete bounded
/// topology preflight before graphjson starts encoding references.
pub fn ValidateForSerialization(ctx: &Ctx, graph: &GraphRef) -> GoResult<()> {
    validateEngineGraph(ctx, "SerializeGraph", Some(graph))
}

/// ContainerOrderForSerialization returns the stable reverse-DFS fixture
/// order while charging the caller's shared serialization work budget.
pub fn ContainerOrderForSerialization(graph: &GraphRef, guard: &limits::WorkGuardRef) -> GoResult<Vec<NodeRef>> {
    graph.containerRDFSOrderContext(None, guard)
}

/// NodeIDForSerialization represents a nil root with the fixture ID zero.
pub fn NodeIDForSerialization(node: Option<&NodeRef>) -> EntityID {
    entityIDOf(node)
}

pub fn OrderedNearsForSerialization(node: &NodeRef) -> Vec<NodeRef> {
    node.orderedNears()
}

pub fn CostsForSerialization(graph: &GraphRef) -> (f64, f64, f64) {
    let g = graph.b();
    (g.crossingCost, g.turnCost, g.nonCenterPortCost)
}

pub fn AddDecodedEdge(node: &NodeRef, edge: &EdgeRef) {
    node.addEdge(edge);
}

pub fn MarkDecodedContainer(node: &NodeRef) {
    node.bm().isContainer = true;
}

pub fn MarkDecodedClusterVessel(node: &NodeRef) {
    node.bm().isClusterVessel = true;
}

pub fn IsActiveClusterForSerialization(cluster: Option<&ClusterRef>) -> bool {
    clusterIsActive(cluster)
}

pub fn IsActiveSequenceForSerialization(sequence: Option<&SequenceRef>) -> bool {
    sequenceIsActive(sequence)
}

pub fn NewDecodedHierarchy(levels: GoMap<NodeRef, i64>) -> HierarchyRef {
    HierarchyRef::new(Hierarchy { level: levels, LevelCount: 0 })
}

pub fn HierarchyLevelsForSerialization(hierarchy: &HierarchyRef) -> GoMap<NodeRef, i64> {
    hierarchy.b().level.clone()
}

/// BuildNodeToTreeForDeserialization derives the auxiliary tree index under
/// the decoder's shared work budget.
pub fn BuildNodeToTreeForDeserialization(
    trees: &GoMap<Option<NodeRef>, Vec<TreeRef>>,
    guard: &limits::WorkGuardRef,
) -> GoResult<GoMap<NodeRef, TreeRef>> {
    Ok(GoMap::from_map(buildNodeToTree(trees, guard)?))
}

/// PublishDeserializedGraph atomically replaces graph-owned state while
/// retaining the destination graph's identity.
pub fn PublishDeserializedGraph(destination: &GraphRef, staged: &GraphRef, crossing: f64, turn: f64, nonCenterPort: f64) {
    let s = staged.b();
    let mut d = destination.bm();
    d.IsRootHierarchy = s.IsRootHierarchy;
    d.Nodes = s.Nodes.clone();
    d.Edges = s.Edges.clone();
    d.CellSize = s.CellSize;

    d.crossingCost = crossing;
    d.turnCost = turn;
    d.nonCenterPortCost = nonCenterPort;
    d.edgeLengthCache = Some(Map::default());

    d.Containers = s.Containers.clone();
    d.Clusters = s.Clusters.clone();
    d.Trees = s.Trees.clone();
    d.NodeToTree = s.NodeToTree.clone();
    d.Hubs = s.Hubs.clone();
    d.Sequences = s.Sequences.clone();
    d.Directions = s.Directions.clone();
    d.CommonUncleSiblings = s.CommonUncleSiblings.clone();
}
