// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/grouping/clusters.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;
use layoutgraph::{ClusterArrangement, Column, Row};
use nodeshape::D2Shape;

/// AssignArrangement selects the cluster axis from node dimensions. Randomness
/// is consumed only for square clusters, preserving the engine's seed contract.
pub fn AssignArrangement(cluster: &ClusterRef, isConnectedToSequence: bool, random: &RandRef) -> ClusterArrangement {
    let (averageWidth, averageHeight) = averageClusterDimensions(cluster);
    if isConnectedToSequence {
        return Row;
    }
    if averageWidth > averageHeight {
        return Column;
    }
    if averageWidth < averageHeight {
        return Row;
    }
    if random.bm().Float64() > 0.5 {
        return Column;
    }
    Row
}

pub(crate) fn averageClusterDimensions(cluster: &ClusterRef) -> (f64, f64) {
    let nodes = cluster.b().Nodes.clone();
    let (mut width, mut height) = (0.0f64, 0.0f64);
    for node in &nodes {
        width += node.w();
        height += node.h();
    }
    let count = nodes.len() as f64;
    ((width / count).round(), (height / count).round())
}

/// PaddingBetween calculates the spacing between adjacent cluster members.
pub fn PaddingBetween(cluster: &ClusterRef, considerPositions: bool) -> f64 {
    let (averageWidth, averageHeight) = averageClusterDimensions(cluster);
    let (nodes, arrangement) = {
        let c = cluster.b();
        (c.Nodes.clone(), c.Arrangement.clone())
    };
    let mut hasIcon = false;
    let (mut maxLabelWidth, mut maxLabelHeight) = (0.0f64, 0.0f64);
    for node in &nodes {
        hasIcon = hasIcon || node.b().Icon.is_some();
        if let Some(l) = node.label() {
            let l = l.b();
            maxLabelWidth = math::Max(maxLabelWidth, l.Width);
            maxLabelHeight = math::Max(maxLabelHeight, l.Height);
        }
    }
    let nodeGap = layoutgraph::NodeGap as f64;
    let padding2 = (2 * label::PADDING) as f64;
    let mut given;
    if arrangement == Row {
        given = math::Max(nodeGap, averageWidth.ceil() * 0.1);
        if hasIcon {
            given = math::Max(given, padding2 + maxLabelWidth);
        }
    } else {
        given = math::Max(nodeGap, averageHeight.ceil() * 0.1);
        if hasIcon {
            given = math::Max(given, padding2 + maxLabelHeight);
        }
    }
    if considerPositions {
        let mut total = 0.0f64;
        let mut index = 0usize;
        while index + 1 < nodes.len() {
            total += nodes[index].DistanceTo(&nodes[index + 1], true);
            index += 1;
        }
        let positionedPadding = total / (nodes.len() as f64 - 1.0);
        if positionedPadding > 0.0 {
            return math::Min(positionedPadding, given).round();
        }
    }
    given.round()
}

/// CreateVessel builds the temporary placement node representing a cluster.
pub fn CreateVessel(cluster: &ClusterRef, vesselID: EntityID) -> NodeRef {
    let (mut minimumX, mut minimumY) = (f64::INFINITY, f64::INFINITY);
    let nodes = cluster.b().Nodes.clone();
    for node in &nodes {
        if let Some(tl) = node.tl() {
            let t = *tl.b();
            minimumX = math::Min(minimumX, t.X);
            minimumY = math::Min(minimumY, t.Y);
        }
    }
    let vessel = layoutgraph::NewNode(vesselID, 0.0, 0.0);
    vessel.SetClusterVessel(true);
    cluster.Resize(&vessel);
    let arrangement = cluster.b().Arrangement.clone();
    if arrangement == Row && !minimumX.is_infinite() && !minimumY.is_infinite() {
        let mut ns = cluster.b().Nodes.clone();
        gocompat::sort::SortSlice(&mut ns, |a: &NodeRef, b: &NodeRef| a.x() < b.x());
        cluster.bm().Nodes = ns;
        vessel.set_tl(Some(geo::NewPoint(minimumX, minimumY)));
    }
    if arrangement == Column && !minimumX.is_infinite() && !minimumY.is_infinite() {
        let mut ns = cluster.b().Nodes.clone();
        gocompat::sort::SortSlice(&mut ns, |a: &NodeRef, b: &NodeRef| a.y() < b.y());
        cluster.bm().Nodes = ns;
        vessel.set_tl(Some(geo::NewPoint(minimumX, minimumY)));
    }
    vessel
}

