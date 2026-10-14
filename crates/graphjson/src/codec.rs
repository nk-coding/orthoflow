// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/graphjson/codec.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;
use layoutgraph::{
    ClusterArrangement, ClusterRef, EdgeAbduction, EdgeAbductionRef, GoMap, IconRef, LabelRef, SequenceRef, StyleScalar, TreeRef,
};
use std::collections::BTreeMap;

pub(crate) fn newWorkGuard(ctx: &Ctx, location: &str) -> GoResult<limits::WorkGuardRef> {
    limits::NewWorkGuard(ctx, location, limits::MaxEngineWorkUnits)
}

pub(crate) fn serializePoint(point: Option<&geo::PointRef>) -> Option<SerializedPoint> {
    let p = *point?.b();
    Some(SerializedPoint { X: p.X, Y: p.Y })
}

pub(crate) fn deserializePoint(point: Option<&SerializedPoint>) -> Option<geo::PointRef> {
    let p = point?;
    Some(geo::NewPoint(p.X, p.Y))
}

/// edgeSerializationIDs assigns a stable, serialization-only ID to every edge
/// whose ID is unset.
pub(crate) struct edgeSerializationIDs {
    byEdge: Map<EdgeRef, i64>,
    used: Set<i64>,
    next: i64,
}

pub(crate) fn newEdgeSerializationIDs(g: &GraphRef, guard: &limits::WorkGuardRef) -> GoResult<edgeSerializationIDs> {
    let mut ids = edgeSerializationIDs { byEdge: Map::default(), used: Set::default(), next: -1 };

    let mut seenNodes: Set<NodeRef> = Set::default();
    fn reserveEdge(ids: &mut edgeSerializationIDs, guard: &limits::WorkGuardRef, edge: Option<&EdgeRef>) -> GoResult<()> {
        guard.Step()?;
        if let Some(edge) = edge {
            let id = edge.id();
            if id != 0 {
                ids.used.insert(id);
            }
        }
        Ok(())
    }
    fn reserveNode(
        ids: &mut edgeSerializationIDs,
        seenNodes: &mut Set<NodeRef>,
        guard: &limits::WorkGuardRef,
        node: Option<&NodeRef>,
    ) -> GoResult<()> {
        guard.Step()?;
        let Some(node) = node else { return Ok(()) };
        if seenNodes.contains(node) {
            return Ok(());
        }
        seenNodes.insert(node.clone());
        for edge in node.edges() {
            reserveEdge(ids, guard, Some(&edge))?;
        }
        Ok(())
    }

    let (gedges, gnodes, containers, clusters, sequences, trees) = {
        let gb = g.b();
        (gb.Edges.clone(), gb.Nodes.clone(), gb.Containers.entries(), gb.Clusters.entries(), gb.Sequences.entries(), gb.Trees.entries())
    };
    for edge in &gedges {
        reserveEdge(&mut ids, guard, Some(edge))?;
    }
    for node in &gnodes {
        reserveNode(&mut ids, &mut seenNodes, guard, Some(node))?;
    }
    for (container, children) in &containers {
        reserveNode(&mut ids, &mut seenNodes, guard, container.as_ref())?;
        for child in children {
            reserveNode(&mut ids, &mut seenNodes, guard, Some(child))?;
        }
    }
    for (vessel, cluster) in &clusters {
        reserveNode(&mut ids, &mut seenNodes, guard, Some(vessel))?;
        let c = cluster.b().clone();
        reserveNode(&mut ids, &mut seenNodes, guard, c.Container.as_ref())?;
        for node in &c.Nodes {
            reserveNode(&mut ids, &mut seenNodes, guard, Some(node))?;
        }
        for abduction in &c.EdgeAbductions {
            let e = abduction.b().Edge.clone();
            reserveEdge(&mut ids, guard, Some(&e))?;
        }
    }
    for (vessel, sequence) in &sequences {
        reserveNode(&mut ids, &mut seenNodes, guard, Some(vessel))?;
        let s = sequence.b().clone();
        reserveNode(&mut ids, &mut seenNodes, guard, s.Container.as_ref())?;
        for node in &s.Nodes {
            reserveNode(&mut ids, &mut seenNodes, guard, Some(node))?;
        }
        for abduction in &s.EdgeAbductions {
            let e = abduction.b().Edge.clone();
            reserveEdge(&mut ids, guard, Some(&e))?;
        }
    }
    for (rootSentinel, roots) in &trees {
        reserveNode(&mut ids, &mut seenNodes, guard, rootSentinel.as_ref())?;
        let mut stack: Vec<TreeRef> = roots.iter().rev().cloned().collect();
        while let Some(tree) = stack.pop() {
            let (tnode, sentinel, children) = {
                let t = tree.b();
                (t.Node.clone(), t.SentinelEdge.clone(), t.Children.clone())
            };
            reserveNode(&mut ids, &mut seenNodes, guard, Some(&tnode))?;
            reserveEdge(&mut ids, guard, sentinel.as_ref())?;
            for v in children.iter().rev() {
                stack.push(v.clone());
            }
        }
    }

    // Allocate unset references in the same stable order used by the serialized
    // representation.
    for edge in &gedges {
        guard.Step()?;
        ids.reference(Some(edge));
    }
    let clusterMap = g.b().Clusters.clone();
    for vessel in g.ClusterOrder() {
        let abductions = clusterMap.get(&vessel).unwrap().b().EdgeAbductions.clone();
        for abduction in &abductions {
            guard.Step()?;
            let e = abduction.b().Edge.clone();
            ids.reference(Some(&e));
        }
    }
    let sequenceMap = g.b().Sequences.clone();
    for vessel in g.SequenceOrder() {
        let abductions = sequenceMap.get(&vessel).unwrap().b().EdgeAbductions.clone();
        for abduction in &abductions {
            guard.Step()?;
            let e = abduction.b().Edge.clone();
            ids.reference(Some(&e));
        }
    }
    let treeMap = g.b().Trees.clone();
    for rootSentinel in g.TreeOrder() {
        let roots = treeMap.at(&rootSentinel);
        let mut stack: Vec<TreeRef> = roots.iter().rev().cloned().collect();
        while let Some(tree) = stack.pop() {
            guard.Step()?;
            let (sentinel, children) = {
                let t = tree.b();
                (t.SentinelEdge.clone(), t.Children.clone())
            };
            ids.reference(sentinel.as_ref());
            for v in children.iter().rev() {
                stack.push(v.clone());
            }
        }
    }

    guard.Finish()?;
    Ok(ids)
}

