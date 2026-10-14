// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/layoutgraph/clone.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

/// Clone returns the layout graph projection used for an independent layout
/// attempt. It deliberately copies the stable graph state, not transient
/// optimizer state: derived caches, herd assignments, long-distance placement
/// requirements, route diagnostics, and fixed-label bookkeeping are reset.
///
/// The returned graph is published only after the complete bounded copy has
/// succeeded. Every graph-owned node, cluster, and sequence is rebound to the
/// returned graph; inactive cluster and sequence vessels remain detached.
pub fn Clone(ctx: &Ctx, source: Option<&GraphRef>) -> GoResult<GraphRef> {
    let staged = NewZeroGraph();
    cloneInto(ctx, source, &staged)?;
    Ok(staged)
}

pub(crate) fn cloneInto(ctx: &Ctx, source: Option<&GraphRef>, staged: &GraphRef) -> GoResult<()> {
    let Some(source) = source else {
        return Err(errorf!("cannot clone a nil graph"));
    };
    validateEngineGraph(ctx, "CloneGraph", Some(source))?;
    let guard = limits::NewWorkGuard(ctx, "CloneGraph", limits::MaxEngineWorkUnits)?;

    let mut state = graphCloneState {
        source: source.clone(),
        clone: staged.clone(),
        guard: guard.clone(),
        nodesByID: Map::default(),
        nodeRecordsByID: Map::default(),
        topLevelNodeIDs: Set::default(),
        nodesBySource: Map::default(),
        nodeRecords: Vec::new(),
        treeNodeRecords: Vec::new(),
        edgesBySource: Map::default(),
        edgesByID: Map::default(),
        clustersBySource: Map::default(),
        sequencesBySource: Map::default(),
    };
    state.copy()?;
    guard.Finish()?;
    Ok(())
}

pub(crate) struct graphCloneState {
    source: GraphRef,
    clone: GraphRef,
    guard: limits::WorkGuardRef,

    nodesByID: Map<EntityID, NodeRef>,
    nodeRecordsByID: Map<EntityID, NodeRef>,
    // Cluster and sequence members join Graph.Nodes in the codec's top-level
    // node records; grouping vessels do not. Trees may reuse only the former.
    topLevelNodeIDs: Set<EntityID>,
    nodesBySource: Map<NodeRef, NodeRef>,
    nodeRecords: Vec<NodeRef>,
    treeNodeRecords: Vec<NodeRef>,

    edgesBySource: Map<EdgeRef, EdgeRef>,
    edgesByID: Map<EntityID, EdgeRef>,

    clustersBySource: Map<ClusterRef, ClusterRef>,
    sequencesBySource: Map<SequenceRef, SequenceRef>,
}

