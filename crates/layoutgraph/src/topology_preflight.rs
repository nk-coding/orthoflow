// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/layoutgraph/topology_preflight.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

pub(crate) const maxEngineTopologyReferences: i64 = 1_000_000;
pub(crate) const maxEngineRoutePoints: i64 = limits::MaxEngineRoutePoints;
pub(crate) const maxEngineTopologyDepth: i64 = limits::MaxEngineTreeDepth;
pub(crate) const maxEnginePreflightWork: i64 = maxEngineTopologyReferences * 8;
pub(crate) const maxPooledParentRelationNodes: i64 = 4096;

/// validateNodeParentRelation checks that a parent relation is acyclic and
/// within the depth limit. `nodes` is visited in the given order.
pub(crate) fn validateNodeParentRelation(
    guard: &dyn workStepper,
    nodes: &[NodeRef],
    relation: &str,
    parentOf: &dyn Fn(&NodeRef) -> Option<NodeRef>,
) -> GoResult<()> {
    let mut state: Map<NodeRef, u8> = Map::default();
    let mut depths: Map<NodeRef, i64> = Map::default();
    let mut path: Vec<NodeRef> = Vec::new();
    for node in nodes {
        if state.get(node).copied().unwrap_or(0) == 2 {
            continue;
        }
        path.clear();
        let mut current = Some(node.clone());
        while let Some(c) = current.clone() {
            if state.get(&c).copied().unwrap_or(0) != 0 {
                break;
            }
            guard.Step()?;
            state.insert(c.clone(), 1);
            path.push(c.clone());
            current = parentOf(&c);
        }
        if let Some(c) = &current {
            if state.get(c).copied().unwrap_or(0) == 1 {
                return Err(errorf!("TALA engine {} cycle detected at node {}", relation, c.id()));
            }
        }
        let mut depth: i64 = 0;
        if let Some(c) = &current {
            depth = depths.get(c).copied().unwrap_or(0);
        }
        for p in path.iter().rev() {
            depth += 1;
            if depth > maxEngineTopologyDepth {
                return Err(errorf!("TALA engine {} depth exceeds limit {}", relation, maxEngineTopologyDepth));
            }
            depths.insert(p.clone(), depth);
            state.insert(p.clone(), 2);
        }
    }
    Ok(())
}

/// ancestryParent matches the precedence used by isDescendantOf. Keep this
/// separate from container: active cluster and sequence ancestry follows the
/// vessel itself, while container follows the vessel's direct container.
pub(crate) fn ancestryParent(node: Option<&NodeRef>) -> Option<NodeRef> {
    let node = node?;
    let (container, cluster, sequence) = {
        let n = node.b();
        (n.Container.clone(), n.Cluster.clone(), n.Sequence.clone())
    };
    if container.is_some() {
        return container;
    }
    if let Some(c) = cluster {
        let v = c.b().Vessel.clone();
        return Some(v);
    }
    if let Some(s) = sequence {
        let v = s.b().Vessel.clone();
        return Some(v);
    }
    None
}

/// Shared bounded inventory of runtime topology records used by both
/// preflight walks.
struct topologyInventory<'a> {
    guard: &'a limits::WorkGuardRef,
    prefix: &'static str,
    referenceCount: i64,
    routePointCount: i64,

    nodes: Set<NodeRef>,
    edges: Set<EdgeRef>,
    clusters: Set<ClusterRef>,
    sequences: Set<SequenceRef>,
    trees: Set<TreeRef>,
    abductions: Set<EdgeAbductionRef>,
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

impl<'a> topologyInventory<'a> {
    fn new(guard: &'a limits::WorkGuardRef, prefix: &'static str) -> Self {
        topologyInventory {
            guard,
            prefix,
            referenceCount: 0,
            routePointCount: 0,
            nodes: Set::default(),
            edges: Set::default(),
            clusters: Set::default(),
            sequences: Set::default(),
            trees: Set::default(),
            abductions: Set::default(),
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
        }
    }

