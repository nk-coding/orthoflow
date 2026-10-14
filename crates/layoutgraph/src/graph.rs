// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/layoutgraph/graph.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

pub(crate) const minCellSize: f64 = 10.0;
pub(crate) const turnPenaltyMultiplier: f64 = 0.125;
// Keep the exact value because center-port route scoring is output-sensitive.
pub(crate) const centerPortMultiplier: f64 = 0.04287499999999999;

pub struct Graph {
    /// Unfortunate, but because we represent root as nil, we need to indicate root hierarchy as a field in graph
    pub IsRootHierarchy: bool,
    pub Nodes: Vec<NodeRef>,
    pub Edges: Vec<EdgeRef>,
    pub CellSize: f64,

    // ovg
    pub(crate) crossingCost: f64,
    pub(crate) turnCost: f64,
    pub(crate) nonCenterPortCost: f64,

    // These are special entities within a graph. All of them are Go maps, which
    // split subgraphs share by reference (CopyEntitiesFrom), hence GoMap.
    /// -- Containers are nodes which have nodes enclosed inside them
    /// ---- They can be nested
    /// ---- The top level is considered a container of nil
    pub Containers: GoMap<Option<NodeRef>, Vec<NodeRef>>,

    /// -- Clusters are nodes which should stick together and can be reoriented and resized
    pub Clusters: GoMap<NodeRef, ClusterRef>,

    /// -- Trees are nodes which are in a tree structure (i.e. any non-cyclic subgraph)
    /// Root-sentinel trees are keyed by nil.
    pub Trees: GoMap<Option<NodeRef>, Vec<TreeRef>>,
    pub NodeToTree: GoMap<NodeRef, TreeRef>,

    pub Hubs: GoMap<NodeRef, Vec<NodeRef>>,

    pub Sequences: GoMap<NodeRef, SequenceRef>,

    /// -- Directions is used to lookup the direction the edges should point towards within each container
    /// e.g. Bottom means the edges should primarily go from Top to Bottom
    pub Directions: GoMap<Option<NodeRef>, geo::Orientation>,
    pub CommonUncleSiblings: GoMap<NodeRef, Nodes>,

    /// nil (`None`) disables placement caching.
    pub(crate) edgeLengthCache: Option<Map<u64, f64>>,
}

impl Default for Graph {
    /// Go zero value `&Graph{}` (nil maps are represented by fresh empty maps;
    /// `edgeLengthCache` stays nil).
    fn default() -> Self {
        Graph {
            IsRootHierarchy: false,
            Nodes: Vec::new(),
            Edges: Vec::new(),
            CellSize: 0.0,
            crossingCost: 0.0,
            turnCost: 0.0,
            nonCenterPortCost: 0.0,
            Containers: GoMap::new(),
            Clusters: GoMap::new(),
            Trees: GoMap::new(),
            NodeToTree: GoMap::new(),
            Hubs: GoMap::new(),
            Sequences: GoMap::new(),
            Directions: GoMap::new(),
            CommonUncleSiblings: GoMap::new(),
            edgeLengthCache: None,
        }
    }
}

gocompat::go_ptr!(GraphRef, Graph);

pub fn NewGraph() -> GraphRef {
    GraphRef::new(Graph { edgeLengthCache: Some(Map::default()), ..Graph::default() })
}

/// Go `&Graph{}` (zero value; placement caching disabled).
pub fn NewZeroGraph() -> GraphRef {
    GraphRef::new(Graph::default())
}

impl GraphRef {
    /// Direction reports the layout direction assigned to container.
    pub fn Direction(&self, container: Option<&NodeRef>) -> geo::Orientation {
        let dirs = self.b().Directions.clone();
        if let Some(direction) = dirs.get(&container.cloned()) {
            return direction;
        }
        geo::NONE
    }

    /// Go `g.Nodes` (slice copy).
    #[inline]
    pub fn nodes(&self) -> Vec<NodeRef> {
        self.b().Nodes.clone()
    }
    /// Go `g.Edges` (slice copy).
    #[inline]
    pub fn edges(&self) -> Vec<EdgeRef> {
        self.b().Edges.clone()
    }
    /// Go `g.Containers` (shared map handle).
    #[inline]
    pub fn containers(&self) -> GoMap<Option<NodeRef>, Vec<NodeRef>> {
        self.b().Containers.clone()
    }
    /// Go `g.Clusters` (shared map handle).
    #[inline]
    pub fn clusters(&self) -> GoMap<NodeRef, ClusterRef> {
        self.b().Clusters.clone()
    }
    /// Go `g.Sequences` (shared map handle).
    #[inline]
    pub fn sequences(&self) -> GoMap<NodeRef, SequenceRef> {
        self.b().Sequences.clone()
    }
    /// Go `g.Trees` (shared map handle).
    #[inline]
    pub fn trees(&self) -> GoMap<Option<NodeRef>, Vec<TreeRef>> {
        self.b().Trees.clone()
    }
    /// Go `g.NodeToTree` (shared map handle).
    #[inline]
    pub fn nodeToTree(&self) -> GoMap<NodeRef, TreeRef> {
        self.b().NodeToTree.clone()
    }