/// AddCluster installs a discovered cluster and its temporary vessel into the
/// mutable layout graph.
pub fn AddCluster(graph: &GraphRef, cluster: &ClusterRef) {
    let (container, vessel, nodes) = {
        let c = cluster.b();
        (c.Container.clone(), c.Vessel.clone(), c.Nodes.clone())
    };
    graph.AddNewNodeToContainer(container.as_ref(), &vessel);
    for node in &nodes {
        node.bm().Cluster = Some(cluster.clone());
    }
    let mut updated: Vec<NodeRef> = Vec::new();
    for child in graph.containers().at(&container) {
        if child.b().Cluster.as_ref() != Some(cluster) {
            updated.push(child);
        }
    }
    graph.containers().set(container.clone(), updated);
    for node in &nodes {
        graph.RemoveNode(node);
        node.bm().Container = None;
    }
    graph.clusters().set(vessel, cluster.clone());
}

pub(crate) fn abductClusterEdges(cluster: &ClusterRef, edges: &[EdgeRef], guard: &WorkGuardRef) -> GoResult<()> {
    let vessel = cluster.b().Vessel.clone();
    let mut abductions: Vec<EdgeAbductionRef> = Vec::new();
    for edge in edges {
        guard.Step()?;
        let from = edge.from();
        if from.b().Cluster.as_ref() == Some(cluster) {
            let n = from.b().Edges.len() + vessel.b().Edges.len();
            for _ in 0..n {
                guard.Step()?;
            }
            abductions.push(EdgeAbductionRef::new(EdgeAbduction {
                Edge: edge.clone(),
                OriginallyFrom: Some(from.clone()),
                OriginallyTo: None,
                CurrentFrom: Some(vessel.clone()),
                CurrentTo: Some(edge.to()),
            }));
            edge.Reconnect(&vessel, false);
        }
        let to = edge.to();
        if to.b().Cluster.as_ref() == Some(cluster) {
            let n = to.b().Edges.len() + vessel.b().Edges.len();
            for _ in 0..n {
                guard.Step()?;
            }
            abductions.push(EdgeAbductionRef::new(EdgeAbduction {
                Edge: edge.clone(),
                OriginallyFrom: None,
                OriginallyTo: Some(to.clone()),
                CurrentFrom: Some(edge.from()),
                CurrentTo: Some(vessel.clone()),
            }));
            edge.Reconnect(&vessel, true);
        }
    }
    cluster.bm().EdgeAbductions = abductions;
    guard.Finish()
}

struct addClustersState<'a> {
    graph: &'a GraphRef,
    guard: &'a WorkGuardRef,
    random: &'a RandRef,
    index: clusterDiscoveryIndex,
    reservedIDs: Set<EntityID>,
}

impl<'a> addClustersState<'a> {
    fn nextVesselID(&self, mut candidate: EntityID) -> GoResult<EntityID> {
        loop {
            self.guard.Step()?;
            if !self.reservedIDs.contains(&candidate) {
                return Ok(candidate);
            }
            if candidate == i64::MAX {
                candidate = 0;
            } else {
                candidate += 1;
            }
        }
    }

    fn chargeClusterKernel(&self, cluster: &ClusterRef) -> GoResult<()> {
        let n = cluster.b().Nodes.len();
        for _ in 0..n {
            for _ in 0..n {
                self.guard.Step()?;
            }
        }
        let mut width = 1usize;
        while width < n {
            for _ in 0..n {
                self.guard.Step()?;
            }
            if width > n / 2 {
                break;
            }
            width *= 2;
        }
        self.guard.Finish()
    }