    fn isCombine(&self) -> bool {
        self.prefix == "CombineSubgraphs"
    }

    fn addReference(&mut self, kind: &str) -> GoResult<()> {
        if self.referenceCount >= maxEngineTopologyReferences {
            if self.isCombine() {
                return Err(errorf!(
                    "TALA CombineSubgraphs topology references exceed limit {} while visiting {}",
                    maxEngineTopologyReferences,
                    kind
                ));
            }
            return Err(errorf!("TALA engine topology references exceed limit {} while visiting {}", maxEngineTopologyReferences, kind));
        }
        self.referenceCount += 1;
        self.guard.Step()
    }

    /// Go accounts `cap(s) - len(s)` spare slice capacity; Rust vectors have no
    /// observable Go capacity, so the spare count is zero (see DEVIATIONS.md).
    fn accountSpareCapacity(&mut self, _kind: &str, _length: usize) -> GoResult<()> {
        self.guard.Check()
    }

    fn structuralNil(&self, kind: &str) -> GoError {
        if self.isCombine() {
            return errorf!("TALA CombineSubgraphs node-owned topology contains nil {}", kind);
        }
        errorf!("TALA engine topology contains nil {}", kind)
    }

    fn queueNode(&mut self, node: Option<&NodeRef>, required: bool, kind: &str) -> GoResult<()> {
        let Some(node) = node else {
            if required {
                return Err(self.structuralNil(kind));
            }
            return Ok(());
        };
        self.addReference(kind)?;
        if self.nodes.contains(node) {
            return Ok(());
        }
        if self.nodes.len() as i64 >= maxEngineNodes {
            if self.isCombine() {
                return Err(errorf!("TALA CombineSubgraphs unique node count exceeds limit {}", maxEngineNodes));
            }
            return Err(errorf!("TALA engine unique node count exceeds limit {}", maxEngineNodes));
        }
        self.nodes.insert(node.clone());
        self.nodeQueue.push(node.clone());
        Ok(())
    }

    fn queueEdge(&mut self, edge: Option<&EdgeRef>, required: bool, kind: &str) -> GoResult<()> {
        let Some(edge) = edge else {
            if required {
                return Err(self.structuralNil(kind));
            }
            return Ok(());
        };
        self.addReference(kind)?;
        if self.edges.contains(edge) {
            return Ok(());
        }
        if self.edges.len() as i64 >= maxEngineEdges {
            if self.isCombine() {
                return Err(errorf!("TALA CombineSubgraphs unique edge count exceeds limit {}", maxEngineEdges));
            }
            return Err(errorf!("TALA engine unique edge count exceeds limit {}", maxEngineEdges));
        }
        self.edges.insert(edge.clone());
        self.edgeQueue.push(edge.clone());
        Ok(())
    }

    fn queueCluster(&mut self, cluster: Option<&ClusterRef>, required: bool, kind: &str) -> GoResult<()> {
        let Some(cluster) = cluster else {
            if required {
                return Err(self.structuralNil(kind));
            }
            return Ok(());
        };
        self.addReference(kind)?;
        if self.clusters.insert(cluster.clone()) {
            self.clusterQueue.push(cluster.clone());
        }
        Ok(())
    }

    fn queueSequence(&mut self, sequence: Option<&SequenceRef>, required: bool, kind: &str) -> GoResult<()> {
        let Some(sequence) = sequence else {
            if required {
                return Err(self.structuralNil(kind));
            }
            return Ok(());
        };
        self.addReference(kind)?;
        if self.sequences.insert(sequence.clone()) {
            self.sequenceQueue.push(sequence.clone());
        }
        Ok(())
    }

    fn queueTree(&mut self, tree: Option<&TreeRef>, required: bool, kind: &str) -> GoResult<()> {
        let Some(tree) = tree else {
            if required {
                return Err(self.structuralNil(kind));
            }
            return Ok(());
        };
        self.addReference(kind)?;
        if self.trees.insert(tree.clone()) {
            self.treeQueue.push(tree.clone());
        }
        Ok(())
    }