    pub(crate) fn isBadStateContext(
        &self,
        node: Option<&NodeRef>,
        graphState: Option<&GraphStateRef>,
        ignoreContainerEscape: bool,
        guard: &dyn workStepper,
    ) -> GoResult<bool> {
        let (bad, skipOverlapCheck) = self.isStructurallyBadStateContext(node, graphState, ignoreContainerEscape, guard)?;
        if bad {
            return Ok(bad);
        }
        if skipOverlapCheck {
            guard.Finish()?;
            return Ok(false);
        }
        self.hasBadOverlapStateContext(node, graphState, guard)
    }

    /// isStructurallyBadStateContext checks the linear-time node invariants that do
    /// not depend on any other non-descendant graph node.
    pub(crate) fn isStructurallyBadStateContext(
        &self,
        node: Option<&NodeRef>,
        graphState: Option<&GraphStateRef>,
        ignoreContainerEscape: bool,
        guard: &dyn workStepper,
    ) -> GoResult<(bool, bool)> {
        self.isStructurallyBadStateWithFixedOriginContext(node, graphState, ignoreContainerEscape, None, false, guard)
    }

    pub(crate) fn isStructurallyBadStateWithFixedOriginContext(
        &self,
        node: Option<&NodeRef>,
        graphState: Option<&GraphStateRef>,
        ignoreContainerEscape: bool,
        fixedOrigin: Option<geo::Point>,
        fixedOriginCached: bool,
        guard: &dyn workStepper,
    ) -> GoResult<(bool, bool)> {
        let node = match node {
            Some(n) if n.has_tl() => n,
            _ => return Err(invariant::New("transaction bad-state check received an incomplete node")),
        };
        guard.Step()?;
        // No need to check if cluster node is in bad state, the vessel covers it
        // Right now only happens when checking if containers bad state, since cluster nodes are still containers
        if node.b().Cluster.is_some() {
            return Ok((false, true));
        }
        if !ignoreContainerEscape {
            if let Some(c) = node.container() {
                if c.has_tl() && node.has_tl() && !c.Surrounds(node, 0.0) {
                    return Ok((true, false));
                }
            }
            if node.b().isContainer {
                let children = self.b().Containers.at(&Some(node.clone()));
                for child in &children {
                    guard.Step()?;
                    if !child.has_tl() {
                        return Err(invariant::New("transaction container has an incomplete child"));
                    }
                    if !node.Surrounds(child, 0.0) {
                        return Ok((true, false));
                    }
                }
            }
        }

        // Nothing should be past the fixed origin. Transactions cache the first
        // fixed origin for each container once per commit; standalone callers retain
        // the legacy lookup path.
        let t = node.tlv();
        let pastFixedOrigin = if fixedOriginCached {
            match fixedOrigin {
                Some(fo) => t.X < fo.X || t.Y < fo.Y,
                None => false,
            }
        } else {
            node.isPointPastFixedOrigin(t.X, t.Y, true)
        };
        if pastFixedOrigin {
            return Ok((true, false));
        }

        if node.b().FixedTopLeft.is_some() {
            if let Some(graphState) = graphState {
                let mut originalPoint = geo::Point::default();
                if let Some(original) = graphState.b().nodeGeometry.get(node) {
                    if original.topLeft.pointer.is_some() {
                        originalPoint = original.topLeft.value;
                    }
                }
                if t.X != originalPoint.X || t.Y != originalPoint.Y {
                    return Ok((true, false));
                }
            }
        }
        Ok((false, false))
    }

    pub(crate) fn hasBadOverlapStateContext(
        &self,
        node: Option<&NodeRef>,
        graphState: Option<&GraphStateRef>,
        guard: &dyn workStepper,
    ) -> GoResult<bool> {
        let node = match node {
            Some(n) if n.has_tl() => n,
            _ => return Err(invariant::New("transaction overlap check received an incomplete node")),
        };
        if node.b().Cluster.is_some() {
            return Ok(false);
        }
        let pairwiseNodeExceptions: Option<Set<NodeRef>> =
            graphState.and_then(|gs| gs.b().existingOverlaps.get(node).cloned());

        let mut exceptions: Vec<NodeRef> = Vec::new();
        let (isContainer, isVessel, hasContainer, hasCluster, hasSequence) = {
            let n = node.b();
            (n.isContainer, n.isClusterVessel, n.Container.is_some(), n.Cluster.is_some(), n.Sequence.is_some())
        };
        if isContainer || isVessel || self.b().Sequences.has(node) {
            exceptions = self.allDescendantNodesGuarded(Some(node), true, guard)?;
        }
        // doesOverlapWithDimensionsContext already skips node identity, so only
        // descendants and ancestors need to be materialized as exceptions.
        if hasContainer || hasCluster || hasSequence {
            let ancestors = self.ancestorsOfGuarded(node, guard)?;
            exceptions.extend(ancestors);
        }
        let (tl, w, h) = (node.tl(), node.w(), node.h());
        self.doesOverlapWithDimensionsContext(Some(node), tl.as_ref(), w, h, &exceptions, pairwiseNodeExceptions.as_ref(), guard)
    }

    pub fn AddNodeUnchecked(&self, node: &NodeRef) {
        self.bm().Nodes.push(node.clone());
        node.bm().Graph = Some(self.clone());
    }

    pub fn AddNodeToContainer(&self, container: Option<&NodeRef>, node: &NodeRef) {
        let containers = self.b().Containers.clone();
        {
            let mut m = containers.bm();
            m.entry(container.cloned()).or_default().push(node.clone());
        }
        node.bm().Container = container.cloned();
        if let Some(container) = container {
            container.bm().isContainer = true;
        }
    }