impl graphCloneState {
    fn copy(&mut self) -> GoResult<()> {
        let mut inSerializedNodes: Set<NodeRef> = Set::default();
        for node in self.source.nodes() {
            self.addNodeRecord(&node)?;
            inSerializedNodes.insert(node);
        }
        let sourceClusters = self.source.b().Clusters.clone();
        for vessel in self.source.ClusterOrder() {
            inSerializedNodes.insert(vessel.clone());
            let cluster = sourceClusters.get(&vessel).unwrap();
            let members = cluster.b().Nodes.clone();
            for node in &members {
                if inSerializedNodes.contains(node) {
                    continue;
                }
                self.addNodeRecord(node)?;
                inSerializedNodes.insert(node.clone());
            }
        }
        let sourceSequences = self.source.b().Sequences.clone();
        for vessel in self.source.SequenceOrder() {
            inSerializedNodes.insert(vessel.clone());
            let sequence = sourceSequences.get(&vessel).unwrap();
            let members = sequence.b().Nodes.clone();
            for node in &members {
                if inSerializedNodes.contains(node) {
                    continue;
                }
                self.addNodeRecord(node)?;
                inSerializedNodes.insert(node.clone());
            }
        }

        let sourceEdges = self.source.edges();
        let mut clonedEdges: Vec<EdgeRef> = Vec::with_capacity(sourceEdges.len());
        for edge in &sourceEdges {
            self.guard.Step()?;
            if self.edgesBySource.contains_key(edge) {
                return Err(errorf!("cannot clone duplicate edge record {}", edge.id()));
            }
            let id = edge.id();
            if id != 0 && self.edgesByID.contains_key(&id) {
                return Err(errorf!("cannot clone duplicate edge ID {}", id));
            }
            let cloned = self.copyEdge(edge)?;
            self.edgesBySource.insert(edge.clone(), cloned.clone());
            if id != 0 {
                self.edgesByID.insert(id, cloned.clone());
            }
            cloned.from().addEdge(&cloned);
            if !cloned.isLoop() {
                cloned.to().addEdge(&cloned);
            }
            clonedEdges.push(cloned);
        }
        self.clone.bm().Edges = clonedEdges;

        self.copyContainers()?;
        self.copyClusters()?;
        self.copySequences()?;
        self.copyHubs()?;
        self.copyAbductions()?;
        self.copyTrees()?;
        self.copyNears()?;
        self.copyHierarchies()?;
        self.copyDirections()?;

        let mut filtered: Vec<NodeRef> = Vec::with_capacity(self.nodeRecords.len());
        for sourceNode in &self.nodeRecords {
            self.guard.Step()?;
            let node = self.nodesBySource.get(sourceNode).unwrap().clone();
            if clusterIsActive(node.clusterField().as_ref()) {
                continue;
            }
            if sequenceIsActive(node.sequenceField().as_ref()) {
                continue;
            }
            filtered.push(node);
        }
        let (cellSize, isRoot, crossing, turn, ncp) = {
            let s = self.source.b();
            (s.CellSize, s.IsRootHierarchy, s.crossingCost, s.turnCost, s.nonCenterPortCost)
        };
        let mut c = self.clone.bm();
        c.Nodes = filtered;
        c.CellSize = cellSize;
        c.IsRootHierarchy = isRoot;
        c.CommonUncleSiblings = GoMap::new();
        c.edgeLengthCache = Some(Map::default());

        c.crossingCost = crossing;
        c.turnCost = turn;
        c.nonCenterPortCost = ncp;
        Ok(())
    }

    fn addNodeRecord(&mut self, source: &NodeRef) -> GoResult<NodeRef> {
        self.guard.Step()?;
        let id = source.id();
        if id == 0 {
            return Err(errorf!("cannot clone reserved node ID 0"));
        }
        if self.nodesByID.contains_key(&id) {
            return Err(errorf!("cannot clone duplicate node ID {}", id));
        }
        let cloned = copyNodeRecord(source, Some(&self.clone));
        self.nodesByID.insert(id, cloned.clone());
        self.nodeRecordsByID.insert(id, source.clone());
        self.topLevelNodeIDs.insert(id);
        self.nodesBySource.insert(source.clone(), cloned.clone());
        self.nodeRecords.push(source.clone());
        Ok(cloned)
    }

    /// copyAuxiliaryNodeRecord copies a node record embedded in another graph
    /// record, such as a grouping vessel or tree node.
    fn copyAuxiliaryNodeRecord(&mut self, source: &NodeRef, relation: &str) -> GoResult<(NodeRef, bool)> {
        let id = source.id();
        if id == 0 {
            return Err(errorf!("cannot clone {}: reserved node ID 0", relation));
        }
        if let Some(declared) = self.nodeRecordsByID.get(&id) {
            if declared != source {
                return Err(errorf!("cannot clone {}: distinct node record reuses ID {}", relation, id));
            }
            return Ok((self.nodesByID.get(&id).unwrap().clone(), false));
        }

        let cloned = copyNodeRecord(source, Some(&self.clone));
        self.nodesByID.insert(id, cloned.clone());
        self.nodeRecordsByID.insert(id, source.clone());
        self.nodesBySource.insert(source.clone(), cloned.clone());
        Ok((cloned, true))
    }