    fn queueAbduction(&mut self, abduction: Option<&EdgeAbductionRef>, required: bool, kind: &str) -> GoResult<()> {
        let Some(abduction) = abduction else {
            if required {
                return Err(self.structuralNil(kind));
            }
            return Ok(());
        };
        self.addReference(kind)?;
        if self.abductions.insert(abduction.clone()) {
            self.abductionQueue.push(abduction.clone());
        }
        Ok(())
    }

    fn queueHerd(&mut self, herd: Option<&HerdAssignmentRef>, required: bool, kind: &str) -> GoResult<()> {
        let Some(herd) = herd else {
            if required {
                return Err(self.structuralNil(kind));
            }
            return Ok(());
        };
        self.addReference(kind)?;
        if self.herds.insert(herd.clone()) {
            self.herdQueue.push(herd.clone());
        }
        Ok(())
    }

    fn queueHierarchy(&mut self, hierarchy: Option<&HierarchyRef>, required: bool, kind: &str) -> GoResult<()> {
        let Some(hierarchy) = hierarchy else {
            if required {
                return Err(self.structuralNil(kind));
            }
            return Ok(());
        };
        self.addReference(kind)?;
        if self.hierarchies.insert(hierarchy.clone()) {
            self.hierarchyQueue.push(hierarchy.clone());
        }
        Ok(())
    }