    pub fn AddNewNodeToContainer(&self, container: Option<&NodeRef>, node: &NodeRef) {
        self.AddNodeUnchecked(node);
        self.AddNodeToContainer(container, node);
    }

    pub fn AddNode(&self, node: &NodeRef) -> NodeRef {
        let tl = node.tl();
        let (occupant, is) = self.isOccupied(tl.as_ref());
        if !is {
            self.AddNodeUnchecked(node);
            node.clone()
        } else {
            occupant.unwrap()
        }
    }

    pub fn RemoveNode(&self, node: &NodeRef) {
        let mut g = self.bm();
        let newNodes: Vec<NodeRef> = g.Nodes.iter().filter(|n| *n != node).cloned().collect();
        g.Nodes = newNodes;
    }

    pub fn AddEdge(&self, edge: &EdgeRef) {
        let mut g = self.bm();
        if g.Edges.contains(edge) {
            return;
        }
        g.Edges.push(edge.clone());
    }

    pub fn ComputeCellSize(&self) {
        let mut minHeight = f64::INFINITY;
        let mut minWidth = f64::INFINITY;
        let mut maxHeight = f64::NEG_INFINITY;
        let mut maxWidth = f64::NEG_INFINITY;

        for node in self.nodes() {
            let (w, h) = (node.w(), node.h());
            minWidth = fmin(minWidth, w);
            minHeight = fmin(minHeight, h);
            maxWidth = fmax(maxWidth, w);
            maxHeight = fmax(maxHeight, h);
        }

        let minLength = fmin(minWidth, minHeight);
        let maxLength = fmax(maxWidth, maxHeight);

        let mut cell = if maxLength < (3.0 * minLength) { maxLength.ceil() } else { ((3.0 * minLength) / 2.0).ceil() };
        cell = fmax(cell, minCellSize);
        self.bm().CellSize = cell;
    }

    pub fn Disconnect(&self, edge: &EdgeRef) {
        edge.from().removeEdge(edge);
        edge.to().removeEdge(edge);

        let mut g = self.bm();
        let newEdges: Vec<EdgeRef> = g.Edges.iter().filter(|e| *e != edge).cloned().collect();
        g.Edges = newEdges;
    }

    pub fn Connect(&self, nodeA: &NodeRef, nodeB: &NodeRef) -> EdgeRef {
        let edge = NewEdge(nodeA, nodeB);
        self.bm().Edges.push(edge.clone());
        nodeA.addEdge(&edge);
        if nodeA != nodeB {
            nodeB.addEdge(&edge);
        }
        edge
    }

    pub(crate) fn doesOverlapWithDimensionsContext(
        &self,
        node: Option<&NodeRef>,
        p: Option<&geo::PointRef>,
        newWidth: f64,
        newHeight: f64,
        exceptions: &[NodeRef],
        // Go passes the whole pairwise map and resolves `pairwiseExceptions[node]`
        // once; the port passes that resolved per-node set.
        pairwiseNodeExceptions: Option<&Set<NodeRef>>,
        guard: &dyn workStepper,
    ) -> GoResult<bool> {
        let (Some(node), Some(p)) = (node, p) else {
            return Err(invariant::New("overlap check received an incomplete node"));
        };
        let pv = *p.b();
        let right = pv.X + newWidth;
        let bottom = pv.Y + newHeight;
        let mut exceptionSet: Set<NodeRef> = Set::default();
        if !exceptions.is_empty() {
            for exception in exceptions {
                guard.Step()?;
                exceptionSet.insert(exception.clone());
            }
        }

        for otherNode in self.nodes() {
            guard.Step()?;
            if otherNode == *node {
                continue;
            }
            if let Some(otl) = otherNode.tl() {
                let o = *otl.b();
                let (ow, oh) = (otherNode.w(), otherNode.h());
                // This heuristic is only a trial filter; the transaction constructor uses
                // the proven graph-wide padding above as its construction bound.
                const maxSafeDelta: f64 = 500.0;
                if pv.X > o.X + ow + maxSafeDelta
                    || pv.X + newWidth + maxSafeDelta < o.X
                    || pv.Y > o.Y + oh + maxSafeDelta
                    || pv.Y + newHeight + maxSafeDelta < o.Y
                {
                    continue;
                }
                // Distant boxes cannot overlap regardless of the exception sets. Test
                // geometry first so sparse layouts avoid two map probes per box.
                if let Some(pe) = pairwiseNodeExceptions {
                    if pe.contains(&otherNode) {
                        continue;
                    }
                }
                if exceptionSet.contains(&otherNode) {
                    continue;
                }
                let deltaValue = node.deltaToGuarded(Some(&otherNode), Some(p), Some(guard))?;
                let delta = deltaValue as f64;

                // Standard overlap test
                if (pv.X < (o.X + ow + delta)) && ((right + delta) > o.X) && (pv.Y < (o.Y + oh + delta)) && ((bottom + delta) > o.Y) {
                    return Ok(true);
                }
            }
        }
        guard.Finish()?;
        Ok(false)
    }

