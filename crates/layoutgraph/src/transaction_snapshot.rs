// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/layoutgraph/transaction_snapshot.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

/// Go `pointerSnapshot[T]`: the original pointer plus the pointee's value.
#[derive(Clone)]
pub(crate) struct pointerSnapshot<R: Clone, T: Clone> {
    pub(crate) pointer: Option<R>,
    pub(crate) value: T,
}

pub(crate) fn snapshotPoint(pointer: Option<&geo::PointRef>) -> pointerSnapshot<geo::PointRef, geo::Point> {
    match pointer {
        None => pointerSnapshot { pointer: None, value: geo::Point::default() },
        Some(p) => pointerSnapshot { pointer: Some(p.clone()), value: *p.b() },
    }
}

impl pointerSnapshot<geo::PointRef, geo::Point> {
    pub(crate) fn restore(&self) -> Option<geo::PointRef> {
        let p = self.pointer.as_ref()?;
        *p.bm() = self.value;
        Some(p.clone())
    }
}

pub(crate) fn snapshotLabel(pointer: Option<&LabelRef>) -> pointerSnapshot<LabelRef, Label> {
    match pointer {
        None => pointerSnapshot { pointer: None, value: Label::default() },
        Some(p) => pointerSnapshot { pointer: Some(p.clone()), value: p.b().clone() },
    }
}

impl pointerSnapshot<LabelRef, Label> {
    pub(crate) fn restore(&self) -> Option<LabelRef> {
        let p = self.pointer.as_ref()?;
        *p.bm() = self.value.clone();
        Some(p.clone())
    }
}

pub(crate) fn snapshotIcon(pointer: Option<&IconRef>) -> pointerSnapshot<IconRef, Icon> {
    match pointer {
        None => pointerSnapshot { pointer: None, value: Icon::default() },
        Some(p) => pointerSnapshot { pointer: Some(p.clone()), value: p.b().clone() },
    }
}

impl pointerSnapshot<IconRef, Icon> {
    pub(crate) fn restore(&self) -> Option<IconRef> {
        let p = self.pointer.as_ref()?;
        *p.bm() = self.value.clone();
        Some(p.clone())
    }
}

/// Go `nodeSnapshot`. The struct value excludes the embedded box, whose value is
/// captured separately so the restore keeps the node's (shape-aliased) box
/// identity, exactly like Go's `*node = s.value`.
pub(crate) struct nodeSnapshot {
    pub(crate) value: Node,
    pub(crate) boxValue: geo::Box,

    pub(crate) topLeft: pointerSnapshot<geo::PointRef, geo::Point>,
    pub(crate) fixedTopLeft: pointerSnapshot<geo::PointRef, geo::Point>,
    pub(crate) label: pointerSnapshot<LabelRef, Label>,
    pub(crate) icon: pointerSnapshot<IconRef, Icon>,
}

pub(crate) struct edgeSnapshot {
    pub(crate) value: Edge,

    pub(crate) points: Vec<geo::PointRef>,
    pub(crate) pointValues: Vec<pointerSnapshot<geo::PointRef, geo::Point>>,
    pub(crate) label: pointerSnapshot<LabelRef, Label>,
    pub(crate) sourceArrowheadLabel: pointerSnapshot<LabelRef, Label>,
    pub(crate) targetArrowheadLabel: pointerSnapshot<LabelRef, Label>,
}

pub(crate) struct clusterSnapshot {
    pub(crate) value: Cluster,
}

pub(crate) struct sequenceSnapshot {
    pub(crate) value: Sequence,
}

pub(crate) struct treeSnapshot {
    pub(crate) value: Tree,
}

pub(crate) struct herdSnapshot {
    pub(crate) value: HerdAssignment,
}

pub(crate) struct hierarchySnapshot {
    pub(crate) value: Hierarchy,
    pub(crate) level: Map<NodeRef, i64>,
}

/// A captured shared map: the original handle and its contents.
pub(crate) struct mapSnapshot<K, V> {
    pub(crate) original: GoMap<K, V>,
    pub(crate) values: Map<K, V>,
}

