// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/graphjson/preflight.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

pub(crate) const maxEngineNodes: i64 = limits::MaxEngineNodes;
pub(crate) const maxEngineEdges: i64 = limits::MaxEngineEdges;
pub(crate) const maxEngineTopologyReferences: i64 = layoutgraph::MaxSerializationTopologyReferences;
pub(crate) const maxEngineRoutePoints: i64 = layoutgraph::MaxSerializationRoutePoints;
pub(crate) const maxEngineTopologyDepth: i64 = layoutgraph::MaxTopologyDepth;
pub(crate) const maxEnginePreflightWork: i64 = layoutgraph::MaxSerializationPreflightWork;

struct preflightState<'a> {
    guard: &'a limits::WorkGuardRef,
    nodeRecords: i64,
    edgeRecords: i64,
    references: i64,
    routePoints: i64,
    nodesByID: Map<i64, SerializedNode>,
    topLevelNodeIDs: Set<i64>,
    edgesBySerializedID: Map<i64, SerializedEdge>,
    effectiveEdgeIDs: Map<i64, i64>,
    topLevelEdgeIDs: Set<i64>,
}

fn addBounded(guard: &limits::WorkGuardRef, total: &mut i64, count: i64, limit: i64, kind: &str) -> GoResult<()> {
    if count < 0 || count > limit - *total {
        return Err(errorf!("TALA DeserializeGraph {} exceed limit {}", kind, limit));
    }
    *total += count;
    guard.Check()
}

impl<'a> preflightState<'a> {
    fn addReferences(&mut self, count: usize, kind: &str) -> GoResult<()> {
        addBounded(self.guard, &mut self.references, count as i64, maxEngineTopologyReferences, &format!("{} references", kind))
    }

    fn addRoutePoints(&mut self, count: usize) -> GoResult<()> {
        addBounded(self.guard, &mut self.routePoints, count as i64, maxEngineRoutePoints, "route points")
    }

    fn addNode(&mut self, node: &SerializedNode, location: &str) -> GoResult<()> {
        self.guard.Step()?;
        addBounded(self.guard, &mut self.nodeRecords, 1, maxEngineNodes, "node records")?;
        if node.ID == 0 {
            return Err(errorf!("TALA DeserializeGraph {} uses reserved node ID 0", location));
        }
        if let Some(previous) = self.nodesByID.get(&node.ID) {
            if previous != node {
                return Err(errorf!("TALA DeserializeGraph node ID {} has inconsistent repeated records", node.ID));
            }
        } else {
            self.nodesByID.insert(node.ID, node.clone());
        }
        self.addReferences(node.Nears.len(), location)
    }

    fn addEdge(&mut self, edge: &SerializedEdge, location: &str) -> GoResult<()> {
        self.guard.Step()?;
        addBounded(self.guard, &mut self.edgeRecords, 1, maxEngineEdges, "edge records")?;
        if edge.ID == 0 {
            return Err(errorf!("TALA DeserializeGraph {} uses reserved serialized edge ID 0", location));
        }
        let mut effectiveID = edge.ID;
        if let Some(oid) = edge.OriginalID {
            if edge.ID >= 0 || oid != 0 {
                return Err(errorf!(
                    "TALA DeserializeGraph {} has invalid original edge ID {} for serialized ID {}",
                    location,
                    oid,
                    edge.ID
                ));
            }
            effectiveID = oid;
        }
        if effectiveID != 0 {
            if let Some(&previousSerializedID) = self.effectiveEdgeIDs.get(&effectiveID) {
                if previousSerializedID != edge.ID {
                    return Err(errorf!(
                        "TALA DeserializeGraph {} effective edge ID {} is also represented by serialized ID {}",
                        location,
                        effectiveID,
                        previousSerializedID
                    ));
                }
            }
            self.effectiveEdgeIDs.insert(effectiveID, edge.ID);
        }
        if let Some(previous) = self.edgesBySerializedID.get(&edge.ID) {
            if !equalSerializedEdges(previous, edge) {
                return Err(errorf!("TALA DeserializeGraph edge ID {} has inconsistent repeated records", edge.ID));
            }
        } else {
            self.edgesBySerializedID.insert(edge.ID, edge.clone());
        }
        self.addRoutePoints(edge.Points.len())
    }
}