impl edgeSerializationIDs {
    pub(crate) fn reference(&mut self, edge: Option<&EdgeRef>) -> i64 {
        let Some(edge) = edge else { return 0 };
        if let Some(&id) = self.byEdge.get(edge) {
            return id;
        }
        let eid = edge.id();
        if eid != 0 {
            self.byEdge.insert(edge.clone(), eid);
            return eid;
        }
        loop {
            let candidate = self.next;
            self.next -= 1;
            if candidate == 0 {
                continue;
            }
            if self.used.contains(&candidate) {
                continue;
            }
            self.used.insert(candidate);
            self.byEdge.insert(edge.clone(), candidate);
            return candidate;
        }
    }
}

/// Serialize validates graph bounds before walking any nested
/// topology, then observes ctx throughout the bounded serialization.
pub fn Serialize(ctx: &Ctx, g: Option<&GraphRef>) -> GoResult<SerializedGraph> {
    let Some(g) = g else {
        return Err(errorf!("cannot serialize a nil graph"));
    };
    layoutgraph::ValidateForSerialization(ctx, g)?;
    let guard = newWorkGuard(ctx, "SerializeGraph")?;

    let mut edgeIDs = newEdgeSerializationIDs(g, &guard)?;
    let mut inSerializedNodes: Set<NodeRef> = Set::default();
    let mut serializedNodes: Vec<SerializedNode> = Vec::new();
    let gnodes = g.nodes();
    for node in &gnodes {
        let serialized = serializeNode(node, &guard)?;
        serializedNodes.push(serialized);
        inSerializedNodes.insert(node.clone());
    }
    let clusterMap = g.b().Clusters.clone();
    let clusterOrder = g.ClusterOrder();
    for vessel in &clusterOrder {
        guard.Step()?;
        inSerializedNodes.insert(vessel.clone());
        let members = clusterMap.get(vessel).unwrap().b().Nodes.clone();
        for n in &members {
            guard.Step()?;
            if !inSerializedNodes.contains(n) {
                let serialized = serializeNode(n, &guard)?;
                serializedNodes.push(serialized);
                inSerializedNodes.insert(n.clone());
            }
        }
    }
    let sequenceMap = g.b().Sequences.clone();
    let sequenceOrder = g.SequenceOrder();
    for vessel in &sequenceOrder {
        guard.Step()?;
        inSerializedNodes.insert(vessel.clone());
        let members = sequenceMap.get(vessel).unwrap().b().Nodes.clone();
        for n in &members {
            guard.Step()?;
            if !inSerializedNodes.contains(n) {
                let serialized = serializeNode(n, &guard)?;
                serializedNodes.push(serialized);
                inSerializedNodes.insert(n.clone());
            }
        }
    }

    let mut serializedEdges: Vec<SerializedEdge> = Vec::new();
    for edge in g.edges() {
        let serialized = serializeEdge(&edge, &mut edgeIDs, &guard)?;
        serializedEdges.push(serialized);
    }

    // serialize in a fixed order based on node id
    let mut containers: BTreeMap<i64, Vec<i64>> = BTreeMap::new();
    let containerMap = g.b().Containers.clone();
    if containerMap.len() > 0 {
        let ordered = layoutgraph::ContainerOrderForSerialization(g, &guard)?;
        if containerMap.len() != ordered.len() + 1 {
            return Err(errorf!(
                "g.Containers length ({}) does not match rdfs ordered container length ({})",
                containerMap.len(),
                ordered.len() + 1
            ));
        }
        let mut order: Vec<Option<NodeRef>> = ordered.into_iter().map(Some).collect();
        order.push(None);
        for containerNode in &order {
            guard.Step()?;
            let mut newvals: Vec<i64> = Vec::new();
            for node in containerMap.at(containerNode) {
                guard.Step()?;
                if !inSerializedNodes.contains(&node) {
                    return Err(errorf!(
                        "error while serializing: node {} is missing from g.Nodes but is a child of {} in g.Containers",
                        node.DebugID(),
                        layoutgraph::DebugIDOf(containerNode.as_ref())
                    ));
                }
                newvals.push(layoutgraph::NodeIDForSerialization(Some(&node)));
            }
            containers.insert(layoutgraph::NodeIDForSerialization(containerNode.as_ref()), newvals);
        }
    }

    let mut clusters: BTreeMap<i64, SerializedCluster> = BTreeMap::new();
    let mut clusterVessels: BTreeMap<i64, SerializedNode> = BTreeMap::new();
    for vessel in &clusterOrder {
        guard.Step()?;
        let mut clusterNodes: Vec<i64> = Vec::new();
        let cluster = clusterMap.get(vessel).unwrap().b().clone();
        for node in &cluster.Nodes {
            guard.Step()?;
            if !inSerializedNodes.contains(node) {
                return Err(errorf!(
                    "error while serializing: node {} is missing from g.Nodes but is in cluster {} in g.Clusters",
                    node.DebugID(),
                    vessel.DebugID()
                ));
            }
            clusterNodes.push(layoutgraph::NodeIDForSerialization(Some(node)));
        }

        let mut abductions: Vec<SerializedEdgeAbduction> = Vec::new();
        for abduction in &cluster.EdgeAbductions {
            guard.Step()?;
            abductions.push(serializeAbduction(abduction, &mut edgeIDs));
        }

        let newval = SerializedCluster {
            Nodes: clusterNodes,
            Arrangement: cluster.Arrangement.as_str().to_string(),
            DesiredArrangement: cluster.DesiredArrangement.as_str().to_string(),
            Container: layoutgraph::NodeIDForSerialization(cluster.Container.as_ref()),
            EdgeAbductions: abductions,
            Padding: cluster.Padding,
            FixedSize: cluster.FixedSize,
        };

        clusters.insert(layoutgraph::NodeIDForSerialization(Some(vessel)), newval);
        let serializedVessel = serializeNode(vessel, &guard)?;
        clusterVessels.insert(layoutgraph::NodeIDForSerialization(Some(vessel)), serializedVessel);
    }

    let mut sequences: BTreeMap<i64, SerializedSequence> = BTreeMap::new();
    let mut sequenceVessels: BTreeMap<i64, SerializedNode> = BTreeMap::new();
    for vessel in &sequenceOrder {
        guard.Step()?;
        let mut sequenceNodes: Vec<i64> = Vec::new();
        let seq = sequenceMap.get(vessel).unwrap().b().clone();
        for node in &seq.Nodes {
            guard.Step()?;
            if !inSerializedNodes.contains(node) {
                return Err(errorf!(
                    "error while serializing: node {} is missing from g.Nodes but is in sequence {} in g.Sequences",
                    layoutgraph::NodeIDForSerialization(Some(node)),
                    layoutgraph::NodeIDForSerialization(Some(vessel))
                ));
            }
            sequenceNodes.push(layoutgraph::NodeIDForSerialization(Some(node)));
        }

        let mut abductions: Vec<SerializedEdgeAbduction> = Vec::new();
        for abduction in &seq.EdgeAbductions {
            guard.Step()?;
            abductions.push(serializeAbduction(abduction, &mut edgeIDs));
        }

        let newval = SerializedSequence {
            Nodes: sequenceNodes,
            Container: layoutgraph::NodeIDForSerialization(seq.Container.as_ref()),
            EdgeAbductions: abductions,
        };

        sequences.insert(layoutgraph::NodeIDForSerialization(Some(vessel)), newval);
        let serializedVessel = serializeNode(vessel, &guard)?;
        sequenceVessels.insert(layoutgraph::NodeIDForSerialization(Some(vessel)), serializedVessel);
    }

    let mut hubs: BTreeMap<i64, Vec<i64>> = BTreeMap::new();
    let hubMap = g.b().Hubs.clone();
    if hubMap.len() > 0 {
        for n in g.HubOrder() {
            guard.Step()?;
            let mut newvals: Vec<i64> = Vec::new();
            for node in hubMap.at(&n) {
                guard.Step()?;
                if !inSerializedNodes.contains(&node) {
                    return Err(errorf!(
                        "error while serializing: node {} is missing from g.Nodes but is a spoke of {} in g.Hubs",
                        node.DebugID(),
                        n.DebugID()
                    ));
                }
                newvals.push(layoutgraph::NodeIDForSerialization(Some(&node)));
            }
            hubs.insert(layoutgraph::NodeIDForSerialization(Some(&n)), newvals);
        }
    }

    let mut trees: BTreeMap<i64, Vec<SerializedTree>> = BTreeMap::new();
    let treeMap = g.b().Trees.clone();
    for rootSentinel in g.TreeOrder() {
        for root in treeMap.at(&rootSentinel) {
            guard.Step()?;
            let serialized = serializeTree(&root, &mut edgeIDs, &guard)?;
            let rootID = layoutgraph::NodeIDForSerialization(rootSentinel.as_ref());
            trees.entry(rootID).or_default().push(serialized);
        }
    }

    let mut seenHierarchies: Set<layoutgraph::HierarchyRef> = Set::default();
    let mut hierarchies: Vec<SerializedHierarchy> = Vec::new();
    for node in &gnodes {
        guard.Step()?;
        let Some(hierarchy) = node.b().Hierarchy.clone() else { continue };
        if seenHierarchies.contains(&hierarchy) {
            continue;
        }
        seenHierarchies.insert(hierarchy.clone());

        let mut nodeToLevel: BTreeMap<i64, i64> = BTreeMap::new();
        for (n, level) in layoutgraph::HierarchyLevelsForSerialization(&hierarchy).entries() {
            guard.Step()?;
            nodeToLevel.insert(n.id(), level);
        }
        hierarchies.push(SerializedHierarchy { Levels: nodeToLevel });
    }

    let mut directions: BTreeMap<i64, String> = BTreeMap::new();
    for (container, dir) in g.b().Directions.entries() {
        guard.Step()?;
        directions.insert(layoutgraph::NodeIDForSerialization(container.as_ref()), serializeDirection(dir));
    }
    guard.Finish()?;
    let (crossingCost, turnCost, nonCenterPortCost) = layoutgraph::CostsForSerialization(g);

    let (cellSize, isRoot) = {
        let gb = g.b();
        (gb.CellSize, gb.IsRootHierarchy)
    };
    Ok(SerializedGraph {
        Nodes: serializedNodes,
        Edges: serializedEdges,
        CellSize: cellSize,
        CrossingCost: crossingCost,
        TurnCost: turnCost,
        NonCenterPortCost: nonCenterPortCost,
        IsRootHierarchy: isRoot,
        Containers: containers,
        Hubs: hubs,
        Clusters: clusters,
        ClusterVessels: clusterVessels,
        Sequences: sequences,
        SequenceVessels: sequenceVessels,
        Trees: trees,
        Hierarchies: hierarchies,
        Directions: directions,
    })
}