impl<K: Eq + std::hash::Hash + Clone, V: Clone> mapSnapshot<K, V> {
    pub(crate) fn capture(m: &GoMap<K, V>) -> Self {
        mapSnapshot { original: m.clone(), values: m.to_map() }
    }
    /// Go `restoreMap` / `exactSliceMapSnapshot.restore`: refill the original
    /// map object and hand it back.
    pub(crate) fn restore(&self) -> GoMap<K, V> {
        *self.original.bm() = self.values.clone();
        self.original.clone()
    }
}

pub(crate) struct graphSnapshot {
    pub(crate) isRootHierarchy: bool,
    pub(crate) nodes: Vec<NodeRef>,
    pub(crate) edges: Vec<EdgeRef>,
    pub(crate) cellSize: f64,

    pub(crate) containers: mapSnapshot<Option<NodeRef>, Vec<NodeRef>>,
    pub(crate) clusters: mapSnapshot<NodeRef, ClusterRef>,
    pub(crate) trees: mapSnapshot<Option<NodeRef>, Vec<TreeRef>>,
    pub(crate) nodeToTree: mapSnapshot<NodeRef, TreeRef>,
    pub(crate) hubs: mapSnapshot<NodeRef, Vec<NodeRef>>,
    pub(crate) sequences: mapSnapshot<NodeRef, SequenceRef>,
    pub(crate) directions: mapSnapshot<Option<NodeRef>, geo::Orientation>,
    pub(crate) commonSiblings: mapSnapshot<NodeRef, Nodes>,
}

pub(crate) fn captureNode(node: &NodeRef) -> nodeSnapshot {
    let n = node.b();
    let boxValue = n.Box.b().clone();
    nodeSnapshot {
        value: n.shallowCopy(),
        topLeft: snapshotPoint(boxValue.TopLeft.as_ref()),
        boxValue,
        fixedTopLeft: snapshotPoint(n.FixedTopLeft.as_ref()),
        label: snapshotLabel(n.Label.as_ref()),
        icon: snapshotIcon(n.Icon.as_ref()),
    }
}

impl nodeSnapshot {
    pub(crate) fn restore(&self, node: &NodeRef) {
        self.topLeft.restore();
        self.fixedTopLeft.restore();
        self.label.restore();
        self.icon.restore();

        let mut value = self.value.shallowCopy();
        let mut n = node.bm();
        // Keep the node's own box object (shared with its shape) and restore its
        // contents, as Go's embedded-value assignment does.
        value.Box = n.Box.clone();
        *n = value;
        *n.Box.bm() = self.boxValue.clone();
    }
}

pub(crate) fn captureEdge(edge: &EdgeRef) -> edgeSnapshot {
    let e = edge.b();
    let pointValues: Vec<_> = e.Points.iter().map(|p| snapshotPoint(Some(p))).collect();
    edgeSnapshot {
        value: e.shallowCopy(),
        points: e.Points.clone(),
        pointValues,
        label: snapshotLabel(e.Label.as_ref()),
        sourceArrowheadLabel: snapshotLabel(e.SourceArrowheadLabel.as_ref()),
        targetArrowheadLabel: snapshotLabel(e.TargetArrowheadLabel.as_ref()),
    }
}

impl edgeSnapshot {
    pub(crate) fn restore(&self, edge: &EdgeRef) {
        self.label.restore();
        self.sourceArrowheadLabel.restore();
        self.targetArrowheadLabel.restore();
        // Repair the point objects before reattaching the route so rollback
        // preserves aliases to both the route slice and its point objects.
        for point in &self.pointValues {
            point.restore();
        }
        let mut value = self.value.shallowCopy();
        value.Points = self.points.clone();
        *edge.bm() = value;
    }
}

pub(crate) fn captureGraph(graph: &GraphRef) -> graphSnapshot {
    let g = graph.b();
    graphSnapshot {
        isRootHierarchy: g.IsRootHierarchy,
        nodes: g.Nodes.clone(),
        edges: g.Edges.clone(),
        cellSize: g.CellSize,
        containers: mapSnapshot::capture(&g.Containers),
        clusters: mapSnapshot::capture(&g.Clusters),
        trees: mapSnapshot::capture(&g.Trees),
        nodeToTree: mapSnapshot::capture(&g.NodeToTree),
        hubs: mapSnapshot::capture(&g.Hubs),
        sequences: mapSnapshot::capture(&g.Sequences),
        directions: mapSnapshot::capture(&g.Directions),
        commonSiblings: mapSnapshot::capture(&g.CommonUncleSiblings),
    }
}