    fn resolveNode(&mut self, source: Option<&NodeRef>, relation: &str) -> GoResult<Option<NodeRef>> {
        let Some(source) = source else {
            return Ok(None);
        };
        self.guard.Step()?;
        let id = source.id();
        let Some(cloned) = self.nodesByID.get(&id).cloned() else {
            return Err(errorf!("cannot clone {}: node {} is not included in the graph", relation, id));
        };
        self.nodesBySource.insert(source.clone(), cloned.clone());
        Ok(Some(cloned))
    }

    fn copyEdge(&mut self, source: &EdgeRef) -> GoResult<EdgeRef> {
        let (sfrom, sto) = (source.from(), source.to());
        let from = self.resolveNode(Some(&sfrom), "edge source")?;
        let to = self.resolveNode(Some(&sto), "edge target")?;
        let (Some(from), Some(to)) = (from, to) else {
            return Err(errorf!("cannot clone edge {} with a nil endpoint", source.id()));
        };

        let cloned = NewEdge(&from, &to);
        let s = source.b();
        {
            let mut c = cloned.bm();
            c.ID = s.ID;
            c.D2ID = s.D2ID.clone();
            c.SourceArrowhead = s.SourceArrowhead.clone();
            c.TargetArrowhead = s.TargetArrowhead.clone();
            c.SourceArrowheadLabel = copyLabelRecord(s.SourceArrowheadLabel.as_ref());
            c.TargetArrowheadLabel = copyLabelRecord(s.TargetArrowheadLabel.as_ref());
            c.MinWidth = s.MinWidth;
            c.MinHeight = s.MinHeight;
            c.Label = copyLabelRecord(s.Label.as_ref());
            c.LabelPercentage = s.LabelPercentage;
            c.FromTableColumnIndex = s.FromTableColumnIndex;
            c.ToTableColumnIndex = s.ToTableColumnIndex;
            c.IsInvisible = s.IsInvisible;
            c.Style = copyEdgeStyle(&s.Style);
        }
        for point in &s.Points {
            self.guard.Step()?;
            let p = *point.b();
            cloned.bm().Points.push(geo::PointRef::new(p));
        }
        Ok(cloned)
    }

    fn copyContainers(&mut self) -> GoResult<()> {
        let sourceContainers = self.source.b().Containers.clone();
        let containers: GoMap<Option<NodeRef>, Vec<NodeRef>> = GoMap::new();
        self.clone.bm().Containers = containers.clone();
        if sourceContainers.len() == 0 {
            return Ok(());
        }
        let order = self.source.containerRDFSOrderContext(None, &self.guard)?;
        if sourceContainers.len() != order.len() + 1 {
            return Err(errorf!(
                "cannot clone containers: map length {} does not match hierarchy length {}",
                sourceContainers.len(),
                order.len() + 1
            ));
        }
        let mut order: Vec<Option<NodeRef>> = order.into_iter().map(Some).collect();
        order.push(None);
        for sourceContainer in &order {
            let container = self.resolveNode(sourceContainer.as_ref(), "container")?;
            let children = sourceContainers.at(sourceContainer);
            let mut clonedChildren: Vec<NodeRef> = Vec::with_capacity(children.len());
            for sourceChild in &children {
                if self.nodeRecordsByID.get(&sourceChild.id()) != Some(sourceChild) {
                    return Err(errorf!("cannot clone container child: exact node record is not available"));
                }
                let child = self.resolveNode(Some(sourceChild), "container child")?;
                let Some(child) = child else {
                    return Err(errorf!("cannot clone a nil container child"));
                };
                child.bm().Container = container.clone();
                clonedChildren.push(child);
            }
            containers.set(container.clone(), clonedChildren);
            if let Some(container) = &container {
                container.bm().isContainer = true;
            }
        }
        Ok(())
    }