    fn maybeCreateCluster(&mut self, cluster: &ClusterRef, arrangement: ClusterArrangement) -> GoResult<bool> {
        let guard = self.guard;
        let nodes = cluster.b().Nodes.clone();
        for node in &nodes {
            guard.Step()?;
            for edge in node.edges() {
                guard.Step()?;
                let adj = node.Adjacent(&edge);
                if clusterIsDescendantOfGuarded(Some(node), Some(&adj), guard)? {
                    return Ok(false);
                }
            }
        }
        let candidate = self.random.bm().Int63();
        let vesselID = self.nextVesselID(candidate)?;
        let clusterEdges = clusterIncidentEdges(cluster, &self.index.infos, &self.index.edgeOrder, guard)?;
        self.chargeClusterKernel(cluster)?;
        {
            let mut c = cluster.bm();
            c.Arrangement = arrangement.clone();
            c.DesiredArrangement = arrangement;
        }
        let padding = PaddingBetween(cluster, false);
        cluster.bm().Padding = padding;
        let vessel = CreateVessel(cluster, vesselID);
        cluster.bm().Vessel = vessel;
        AddCluster(self.graph, cluster);
        guard.Step()?;
        abductClusterEdges(cluster, &clusterEdges, guard)?;
        self.index.refreshAfterClusterAbduction(self.graph, cluster, &clusterEdges, guard)?;
        self.reservedIDs.insert(vesselID);
        let nodes = cluster.b().Nodes.clone();
        for node in &nodes {
            guard.Step()?;
            self.reservedIDs.insert(node.id());
        }
        Ok(true)
    }

    fn refreshContainerEstimate(&mut self, container: Option<&NodeRef>) -> GoResult<()> {
        let Some(container) = container else { return Ok(()) };
        if !container.IsContainer() {
            return Ok(());
        }
        if !self.index.infos.contains_key(container) {
            return Err(errorf!("TALA AddClusters cannot find container discovery index"));
        }
        let padding = self.graph.ContainerPadding(Some(container), true);
        let (mut childrenWidth, mut childrenHeight) = (0.0f64, 0.0f64);
        for child in self.graph.containers().at(&Some(container.clone())) {
            self.guard.Step()?;
            let (mut childWidth, mut childHeight) = (child.w(), child.h());
            if let Some(ci) = self.index.infos.get(&child) {
                childWidth = ci.estimatedWidth;
                childHeight = ci.estimatedHeight;
            }
            childrenWidth += childWidth;
            childrenHeight += childHeight;
        }
        let (cw, ch) = (container.w(), container.h());
        let info = self.index.infos.get_mut(container).unwrap();
        info.estimatedWidth = math::BuiltinMax(cw, childrenWidth + padding.Left() + padding.Right());
        info.estimatedHeight = math::BuiltinMax(ch, childrenHeight + padding.Top() + padding.Bottom());
        self.guard.Finish()
    }
}

/// AddClusters discovers interchangeable sibling nodes and replaces accepted
/// groups with temporary vessels for node placement.
pub fn AddClusters(ctx: &Ctx, graph: &GraphRef, randomSeed: i64, random: &RandRef) -> GoResult<()> {
    layoutgraph::Validate(ctx, "AddClusters", graph)?;
    let (_, guard) = layoutgraph::EnsureTransactionWorkGuard(ctx, "AddClustersTransactions")?;
    let containerOrder = graph.ContainerRDFSOrder(None, &guard)?;

    let stageState = layoutgraph::NewGraphStateSnapshot(layoutgraph::GraphStateSnapshotOptions {
        CaptureTopology: true,
        CaptureEdgeRoutes: true,
    });
    stageState.UpdateWithWorkGuard(graph, &guard)?;
    let result = addClustersInner(graph, randomSeed, random, &guard, &containerOrder);
    if result.is_err() {
        layoutgraph::RestoreGraphState(graph, &stageState);
    }
    result
}