fn serializeAbduction(abduction: &EdgeAbductionRef, edgeIDs: &mut edgeSerializationIDs) -> SerializedEdgeAbduction {
    let a = abduction.b().clone();
    SerializedEdgeAbduction {
        Edge: edgeIDs.reference(Some(&a.Edge)),
        OriginallyFromNode: layoutgraph::NodeIDForSerialization(a.OriginallyFrom.as_ref()),
        OriginallyToNode: layoutgraph::NodeIDForSerialization(a.OriginallyTo.as_ref()),
        CurrentFromNode: layoutgraph::NodeIDForSerialization(a.CurrentFrom.as_ref()),
        CurrentToNode: layoutgraph::NodeIDForSerialization(a.CurrentTo.as_ref()),
    }
}

pub(crate) fn serializeLabel(l: Option<&LabelRef>) -> Option<SerializedLabel> {
    let l = l?.b();
    Some(SerializedLabel { Text: l.Text.clone(), Position: l.Position.String(), Width: l.Width, Height: l.Height })
}

pub(crate) fn serializeIcon(i: Option<&IconRef>) -> Option<SerializedIcon> {
    let i = i?.b();
    Some(SerializedIcon { Position: i.Position.String() })
}

pub(crate) fn deserializeLabel(l: Option<&SerializedLabel>, guard: &limits::WorkGuardRef) -> GoResult<Option<LabelRef>> {
    let Some(l) = l else { return Ok(None) };
    guard.Step()?;
    Ok(Some(LabelRef::new(layoutgraph::Label::new(l.Text.clone(), label::FromString(&l.Position), l.Width, l.Height))))
}