    fn copyClusters(&mut self) -> GoResult<()> {
        let sourceClusters = self.source.b().Clusters.clone();
        let clusters: GoMap<NodeRef, ClusterRef> = GoMap::new();
        self.clone.bm().Clusters = clusters.clone();
        for sourceVessel in self.source.ClusterOrder() {
            self.guard.Step()?;
            let sourceCluster = sourceClusters.get(&sourceVessel).unwrap();
            let sc = sourceCluster.b().clone();
            if sc.Vessel != sourceVessel {
                return Err(errorf!("cannot clone cluster under vessel {} because its record vessel differs", sourceVessel.id()));
            }
            if let Some(c) = &sc.Container {
                if c.id() == sourceVessel.id() {
                    return Err(errorf!("cannot clone cluster {} because it cannot contain itself", sourceVessel.id()));
                }
            }
            let (vessel, copied) = self.copyAuxiliaryNodeRecord(&sourceVessel, "cluster vessel")?;
            if copied {
                vessel.bm().Graph = None;
            }
            vessel.bm().isClusterVessel = true;

            let container = self.resolveNode(sc.Container.as_ref(), "cluster container")?;
            let mut members: Vec<NodeRef> = Vec::with_capacity(sc.Nodes.len());
            for sourceMember in &sc.Nodes {
                let member = self.resolveNode(Some(sourceMember), "cluster member")?.unwrap();
                members.push(member);
            }
            let cluster = ClusterRef::new(Cluster {
                Vessel: vessel.clone(),
                Nodes: members.clone(),
                Arrangement: sc.Arrangement.clone(),
                DesiredArrangement: sc.DesiredArrangement.clone(),
                Graph: Some(self.clone.clone()),
                EdgeAbductions: Vec::new(),
                Padding: sc.Padding,
                FixedSize: sc.FixedSize,
                Container: container,
            });
            clusters.set(vessel.clone(), cluster.clone());
            self.clustersBySource.insert(sourceCluster.clone(), cluster.clone());
            for member in &members {
                self.guard.Step()?;
                member.bm().Cluster = Some(cluster.clone());
            }
        }
        Ok(())
    }

    fn copySequences(&mut self) -> GoResult<()> {
        let sourceSequences = self.source.b().Sequences.clone();
        let sequences: GoMap<NodeRef, SequenceRef> = GoMap::new();
        self.clone.bm().Sequences = sequences.clone();
        for sourceVessel in self.source.SequenceOrder() {
            self.guard.Step()?;
            let sourceSequence = sourceSequences.get(&sourceVessel).unwrap();
            let ss = sourceSequence.b().clone();
            if ss.Vessel != sourceVessel {
                return Err(errorf!("cannot clone sequence under vessel {} because its record vessel differs", sourceVessel.id()));
            }
            if let Some(c) = &ss.Container {
                if c.id() == sourceVessel.id() {
                    return Err(errorf!("cannot clone sequence {} because it cannot contain itself", sourceVessel.id()));
                }
            }
            if ss.Nodes.len() < 2 {
                return Err(errorf!("cannot clone sequence {} with {} steps; want at least 2", sourceVessel.id(), ss.Nodes.len()));
            }
            let (vessel, copied) = self.copyAuxiliaryNodeRecord(&sourceVessel, "sequence vessel")?;
            if copied {
                vessel.bm().Graph = None;
            }

            let container = self.resolveNode(ss.Container.as_ref(), "sequence container")?;
            let mut members: Vec<NodeRef> = Vec::with_capacity(ss.Nodes.len());
            for sourceMember in &ss.Nodes {
                let member = self.resolveNode(Some(sourceMember), "sequence member")?.unwrap();
                members.push(member);
            }
            let sequence = SequenceRef::new(Sequence {
                Vessel: vessel.clone(),
                Nodes: members.clone(),
                Graph: Some(self.clone.clone()),
                EdgeAbductions: Vec::new(),
                Container: container,
            });
            sequences.set(vessel.clone(), sequence.clone());
            self.sequencesBySource.insert(sourceSequence.clone(), sequence.clone());
            for member in &members {
                self.guard.Step()?;
                member.bm().Sequence = Some(sequence.clone());
            }
        }
        Ok(())
    }