    /// WouldOverlapWithWorkGuard reports whether placing node at point overlaps an
    /// unexcepted graph node while charging the complete comparison.
    pub fn WouldOverlapWithWorkGuard(
        &self,
        node: Option<&NodeRef>,
        point: Option<&geo::PointRef>,
        exceptions: &[NodeRef],
        pairwiseExceptions: Option<&Map<NodeRef, Set<NodeRef>>>,
        guard: Option<&limits::WorkGuardRef>,
    ) -> GoResult<bool> {
        let Some(guard) = guard else {
            return Err(errorf!("TALA overlap check requires a work guard"));
        };
        let pairwiseNodeExceptions = match (pairwiseExceptions, node) {
            (Some(m), Some(n)) => m.get(n),
            _ => None,
        };
        match node {
            None => self.doesOverlapWithDimensionsContext(node, point, 0.0, 0.0, exceptions, pairwiseNodeExceptions, guard),
            Some(n) => {
                let (w, h) = (n.w(), n.h());
                self.doesOverlapWithDimensionsContext(node, point, w, h, exceptions, pairwiseNodeExceptions, guard)
            }
        }
    }

    pub(crate) fn isOccupied(&self, p: Option<&geo::PointRef>) -> (Option<NodeRef>, bool) {
        for n in self.nodes() {
            let tl = n.tl();
            if tl.is_some() && p.is_some() && nonNilEquals(tl.as_ref(), p) {
                return (Some(n), true);
            }
        }
        (None, false)
    }
}

impl NodeRef {
    pub(crate) fn isPointPastFixedOrigin(&self, x: f64, y: f64, includeSizes: bool) -> bool {
        let graph = self.g();
        let fixedOrigin = graph.containerFixedOrigin(self.container().as_ref());
        let Some(fixedOrigin) = fixedOrigin else {
            return false;
        };
        let mut fo = *fixedOrigin.b();
        if !includeSizes {
            let cs = graph.b().CellSize;
            fo.X = (fo.X / cs).round();
            fo.Y = (fo.Y / cs).round();
        }
        x < fo.X || y < fo.Y
    }
}

/// Go `nodeGraphOwnershipJournal map[*Node]*Graph`.
pub(crate) type nodeGraphOwnershipJournal = Map<NodeRef, Option<GraphRef>>;

pub(crate) fn restoreOwnershipJournal(journal: &nodeGraphOwnershipJournal) {
    for (node, graph) in journal.iter() {
        node.bm().Graph = graph.clone();
    }
}

/// SplitOptions controls which relationships connect nodes while splitting a
/// graph into subgraphs.
#[derive(Clone, Copy, Debug, Default)]
pub struct SplitOptions {
    pub IncludeContainers: bool,
    pub IncludeNears: bool,
    pub TraverseTrees: bool,
}

impl GraphRef {
    /// SplitSubgraphs partitions the graph into sets of mutually reachable nodes.
    pub fn SplitSubgraphs(&self, ctx: &Ctx, options: SplitOptions) -> GoResult<Vec<GraphRef>> {
        self.splitSubgraphsWithOwnership(ctx, options, None, None)
    }