impl GraphStateRef {
    /// captureGeometryState records the fields changed by ordinary speculative
    /// placement transactions without walking and copying the graph's ownership
    /// maps. Topology-changing transactions additionally call captureRuntimeState.
    pub(crate) fn captureGeometryStateContext(&self, graph: &GraphRef, guard: &limits::WorkGuardRef) -> GoResult<()> {
        let captureEdgeRoutes = self.b().captureEdgeRoutes;
        if captureEdgeRoutes {
            // Go collects the edges into a set and then iterates it; the
            // captured snapshots are order-independent.
            let mut edges: Set<EdgeRef> = Set::default();
            let mut edgeOrder: Vec<EdgeRef> = Vec::new();
            for edge in graph.edges() {
                guard.Step()?;
                if edges.insert(edge.clone()) {
                    edgeOrder.push(edge);
                }
            }
            let geometryNodes: Vec<NodeRef> = self.b().nodeGeometry.keys().cloned().collect();
            for node in &geometryNodes {
                guard.Step()?;
                for edge in node.edges() {
                    guard.Step()?;
                    if edges.insert(edge.clone()) {
                        edgeOrder.push(edge);
                    }
                }
            }
            let mut edgeGeometry: Map<EdgeRef, edgeSnapshot> = Map::default();
            for edge in &edgeOrder {
                guard.Step()?;
                let npoints = edge.b().Points.len();
                for _ in 0..npoints {
                    guard.Step()?;
                }
                edgeGeometry.insert(edge.clone(), captureEdge(edge));
            }
            self.bm().edgeGeometry = Some(edgeGeometry);
        } else {
            self.bm().edgeGeometry = None;
        }

        let nodeToTree = graph.b().NodeToTree.clone();
        let mut treeOrientations: Map<TreeRef, geo::Orientation> = Map::default();
        for tree in nodeToTree.values() {
            guard.Step()?;
            let o = tree.b().Orientation;
            treeOrientations.insert(tree, o);
        }
        self.bm().treeOrientations = treeOrientations;
        guard.Finish()
    }
}

impl graphSnapshot {
    pub(crate) fn restore(&self, graph: &GraphRef) {
        let containers = self.containers.restore();
        let clusters = self.clusters.restore();
        let trees = self.trees.restore();
        let nodeToTree = self.nodeToTree.restore();
        let hubs = self.hubs.restore();
        let sequences = self.sequences.restore();
        let directions = self.directions.restore();
        let commonSiblings = self.commonSiblings.restore();
        let mut g = graph.bm();
        g.IsRootHierarchy = self.isRootHierarchy;
        g.Nodes = self.nodes.clone();
        g.Edges = self.edges.clone();
        g.CellSize = self.cellSize;
        g.Containers = containers;
        g.Clusters = clusters;
        g.Trees = trees;
        g.NodeToTree = nodeToTree;
        g.Hubs = hubs;
        g.Sequences = sequences;
        g.Directions = directions;
        g.CommonUncleSiblings = commonSiblings;
    }
}

/// Go `runtimeObjectCollection`. `nodeOrder` is the deterministic discovery
/// order; the other members are kept in discovery order as well.
#[derive(Default)]
pub(crate) struct runtimeObjectCollection {
    pub(crate) nodes: Set<NodeRef>,
    pub(crate) nodeOrder: Vec<NodeRef>,
    pub(crate) edges: Vec<EdgeRef>,
    pub(crate) clusters: Vec<ClusterRef>,
    pub(crate) sequences: Vec<SequenceRef>,
    pub(crate) trees: Vec<TreeRef>,
    pub(crate) edgeAbductions: Vec<EdgeAbductionRef>,
    pub(crate) herds: Vec<HerdAssignmentRef>,
    pub(crate) hierarchies: Vec<HierarchyRef>,
}