    fn copyHubs(&mut self) -> GoResult<()> {
        let sourceHubs = self.source.b().Hubs.clone();
        let hubs: GoMap<NodeRef, Vec<NodeRef>> = GoMap::new();
        self.clone.bm().Hubs = hubs.clone();
        for sourceHub in self.source.HubOrder() {
            let hub = self.resolveNode(Some(&sourceHub), "hub")?.unwrap();
            let sourceSpokes = sourceHubs.at(&sourceHub);
            let mut spokes: Vec<NodeRef> = Vec::with_capacity(sourceSpokes.len());
            for sourceSpoke in &sourceSpokes {
                if self.nodeRecordsByID.get(&sourceSpoke.id()) != Some(sourceSpoke) {
                    return Err(errorf!("cannot clone hub spoke: exact node record is not included in the graph"));
                }
                let spoke = self.resolveNode(Some(sourceSpoke), "hub spoke")?.unwrap();
                spokes.push(spoke);
            }
            hubs.set(hub, spokes);
        }
        Ok(())
    }

    fn copyAbductionsFor(&mut self, sourceAbductions: &[EdgeAbductionRef]) -> GoResult<Vec<EdgeAbductionRef>> {
        let mut cloned: Vec<EdgeAbductionRef> = Vec::with_capacity(sourceAbductions.len());
        for sourceAbduction in sourceAbductions {
            self.guard.Step()?;
            let sa = sourceAbduction.b().clone();
            let mut edge = self.edgesBySource.get(&sa.Edge).cloned();
            if edge.is_none() && sa.Edge.id() != 0 {
                edge = self.edgesByID.get(&sa.Edge.id()).cloned();
            }
            let Some(edge) = edge else {
                return Err(errorf!("cannot clone edge abduction for edge {} before the edge is copied", sa.Edge.id()));
            };
            let originallyFrom = self.resolveNode(sa.OriginallyFrom.as_ref(), "abduction original source")?;
            let originallyTo = self.resolveNode(sa.OriginallyTo.as_ref(), "abduction original target")?;
            let currentFrom = self.resolveNode(sa.CurrentFrom.as_ref(), "abduction current source")?;
            let currentTo = self.resolveNode(sa.CurrentTo.as_ref(), "abduction current target")?;
            cloned.push(EdgeAbductionRef::new(EdgeAbduction {
                Edge: edge,
                OriginallyFrom: originallyFrom,
                OriginallyTo: originallyTo,
                CurrentFrom: currentFrom,
                CurrentTo: currentTo,
            }));
        }
        Ok(cloned)
    }

    fn copyAbductions(&mut self) -> GoResult<()> {
        let sourceClusters = self.source.b().Clusters.clone();
        for sourceVessel in self.source.ClusterOrder() {
            let sourceCluster = sourceClusters.get(&sourceVessel).unwrap();
            let abductions = sourceCluster.b().EdgeAbductions.clone();
            let cloned = self.copyAbductionsFor(&abductions)?;
            self.clustersBySource.get(&sourceCluster).unwrap().bm().EdgeAbductions = cloned;
        }
        let sourceSequences = self.source.b().Sequences.clone();
        for sourceVessel in self.source.SequenceOrder() {
            let sourceSequence = sourceSequences.get(&sourceVessel).unwrap();
            let abductions = sourceSequence.b().EdgeAbductions.clone();
            let cloned = self.copyAbductionsFor(&abductions)?;
            self.sequencesBySource.get(&sourceSequence).unwrap().bm().EdgeAbductions = cloned;
        }
        Ok(())
    }