pub(crate) fn deserializeIcon(i: Option<&SerializedIcon>, guard: &limits::WorkGuardRef) -> GoResult<Option<IconRef>> {
    let Some(i) = i else { return Ok(None) };
    guard.Step()?;
    Ok(Some(IconRef::new(layoutgraph::Icon::new(label::FromString(&i.Position)))))
}

pub(crate) fn serializeNode(node: &NodeRef, guard: &limits::WorkGuardRef) -> GoResult<SerializedNode> {
    let mut nears: Vec<i64> = Vec::new();
    for n in layoutgraph::OrderedNearsForSerialization(node) {
        guard.Step()?;
        nears.push(layoutgraph::NodeIDForSerialization(Some(&n)));
    }
    let shapeType = node.ShapeType();
    let numColumns = node.NumColumns();
    let n = node.b();
    let bx = n.Box.b();

    Ok(SerializedNode {
        ID: n.ID,
        IsInvisible: n.IsInvisible,
        D2ID: n.D2ID.clone(),
        Width: bx.Width,
        Height: bx.Height,
        TopLeft: serializePoint(bx.TopLeft.as_ref()),
        FixedTopLeft: serializePoint(n.FixedTopLeft.as_ref()),
        DesiredWidth: n.DesiredWidth,
        DesiredHeight: n.DesiredHeight,
        Nears: nears,
        FontSize: n.FontSize,
        ShapeType: shapeType,
        ForceHierarchy: n.ForceHierarchy,
        Label: serializeLabel(n.Label.as_ref()),
        Icon: serializeIcon(n.Icon.as_ref()),
        NumColumns: numColumns,
        Is3D: n.Is3D,
        IsMultiple: n.IsMultiple,
    })
}

pub(crate) fn serializeEdge(edge: &EdgeRef, edgeIDs: &mut edgeSerializationIDs, guard: &limits::WorkGuardRef) -> GoResult<SerializedEdge> {
    let e = edge.b();
    let mut points: Vec<Option<SerializedPoint>> = Vec::new();
    for point in &e.Points {
        guard.Step()?;
        points.push(serializePoint(Some(point)));
    }

    let serializedID = edgeIDs.reference(Some(edge));
    let originalID = if serializedID != e.ID { Some(e.ID) } else { None };

    Ok(SerializedEdge {
        Points: points,
        ID: serializedID,
        OriginalID: originalID,
        IsInvisible: e.IsInvisible,
        Style: serializeStyle(&e.Style),
        D2ID: e.D2ID.clone(),
        FromNode: layoutgraph::NodeIDForSerialization(Some(&e.From)),
        ToNode: layoutgraph::NodeIDForSerialization(Some(&e.To)),
        SourceArrowhead: e.SourceArrowhead.clone(),
        TargetArrowhead: e.TargetArrowhead.clone(),
        SourceArrowheadLabel: serializeLabel(e.SourceArrowheadLabel.as_ref()),
        TargetArrowheadLabel: serializeLabel(e.TargetArrowheadLabel.as_ref()),
        MinWidth: e.MinWidth,
        MinHeight: e.MinHeight,
        LabelPercentage: e.LabelPercentage,
        Label: serializeLabel(e.Label.as_ref()),
        FromTableColumnIndex: e.FromTableColumnIndex,
        ToTableColumnIndex: e.ToTableColumnIndex,
    })
}

/// serializeTree: post-order (children before the tree's own node and
/// sentinel edge), charging one step per child occurrence before descending,
/// exactly like Go's explicit frame stack.
pub(crate) fn serializeTree(t: &TreeRef, edgeIDs: &mut edgeSerializationIDs, guard: &limits::WorkGuardRef) -> GoResult<SerializedTree> {
    let (node, sentinel, orientation, children) = {
        let tb = t.b();
        (tb.Node.clone(), tb.SentinelEdge.clone(), tb.Orientation, tb.Children.clone())
    };
    let mut out = SerializedTree { Children: Vec::with_capacity(children.len()), ..Default::default() };
    for child in &children {
        guard.Step()?;
        out.Children.push(serializeTree(child, edgeIDs, guard)?);
    }
    let serializedNode = serializeNode(&node, guard)?;
    let Some(sentinel) = sentinel else {
        return Err(errorf!("cannot serialize a tree with a nil sentinel edge"));
    };
    let edge = serializeEdge(&sentinel, edgeIDs, guard)?;
    out.Node = serializedNode;
    out.SentinelEdge = edge;
    out.Orientation = orientation.0;
    Ok(out)
}

pub(crate) fn deserializeTreeNode(
    g: &GraphRef,
    serializedTree: &SerializedTree,
    parent: Option<&TreeRef>,
    nodesByID: &mut Map<i64, NodeRef>,
    edgesByID: &mut Map<i64, EdgeRef>,
    guard: &limits::WorkGuardRef,
) -> GoResult<TreeRef> {
    guard.Step()?;
    let node = match nodesByID.get(&serializedTree.Node.ID) {
        Some(n) => n.clone(),
        None => {
            let node = deserializeNode(Some(g), &serializedTree.Node, guard)?;
            for nearID in &serializedTree.Node.Nears {
                guard.Step()?;
                let Some(near) = nodesByID.get(nearID).cloned() else {
                    return Err(errorf!("could not find tree node near of ID: {}", nearID));
                };
                node.AddNear(Some(&near));
            }
            nodesByID.insert(node.id(), node.clone());
            node
        }
    };

    let edge = match edgesByID.get(&serializedTree.SentinelEdge.ID) {
        Some(e) => e.clone(),
        None => {
            let edge = deserializeEdge(&serializedTree.SentinelEdge, nodesByID, guard)?;
            edgesByID.insert(serializedTree.SentinelEdge.ID, edge.clone());
            edge
        }
    };

    Ok(TreeRef::new(layoutgraph::Tree {
        Node: node,
        SentinelEdge: Some(edge),
        Parent: parent.cloned(),
        Children: Vec::new(),
        Orientation: geo::Orientation(serializedTree.Orientation),
    }))
}