struct runtimeCollector<'a> {
    guard: &'a limits::WorkGuardRef,
    scope: &'a str,
    captureErr: Option<GoError>,
    nodes: Set<NodeRef>,
    edges: Set<EdgeRef>,
    clusters: Set<ClusterRef>,
    sequences: Set<SequenceRef>,
    trees: Set<TreeRef>,
    edgeAbductions: Set<EdgeAbductionRef>,
    herds: Set<HerdAssignmentRef>,
    hierarchies: Set<HierarchyRef>,
    nodeQueue: Vec<NodeRef>,
    edgeQueue: Vec<EdgeRef>,
    clusterQueue: Vec<ClusterRef>,
    sequenceQueue: Vec<SequenceRef>,
    treeQueue: Vec<TreeRef>,
    abductionQueue: Vec<EdgeAbductionRef>,
    herdQueue: Vec<HerdAssignmentRef>,
    hierarchyQueue: Vec<HierarchyRef>,
}

impl<'a> runtimeCollector<'a> {
    fn charge(&mut self) -> bool {
        if self.captureErr.is_some() {
            return false;
        }
        if let Err(e) = self.guard.Step() {
            self.captureErr = Some(e);
            return false;
        }
        true
    }
    fn addNode(&mut self, node: Option<&NodeRef>) {
        if !self.charge() {
            return;
        }
        let Some(node) = node else { return };
        if self.nodes.contains(node) {
            return;
        }
        if self.nodes.len() as i64 >= maxEngineNodes {
            self.captureErr = Some(errorf!("TALA {} node snapshot exceeds limit {}", self.scope, maxEngineNodes));
            return;
        }
        self.nodes.insert(node.clone());
        self.nodeQueue.push(node.clone());
    }
    fn addEdge(&mut self, edge: Option<&EdgeRef>) {
        if !self.charge() {
            return;
        }
        let Some(edge) = edge else { return };
        if self.edges.contains(edge) {
            return;
        }
        if self.edges.len() as i64 >= maxEngineEdges {
            self.captureErr = Some(errorf!("TALA {} edge snapshot exceeds limit {}", self.scope, maxEngineEdges));
            return;
        }
        self.edges.insert(edge.clone());
        self.edgeQueue.push(edge.clone());
    }
    fn addCluster(&mut self, cluster: Option<&ClusterRef>) {
        if !self.charge() {
            return;
        }
        let Some(cluster) = cluster else { return };
        if self.clusters.contains(cluster) {
            return;
        }
        if self.clusters.len() as i64 >= maxEngineNodes {
            self.captureErr = Some(errorf!("TALA {} cluster snapshot exceeds limit {}", self.scope, maxEngineNodes));
            return;
        }
        self.clusters.insert(cluster.clone());
        self.clusterQueue.push(cluster.clone());
    }
    fn addSequence(&mut self, sequence: Option<&SequenceRef>) {
        if !self.charge() {
            return;
        }
        let Some(sequence) = sequence else { return };
        if self.sequences.contains(sequence) {
            return;
        }
        if self.sequences.len() as i64 >= maxEngineNodes {
            self.captureErr = Some(errorf!("TALA {} sequence snapshot exceeds limit {}", self.scope, maxEngineNodes));
            return;
        }
        self.sequences.insert(sequence.clone());
        self.sequenceQueue.push(sequence.clone());
    }
    fn addTree(&mut self, tree: Option<&TreeRef>) {
        if !self.charge() {
            return;
        }
        let Some(tree) = tree else { return };
        if self.trees.contains(tree) {
            return;
        }
        if self.trees.len() as i64 >= maxEngineNodes {
            self.captureErr = Some(errorf!("TALA {} tree snapshot exceeds limit {}", self.scope, maxEngineNodes));
            return;
        }
        self.trees.insert(tree.clone());
        self.treeQueue.push(tree.clone());
    }
    fn addEdgeAbduction(&mut self, abduction: Option<&EdgeAbductionRef>) {
        if !self.charge() {
            return;
        }
        let Some(abduction) = abduction else { return };
        if self.edgeAbductions.contains(abduction) {
            return;
        }
        if self.edgeAbductions.len() as i64 >= maxEngineEdges {
            self.captureErr = Some(errorf!("TALA {} edge-abduction snapshot exceeds limit {}", self.scope, maxEngineEdges));
            return;
        }
        self.edgeAbductions.insert(abduction.clone());
        self.abductionQueue.push(abduction.clone());
    }
    fn addHerd(&mut self, herd: Option<&HerdAssignmentRef>) {
        if !self.charge() {
            return;
        }
        let Some(herd) = herd else { return };
        if self.herds.contains(herd) {
            return;
        }
        if self.herds.len() as i64 >= maxEngineNodes {
            self.captureErr = Some(errorf!("TALA {} herd snapshot exceeds limit {}", self.scope, maxEngineNodes));
            return;
        }
        self.herds.insert(herd.clone());
        self.herdQueue.push(herd.clone());
    }
    fn addHierarchy(&mut self, hierarchy: Option<&HierarchyRef>) {
        if !self.charge() {
            return;
        }
        let Some(hierarchy) = hierarchy else { return };
        if self.hierarchies.contains(hierarchy) {
            return;
        }
        if self.hierarchies.len() as i64 >= maxEngineNodes {
            self.captureErr = Some(errorf!("TALA {} hierarchy snapshot exceeds limit {}", self.scope, maxEngineNodes));
            return;
        }
        self.hierarchies.insert(hierarchy.clone());
        self.hierarchyQueue.push(hierarchy.clone());
    }
}