    fn copyTree(
        &mut self,
        source: &TreeRef,
        parent: Option<&TreeRef>,
        treeOwnersByNodeID: &mut Set<EntityID>,
        nodeToTree: &GoMap<NodeRef, TreeRef>,
    ) -> GoResult<TreeRef> {
        self.guard.Step()?;
        let (sourceSentinel, sourceNode, sourceOrientation, sourceChildren) = {
            let t = source.b();
            (t.SentinelEdge.clone(), t.Node.clone(), t.Orientation, t.Children.clone())
        };
        let Some(sourceSentinel) = sourceSentinel else {
            return Err(errorf!("cannot clone a tree with a nil sentinel edge"));
        };
        let nodeID = sourceNode.id();
        if treeOwnersByNodeID.contains(&nodeID) {
            return Err(errorf!("cannot clone tree node {} with more than one tree owner", nodeID));
        }
        treeOwnersByNodeID.insert(nodeID);
        if self.nodeRecordsByID.get(&nodeID) == Some(&sourceNode) && !self.topLevelNodeIDs.contains(&nodeID) {
            return Err(errorf!("cannot clone tree node {} because it repeats a non-top-level node record", nodeID));
        }

        let (node, copied) = self.copyAuxiliaryNodeRecord(&sourceNode, "tree node")?;
        if copied {
            self.treeNodeRecords.push(sourceNode.clone());
        }

        let mut sentinel = self.edgesBySource.get(&sourceSentinel).cloned();
        let sid = sourceSentinel.id();
        if sentinel.is_none() && sid != 0 && self.edgesByID.contains_key(&sid) {
            return Err(errorf!("cannot clone tree sentinel edge: distinct edge record reuses ID {}", sid));
        }
        if sentinel.is_none() {
            let s = self.copyEdge(&sourceSentinel)?;
            self.edgesBySource.insert(sourceSentinel.clone(), s.clone());
            if sid != 0 {
                self.edgesByID.insert(sid, s.clone());
            }
            sentinel = Some(s);
        }
        let cloned = TreeRef::new(Tree {
            Node: node.clone(),
            Parent: parent.cloned(),
            Children: Vec::new(),
            SentinelEdge: sentinel,
            Orientation: sourceOrientation,
        });
        nodeToTree.set(node, cloned.clone());
        for sourceChild in &sourceChildren {
            let child = self.copyTree(sourceChild, Some(&cloned), treeOwnersByNodeID, nodeToTree)?;
            cloned.bm().Children.push(child);
        }
        Ok(cloned)
    }

    fn copyTrees(&mut self) -> GoResult<()> {
        let sourceTrees = self.source.b().Trees.clone();
        let trees: GoMap<Option<NodeRef>, Vec<TreeRef>> = GoMap::new();
        let nodeToTree: GoMap<NodeRef, TreeRef> = GoMap::new();
        {
            let mut c = self.clone.bm();
            c.Trees = trees.clone();
            c.NodeToTree = nodeToTree.clone();
        }
        let mut treeOwnersByNodeID: Set<EntityID> = Set::default();

        for sourceSentinel in self.source.TreeOrder() {
            let sentinel = self.resolveNode(sourceSentinel.as_ref(), "tree root sentinel")?;
            let mut roots: Vec<TreeRef> = Vec::new();
            for sourceRoot in sourceTrees.at(&sourceSentinel) {
                let root = self.copyTree(&sourceRoot, None, &mut treeOwnersByNodeID, &nodeToTree)?;
                roots.push(root);
            }
            if !roots.is_empty() {
                trees.set(sentinel, roots);
            }
        }
        Ok(())
    }

    fn copyNears(&mut self) -> GoResult<()> {
        let mut sources = self.nodeRecords.clone();
        sources.extend(self.treeNodeRecords.iter().cloned());
        for source in &sources {
            let node = self.nodesBySource.get(source).unwrap().clone();
            for sourceNear in source.orderedNears() {
                self.guard.Step()?;
                let near = self.resolveNode(Some(&sourceNear), "near relation")?;
                node.AddNear(near.as_ref());
            }
        }
        Ok(())
    }