pub(crate) fn deserializeTree(
    g: &GraphRef,
    serializedRoot: &SerializedTree,
    nodesByID: &mut Map<i64, NodeRef>,
    edgesByID: &mut Map<i64, EdgeRef>,
    guard: &limits::WorkGuardRef,
) -> GoResult<TreeRef> {
    let root = deserializeTreeNode(g, serializedRoot, None, nodesByID, edgesByID, guard)?;
    // Preorder: each child is created when first reached, then its subtree.
    struct frame<'a> {
        serialized: &'a SerializedTree,
        tree: TreeRef,
        nextChild: usize,
    }
    let mut stack: Vec<frame> = vec![frame { serialized: serializedRoot, tree: root.clone(), nextChild: 0 }];
    while !stack.is_empty() {
        let top = stack.len() - 1;
        if stack[top].nextChild >= stack[top].serialized.Children.len() {
            stack.pop();
            continue;
        }
        let serializedChild = &stack[top].serialized.Children[stack[top].nextChild];
        stack[top].nextChild += 1;
        let parentTree = stack[top].tree.clone();
        let child = deserializeTreeNode(g, serializedChild, Some(&parentTree), nodesByID, edgesByID, guard)?;
        parentTree.bm().Children.push(child.clone());
        stack.push(frame { serialized: serializedChild, tree: child, nextChild: 0 });
    }
    Ok(root)
}

/// Unmarshal decodes fixture JSON into out.
pub fn Unmarshal(ctx: &Ctx, data: &[u8], out: Option<&GraphRef>) -> GoResult<()> {
    let sg: Option<SerializedGraph> = match serde_json::from_slice(data) {
        Ok(sg) => sg,
        Err(err) => return Err(errorf!("{}", err)),
    };
    Deserialize(ctx, sg.as_ref(), out)
}

pub(crate) fn deserializeEdge(edge: &SerializedEdge, nodesByID: &Map<i64, NodeRef>, guard: &limits::WorkGuardRef) -> GoResult<EdgeRef> {
    guard.Step()?;
    let Some(from) = nodesByID.get(&edge.FromNode) else {
        return Err(errorf!("error while deserializing: edge {} has from node {} which is missing from g.Nodes", edge.ID, edge.FromNode));
    };
    let Some(to) = nodesByID.get(&edge.ToNode) else {
        return Err(errorf!("error while deserializing: edge {} has to node {} which is missing from g.Nodes", edge.ID, edge.ToNode));
    };
    let newedge = layoutgraph::NewEdge(from, to);
    let sourceArrowheadLabel = deserializeLabel(edge.SourceArrowheadLabel.as_ref(), guard)?;
    let targetArrowheadLabel = deserializeLabel(edge.TargetArrowheadLabel.as_ref(), guard)?;
    {
        let mut e = newedge.bm();
        e.ID = edge.ID;
        if let Some(oid) = edge.OriginalID {
            e.ID = oid;
        }
        e.D2ID = edge.D2ID.clone();
        e.SourceArrowhead = edge.SourceArrowhead.clone();
        e.TargetArrowhead = edge.TargetArrowhead.clone();
        e.SourceArrowheadLabel = sourceArrowheadLabel;
        e.TargetArrowheadLabel = targetArrowheadLabel;
        e.MinWidth = edge.MinWidth;
        e.MinHeight = edge.MinHeight;
        e.LabelPercentage = edge.LabelPercentage;
        e.FromTableColumnIndex = edge.FromTableColumnIndex;
        e.ToTableColumnIndex = edge.ToTableColumnIndex;
    }
    for point in &edge.Points {
        guard.Step()?;
        // DEVIATION: Go keeps a nil route point (rejected later by topology
        // preflight); the port rejects it while decoding.
        let Some(p) = deserializePoint(point.as_ref()) else {
            return Err(errorf!("TALA engine topology contains nil edge route point"));
        };
        newedge.bm().Points.push(p);
    }
    let lbl = deserializeLabel(edge.Label.as_ref(), guard)?;
    {
        let mut e = newedge.bm();
        e.Label = lbl;
        e.IsInvisible = edge.IsInvisible;
        e.Style = deserializeStyle(&edge.Style);
    }
    Ok(newedge)
}

fn ser(s: &Option<StyleScalar>) -> Option<SerializedScalar> {
    s.as_ref().map(|v| SerializedScalar { Value: v.Value.clone() })
}
fn de(s: &Option<SerializedScalar>) -> Option<StyleScalar> {
    s.as_ref().map(|v| StyleScalar { Value: v.Value.clone() })
}

pub(crate) fn serializeStyle(style: &layoutgraph::EdgeStyle) -> SerializedStyle {
    SerializedStyle {
        Opacity: ser(&style.Opacity),
        Stroke: ser(&style.Stroke),
        Fill: ser(&style.Fill),
        FillPattern: ser(&style.FillPattern),
        StrokeWidth: ser(&style.StrokeWidth),
        StrokeDash: ser(&style.StrokeDash),
        BorderRadius: ser(&style.BorderRadius),
        Shadow: ser(&style.Shadow),
        ThreeDee: ser(&style.ThreeDee),
        Multiple: ser(&style.Multiple),
        Font: ser(&style.Font),
        FontSize: ser(&style.FontSize),
        FontColor: ser(&style.FontColor),
        Animated: ser(&style.Animated),
        Bold: ser(&style.Bold),
        Italic: ser(&style.Italic),
        Underline: ser(&style.Underline),
        Filled: ser(&style.Filled),
        DoubleBorder: ser(&style.DoubleBorder),
        TextTransform: ser(&style.TextTransform),
    }
}