/// collectRuntimeObjectsContext follows every reference that can keep mutable
/// layout state alive outside Graph.Nodes.
///
/// NOTE: Go seeds the queues by iterating the graph's maps (random order); the
/// set of collected objects is order-independent, but `nodeOrder` (and the
/// guard step at which a limit trips) follow the Rust map order.
pub(crate) fn collectRuntimeObjectsContext(
    graph: &GraphRef,
    guard: &limits::WorkGuardRef,
    scope: &str,
) -> GoResult<runtimeObjectCollection> {
    let mut c = runtimeCollector {
        guard,
        scope,
        captureErr: None,
        nodes: Set::default(),
        edges: Set::default(),
        clusters: Set::default(),
        sequences: Set::default(),
        trees: Set::default(),
        edgeAbductions: Set::default(),
        herds: Set::default(),
        hierarchies: Set::default(),
        nodeQueue: Vec::new(),
        edgeQueue: Vec::new(),
        clusterQueue: Vec::new(),
        sequenceQueue: Vec::new(),
        treeQueue: Vec::new(),
        abductionQueue: Vec::new(),
        herdQueue: Vec::new(),
        hierarchyQueue: Vec::new(),
    };

    let (gnodes, gedges, containers, clusters, sequences, trees, nodeToTree, hubs, cus, directions) = {
        let g = graph.b();
        (
            g.Nodes.clone(),
            g.Edges.clone(),
            g.Containers.entries(),
            g.Clusters.entries(),
            g.Sequences.entries(),
            g.Trees.entries(),
            g.NodeToTree.entries(),
            g.Hubs.entries(),
            g.CommonUncleSiblings.entries(),
            g.Directions.keys(),
        )
    };
    for node in &gnodes {
        c.addNode(Some(node));
    }
    for edge in &gedges {
        c.addEdge(Some(edge));
    }
    for (container, children) in &containers {
        c.addNode(container.as_ref());
        for child in children {
            c.addNode(Some(child));
        }
    }
    for (vessel, cluster) in &clusters {
        c.addNode(Some(vessel));
        c.addCluster(Some(cluster));
    }
    for (vessel, sequence) in &sequences {
        c.addNode(Some(vessel));
        c.addSequence(Some(sequence));
    }
    for (node, nodeTrees) in &trees {
        c.addNode(node.as_ref());
        for tree in nodeTrees {
            c.addTree(Some(tree));
        }
    }
    for (node, tree) in &nodeToTree {
        c.addNode(Some(node));
        c.addTree(Some(tree));
    }
    for (hub, hubNodes) in &hubs {
        c.addNode(Some(hub));
        for node in hubNodes {
            c.addNode(Some(node));
        }
    }
    for (node, siblings) in &cus {
        c.addNode(Some(node));
        for sibling in siblings {
            c.addNode(Some(sibling));
        }
    }
    for node in &directions {
        c.addNode(node.as_ref());
    }
    if let Some(err) = c.captureErr.take() {
        return Err(err);
    }

    let (mut nodeIndex, mut edgeIndex, mut clusterIndex, mut sequenceIndex) = (0usize, 0usize, 0usize, 0usize);
    let (mut treeIndex, mut abductionIndex, mut herdIndex, mut hierarchyIndex) = (0usize, 0usize, 0usize, 0usize);
    while nodeIndex < c.nodeQueue.len()
        || edgeIndex < c.edgeQueue.len()
        || clusterIndex < c.clusterQueue.len()
        || sequenceIndex < c.sequenceQueue.len()
        || treeIndex < c.treeQueue.len()
        || abductionIndex < c.abductionQueue.len()
        || herdIndex < c.herdQueue.len()
        || hierarchyIndex < c.hierarchyQueue.len()
    {
        if let Some(err) = c.captureErr.take() {
            return Err(err);
        }
        if nodeIndex < c.nodeQueue.len() {
            let node = c.nodeQueue[nodeIndex].clone();
            nodeIndex += 1;
            let (container, nears, ldnr, edges, cluster, sequence, herd, hierarchy) = {
                let n = node.b();
                (
                    n.Container.clone(),
                    n.Nears.iter().cloned().collect::<Vec<_>>(),
                    n.LongDistanceNeighborRequirements.keys().cloned().collect::<Vec<_>>(),
                    n.Edges.clone(),
                    n.Cluster.clone(),
                    n.Sequence.clone(),
                    n.HerdAssignment.clone(),
                    n.Hierarchy.clone(),
                )
            };
            c.addNode(container.as_ref());
            for near in &nears {
                c.addNode(Some(near));
            }
            for neighbor in &ldnr {
                c.addNode(Some(neighbor));
            }
            for edge in &edges {
                c.addEdge(Some(edge));
            }
            c.addCluster(cluster.as_ref());
            c.addSequence(sequence.as_ref());
            c.addHerd(herd.as_ref());
            c.addHierarchy(hierarchy.as_ref());
        } else if edgeIndex < c.edgeQueue.len() {
            let edge = c.edgeQueue[edgeIndex].clone();
            edgeIndex += 1;
            let (from, to) = (edge.from(), edge.to());
            c.addNode(Some(&from));
            c.addNode(Some(&to));
        } else if clusterIndex < c.clusterQueue.len() {
            let cluster = c.clusterQueue[clusterIndex].clone();
            clusterIndex += 1;
            let cl = cluster.b().clone();
            c.addNode(Some(&cl.Vessel));
            c.addNode(cl.Container.as_ref());
            for node in &cl.Nodes {
                c.addNode(Some(node));
            }
            for abduction in &cl.EdgeAbductions {
                c.addEdgeAbduction(Some(abduction));
            }
        } else if sequenceIndex < c.sequenceQueue.len() {
            let sequence = c.sequenceQueue[sequenceIndex].clone();
            sequenceIndex += 1;
            let s = sequence.b().clone();
            c.addNode(Some(&s.Vessel));
            c.addNode(s.Container.as_ref());
            for node in &s.Nodes {
                c.addNode(Some(node));
            }
            for abduction in &s.EdgeAbductions {
                c.addEdgeAbduction(Some(abduction));
            }
        } else if treeIndex < c.treeQueue.len() {
            let tree = c.treeQueue[treeIndex].clone();
            treeIndex += 1;
            let t = tree.b().shallowCopy();
            c.addNode(Some(&t.Node));
            c.addEdge(t.SentinelEdge.as_ref());
            c.addTree(t.Parent.as_ref());
            for child in &t.Children {
                c.addTree(Some(child));
            }
        } else if abductionIndex < c.abductionQueue.len() {
            let abduction = c.abductionQueue[abductionIndex].clone();
            abductionIndex += 1;
            let a = abduction.b().clone();
            c.addEdge(Some(&a.Edge));
            c.addNode(a.OriginallyFrom.as_ref());
            c.addNode(a.OriginallyTo.as_ref());
            c.addNode(a.CurrentFrom.as_ref());
            c.addNode(a.CurrentTo.as_ref());
        } else if herdIndex < c.herdQueue.len() {
            let herd = c.herdQueue[herdIndex].clone();
            herdIndex += 1;
            let (opp, same) = {
                let h = herd.b();
                (h.oppositeSidePaired.iter().cloned().collect::<Vec<_>>(), h.sameSidePaired.iter().cloned().collect::<Vec<_>>())
            };
            for node in &opp {
                c.addNode(Some(node));
            }
            for node in &same {
                c.addNode(Some(node));
            }
        } else if hierarchyIndex < c.hierarchyQueue.len() {
            let hierarchy = c.hierarchyQueue[hierarchyIndex].clone();
            hierarchyIndex += 1;
            let levels = hierarchy.b().level.keys();
            for node in &levels {
                c.addNode(Some(node));
            }
        }
        if let Some(err) = c.captureErr.take() {
            return Err(err);
        }
    }

    Ok(runtimeObjectCollection {
        nodes: c.nodes,
        nodeOrder: c.nodeQueue,
        edges: c.edgeQueue,
        clusters: c.clusterQueue,
        sequences: c.sequenceQueue,
        trees: c.treeQueue,
        edgeAbductions: c.abductionQueue,
        herds: c.herdQueue,
        hierarchies: c.hierarchyQueue,
    })
}