/// validateSerializedGraphForDeserialization bounds and validates the value
/// topology before reconstruction allocates runtime records or mutates any
/// caller-visible object.
pub(crate) fn validateSerializedGraphForDeserialization(ctx: &Ctx, graph: &SerializedGraph) -> GoResult<()> {
    let guard = newWorkGuard(ctx, "DeserializeGraph preflight")?;
    guard.SetLimit(maxEnginePreflightWork);
    if graph.Nodes.len() as i64 > maxEngineNodes {
        return Err(errorf!("TALA DeserializeGraph node records exceed limit {}", maxEngineNodes));
    }
    if graph.Edges.len() as i64 > maxEngineEdges {
        return Err(errorf!("TALA DeserializeGraph edge records exceed limit {}", maxEngineEdges));
    }

    let mut st = preflightState {
        guard: &guard,
        nodeRecords: 0,
        edgeRecords: 0,
        references: 0,
        routePoints: 0,
        nodesByID: Map::default(),
        topLevelNodeIDs: Set::default(),
        edgesBySerializedID: Map::default(),
        effectiveEdgeIDs: Map::default(),
        topLevelEdgeIDs: Set::default(),
    };

    st.addReferences(graph.Containers.len(), "container map")?;
    st.addReferences(graph.Hubs.len(), "hub map")?;
    st.addReferences(graph.Clusters.len(), "cluster map")?;
    st.addReferences(graph.Sequences.len(), "sequence map")?;
    st.addReferences(graph.ClusterVessels.len(), "cluster vessel map")?;
    st.addReferences(graph.SequenceVessels.len(), "sequence vessel map")?;
    st.addReferences(graph.Trees.len(), "tree map")?;
    st.addReferences(graph.Hierarchies.len(), "hierarchy list")?;
    st.addReferences(graph.Directions.len(), "direction map")?;

    for (i, node) in graph.Nodes.iter().enumerate() {
        if st.topLevelNodeIDs.contains(&node.ID) {
            return Err(errorf!("TALA DeserializeGraph has duplicate node ID {} in top-level records", node.ID));
        }
        st.topLevelNodeIDs.insert(node.ID);
        st.addNode(node, &format!("node {}", i))?;
    }
    for (i, edge) in graph.Edges.iter().enumerate() {
        if st.topLevelEdgeIDs.contains(&edge.ID) {
            return Err(errorf!("TALA DeserializeGraph has duplicate edge ID {} in top-level records", edge.ID));
        }
        st.topLevelEdgeIDs.insert(edge.ID);
        st.addEdge(edge, &format!("edge {}", i))?;
    }
    for (id, children) in &graph.Containers {
        guard.Step()?;
        st.addReferences(children.len(), &format!("container {}", id))?;
    }
    for (id, spokes) in &graph.Hubs {
        guard.Step()?;
        st.addReferences(spokes.len(), &format!("hub {}", id))?;
    }
    for (id, node) in &graph.ClusterVessels {
        st.addNode(node, &format!("cluster vessel {}", id))?;
    }
    for (id, node) in &graph.SequenceVessels {
        st.addNode(node, &format!("sequence vessel {}", id))?;
    }
    for (id, cluster) in &graph.Clusters {
        guard.Step()?;
        if cluster.Container == *id {
            return Err(errorf!("TALA DeserializeGraph cluster {} cannot contain itself", id));
        }
        st.addReferences(cluster.Nodes.len() + cluster.EdgeAbductions.len(), &format!("cluster {}", id))?;
    }
    for (id, sequence) in &graph.Sequences {
        guard.Step()?;
        if sequence.Container == *id {
            return Err(errorf!("TALA DeserializeGraph sequence {} cannot contain itself", id));
        }
        st.addReferences(sequence.Nodes.len() + sequence.EdgeAbductions.len(), &format!("sequence {}", id))?;
    }

    let mut treeOwners: Map<i64, String> = Map::default();
    struct treeFrame<'t> {
        tree: &'t SerializedTree,
        depth: i64,
        location: String,
    }
    for (rootID, roots) in &graph.Trees {
        st.addReferences(roots.len(), &format!("tree root {}", rootID))?;
        let mut stack: Vec<treeFrame> = Vec::with_capacity(roots.len());
        for i in (0..roots.len()).rev() {
            stack.push(treeFrame { tree: &roots[i], depth: 1, location: format!("tree {}[{}]", rootID, i) });
        }
        while let Some(frame) = stack.pop() {
            guard.Step()?;
            if frame.depth > maxEngineTopologyDepth {
                return Err(errorf!("TALA DeserializeGraph tree depth exceeds limit {}", maxEngineTopologyDepth));
            }
            let nodeID = frame.tree.Node.ID;
            if let Some(previous) = treeOwners.get(&nodeID) {
                return Err(errorf!(
                    "TALA DeserializeGraph tree node {} has more than one tree owner ({} and {})",
                    nodeID,
                    previous,
                    frame.location
                ));
            }
            if st.nodesByID.contains_key(&nodeID) && !st.topLevelNodeIDs.contains(&nodeID) {
                return Err(errorf!("TALA DeserializeGraph tree node {} repeats a non-top-level node record", nodeID));
            }
            treeOwners.insert(nodeID, frame.location.clone());
            st.addNode(&frame.tree.Node, &format!("{} node", frame.location))?;
            st.addEdge(&frame.tree.SentinelEdge, &format!("{} sentinel edge", frame.location))?;
            st.addReferences(frame.tree.Children.len(), &format!("{} children", frame.location))?;
            for i in (0..frame.tree.Children.len()).rev() {
                stack.push(treeFrame {
                    tree: &frame.tree.Children[i],
                    depth: frame.depth + 1,
                    location: format!("{} child {}", frame.location, i),
                });
            }
        }
    }
    let mut hierarchyOwners: Map<i64, usize> = Map::default();
    for (i, hierarchy) in graph.Hierarchies.iter().enumerate() {
        guard.Step()?;
        st.addReferences(hierarchy.Levels.len(), &format!("hierarchy {}", i))?;
        for nodeID in hierarchy.Levels.keys() {
            if let Some(previous) = hierarchyOwners.get(nodeID) {
                return Err(errorf!(
                    "TALA DeserializeGraph hierarchy node {} appears in both hierarchy {} and hierarchy {}",
                    nodeID,
                    previous,
                    i
                ));
            }
            hierarchyOwners.insert(*nodeID, i);
        }
    }
    guard.Finish()
}

pub(crate) fn equalSerializedEdges(left: &SerializedEdge, right: &SerializedEdge) -> bool {
    left == right
}