pub(crate) fn deserializeStyle(style: &SerializedStyle) -> layoutgraph::EdgeStyle {
    layoutgraph::EdgeStyle {
        Opacity: de(&style.Opacity),
        Stroke: de(&style.Stroke),
        Fill: de(&style.Fill),
        FillPattern: de(&style.FillPattern),
        StrokeWidth: de(&style.StrokeWidth),
        StrokeDash: de(&style.StrokeDash),
        BorderRadius: de(&style.BorderRadius),
        Shadow: de(&style.Shadow),
        ThreeDee: de(&style.ThreeDee),
        Multiple: de(&style.Multiple),
        Font: de(&style.Font),
        FontSize: de(&style.FontSize),
        FontColor: de(&style.FontColor),
        Animated: de(&style.Animated),
        Bold: de(&style.Bold),
        Italic: de(&style.Italic),
        Underline: de(&style.Underline),
        Filled: de(&style.Filled),
        DoubleBorder: de(&style.DoubleBorder),
        TextTransform: de(&style.TextTransform),
    }
}

pub(crate) fn deserializeNode(g: Option<&GraphRef>, node: &SerializedNode, guard: &limits::WorkGuardRef) -> GoResult<NodeRef> {
    guard.Step()?;
    for _ in &node.Nears {
        guard.Step()?;
    }
    let n = layoutgraph::NewNode(node.ID, node.Width, node.Height);
    {
        let mut nb = n.bm();
        nb.D2ID = node.D2ID.clone();
        nb.Box.bm().TopLeft = deserializePoint(node.TopLeft.as_ref());
        nb.FixedTopLeft = deserializePoint(node.FixedTopLeft.as_ref());
        nb.Graph = g.cloned();
        nb.FontSize = node.FontSize;
        nb.DesiredWidth = node.DesiredWidth;
        nb.DesiredHeight = node.DesiredHeight;
    }
    n.SetShape(&node.ShapeType);
    n.SetNumColumns(node.NumColumns);
    let lbl = deserializeLabel(node.Label.as_ref(), guard)?;
    let icon = deserializeIcon(node.Icon.as_ref(), guard)?;
    {
        let mut nb = n.bm();
        nb.Label = lbl;
        nb.Icon = icon;
        nb.Is3D = node.Is3D;
        nb.IsMultiple = node.IsMultiple;
        nb.ForceHierarchy = node.ForceHierarchy;
        nb.IsInvisible = node.IsInvisible;
    }
    Ok(n)
}

/// Deserialize reconstructs graph while observing ctx throughout
/// its topology walks. It publishes the reconstructed graph atomically after
/// every fallible operation and the final context check have succeeded.
pub fn Deserialize(ctx: &Ctx, graph: Option<&SerializedGraph>, out: Option<&GraphRef>) -> GoResult<()> {
    let Some(graph) = graph else {
        return Err(errorf!("cannot deserialize a nil graph"));
    };
    let Some(out) = out else {
        return Err(errorf!("cannot deserialize into a nil graph"));
    };
    validateSerializedGraphForDeserialization(ctx, graph)?;
    let staged = layoutgraph::NewZeroGraph();
    deserializeGraphContext(ctx, graph, &staged, out)?;
    layoutgraph::PublishDeserializedGraph(out, &staged, graph.CrossingCost, graph.TurnCost, graph.NonCenterPortCost);
    Ok(())
}

fn deserializeAbductions(
    serialized: &[SerializedEdgeAbduction],
    owner: &str,
    id: i64,
    nodesByID: &Map<i64, NodeRef>,
    edgesByID: &Map<i64, EdgeRef>,
    guard: &limits::WorkGuardRef,
) -> GoResult<Vec<EdgeAbductionRef>> {
    let mut edgeAbductions: Vec<EdgeAbductionRef> = Vec::new();
    for val in serialized {
        guard.Step()?;
        let Some(edge) = edgesByID.get(&val.Edge).cloned() else {
            return Err(errorf!("couldn't find edge abduction edge {} for {} {}", val.Edge, owner, id));
        };
        let originallyFrom = nodesByID.get(&val.OriginallyFromNode).cloned();
        if val.OriginallyFromNode != 0 && originallyFrom.is_none() {
            return Err(errorf!("couldn't find edge abduction OriginallyFrom {} for {} {}", val.OriginallyFromNode, owner, id));
        }
        let originallyTo = nodesByID.get(&val.OriginallyToNode).cloned();
        if val.OriginallyToNode != 0 && originallyTo.is_none() {
            return Err(errorf!("couldn't find edge abduction OriginallyTo {} for {} {}", val.OriginallyToNode, owner, id));
        }
        let currentFrom = nodesByID.get(&val.CurrentFromNode).cloned();
        if val.CurrentFromNode != 0 && currentFrom.is_none() {
            return Err(errorf!("couldn't find edge abduction CurrentFrom {} for {} {}", val.CurrentFromNode, owner, id));
        }
        let currentTo = nodesByID.get(&val.CurrentToNode).cloned();
        if val.CurrentToNode != 0 && currentTo.is_none() {
            return Err(errorf!("couldn't find edge abduction CurrentTo {} for {} {}", val.CurrentToNode, owner, id));
        }

        edgeAbductions.push(EdgeAbductionRef::new(EdgeAbduction {
            Edge: edge,
            OriginallyFrom: originallyFrom,
            OriginallyTo: originallyTo,
            CurrentFrom: currentFrom,
            CurrentTo: currentTo,
        }));
    }
    Ok(edgeAbductions)
}