    /// Drain the queues, following every runtime reference. `withTrees`
    /// enables the tree queue (the CombineSubgraphs walk has none).
    fn drain(&mut self, withTrees: bool) -> GoResult<()> {
        let (mut nodeIndex, mut edgeIndex, mut clusterIndex, mut sequenceIndex) = (0usize, 0usize, 0usize, 0usize);
        let (mut treeIndex, mut abductionIndex, mut herdIndex, mut hierarchyIndex) = (0usize, 0usize, 0usize, 0usize);
        while nodeIndex < self.nodeQueue.len()
            || edgeIndex < self.edgeQueue.len()
            || clusterIndex < self.clusterQueue.len()
            || sequenceIndex < self.sequenceQueue.len()
            || (withTrees && treeIndex < self.treeQueue.len())
            || abductionIndex < self.abductionQueue.len()
            || herdIndex < self.herdQueue.len()
            || hierarchyIndex < self.hierarchyQueue.len()
        {
            if nodeIndex < self.nodeQueue.len() {
                let node = self.nodeQueue[nodeIndex].clone();
                nodeIndex += 1;
                let (container, edges, nears, ldnr, cluster, sequence, herd, hierarchy) = {
                    let n = node.b();
                    (
                        n.Container.clone(),
                        n.Edges.clone(),
                        n.Nears.iter().cloned().collect::<Vec<_>>(),
                        n.LongDistanceNeighborRequirements.keys().cloned().collect::<Vec<_>>(),
                        n.Cluster.clone(),
                        n.Sequence.clone(),
                        n.HerdAssignment.clone(),
                        n.Hierarchy.clone(),
                    )
                };
                self.queueNode(container.as_ref(), false, "node container")?;
                self.accountSpareCapacity("node edges", edges.len())?;
                for edge in &edges {
                    self.queueEdge(Some(edge), true, "node edge")?;
                }
                for near in &nears {
                    self.queueNode(Some(near), true, "near node")?;
                }
                for neighbor in &ldnr {
                    self.queueNode(Some(neighbor), true, "long-distance neighbor requirement key")?;
                }
                self.queueCluster(cluster.as_ref(), false, "node cluster")?;
                self.queueSequence(sequence.as_ref(), false, "node sequence")?;
                self.queueHerd(herd.as_ref(), false, "node herd assignment")?;
                self.queueHierarchy(hierarchy.as_ref(), false, "node hierarchy")?;
            } else if edgeIndex < self.edgeQueue.len() {
                let edge = self.edgeQueue[edgeIndex].clone();
                edgeIndex += 1;
                let (from, to, npoints) = {
                    let e = edge.b();
                    (e.From.clone(), e.To.clone(), e.Points.len())
                };
                self.queueNode(Some(&from), true, "edge source")?;
                self.queueNode(Some(&to), true, "edge target")?;
                let pointCapacity = npoints as i64;
                if pointCapacity > maxEngineRoutePoints - self.routePointCount {
                    if self.isCombine() {
                        return Err(errorf!("TALA CombineSubgraphs route point count exceeds limit {}", maxEngineRoutePoints));
                    }
                    return Err(errorf!("TALA engine route point count exceeds limit {}", maxEngineRoutePoints));
                }
                self.routePointCount += pointCapacity;
                for _ in 0..npoints {
                    self.guard.Step()?;
                }
            } else if clusterIndex < self.clusterQueue.len() {
                let cluster = self.clusterQueue[clusterIndex].clone();
                clusterIndex += 1;
                let c = cluster.b().clone();
                self.queueNode(Some(&c.Vessel), true, "cluster vessel")?;
                self.queueNode(c.Container.as_ref(), false, "cluster container")?;
                self.accountSpareCapacity("cluster nodes", c.Nodes.len())?;
                for node in &c.Nodes {
                    self.queueNode(Some(node), true, "cluster node")?;
                }
                self.accountSpareCapacity("cluster edge abductions", c.EdgeAbductions.len())?;
                for abduction in &c.EdgeAbductions {
                    self.queueAbduction(Some(abduction), true, "cluster edge abduction")?;
                }
            } else if sequenceIndex < self.sequenceQueue.len() {
                let sequence = self.sequenceQueue[sequenceIndex].clone();
                sequenceIndex += 1;
                let s = sequence.b().clone();
                self.queueNode(Some(&s.Vessel), true, "sequence vessel")?;
                self.queueNode(s.Container.as_ref(), false, "sequence container")?;
                self.accountSpareCapacity("sequence nodes", s.Nodes.len())?;
                for node in &s.Nodes {
                    self.queueNode(Some(node), true, "sequence node")?;
                }
                self.accountSpareCapacity("sequence edge abductions", s.EdgeAbductions.len())?;
                for abduction in &s.EdgeAbductions {
                    self.queueAbduction(Some(abduction), true, "sequence edge abduction")?;
                }
            } else if withTrees && treeIndex < self.treeQueue.len() {
                let tree = self.treeQueue[treeIndex].clone();
                treeIndex += 1;
                let t = tree.b().shallowCopy();
                self.queueNode(Some(&t.Node), true, "tree node")?;
                self.queueTree(t.Parent.as_ref(), false, "tree parent")?;
                self.queueEdge(t.SentinelEdge.as_ref(), false, "tree sentinel edge")?;
                self.accountSpareCapacity("tree children", t.Children.len())?;
                for child in &t.Children {
                    self.queueTree(Some(child), true, "tree child")?;
                }
            } else if abductionIndex < self.abductionQueue.len() {
                let abduction = self.abductionQueue[abductionIndex].clone();
                abductionIndex += 1;
                let a = abduction.b().clone();
                self.queueEdge(Some(&a.Edge), true, "abducted edge")?;
                self.queueNode(a.OriginallyFrom.as_ref(), false, "abduction original source")?;
                self.queueNode(a.OriginallyTo.as_ref(), false, "abduction original target")?;
                self.queueNode(a.CurrentFrom.as_ref(), false, "abduction current source")?;
                self.queueNode(a.CurrentTo.as_ref(), false, "abduction current target")?;
            } else if herdIndex < self.herdQueue.len() {
                let herd = self.herdQueue[herdIndex].clone();
                herdIndex += 1;
                let (opp, same) = {
                    let h = herd.b();
                    (h.oppositeSidePaired.iter().cloned().collect::<Vec<_>>(), h.sameSidePaired.iter().cloned().collect::<Vec<_>>())
                };
                for node in &opp {
                    self.queueNode(Some(node), true, "opposite-side herd node")?;
                }
                for node in &same {
                    self.queueNode(Some(node), true, "same-side herd node")?;
                }
            } else if hierarchyIndex < self.hierarchyQueue.len() {
                let hierarchy = self.hierarchyQueue[hierarchyIndex].clone();
                hierarchyIndex += 1;
                let levels = hierarchy.b().level.keys();
                for node in &levels {
                    self.queueNode(Some(node), true, "hierarchy level node")?;
                }
            }
        }
        Ok(())
    }
}