    /// splitSubgraphsWithOwnership optionally returns every Node.Graph value changed
    /// by the split.
    pub(crate) fn splitSubgraphsWithOwnership(
        &self,
        ctx: &Ctx,
        options: SplitOptions,
        ownership: Option<&mut nodeGraphOwnershipJournal>,
        sharedGuard: Option<&dyn workStepper>,
    ) -> GoResult<Vec<GraphRef>> {
        let ownedGuard;
        let guard: &dyn workStepper = match sharedGuard {
            None => {
                validateEngineGraph(ctx, "SplitSubgraphs", Some(self))?;
                ownedGuard = limits::NewWorkGuard(ctx, "SplitSubgraphs", maxEngineWorkUnits)?;
                &ownedGuard
            }
            Some(g) => {
                g.Finish()?;
                g
            }
        };
        let gnodes = self.nodes();
        let mut originalGraphReferences: nodeGraphOwnershipJournal = Map::default();
        // Insertion order of the journal (the Go defer iterates a map; restoring
        // is order-independent).
        for node in &gnodes {
            guard.Step()?;
            originalGraphReferences.insert(node.clone(), node.graph());
        }

        let result = (|| -> GoResult<Vec<GraphRef>> {
            let mut graphs: Vec<GraphRef> = Vec::new();
            if gnodes.is_empty() {
                return Ok(graphs);
            }

            let mut added: Set<NodeRef> = Set::default();
            let mut nodeToSubgraph: Map<NodeRef, GraphRef> = Map::default();
            let addReachable = |startingNode: &NodeRef,
                                subgraph: &GraphRef,
                                originalGraphReferences: &mut nodeGraphOwnershipJournal,
                                added: &mut Set<NodeRef>,
                                nodeToSubgraph: &mut Map<NodeRef, GraphRef>|
             -> GoResult<()> {
                let reachableNodes = startingNode.allReachableNodesGuarded(
                    options.IncludeContainers,
                    options.IncludeNears,
                    options.TraverseTrees,
                    None,
                    guard,
                )?;
                for node in &reachableNodes {
                    guard.Step()?;
                    if added.contains(node) {
                        continue;
                    }
                    if !originalGraphReferences.contains_key(node) {
                        if originalGraphReferences.len() as i64 >= maxEngineNodes {
                            return Err(errorf!("TALA SplitSubgraphs unique node ownership exceeds limit {}", maxEngineNodes));
                        }
                        originalGraphReferences.insert(node.clone(), node.graph());
                    }
                    subgraph.AddNodeUnchecked(node);
                    added.insert(node.clone());
                    nodeToSubgraph.insert(node.clone(), subgraph.clone());
                }
                Ok(())
            };

            let mut fixedNodes: Vec<NodeRef> = Vec::new();
            for node in &gnodes {
                guard.Step()?;
                if node.b().FixedTopLeft.is_some() {
                    fixedNodes.push(node.clone());
                }
            }
            if !fixedNodes.is_empty() {
                // create first subgraph with all fixed nodes
                // it will be placed at 0,0 in CombineSubgraphs so fixed locations are accurate
                let subgraph = NewGraph();
                subgraph.CopyEntitiesFrom(self);

                for startingNode in &fixedNodes {
                    addReachable(startingNode, &subgraph, &mut originalGraphReferences, &mut added, &mut nodeToSubgraph)?;
                }

                graphs.push(subgraph);
            }

            for startingNode in &gnodes {
                guard.Step()?;
                if added.contains(startingNode) {
                    continue;
                }

                let subgraph = NewGraph();
                subgraph.CopyEntitiesFrom(self);
                addReachable(startingNode, &subgraph, &mut originalGraphReferences, &mut added, &mut nodeToSubgraph)?;

                graphs.push(subgraph);
            }

            // Each original edge is considered once. This preserves the original graph's
            // edge order without the quadratic duplicate scan in AddEdge.
            let mut seenEdges: Map<GraphRef, Set<EdgeRef>> = Map::default();
            let mut appendEdge = |graph: &GraphRef, edge: &EdgeRef| {
                let graphEdges = seenEdges.entry(graph.clone()).or_default();
                if graphEdges.contains(edge) {
                    return;
                }
                graphEdges.insert(edge.clone());
                graph.bm().Edges.push(edge.clone());
            };
            for edge in self.edges() {
                guard.Step()?;
                let fromGraph = nodeToSubgraph.get(&edge.from()).cloned();
                let toGraph = nodeToSubgraph.get(&edge.to()).cloned();
                if let Some(fg) = &fromGraph {
                    appendEdge(fg, &edge);
                }
                if let Some(tg) = &toGraph {
                    if fromGraph.as_ref() != Some(tg) {
                        appendEdge(tg, &edge);
                    }
                }
            }
            guard.Finish()?;
            Ok(graphs)
        })();

        match result {
            Ok(graphs) => {
                if let Some(ownership) = ownership {
                    *ownership = originalGraphReferences;
                }
                Ok(graphs)
            }
            Err(err) => {
                // defer: restore every captured Graph pointer on failure.
                restoreOwnershipJournal(&originalGraphReferences);
                Err(err)
            }
        }
    }

    pub(crate) fn area(&self) -> f64 {
        if self.b().Nodes.is_empty() {
            return 0.0;
        }
        let (topLeft, bottomRight) = self.unroundedBounds();
        let tl = *topLeft.expect("nil pointer dereference").b();
        let br = *bottomRight.expect("nil pointer dereference").b();
        (tl.X - br.X).abs() * (tl.Y - br.Y).abs()
    }

    pub(crate) fn bounds(&self) -> (Option<geo::PointRef>, Option<geo::PointRef>) {
        let (tl, br) = self.boundingBox(true);
        let (Some(tl), Some(br)) = (tl, br) else {
            return (None, None);
        };
        // Placement consumes a pixel-aligned graph box even when edge points are
        // fractional.
        let (tl, br) = (*tl.b(), *br.b());
        (Some(newPoint(tl.X.round(), tl.Y.round())), Some(newPoint(br.X.round(), br.Y.round())))
    }

    pub(crate) fn unroundedBounds(&self) -> (Option<geo::PointRef>, Option<geo::PointRef>) {
        self.boundingBox(false)
    }

    pub(crate) fn boundingBox(&self, roundNodeDimensions: bool) -> (Option<geo::PointRef>, Option<geo::PointRef>) {
        let nodes = self.nodes();
        let (tl, br) = if roundNodeDimensions { nodes.fixedBounds() } else { nodes.unroundedFixedBounds() };
        let (Some(tl), Some(br)) = (tl, br) else {
            return (None, None);
        };
        let (tl, br) = (*tl.b(), *br.b());

        let mut minX = tl.X;
        let mut minY = tl.Y;

        let mut maxX = br.X;
        let mut maxY = br.Y;

        for edge in self.edges() {
            let (edgeTL, edgeBR) = edge.boundingBoxValues();
            if !edgeTL.X.is_infinite() {
                minX = fmin(minX, edgeTL.X);
                minY = fmin(minY, edgeTL.Y);
                maxX = fmax(maxX, edgeBR.X);
                maxY = fmax(maxY, edgeBR.Y);
            }
        }

        (Some(newPoint(minX, minY)), Some(newPoint(maxX, maxY)))
    }

    pub(crate) fn turnCostValue(&self) -> f64 {
        {
            let g = self.b();
            if g.turnCost != 0.0 {
                return g.turnCost;
            }
        }
        let n = self.b().Edges.len() as f64;
        let cost = turnPenaltyMultiplier * n * self.maxEdgeLength();
        self.bm().turnCost = cost;
        cost
    }