pub(crate) fn deserializeGraphContext(ctx: &Ctx, graph: &SerializedGraph, destination: &GraphRef, owner: &GraphRef) -> GoResult<()> {
    let guard = newWorkGuard(ctx, "DeserializeGraph")?;

    let mut nodes: Vec<NodeRef> = Vec::new();
    let mut nodesByID: Map<i64, NodeRef> = Map::default();

    for node in &graph.Nodes {
        guard.Step()?;
        if nodesByID.contains_key(&node.ID) {
            return Err(errorf!("error while deserializing: duplicate node ID {}", node.ID));
        }
        let newNode = deserializeNode(Some(owner), node, &guard)?;
        nodesByID.insert(node.ID, newNode.clone());
        nodes.push(newNode);
    }

    let mut edges: Vec<EdgeRef> = Vec::new();
    let mut edgesByID: Map<i64, EdgeRef> = Map::default();

    for edge in &graph.Edges {
        guard.Step()?;
        if edgesByID.contains_key(&edge.ID) {
            return Err(errorf!("error while deserializing: duplicate edge ID {}", edge.ID));
        }
        let newedge = deserializeEdge(edge, &nodesByID, &guard)?;
        layoutgraph::AddDecodedEdge(&newedge.from(), &newedge);
        if !newedge.IsLoop() {
            layoutgraph::AddDecodedEdge(&newedge.to(), &newedge);
        }
        edgesByID.insert(edge.ID, newedge.clone());
        edges.push(newedge);
    }

    // deserialize in a fixed order (BTreeMap keys are already sorted)
    let mut containerOrder: Vec<i64> = Vec::with_capacity(graph.Containers.len());
    for id in graph.Containers.keys() {
        guard.Step()?;
        containerOrder.push(*id);
    }

    let containers: GoMap<Option<NodeRef>, Vec<NodeRef>> = GoMap::new();
    for containerID in &containerOrder {
        guard.Step()?;
        let container = nodesByID.get(containerID).cloned();
        if *containerID != 0 && container.is_none() {
            return Err(errorf!("error while deserializing: missing container node {}", containerID));
        }

        let mut containerNodes: Vec<NodeRef> = Vec::new();
        for nID in &graph.Containers[containerID] {
            guard.Step()?;
            let Some(n) = nodesByID.get(nID).cloned() else {
                return Err(errorf!("error while deserializing: container {} has node {} which is missing from g.Nodes", containerID, nID));
            };
            containerNodes.push(n.clone());
            n.bm().Container = container.clone();
        }

        containers.set(container.clone(), containerNodes);
        if let Some(c) = &container {
            layoutgraph::MarkDecodedContainer(c);
        }
    }

    let mut hubOrder: Vec<i64> = Vec::with_capacity(graph.Hubs.len());
    for id in graph.Hubs.keys() {
        guard.Step()?;
        hubOrder.push(*id);
    }

    let mut clusterOrder: Vec<i64> = Vec::with_capacity(graph.Clusters.len());
    for id in graph.Clusters.keys() {
        guard.Step()?;
        clusterOrder.push(*id);
    }

    let clusters: GoMap<NodeRef, ClusterRef> = GoMap::new();
    for id in &clusterOrder {
        guard.Step()?;
        let mut vessel: Option<NodeRef> = None;
        if let Some(n) = nodesByID.get(id).cloned() {
            layoutgraph::MarkDecodedClusterVessel(&n);
            vessel = Some(n);
        } else if let Some(cv) = graph.ClusterVessels.get(id) {
            if cv.ID != *id {
                return Err(errorf!("error while deserializing: cluster vessel key {} does not match node ID {}", id, cv.ID));
            }
            let v = deserializeNode(Some(owner), cv, &guard)?;
            layoutgraph::MarkDecodedClusterVessel(&v);
            v.bm().Graph = None;
            nodesByID.insert(*id, v.clone());
            vessel = Some(v);
        }
        let Some(vessel) = vessel else {
            return Err(errorf!("error while deserializing: cluster {} has no node or serialized vessel", id));
        };
        let serializedCluster = &graph.Clusters[id];
        if serializedCluster.Container != 0 && !nodesByID.contains_key(&serializedCluster.Container) {
            return Err(errorf!("error while deserializing: cluster {} references missing container {}", id, serializedCluster.Container));
        }

        let mut clusterNodes: Vec<NodeRef> = Vec::new();
        for nID in &serializedCluster.Nodes {
            guard.Step()?;
            let Some(n) = nodesByID.get(nID).cloned() else {
                return Err(errorf!("error while deserializing: cluster {} has node {} which is missing from g.Nodes", id, nID));
            };
            clusterNodes.push(n);
        }

        let cluster = ClusterRef::new(layoutgraph::Cluster {
            Vessel: vessel.clone(),
            Arrangement: ClusterArrangement::from_string(serializedCluster.Arrangement.clone()),
            DesiredArrangement: ClusterArrangement::from_string(serializedCluster.DesiredArrangement.clone()),
            Graph: Some(owner.clone()),
            Container: nodesByID.get(&serializedCluster.Container).cloned(),
            Nodes: clusterNodes.clone(),
            Padding: serializedCluster.Padding,
            FixedSize: serializedCluster.FixedSize,
            EdgeAbductions: Vec::new(),
        });
        clusters.set(vessel, cluster.clone());
        for n in &clusterNodes {
            guard.Step()?;
            n.bm().Cluster = Some(cluster.clone());
        }
    }

    let mut sequenceOrder: Vec<i64> = Vec::with_capacity(graph.Sequences.len());
    for vessel in graph.Sequences.keys() {
        guard.Step()?;
        sequenceOrder.push(*vessel);
    }

    let sequences: GoMap<NodeRef, SequenceRef> = GoMap::new();
    for id in &sequenceOrder {
        guard.Step()?;
        let mut vessel: Option<NodeRef> = None;
        if let Some(n) = nodesByID.get(id).cloned() {
            vessel = Some(n);
        } else if let Some(sv) = graph.SequenceVessels.get(id) {
            if sv.ID != *id {
                return Err(errorf!("error while deserializing: sequence vessel key {} does not match node ID {}", id, sv.ID));
            }
            let v = deserializeNode(Some(owner), sv, &guard)?;
            v.bm().Graph = None;
            nodesByID.insert(*id, v.clone());
            vessel = Some(v);
        }
        let Some(vessel) = vessel else {
            return Err(errorf!("error while deserializing: sequence {} has no node or serialized vessel", id));
        };
        let seq = &graph.Sequences[id];
        if seq.Nodes.len() < 2 {
            return Err(errorf!("error while deserializing: sequence {} has {} steps; want at least 2", id, seq.Nodes.len()));
        }
        if seq.Container != 0 && !nodesByID.contains_key(&seq.Container) {
            return Err(errorf!("error while deserializing: sequence {} references missing container {}", id, seq.Container));
        }

        let mut sequenceNodes: Vec<NodeRef> = Vec::new();
        for nID in &seq.Nodes {
            guard.Step()?;
            let Some(n) = nodesByID.get(nID).cloned() else {
                return Err(errorf!("error while deserializing: sequence {} has node {} which is missing from g.Nodes", id, nID));
            };
            sequenceNodes.push(n);
        }

        let sequence = SequenceRef::new(layoutgraph::Sequence {
            Vessel: vessel.clone(),
            Graph: Some(owner.clone()),
            Container: nodesByID.get(&seq.Container).cloned(),
            Nodes: sequenceNodes.clone(),
            EdgeAbductions: Vec::new(),
        });
        sequences.set(vessel, sequence.clone());
        for n in &sequenceNodes {
            guard.Step()?;
            n.bm().Sequence = Some(sequence.clone());
        }
    }

    let hubs: GoMap<NodeRef, Vec<NodeRef>> = GoMap::new();
    for id in &hubOrder {
        guard.Step()?;
        let Some(hub) = nodesByID.get(id).cloned() else {
            return Err(errorf!("error while deserializing: hub {} is missing from g.Nodes", id));
        };

        let mut spokes: Vec<NodeRef> = Vec::new();
        for nID in &graph.Hubs[id] {
            guard.Step()?;
            let Some(n) = nodesByID.get(nID).cloned() else {
                return Err(errorf!("error while deserializing: hub {} has node {} which is missing from g.Nodes", id, nID));
            };
            spokes.push(n);
        }

        hubs.set(hub, spokes);
    }

    for id in &clusterOrder {
        guard.Step()?;
        let edgeAbductions = deserializeAbductions(&graph.Clusters[id].EdgeAbductions, "cluster", *id, &nodesByID, &edgesByID, &guard)?;
        let vessel = nodesByID.get(id).cloned().unwrap();
        clusters.get(&vessel).unwrap().bm().EdgeAbductions = edgeAbductions;
    }

    for id in &sequenceOrder {
        guard.Step()?;
        let edgeAbductions = deserializeAbductions(&graph.Sequences[id].EdgeAbductions, "sequence", *id, &nodesByID, &edgesByID, &guard)?;
        let vessel = nodesByID.get(id).cloned().unwrap();
        sequences.get(&vessel).unwrap().bm().EdgeAbductions = edgeAbductions;
    }

    // NOTE: Go ranges over the decoded map (random order); the port visits the
    // sentinel IDs in ascending order.
    let trees: GoMap<Option<NodeRef>, Vec<TreeRef>> = GoMap::new();
    for (rootSentinelID, serializedTrees) in &graph.Trees {
        guard.Step()?;
        let rootSentinel = nodesByID.get(rootSentinelID).cloned();
        if *rootSentinelID != 0 && rootSentinel.is_none() {
            return Err(errorf!("error while deserializing: tree references missing root sentinel {}", rootSentinelID));
        }
        for serializedTree in serializedTrees {
            let t = deserializeTree(owner, serializedTree, &mut nodesByID, &mut edgesByID, &guard)?;
            trees.bm().entry(rootSentinel.clone()).or_default().push(t);
        }
    }

    for node in &graph.Nodes {
        guard.Step()?;
        for nearID in &node.Nears {
            guard.Step()?;
            let Some(near) = nodesByID.get(nearID).cloned() else {
                return Err(errorf!("could not find node {} near node {}", node.ID, nearID));
            };
            nodesByID.get(&node.ID).unwrap().AddNear(Some(&near));
        }
    }

    for serialized in &graph.Hierarchies {
        guard.Step()?;
        let levels: GoMap<NodeRef, i64> = GoMap::new();
        let hierarchy = layoutgraph::NewDecodedHierarchy(levels.clone());
        for (nodeID, level) in &serialized.Levels {
            guard.Step()?;
            let Some(node) = nodesByID.get(nodeID).cloned() else {
                return Err(errorf!("error while deserializing: hierarchy references missing node {}", nodeID));
            };
            levels.set(node.clone(), *level);
            node.bm().Hierarchy = Some(hierarchy.clone());
        }
    }

    let directions: GoMap<Option<NodeRef>, geo::Orientation> = GoMap::new();
    for (nodeID, dirString) in &graph.Directions {
        guard.Step()?;
        let container = nodesByID.get(nodeID).cloned();
        if *nodeID != 0 && container.is_none() {
            return Err(errorf!("error while deserializing: direction references missing container {}", nodeID));
        }
        directions.set(container, deserializeDirection(dirString));
    }

    let mut filtered: Vec<NodeRef> = Vec::with_capacity(nodes.len());
    for n in &nodes {
        guard.Step()?;
        if layoutgraph::IsActiveClusterForSerialization(n.clusterField().as_ref()) {
            continue;
        }
        if layoutgraph::IsActiveSequenceForSerialization(n.sequenceField().as_ref()) {
            continue;
        }
        filtered.push(n.clone());
    }
    // Derive every fallible auxiliary index before publishing any state to out.
    let nodeToTree = layoutgraph::BuildNodeToTreeForDeserialization(&trees, &guard)?;
    guard.Finish()?;

    let mut d = destination.bm();
    d.Nodes = filtered;
    d.Edges = edges;
    d.CellSize = graph.CellSize;
    d.IsRootHierarchy = graph.IsRootHierarchy;
    d.Containers = containers;
    d.Hubs = hubs;
    d.Clusters = clusters;
    d.Sequences = sequences;
    d.Trees = trees;
    d.NodeToTree = nodeToTree;
    d.CommonUncleSiblings = GoMap::new();
    d.Directions = directions;
    Ok(())
}

/// Marshal encodes a graph in the internal fixture representation.
pub fn Marshal(ctx: &Ctx, g: Option<&GraphRef>) -> GoResult<Vec<u8>> {
    let sg = Serialize(ctx, g)?;
    match serde_json::to_vec(&sg) {
        Ok(b) => Ok(b),
        Err(err) => Err(errorf!("{}", err)),
    }
}