/// validateCombineNodeTopology validates node-owned records across all graphs as
/// one bounded inventory.
pub(crate) fn validateCombineNodeTopology(ctx: &Ctx, graphs: &[Option<GraphRef>]) -> GoResult<()> {
    let guard = limits::NewWorkGuard(ctx, "CombineSubgraphs", maxEngineWorkUnits)?;
    guard.SetLimit(maxEnginePreflightWork);

    let mut inv = topologyInventory::new(&guard, "CombineSubgraphs");

    for graph in graphs {
        let Some(graph) = graph else {
            return Err(errorf!("TALA CombineSubgraphs received a nil graph"));
        };
        for node in graph.nodes() {
            inv.queueNode(Some(&node), true, "graph node")?;
        }
        for edge in graph.edges() {
            inv.queueEdge(Some(&edge), true, "graph edge")?;
        }
    }

    inv.drain(false)?;

    let nodes = inv.nodeQueue.clone();
    validateNodeParentRelation(&guard, &nodes, "container parent", &|node| node.containerField())?;
    validateNodeParentRelation(&guard, &nodes, "effective container parent", &|node| node.container())?;
    validateNodeParentRelation(&guard, &nodes, "ancestry parent", &|node| ancestryParent(Some(node)))?;
    guard.Finish()
}