impl GraphStateRef {
    pub(crate) fn captureRuntimeStateContext(&self, graph: &GraphRef, guard: &limits::WorkGuardRef) -> GoResult<()> {
        let objects = collectRuntimeObjectsContext(graph, guard, "transaction runtime")?;

        guard.Step()?;
        let graphSnap = captureGraph(graph);
        let mut nodes: Map<NodeRef, nodeSnapshot> = Map::default();
        for node in &objects.nodeOrder {
            guard.Step()?;
            nodes.insert(node.clone(), captureNode(node));
        }
        let mut edges: Map<EdgeRef, edgeSnapshot> = Map::default();
        for edge in &objects.edges {
            guard.Step()?;
            let npoints = edge.b().Points.len();
            for _ in 0..npoints {
                guard.Step()?;
            }
            edges.insert(edge.clone(), captureEdge(edge));
        }
        let mut clusters: Map<ClusterRef, clusterSnapshot> = Map::default();
        for cluster in &objects.clusters {
            guard.Step()?;
            clusters.insert(cluster.clone(), clusterSnapshot { value: cluster.b().clone() });
        }
        let mut sequences: Map<SequenceRef, sequenceSnapshot> = Map::default();
        for sequence in &objects.sequences {
            guard.Step()?;
            sequences.insert(sequence.clone(), sequenceSnapshot { value: sequence.b().clone() });
        }
        let mut trees: Map<TreeRef, treeSnapshot> = Map::default();
        for tree in &objects.trees {
            guard.Step()?;
            trees.insert(tree.clone(), treeSnapshot { value: tree.b().shallowCopy() });
        }
        let mut edgeAbductions: Map<EdgeAbductionRef, EdgeAbduction> = Map::default();
        for abduction in &objects.edgeAbductions {
            guard.Step()?;
            edgeAbductions.insert(abduction.clone(), abduction.b().clone());
        }
        let mut herds: Map<HerdAssignmentRef, herdSnapshot> = Map::default();
        for herd in &objects.herds {
            guard.Step()?;
            herds.insert(herd.clone(), herdSnapshot { value: herd.b().clone() });
        }
        let mut hierarchies: Map<HierarchyRef, hierarchySnapshot> = Map::default();
        for hierarchy in &objects.hierarchies {
            guard.Step()?;
            let value = hierarchy.b().clone();
            let level = value.level.to_map();
            hierarchies.insert(hierarchy.clone(), hierarchySnapshot { value, level });
        }
        {
            let mut gs = self.bm();
            gs.graph = Some(graphSnap);
            gs.nodes = nodes;
            gs.edges = edges;
            gs.clusters = clusters;
            gs.sequences = sequences;
            gs.trees = trees;
            gs.edgeAbductions = edgeAbductions;
            gs.herds = herds;
            gs.hierarchies = hierarchies;
        }
        guard.Finish()
    }
}