    fn copyHierarchies(&mut self) -> GoResult<()> {
        let mut seen: Set<HierarchyRef> = Set::default();
        for sourceNode in self.source.nodes() {
            self.guard.Step()?;
            let Some(sourceHierarchy) = sourceNode.b().Hierarchy.clone() else {
                continue;
            };
            if seen.contains(&sourceHierarchy) {
                continue;
            }
            seen.insert(sourceHierarchy.clone());
            let levels: GoMap<NodeRef, i64> = GoMap::new();
            let clonedHierarchy = HierarchyRef::new(Hierarchy { level: levels.clone(), LevelCount: 0 });
            let sourceLevels = sourceHierarchy.b().level.entries();
            for (sourceMember, level) in &sourceLevels {
                let member = self.resolveNode(Some(sourceMember), "hierarchy member")?.unwrap();
                levels.set(member.clone(), *level);
                member.bm().Hierarchy = Some(clonedHierarchy.clone());
            }
        }
        Ok(())
    }

    fn copyDirections(&mut self) -> GoResult<()> {
        let sourceDirections = self.source.b().Directions.entries();
        let directions: GoMap<Option<NodeRef>, geo::Orientation> = GoMap::new();
        self.clone.bm().Directions = directions.clone();
        for (sourceContainer, sourceDirection) in &sourceDirections {
            let container = self.resolveNode(sourceContainer.as_ref(), "direction container")?;
            directions.set(container, canonicalDirection(*sourceDirection));
        }
        Ok(())
    }
}

pub(crate) fn copyNodeRecord(source: &NodeRef, owner: Option<&GraphRef>) -> NodeRef {
    let (id, w, h) = (source.id(), source.w(), source.h());
    let cloned = NewNode(id, w, h);
    let shapeType = source.ShapeType();
    let numColumns = source.NumColumns();
    {
        let s = source.b();
        let mut c = cloned.bm();
        c.D2ID = s.D2ID.clone();
        let tl = s.Box.b().TopLeft.as_ref().map(|p| geo::PointRef::new(*p.b()));
        c.Box.bm().TopLeft = tl;
        c.FixedTopLeft = s.FixedTopLeft.as_ref().map(|p| geo::PointRef::new(*p.b()));
        c.Graph = owner.cloned();
        c.FontSize = s.FontSize;
        c.DesiredWidth = s.DesiredWidth;
        c.DesiredHeight = s.DesiredHeight;
    }
    cloned.SetShape(&shapeType);
    cloned.SetNumColumns(numColumns);
    {
        let s = source.b();
        let mut c = cloned.bm();
        c.ForceHierarchy = s.ForceHierarchy;
        c.Label = copyLabelRecord(s.Label.as_ref());
        c.Icon = copyIconRecord(s.Icon.as_ref());
        c.Is3D = s.Is3D;
        c.IsMultiple = s.IsMultiple;
        c.IsInvisible = s.IsInvisible;
    }
    cloned
}

pub(crate) fn copyLabelRecord(source: Option<&LabelRef>) -> Option<LabelRef> {
    let source = source?;
    let s = source.b();
    Some(LabelRef::new(Label {
        Text: s.Text.clone(),
        Position: label::FromString(&s.Position.String()),
        Width: s.Width,
        Height: s.Height,
        positionFixed: false,
    }))
}

pub(crate) fn copyIconRecord(source: Option<&IconRef>) -> Option<IconRef> {
    let source = source?;
    let pos = source.b().Position;
    Some(IconRef::new(Icon { Position: label::FromString(&pos.String()), positionFixed: false }))
}

pub(crate) fn copyEdgeStyle(source: &EdgeStyle) -> EdgeStyle {
    source.clone()
}

pub(crate) fn canonicalDirection(direction: geo::Orientation) -> geo::Orientation {
    if direction == geo::Top || direction == geo::Bottom || direction == geo::Left || direction == geo::Right {
        direction
    } else {
        geo::NONE
    }
}