    pub(crate) fn nonCenterPortCostValue(&self) -> f64 {
        {
            let g = self.b();
            if g.nonCenterPortCost != 0.0 {
                return g.nonCenterPortCost;
            }
            if g.Edges.is_empty() {
                return 0.0;
            }
        }
        let n = self.b().Edges.len() as f64;
        let mut cost = centerPortMultiplier * n * self.maxEdgeLength();

        // For certain diagrams, the `cost` above can be smaller than the distance between ports
        // and then edge routing doesn't prefer center ports as expected
        // the routine below estimates another cost based on the min node size
        let mut minSize = f64::INFINITY;
        for n in self.nodes() {
            if n.b().isContainer {
                continue;
            }
            minSize = fmin(minSize, n.h());
            minSize = fmin(minSize, n.w());
        }

        if !(minSize.is_infinite() && minSize > 0.0) {
            cost = fmax(cost, minSize / 3.0);
        }
        self.bm().nonCenterPortCost = cost;

        cost
    }

    pub(crate) fn crossingCostValue(&self) -> f64 {
        {
            let g = self.b();
            if g.crossingCost != 0.0 {
                return g.crossingCost;
            }
        }
        // c * (m * l_max)
        let n = self.b().Edges.len() as f64;
        let cost = CrossingCostWeight * n * self.maxEdgeLength();
        self.bm().crossingCost = cost;

        cost
    }

    pub(crate) fn resetTurnCost(&self) {
        self.bm().turnCost = 0.0;
    }

    pub(crate) fn maxEdgeLength(&self) -> f64 {
        let mut hasLength = false;
        let mut maxLength = ConnectedNodeGap as f64;
        for n in self.nodes() {
            if !n.has_tl() {
                continue;
            }
            for e in n.edges() {
                let adj = n.adjacent(&e);
                if !adj.has_tl() {
                    continue;
                }
                hasLength = true;
                let distance = n.distanceTo(&adj, true);
                maxLength = fmax(maxLength, distance);
            }
        }
        if !hasLength {
            return 0.0;
        }
        maxLength
    }

    pub(crate) fn containerRDFSOrder(&self, root: Option<&NodeRef>) -> Vec<NodeRef> {
        let mut order: Vec<NodeRef> = Vec::new();

        if let Some(root) = root {
            if !root.b().isContainer {
                return order;
            }
        }

        let (containers, clusters) = {
            let g = self.b();
            (g.Containers.clone(), g.Clusters.clone())
        };
        for child in containers.at(&root.cloned()).iter().rev() {
            if child.b().isContainer {
                order.extend(self.containerRDFSOrder(Some(child)));
                order.push(child.clone());
                continue;
            }

            if child.b().isClusterVessel {
                let cluster = clusters.get(child).expect("nil pointer dereference: cluster");
                let cnodes = cluster.b().Nodes.clone();
                for cNode in cnodes.iter().rev() {
                    if cNode.b().isContainer {
                        order.extend(self.containerRDFSOrder(Some(cNode)));
                        order.push(cNode.clone());
                    }
                }
                continue;
            }
        }

        order
    }

    pub(crate) fn containerRDFSOrderContext(&self, root: Option<&NodeRef>, guard: &dyn workStepper) -> GoResult<Vec<NodeRef>> {
        let mut order: Vec<NodeRef> = Vec::new();
        if let Some(root) = root {
            if !root.b().isContainer {
                return Ok(order);
            }
        }

        let (containers, clusters) = {
            let g = self.b();
            (g.Containers.clone(), g.Clusters.clone())
        };
        for child in containers.at(&root.cloned()).iter().rev() {
            guard.Step()?;

            if child.b().isContainer {
                let descendants = self.containerRDFSOrderContext(Some(child), guard)?;
                order.extend(descendants);
                order.push(child.clone());
                continue;
            }
            if child.b().isClusterVessel {
                let cluster = clusters.get(child).expect("nil pointer dereference: cluster");
                let cnodes = cluster.b().Nodes.clone();
                for clusterNode in cnodes.iter().rev() {
                    guard.Step()?;

                    if clusterNode.b().isContainer {
                        let descendants = self.containerRDFSOrderContext(Some(clusterNode), guard)?;
                        order.extend(descendants);
                        order.push(clusterNode.clone());
                    }
                }
            }
        }
        Ok(order)
    }

    pub(crate) fn syncNested(&self) {
        for n in self.nodes() {
            if n.b().isContainer {
                n.positionContainerChildren(true);
            }
            if n.b().isClusterVessel {
                let c = self.b().Clusters.get(&n).expect("nil pointer dereference: cluster");
                c.SyncGeometry();
                // Since cluster nodes can themselves be containers, we can't merely move all children nodes, we must
                // also care to give container children padding
                let cnodes = c.b().Nodes.clone();
                for cn in &cnodes {
                    if cn.b().isContainer {
                        let padding = self.containerPadding(Some(cn), true);
                        for child in self.b().Containers.at(&Some(cn.clone())) {
                            child.moveNodeWithChildren(padding.left, padding.top);
                        }
                    }
                }
            }
            let seq = self.b().Sequences.get(&n);
            if let Some(seq) = seq {
                seq.SyncGeometry();
            }
        }
    }