/// validateEngineGraph performs a bounded, iterative walk of every runtime
/// topology reference before layout code allocates snapshots or starts work.
pub(crate) fn validateEngineGraph(ctx: &Ctx, location: &str, graph: Option<&GraphRef>) -> GoResult<()> {
    let Some(graph) = graph else {
        return Err(errorf!("TALA engine requires a graph"));
    };
    let guard = limits::NewWorkGuard(ctx, location, maxEngineWorkUnits)?;
    guard.SetLimit(maxEnginePreflightWork);

    let mut inv = topologyInventory::new(&guard, "engine");

    let (gnodes, gedges, containers, clusters, sequences, trees, nodeToTree, hubs, directions, cus) = {
        let g = graph.b();
        (
            g.Nodes.clone(),
            g.Edges.clone(),
            g.Containers.clone(),
            g.Clusters.clone(),
            g.Sequences.clone(),
            g.Trees.clone(),
            g.NodeToTree.clone(),
            g.Hubs.clone(),
            g.Directions.clone(),
            g.CommonUncleSiblings.clone(),
        )
    };

    inv.accountSpareCapacity("graph nodes", gnodes.len())?;
    for node in &gnodes {
        inv.queueNode(Some(node), true, "graph node")?;
    }
    inv.accountSpareCapacity("graph edges", gedges.len())?;
    for edge in &gedges {
        inv.queueEdge(Some(edge), true, "graph edge")?;
    }

    for (container, children) in containers.entries() {
        inv.queueNode(container.as_ref(), false, "container key")?;
        inv.accountSpareCapacity("container children", children.len())?;
        for child in &children {
            inv.queueNode(Some(child), true, "container child")?;
        }
    }
    for (vessel, cluster) in clusters.entries() {
        inv.queueNode(Some(&vessel), true, "cluster vessel key")?;
        inv.queueCluster(Some(&cluster), true, "cluster record")?;
    }
    for (vessel, sequence) in sequences.entries() {
        inv.queueNode(Some(&vessel), true, "sequence vessel key")?;
        inv.queueSequence(Some(&sequence), true, "sequence record")?;
    }
    for (root, roots) in trees.entries() {
        // Root-sentinel trees are intentionally keyed by nil.
        inv.queueNode(root.as_ref(), false, "tree root key")?;
        inv.accountSpareCapacity("tree roots", roots.len())?;
        for tree in &roots {
            inv.queueTree(Some(tree), true, "tree root")?;
        }
    }
    for (node, tree) in nodeToTree.entries() {
        inv.queueNode(Some(&node), true, "node-to-tree key")?;
        inv.queueTree(Some(&tree), true, "node-to-tree value")?;
    }
    for (hub, spokes) in hubs.entries() {
        inv.queueNode(Some(&hub), true, "hub key")?;
        inv.accountSpareCapacity("hub spokes", spokes.len())?;
        for spoke in &spokes {
            inv.queueNode(Some(spoke), true, "hub spoke")?;
        }
    }
    for node in directions.keys() {
        inv.queueNode(node.as_ref(), false, "direction key")?;
    }
    for (node, siblings) in cus.entries() {
        inv.queueNode(Some(&node), true, "common-sibling key")?;
        inv.accountSpareCapacity("common siblings", siblings.len())?;
        for sibling in &siblings {
            inv.queueNode(Some(sibling), true, "common sibling")?;
        }
    }

    inv.drain(true)?;

    let nodesInOrder = inv.nodeQueue.clone();
    let treesInOrder = inv.treeQueue.clone();
    drop(inv);

    // Validate parent chains independently of the declared container map so a
    // stale or hidden parent pointer cannot introduce recursion or excessive
    // depth into later snapshot/layout walks.
    validateNodeParentRelation(&guard, &nodesInOrder, "container parent", &|node| node.containerField())?;

    // Active clusters and sequences override Node.Container in container.
    validateNodeParentRelation(&guard, &nodesInOrder, "effective container parent", &|node| node.container())?;
    validateNodeParentRelation(&guard, &nodesInOrder, "ancestry parent", &|node| ancestryParent(Some(node)))?;

    // Every descendant walk in the engine follows the union of direct
    // containers, active cluster membership, and active sequence membership.
    // Validate that combined ownership graph, not just Containers.
    let descendantChildren = |node: &Option<NodeRef>| -> GoResult<Vec<NodeRef>> {
        let mut children: Vec<NodeRef> = Vec::new();
        let isRootOrContainer = match node {
            None => true,
            Some(n) => n.b().isContainer,
        };
        if isRootOrContainer {
            children.extend(containers.at(node));
        }
        if let Some(n) = node {
            if n.b().isClusterVessel {
                let Some(cluster) = clusters.get(n) else {
                    return Err(errorf!("TALA engine cluster vessel {} has no cluster record", n.id()));
                };
                children.extend(cluster.b().Nodes.clone());
            }
            if let Some(sequence) = sequences.get(n) {
                children.extend(sequence.b().Nodes.clone());
            }
        }
        Ok(children)
    };
    struct descendantFrame {
        node: Option<NodeRef>,
        children: Vec<NodeRef>,
        index: usize,
    }
    let mut descendantColor: Map<Option<NodeRef>, u8> = Map::default();
    let mut descendantHeight: Map<Option<NodeRef>, i64> = Map::default();
    let mut descendantStarts: Vec<Option<NodeRef>> = Vec::with_capacity(nodesInOrder.len() + 1);
    if containers.has(&None) {
        descendantStarts.push(None);
    }
    for node in &nodesInOrder {
        descendantStarts.push(Some(node.clone()));
    }
    for start in &descendantStarts {
        if descendantColor.get(start).copied().unwrap_or(0) != 0 {
            continue;
        }
        let children = descendantChildren(start)?;
        descendantColor.insert(start.clone(), 1);
        let mut stack: Vec<descendantFrame> = vec![descendantFrame { node: start.clone(), children, index: 0 }];
        while !stack.is_empty() {
            let top = stack.len() - 1;
            if stack[top].index >= stack[top].children.len() {
                let frame = &stack[top];
                let ownHeight: i64 = if frame.node.is_none() { 0 } else { 1 };
                let mut height = ownHeight;
                for child in &frame.children {
                    let childHeight = ownHeight + descendantHeight.get(&Some(child.clone())).copied().unwrap_or(0);
                    if childHeight > height {
                        height = childHeight;
                    }
                }
                if height > maxEngineTopologyDepth {
                    return Err(errorf!("TALA engine descendant depth exceeds limit {}", maxEngineTopologyDepth));
                }
                descendantHeight.insert(frame.node.clone(), height);
                descendantColor.insert(frame.node.clone(), 2);
                stack.pop();
                continue;
            }
            let child = stack[top].children[stack[top].index].clone();
            stack[top].index += 1;
            guard.Step()?;
            let key = Some(child.clone());
            match descendantColor.get(&key).copied().unwrap_or(0) {
                1 => {
                    return Err(errorf!("TALA engine descendant cycle detected at node {}", child.id()));
                }
                0 => {
                    let children = descendantChildren(&key)?;
                    descendantColor.insert(key.clone(), 1);
                    stack.push(descendantFrame { node: key, children, index: 0 });
                }
                _ => {}
            }
        }
    }

    // Tree child links and parent links are both traversed because either side
    // can be stale independently.
    struct treeFrame {
        tree: TreeRef,
        children: Vec<TreeRef>,
        index: usize,
    }
    let mut treeColor: Map<TreeRef, u8> = Map::default();
    let mut treeHeight: Map<TreeRef, i64> = Map::default();
    for start in &treesInOrder {
        if treeColor.get(start).copied().unwrap_or(0) != 0 {
            continue;
        }
        treeColor.insert(start.clone(), 1);
        let children = start.b().Children.clone();
        let mut stack: Vec<treeFrame> = vec![treeFrame { tree: start.clone(), children, index: 0 }];
        while !stack.is_empty() {
            let top = stack.len() - 1;
            if stack[top].index >= stack[top].children.len() {
                let frame = &stack[top];
                let mut height: i64 = 1;
                for child in &frame.children {
                    let childHeight = treeHeight.get(child).copied().unwrap_or(0) + 1;
                    if childHeight > height {
                        height = childHeight;
                    }
                }
                if height > maxEngineTopologyDepth {
                    return Err(errorf!("TALA engine tree depth exceeds limit {}", maxEngineTopologyDepth));
                }
                treeHeight.insert(frame.tree.clone(), height);
                treeColor.insert(frame.tree.clone(), 2);
                stack.pop();
                continue;
            }
            let child = stack[top].children[stack[top].index].clone();
            stack[top].index += 1;
            guard.Step()?;
            match treeColor.get(&child).copied().unwrap_or(0) {
                1 => return Err(errorf!("TALA engine tree child cycle detected")),
                0 => {
                    treeColor.insert(child.clone(), 1);
                    let children = child.b().Children.clone();
                    stack.push(treeFrame { tree: child, children, index: 0 });
                }
                _ => {}
            }
        }
    }
    let mut parentState: Map<TreeRef, u8> = Map::default();
    let mut parentDepth: Map<TreeRef, i64> = Map::default();
    for start in &treesInOrder {
        if parentState.get(start).copied().unwrap_or(0) == 2 {
            continue;
        }
        let mut path: Vec<TreeRef> = Vec::new();
        let mut current = Some(start.clone());
        while let Some(c) = current.clone() {
            if parentState.get(&c).copied().unwrap_or(0) != 0 {
                break;
            }
            guard.Step()?;
            parentState.insert(c.clone(), 1);
            path.push(c.clone());
            current = c.b().Parent.clone();
        }
        if let Some(c) = &current {
            if parentState.get(c).copied().unwrap_or(0) == 1 {
                return Err(errorf!("TALA engine tree parent cycle detected"));
            }
        }
        let mut depth: i64 = 0;
        if let Some(c) = &current {
            depth = parentDepth.get(c).copied().unwrap_or(0);
        }
        for p in path.iter().rev() {
            depth += 1;
            if depth > maxEngineTopologyDepth {
                return Err(errorf!("TALA engine tree depth exceeds limit {}", maxEngineTopologyDepth));
            }
            parentDepth.insert(p.clone(), depth);
            parentState.insert(p.clone(), 2);
        }
    }

    // A serialized tree is a value tree, not a graph: every runtime Tree must
    // therefore have exactly one owning root/parent path.
    let mut ownedTrees: Map<TreeRef, Option<TreeRef>> = Map::default();
    let mut treeByNode: Map<NodeRef, TreeRef> = Map::default();
    let treeEntries = trees.entries();
    for (_, roots) in &treeEntries {
        for root in roots {
            let mut stack: Vec<(TreeRef, Option<TreeRef>)> = vec![(root.clone(), None)];
            while let Some((tree, parent)) = stack.pop() {
                guard.Step()?;
                if let Some(owner) = ownedTrees.get(&tree) {
                    if parent.is_none() {
                        return Err(errorf!("TALA engine tree is listed as more than one root"));
                    }
                    if *owner == parent {
                        return Err(errorf!("TALA engine tree is repeated under one parent"));
                    }
                    return Err(errorf!("TALA engine tree is shared by multiple parents"));
                }
                ownedTrees.insert(tree.clone(), parent.clone());
                let tnode = tree.b().Node.clone();
                if let Some(existing) = treeByNode.get(&tnode) {
                    if *existing != tree {
                        return Err(errorf!("TALA engine node {} is owned by multiple trees", tnode.id()));
                    }
                }
                treeByNode.insert(tnode, tree.clone());

                let children = tree.b().Children.clone();
                for child in children.iter().rev() {
                    if child.b().Parent.as_ref() != Some(&tree) {
                        return Err(errorf!("TALA engine tree child has an inconsistent parent"));
                    }
                    stack.push((child.clone(), Some(tree.clone())));
                }
            }
        }
    }
    for (_, roots) in &treeEntries {
        for root in roots {
            let Some(parent) = root.b().Parent.clone() else {
                continue;
            };
            // Placement-tree construction temporarily attaches an installed root beneath a
            // placement-only wrapper.
            if ownedTrees.contains_key(&parent) {
                return Err(errorf!("TALA engine tree root also has an installed parent"));
            }
            let mut occurrences = 0;
            let pchildren = parent.b().Children.clone();
            for child in &pchildren {
                guard.Step()?;
                if child == root {
                    occurrences += 1;
                }
            }
            if occurrences != 1 {
                return Err(errorf!("TALA engine tree root has an inconsistent placement parent"));
            }
        }
    }

    // NodeToTree is derived state and may legitimately be absent on a freshly
    // constructed graph. When present alongside an installed forest, however,
    // it must be the exact inverse of Tree.Node.
    let nttEntries = nodeToTree.entries();
    for (node, tree) in &nttEntries {
        if tree.b().Node != *node {
            return Err(errorf!("TALA engine node-to-tree alias does not match the tree node"));
        }
        if !ownedTrees.is_empty() && !ownedTrees.contains_key(tree) {
            return Err(errorf!("TALA engine node-to-tree alias references a tree outside the installed forest"));
        }
    }
    if !nttEntries.is_empty() && !ownedTrees.is_empty() {
        if nttEntries.len() != treeByNode.len() {
            return Err(errorf!("TALA engine node-to-tree aliases do not cover the installed forest"));
        }
        for (node, tree) in &treeByNode {
            if nodeToTree.get(node).as_ref() != Some(tree) {
                return Err(errorf!("TALA engine node-to-tree aliases do not match the installed forest"));
            }
        }
    }

    guard.Finish()
}