fn addClustersInner(
    graph: &GraphRef,
    randomSeed: i64,
    random: &RandRef,
    guard: &WorkGuardRef,
    containerOrder: &[NodeRef],
) -> GoResult<()> {
    graph.clusters().clear();
    let index = buildClusterDiscoveryIndex(graph, containerOrder, guard)?;
    let mut st = addClustersState { graph, guard, random, index, reservedIDs: Set::default() };
    for node in graph.nodes() {
        guard.Step()?;
        st.reservedIDs.insert(node.id());
    }
    for vessel in graph.SequenceOrder() {
        guard.Step()?;
        st.reservedIDs.insert(vessel.id());
        let nodes = graph.sequences().get(&vessel).unwrap().b().Nodes.clone();
        for node in &nodes {
            guard.Step()?;
            st.reservedIDs.insert(node.id());
        }
    }
    let mut seenTrees: Set<TreeRef> = Set::default();
    for sentinel in graph.TreeOrder() {
        guard.Step()?;
        if let Some(s) = &sentinel {
            st.reservedIDs.insert(s.id());
        }
        let mut stack: Vec<TreeRef> = graph.trees().at(&sentinel);
        while let Some(tree) = stack.pop() {
            guard.Step()?;
            if seenTrees.contains(&tree) {
                continue;
            }
            seenTrees.insert(tree.clone());
            let (node, children) = {
                let t = tree.b();
                (t.Node.clone(), t.Children.clone())
            };
            st.reservedIDs.insert(node.id());
            stack.extend(children);
        }
    }

    const maxSizeDiff: f64 = 4.0;
    let mut containers: Vec<Option<NodeRef>> = containerOrder.iter().cloned().map(Some).collect();
    containers.push(None);
    for container in &containers {
        guard.Step()?;
        let rng = gocompat::rand::New(gocompat::rand::NewSource(randomSeed));
        let children = graph.containers().at(container);
        for node in &children {
            guard.Step()?;
            let Some(info) = st.index.infos.get(node) else {
                return Err(errorf!("TALA AddClusters cannot find node discovery index"));
            };
            if info.noClustering || node.b().Cluster.is_some() {
                continue;
            }
            let mut connectedToCluster = false;
            for edge in node.edges() {
                guard.Step()?;
                if node.Adjacent(&edge).IsClusterVessel() {
                    connectedToCluster = true;
                    break;
                }
            }
            if connectedToCluster || st.index.infos.get(node).unwrap().toTableColumn {
                continue;
            }
            let mut clusterNodes: Vec<NodeRef> = vec![node.clone()];
            let mut isConnectedToSequence = false;
            {
                let info = st.index.infos.get(node).unwrap();
                for otherNode in &children {
                    guard.Step()?;
                    if otherNode == node || !otherNode.SameShape(node) {
                        continue;
                    }
                    let Some(otherInfo) = st.index.infos.get(otherNode) else {
                        return Err(errorf!("TALA AddClusters cannot find comparison-node discovery index"));
                    };
                    if otherInfo.noClustering || otherNode.b().Cluster.is_some() || otherInfo.toTableColumn {
                        continue;
                    }
                    if maxSizeDiff * otherInfo.estimatedWidth < info.estimatedWidth
                        || maxSizeDiff * info.estimatedWidth < otherInfo.estimatedWidth
                        || maxSizeDiff * otherInfo.estimatedHeight < info.estimatedHeight
                        || maxSizeDiff * info.estimatedHeight < otherInfo.estimatedHeight
                    {
                        continue;
                    }
                    let mut sameAdjacentNodes = info.neighbors.len() > 0 && info.neighbors.len() == otherInfo.neighbors.len();
                    if sameAdjacentNodes {
                        for adjacentNode in &info.neighbors {
                            guard.Step()?;
                            isConnectedToSequence = isConnectedToSequence
                                || adjacentNode.as_ref().map_or(false, |a| a.b().Sequence.is_some());
                            if !otherInfo.neighborSet.contains(adjacentNode) {
                                sameAdjacentNodes = false;
                                break;
                            }
                        }
                    }
                    if sameAdjacentNodes && info.edgeSignature.matches(&otherInfo.edgeSignature) {
                        clusterNodes.push(otherNode.clone());
                    }
                }
            }
            if clusterNodes.len() <= 1 {
                continue;
            }
            let cluster = ClusterRef::new(layoutgraph::Cluster {
                // Go leaves Vessel nil until maybeCreateCluster assigns it.
                Vessel: layoutgraph::NewNode(0, 0.0, 0.0),
                Nodes: clusterNodes.clone(),
                Arrangement: ClusterArrangement::default(),
                DesiredArrangement: ClusterArrangement::default(),
                Graph: Some(graph.clone()),
                EdgeAbductions: Vec::new(),
                Padding: 0.0,
                FixedSize: false,
                Container: container.clone(),
            });
            for clusterNode in &clusterNodes {
                guard.Step()?;
                if clusterNode.shape().AspectRatio1() {
                    cluster.bm().FixedSize = true;
                    break;
                }
            }
            st.chargeClusterKernel(&cluster)?;
            let initialArrangement = AssignArrangement(&cluster, isConnectedToSequence, &rng);
            let success = st.maybeCreateCluster(&cluster, initialArrangement.clone())?;
            if !success {
                st.maybeCreateCluster(&cluster, initialArrangement.Flip())?;
            }
        }
        st.refreshContainerEstimate(container.as_ref())?;
    }
    guard.Finish()?;
    Ok(())
}