    // includeClusterNodes should be included when the purpose is to move all nodes
    // But if the purpose is to find nodes to perform operations on, the cluster nodes should not be included (vessels will be included)
    // Does not include current node
    pub(crate) fn allDescendantNodes(&self, node: Option<&NodeRef>, includeClusterNodes: bool) -> Vec<NodeRef> {
        match self.allDescendantNodesGuarded(node, includeClusterNodes, &unboundedWork) {
            Ok(d) => d,
            Err(err) => panic!("{}", err),
        }
    }

    pub(crate) fn allDescendantNodesGuarded(
        &self,
        node: Option<&NodeRef>,
        includeClusterNodes: bool,
        guard: &dyn workStepper,
    ) -> GoResult<Vec<NodeRef>> {
        struct Entry {
            node: NodeRef,
            emit: bool,
        }
        let (containers, clusters, sequences) = {
            let g = self.b();
            (g.Containers.clone(), g.Clusters.clone(), g.Sequences.clone())
        };
        let mut seen: Set<NodeRef> = Set::default();
        let mut stack: Vec<Entry> = Vec::new();
        let mut descendants: Vec<NodeRef> = Vec::new();
        if let Some(node) = node {
            seen.insert(node.clone());
        }
        let pushChildren = |parent: Option<&NodeRef>, stack: &mut Vec<Entry>| -> GoResult<()> {
            // Push in reverse traversal order so the iterative walk retains the old
            // container-before-cluster-before-sequence preorder.
            if let Some(p) = parent {
                if let Some(sequence) = sequences.get(p) {
                    let snodes = sequence.b().Nodes.clone();
                    for v in snodes.iter().rev() {
                        guard.Step()?;
                        stack.push(Entry { node: v.clone(), emit: includeClusterNodes });
                    }
                }
            }
            if let Some(p) = parent {
                if p.b().isClusterVessel {
                    if let Some(cluster) = clusters.get(p) {
                        let cnodes = cluster.b().Nodes.clone();
                        for v in cnodes.iter().rev() {
                            guard.Step()?;
                            stack.push(Entry { node: v.clone(), emit: includeClusterNodes });
                        }
                    }
                }
            }
            let isRootOrContainer = match parent {
                None => true,
                Some(p) => p.b().isContainer,
            };
            if isRootOrContainer {
                let children = containers.at(&parent.cloned());
                for child in children.iter().rev() {
                    guard.Step()?;
                    stack.push(Entry { node: child.clone(), emit: true });
                }
            }
            Ok(())
        };
        pushChildren(node, &mut stack)?;

        while let Some(current) = {
            if stack.is_empty() {
                None
            } else {
                guard.Step()?;
                stack.pop()
            }
        } {
            if seen.contains(&current.node) {
                continue;
            }
            seen.insert(current.node.clone());
            if current.emit {
                descendants.push(current.node.clone());
            }
            pushChildren(Some(&current.node), &mut stack)?;
        }
        guard.Finish()?;
        Ok(descendants)
    }

    pub(crate) fn ancestorsOfGuarded(&self, node: &NodeRef, guard: &dyn workStepper) -> GoResult<Vec<NodeRef>> {
        let mut nodes: Vec<NodeRef> = Vec::new();
        let mut seen: Set<NodeRef> = Set::default();
        let mut current = Some(node.clone());
        while let Some(cur) = current {
            guard.Step()?;
            if seen.contains(&cur) {
                return Err(invariant::New("cycle in transaction ancestry"));
            }
            seen.insert(cur.clone());
            if seen.len() as i64 > maxEngineNodes {
                return Err(invariant::New("transaction ancestry exceeds node limit"));
            }
            if cur != *node {
                // don't include node itself
                nodes.push(cur.clone());
            }
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
                break;
            }
        }
        guard.Finish()?;
        Ok(nodes)
    }

    pub(crate) fn isTreeSentinel(&self, n: Option<&NodeRef>) -> bool {
        self.b().Trees.has(&n.cloned())
    }

    pub(crate) fn isSequence(&self, n: &NodeRef) -> bool {
        self.b().Sequences.has(n)
    }

    pub fn CopyEntitiesFrom(&self, other: &GraphRef) {
        let (containers, clusters, trees, nodeToTree, sequences, hubs, directions, cus, isRoot) = {
            let o = other.b();
            (
                o.Containers.clone(),
                o.Clusters.clone(),
                o.Trees.clone(),
                o.NodeToTree.clone(),
                o.Sequences.clone(),
                o.Hubs.clone(),
                o.Directions.clone(),
                o.CommonUncleSiblings.clone(),
                o.IsRootHierarchy,
            )
        };
        let mut g = self.bm();
        g.Containers = containers;
        g.Clusters = clusters;
        g.Trees = trees;
        g.NodeToTree = nodeToTree;
        g.Sequences = sequences;
        g.Hubs = hubs;
        g.Directions = directions;
        g.CommonUncleSiblings = cus;
        // TODO maybe not
        g.IsRootHierarchy = isRoot;
    }

    pub(crate) fn fixedNodes(&self) -> Vec<NodeRef> {
        self.nodes().fixedNodes()
    }

    pub(crate) fn hasFixedNode(&self) -> bool {
        self.nodes().hasFixedNode()
    }

    pub(crate) fn containerFixedOrigin(&self, container: Option<&NodeRef>) -> Option<geo::PointRef> {
        for n in self.nodes() {
            if !nodeEq(n.container().as_ref(), container) {
                continue;
            }
            if let Some(p) = n.fixedOrigin() {
                return Some(p);
            }
        }
        None
    }
}

impl NodeRef {
    pub(crate) fn fixedOrigin(&self) -> Option<geo::PointRef> {
        let (tl, ftl) = (self.tl(), self.fixedTL());
        if let (Some(tl), Some(ftl)) = (tl, ftl) {
            let t = *tl.b();
            let f = *ftl.b();
            return Some(newPoint(t.X - f.X, t.Y - f.Y));
        }
        None
    }
}

impl GraphRef {
    // return the amount of padding the container should have on each side. (more padding if there are icons)
    // Note: we need the graph reference since container may be the nil container
    // considerChildren should be false if the output is used together with the nodes bounding box, becuase it already accounts for icons
    pub(crate) fn containerPadding(&self, container: Option<&NodeRef>, considerChildren: bool) -> Spacing {
        let mut padding = ContainerPadding as f64;
        let mut spacing = Spacing { top: padding, bottom: padding, left: padding, right: padding };
        let Some(container) = container else {
            return spacing;
        };
        let (shapeType, hasIcon, cpadding, cw, ch) = {
            let c = container.b();
            let bx = c.Box.b().clone();
            (c.shapeType, c.Icon.is_some(), c.padding, bx.Width, bx.Height)
        };
        if shapeType == circleType {
            // Circles just need padding so the corner of child doesn't touch corner of circle
            // Optically, there will be a lot of padding on the sides even given a small value
            padding /= 4.0;
        }

        if hasIcon && shapeType != imageType {
            // we don't know if the icon will be placed at an inside position, but leave enough padding so it has that option
            // Note: we don't know what the exact icon size will be until the final layout size so we use max
            padding = (MaxIconSize as f64) + 2.0 * LABEL_PADDING;
        }

        if let Some(lbl) = container.label() {
            let (lpos, lw, lh) = {
                let l = lbl.b();
                (l.Position, l.Width, l.Height)
            };
            if !lpos.IsOutside() {
                // First ensure total container width/height can accommodate label
                let labelWidth = lw + 2.0 * LABEL_PADDING;
                let labelHeight = lh + 2.0 * LABEL_PADDING;

                use label::*;
                if lpos == InsideTopLeft || lpos == InsideTopCenter || lpos == InsideTopRight {
                    spacing.top = fmax(spacing.top, labelHeight);
                } else if lpos == InsideBottomLeft || lpos == InsideBottomCenter || lpos == InsideBottomRight {
                    spacing.bottom = fmax(spacing.bottom, labelHeight);
                } else if lpos == InsideMiddleLeft {
                    spacing.left = fmax(spacing.left, labelWidth);
                } else if lpos == InsideMiddleRight {
                    spacing.right = fmax(spacing.right, labelWidth);
                }

                // Ensure total width/height can accommodate label
                let minContainerWidth = labelWidth + spacing.left + spacing.right;
                if cw < minContainerWidth {
                    let extraPadding = ((minContainerWidth - cw) / 2.0).ceil();
                    spacing.left = fmax(spacing.left, extraPadding);
                    spacing.right = fmax(spacing.right, extraPadding);
                }

                let minContainerHeight = labelHeight + spacing.top + spacing.bottom;
                if ch < minContainerHeight {
                    let extraPadding = ((minContainerHeight - ch) / 2.0).ceil();
                    spacing.top = fmax(spacing.top, extraPadding);
                    spacing.bottom = fmax(spacing.bottom, extraPadding);
                }
            }
        }

        if considerChildren {
            let mut hasChildWithIcon = false;
            let mut childrenMargin = Spacing::default();
            for child in self.b().Containers.at(&Some(container.clone())) {
                let c = child.b();
                if !hasChildWithIcon {
                    if let Some(icon) = &c.Icon {
                        if c.shapeType != imageType && !icon.b().positionFixed {
                            hasChildWithIcon = true;
                        }
                    }
                }
                childrenMargin.left = fmax(childrenMargin.left, c.margin.left);
                childrenMargin.right = fmax(childrenMargin.right, c.margin.right);
                childrenMargin.top = fmax(childrenMargin.top, c.margin.top);
                childrenMargin.bottom = fmax(childrenMargin.bottom, c.margin.bottom);
            }
            spacing.left = fmax(spacing.left, cpadding.left + childrenMargin.left);
            spacing.right = fmax(spacing.right, cpadding.right + childrenMargin.right);
            spacing.top = fmax(spacing.top, cpadding.top + childrenMargin.top);
            spacing.bottom = fmax(spacing.bottom, cpadding.bottom + childrenMargin.bottom);

            if hasChildWithIcon {
                // we don't know if the icon will be placed at an inside position, but leave enough padding so it has that option
                // Note: we don't know what the exact icon size will be until the final layout size so we use max
                padding = (MaxIconSize as f64) + 2.0 * LABEL_PADDING;
            }
        }

        if shapeType == circleType {
            spacing.top /= 4.0;
            spacing.left /= 4.0;
            spacing.bottom /= 4.0;
            spacing.right /= 4.0;
        }

        spacing.left = fmax(spacing.left, fmax(cpadding.left, padding));
        spacing.right = fmax(spacing.right, fmax(cpadding.right, padding));
        spacing.top = fmax(spacing.top, fmax(cpadding.top, padding));
        spacing.bottom = fmax(spacing.bottom, fmax(cpadding.bottom, padding));

        spacing
    }
}
