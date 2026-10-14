// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/layoutgraph/transaction.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;
use std::cell::RefCell;
use std::rc::Rc;

pub struct GraphState {
    // nodeGeometry is also the transaction's deduplicated node set.
    pub(crate) nodeGeometry: Map<NodeRef, graphGeometrySnapshot>,
    pub(crate) originalNodes: Vec<NodeRef>,
    pub(crate) hasFixedTopLeft: bool,

    pub(crate) clusterArrangements: Map<ClusterRef, ClusterArrangement>,
    pub(crate) clusterDesired: Map<ClusterRef, ClusterArrangement>,
    pub(crate) clusterPaddings: Map<ClusterRef, f64>,
    pub(crate) edgeGeometry: Option<Map<EdgeRef, edgeSnapshot>>,
    pub(crate) treeOrientations: Map<TreeRef, geo::Orientation>,

    // We don't want to be frozen due to some existing overlap, since many can be valid diagrams
    // We just care about not introducing new overlaps. (Immutable once built and
    // shared between successive states, as in Go.)
    pub(crate) existingOverlaps: Rc<Map<NodeRef, Set<NodeRef>>>,
    pub(crate) existingExactOverlaps: Rc<Map<NodeRef, Set<NodeRef>>>,
    pub(crate) captureTopology: bool,
    pub(crate) captureEdgeRoutes: bool,

    pub(crate) graph: Option<graphSnapshot>,
    pub(crate) nodes: Map<NodeRef, nodeSnapshot>,
    pub(crate) edges: Map<EdgeRef, edgeSnapshot>,
    pub(crate) clusters: Map<ClusterRef, clusterSnapshot>,
    pub(crate) sequences: Map<SequenceRef, sequenceSnapshot>,
    pub(crate) trees: Map<TreeRef, treeSnapshot>,
    pub(crate) edgeAbductions: Map<EdgeAbductionRef, EdgeAbduction>,
    pub(crate) herds: Map<HerdAssignmentRef, herdSnapshot>,
    pub(crate) hierarchies: Map<HierarchyRef, hierarchySnapshot>,
}

impl Default for GraphState {
    fn default() -> Self {
        GraphState {
            nodeGeometry: Map::default(),
            originalNodes: Vec::new(),
            hasFixedTopLeft: false,
            clusterArrangements: Map::default(),
            clusterDesired: Map::default(),
            clusterPaddings: Map::default(),
            edgeGeometry: None,
            treeOrientations: Map::default(),
            existingOverlaps: Rc::new(Map::default()),
            existingExactOverlaps: Rc::new(Map::default()),
            captureTopology: false,
            captureEdgeRoutes: false,
            graph: None,
            nodes: Map::default(),
            edges: Map::default(),
            clusters: Map::default(),
            sequences: Map::default(),
            trees: Map::default(),
            edgeAbductions: Map::default(),
            herds: Map::default(),
            hierarchies: Map::default(),
        }
    }
}

gocompat::go_ptr!(GraphStateRef, GraphState);

/// graphGeometrySnapshot keeps geometry in one value for speculative commit
/// checks and preserves the original position pointer for rollback.
#[derive(Clone)]
pub(crate) struct graphGeometrySnapshot {
    pub(crate) topLeft: pointerSnapshot<geo::PointRef, geo::Point>,
    pub(crate) width: f64,
    pub(crate) height: f64,
}

#[derive(Clone)]
pub(crate) struct transactionPendingDescendant {
    pub(crate) node: NodeRef,
    pub(crate) emit: bool,
}

/// A transaction op (Go `func() error`).
pub type TransactionOp = Rc<RefCell<dyn FnMut() -> GoResult<()>>>;

pub struct Transaction {
    pub Ops: Vec<TransactionOp>,
    pub Graph: GraphRef,
    pub(crate) options: TransactionOptions,
    pub(crate) guard: Option<limits::WorkGuardRef>,

    pub PriorGraphState: Option<GraphStateRef>,
    pub(crate) spareGraphState: Option<GraphStateRef>,
    // placementCostSnapshot is optional stage-owned rollback data. Ordinary
    // speculative Rollback deliberately ignores it.
    pub(crate) placementCostSnapshot: Option<PlacementCostSnapshotRef>,

    pub(crate) dirtyNodes: Set<NodeRef>,
    pub(crate) dirtyNodeMarks: Vec<bool>,
    pub(crate) fixedOrigins: Map<Option<NodeRef>, geo::Point>,

    pub(crate) overlapCandidates: Vec<Vec<usize>>,
    pub(crate) overlapSweepNodes: Vec<transactionSweepNode>,
    pub(crate) overlapActiveNodes: Vec<transactionSweepNode>,
    pub(crate) overlapNodeIndices: Option<Map<NodeRef, Vec<usize>>>,
    pub(crate) overlapNodeIndexReady: bool,
    // Geometry trial clones can share an immutable rollback point. Their first
    // accepted UpdateState must not recycle that shared state as scratch.
    pub(crate) priorGraphStateShared: bool,
    pub(crate) overlapInvalidBoxes: Vec<bool>,
    pub(crate) exceptionMarks: Vec<u32>,
    pub(crate) exceptionGeneration: u32,
    pub(crate) descendantStack: Vec<transactionPendingDescendant>,
    pub(crate) descendantNodes: Vec<NodeRef>,
    pub(crate) descendantSeen: Map<NodeRef, u32>,
    pub(crate) descendantGeneration: u32,
}

impl Transaction {
    pub(crate) fn new(graph: GraphRef) -> Transaction {
        Transaction {
            Ops: Vec::new(),
            Graph: graph,
            options: TransactionOptions::default(),
            guard: None,
            PriorGraphState: None,
            spareGraphState: None,
            placementCostSnapshot: None,
            dirtyNodes: Set::default(),
            dirtyNodeMarks: Vec::new(),
            fixedOrigins: Map::default(),
            overlapCandidates: Vec::new(),
            overlapSweepNodes: Vec::new(),
            overlapActiveNodes: Vec::new(),
            overlapNodeIndices: None,
            overlapNodeIndexReady: false,
            priorGraphStateShared: false,
            overlapInvalidBoxes: Vec::new(),
            exceptionMarks: Vec::new(),
            exceptionGeneration: 0,
            descendantStack: Vec::new(),
            descendantNodes: Vec::new(),
            descendantSeen: Map::default(),
            descendantGeneration: 0,
        }
    }
}

gocompat::go_ptr!(TransactionRef, Transaction);

#[derive(Clone, Copy, Debug, Default)]
pub struct TransactionOptions {
    pub AffectContainers: bool,
    pub IgnoreContainerEscape: bool,
    /// AffectEdgeRoutes records route slice and point identity for operations
    /// that move already-routed edges together with nodes.
    pub AffectEdgeRoutes: bool,
}

/// Go `transactionWorkGuardKey{}`.
pub(crate) static transactionWorkGuardKey: CtxKey = CtxKey::new("layoutgraph.transactionWorkGuardKey");

pub(crate) fn contextWithTransactionWorkGuard(ctx: &Ctx, guard: Option<&limits::WorkGuardRef>) -> Ctx {
    match guard {
        None => ctx.clone(),
        Some(g) => ctx.WithValue(&transactionWorkGuardKey, g.clone()),
    }
}

pub(crate) fn existingTransactionWorkGuard(ctx: &Ctx, _location: &str) -> GoResult<(Option<limits::WorkGuardRef>, bool)> {
    if let Some(guard) = ctx.ValueAs::<limits::WorkGuardRef>(&transactionWorkGuardKey) {
        guard.Finish()?;
        return Ok((Some(guard), true));
    }
    Ok((None, false))
}

pub(crate) fn ensureTransactionWorkGuard(ctx: &Ctx, location: &str) -> GoResult<(Ctx, limits::WorkGuardRef)> {
    let (guard, exists) = existingTransactionWorkGuard(ctx, location)?;
    if exists {
        return Ok((ctx.clone(), guard.unwrap()));
    }
    let guard = limits::NewWorkGuard(ctx, location, maxTransactionWorkUnits)?;
    let derived = ctx.WithValue(&transactionWorkGuardKey, guard.clone());
    Ok((derived, guard))
}

impl GraphRef {
    pub(crate) fn newRequestTransaction(&self, ctx: &Ctx, options: TransactionOptions) -> GoResult<TransactionRef> {
        let (_, guard) = ensureTransactionWorkGuard(ctx, "Transaction")?;
        self.newTransactionWithOptionsContext(ctx, options, Some(&guard))
    }

    pub(crate) fn newRequestTransactionWithGuard(
        &self,
        ctx: &Ctx,
        guard: Option<&limits::WorkGuardRef>,
        options: TransactionOptions,
    ) -> GoResult<TransactionRef> {
        let Some(guard) = guard else {
            return Err(errorf!("TALA transaction requires a shared work guard"));
        };
        self.newTransactionWithOptionsContext(ctx, options, Some(guard))
    }
}

impl GraphStateRef {
    pub(crate) fn updateContext(&self, g: &GraphRef, guard: &limits::WorkGuardRef) -> GoResult<()> {
        let (gnodes, nEdges, containers, clusters, sequences, counts) = {
            let gb = g.b();
            (
                gb.Nodes.clone(),
                gb.Edges.len(),
                gb.Containers.clone(),
                gb.Clusters.clone(),
                gb.Sequences.clone(),
                [
                    ("container", gb.Containers.len()),
                    ("cluster", gb.Clusters.len()),
                    ("sequence", gb.Sequences.len()),
                    ("tree", gb.Trees.len()),
                    ("node-tree", gb.NodeToTree.len()),
                    ("hub", gb.Hubs.len()),
                    ("direction", gb.Directions.len()),
                    ("sibling", gb.CommonUncleSiblings.len()),
                ],
            )
        };
        if gnodes.len() as i64 > maxEngineNodes {
            return Err(errorf!("TALA transaction node count exceeds limit {}", maxEngineNodes));
        }
        if nEdges as i64 > maxEngineEdges {
            return Err(errorf!("TALA transaction edge count exceeds limit {}", maxEngineEdges));
        }
        for (name, count) in counts.iter() {
            if *count as i64 > maxEngineNodes + 1 {
                return Err(errorf!("TALA transaction {} map exceeds limit {}", name, maxEngineNodes + 1));
            }
        }

        let mut nodeGeometry: Map<NodeRef, graphGeometrySnapshot> = Map::default();
        let mut hasFixedTopLeft = false;
        let mut clusterArrangements: Map<ClusterRef, ClusterArrangement> = Map::default();
        let mut clusterDesired: Map<ClusterRef, ClusterArrangement> = Map::default();
        let mut clusterPaddings: Map<ClusterRef, f64> = Map::default();

        let originalNodes = gnodes.clone();

        // recordNode / recordDescendants
        let mut recordDescendants = |root: Option<&NodeRef>,
                                     nodeGeometry: &mut Map<NodeRef, graphGeometrySnapshot>,
                                     hasFixedTopLeft: &mut bool|
         -> GoResult<()> {
            let mut queue: Vec<Option<NodeRef>> = vec![root.cloned()];
            while let Some(node) = queue.pop() {
                // recordNode
                let recorded = match &node {
                    None => false,
                    Some(n) => {
                        if nodeGeometry.contains_key(n) {
                            false
                        } else {
                            if nodeGeometry.len() as i64 >= maxEngineNodes {
                                return Err(errorf!("TALA transaction unique node snapshot exceeds limit {}", maxEngineNodes));
                            }
                            guard.Step()?;
                            let (topLeft, width, height, fixed) = {
                                let nb = n.b();
                                let bx = nb.Box.b();
                                (snapshotPoint(bx.TopLeft.as_ref()), bx.Width, bx.Height, nb.FixedTopLeft.is_some())
                            };
                            nodeGeometry.insert(n.clone(), graphGeometrySnapshot { topLeft, width, height });
                            *hasFixedTopLeft = *hasFixedTopLeft || fixed;
                            true
                        }
                    }
                };
                if !recorded {
                    continue;
                }
                let children = containers.at(&node);
                for _ in &children {
                    guard.Step()?;
                }
                queue.extend(children.into_iter().map(Some));
            }
            Ok(())
        };
        for n in &gnodes {
            recordDescendants(Some(n), &mut nodeGeometry, &mut hasFixedTopLeft)?;
        }
        // Some speculative subgraphs keep descendants outside g.Nodes. Record map
        // roots and values as well so rollback does not depend on slice membership.
        for (container, children) in containers.entries() {
            guard.Step()?;
            recordDescendants(container.as_ref(), &mut nodeGeometry, &mut hasFixedTopLeft)?;
            for child in &children {
                recordDescendants(Some(child), &mut nodeGeometry, &mut hasFixedTopLeft)?;
            }
        }

        // Store cluster policy and nodes.
        for c in clusters.values() {
            guard.Step()?;
            let cl = c.b().clone();
            clusterArrangements.insert(c.clone(), cl.Arrangement.clone());
            clusterDesired.insert(c.clone(), cl.DesiredArrangement.clone());
            clusterPaddings.insert(c.clone(), cl.Padding);
            recordDescendants(Some(&cl.Vessel), &mut nodeGeometry, &mut hasFixedTopLeft)?;
            recordDescendants(cl.Container.as_ref(), &mut nodeGeometry, &mut hasFixedTopLeft)?;
            for n in &cl.Nodes {
                recordDescendants(Some(n), &mut nodeGeometry, &mut hasFixedTopLeft)?;
            }
        }

        // Store sequence nodes that may be temporarily removed from g.Nodes.
        for s in sequences.values() {
            guard.Step()?;
            let sq = s.b().clone();
            recordDescendants(Some(&sq.Vessel), &mut nodeGeometry, &mut hasFixedTopLeft)?;
            recordDescendants(sq.Container.as_ref(), &mut nodeGeometry, &mut hasFixedTopLeft)?;
            for n in &sq.Nodes {
                recordDescendants(Some(n), &mut nodeGeometry, &mut hasFixedTopLeft)?;
            }
        }

        {
            let mut gs = self.bm();
            gs.nodeGeometry = nodeGeometry;
            gs.hasFixedTopLeft = hasFixedTopLeft;
            gs.clusterArrangements = clusterArrangements;
            gs.clusterDesired = clusterDesired;
            gs.clusterPaddings = clusterPaddings;
            gs.originalNodes = originalNodes;
        }

        let captureTopology = self.b().captureTopology;
        if captureTopology {
            self.captureRuntimeStateContext(g, guard)?;
        } else {
            self.captureGeometryStateContext(g, guard)?;
        }
        guard.Finish()
    }
}

impl TransactionRef {
    /// PreservePriorGraphState pins the current rollback point so a later
    /// UpdateState cannot recycle it as scratch.
    pub fn PreservePriorGraphState(&self) -> Option<GraphStateRef> {
        let mut t = self.bm();
        t.priorGraphStateShared = true;
        t.PriorGraphState.clone()
    }

    /// CapturePlacementCosts attaches an optional stage-owned cost rollback point
    /// to this transaction.
    pub fn CapturePlacementCosts(&self, location: &str) -> GoResult<()> {
        let (graph, guard, has) = {
            let t = self.b();
            (t.Graph.clone(), t.guard.clone(), t.placementCostSnapshot.is_some())
        };
        let Some(guard) = guard else {
            return Err(errorf!("TALA {} placement-cost snapshot requires an initialized transaction", location));
        };
        if has {
            return Ok(());
        }
        let cacheEntries = graph.EdgeLengthCacheEntries();
        if cacheEntries as i64 > maxEngineTopologyReferences {
            return Err(errorf!("TALA {} edge-length cache entries exceed limit {}", location, maxEngineTopologyReferences));
        }
        for _ in 0..cacheEntries {
            guard.Step()?;
        }
        let costs = graph.SnapshotPlacementCosts();
        guard.Finish()?;
        self.bm().placementCostSnapshot = Some(costs);
        Ok(())
    }

    /// RestorePlacementCosts restores a captured placement-cost rollback point and
    /// reports whether one was present.
    pub fn RestorePlacementCosts(&self) -> bool {
        let costs = self.bm().placementCostSnapshot.take();
        match costs {
            None => false,
            Some(costs) => {
                costs.Restore();
                true
            }
        }
    }
}

pub(crate) fn transactionSweepPadding(g: &GraphRef, guard: &limits::WorkGuardRef) -> GoResult<f64> {
    let mut padding = TableNodeGap as f64;
    let mut maxMargin = 0.0f64;
    let mut maxLoopOffset = 0.0f64;
    let mut edges: Set<EdgeRef> = Set::default();
    let mut edgeOrder: Vec<EdgeRef> = Vec::new();
    let (gnodes, gedges, containers) = {
        let gb = g.b();
        (gb.Nodes.clone(), gb.Edges.clone(), gb.Containers.clone())
    };
    for node in &gnodes {
        guard.Step()?;
        if node.has_tl() {
            if !transactionNodeHasFiniteBox(Some(node)) {
                // Empty containers deliberately use NaN/+Inf sentinel geometry until
                // their final fit stage.
                if node.b().isContainer && containers.at(&Some(node.clone())).is_empty() {
                    continue;
                }
                let t = node.tlv();
                return Err(invariant::Errorf(format!(
                    "transaction node {} has invalid geometry (x={} y={} width={} height={})",
                    node.id(),
                    t.X,
                    t.Y,
                    node.w(),
                    node.h()
                )));
            }
        }
        let (margin, loops, nedges) = {
            let n = node.b();
            (n.margin, n.LoopOffsets.values().cloned().collect::<Vec<f64>>(), n.Edges.clone())
        };
        for m in [margin.top, margin.right, margin.bottom, margin.left] {
            if !transactionFinite(m) {
                return Err(invariant::Errorf(format!("transaction node {} has non-finite margin", node.id())));
            }
        }
        maxMargin = fmaxN(&[maxMargin, margin.top, margin.right, margin.bottom, margin.left]);
        for offset in loops {
            guard.Step()?;
            if !transactionFinite(offset) {
                return Err(invariant::Errorf(format!("transaction node {} has a non-finite loop offset", node.id())));
            }
            if offset > maxLoopOffset {
                maxLoopOffset = offset;
            }
        }
        // Direct internal callers can temporarily expose an edge through only one
        // endpoint before graph.Edges is synchronized.
        for edge in nedges {
            guard.Step()?;
            if edges.insert(edge.clone()) {
                edgeOrder.push(edge);
            }
        }
    }
    for edge in gedges {
        guard.Step()?;
        if edges.insert(edge.clone()) {
            edgeOrder.push(edge);
        }
    }
    for edge in &edgeOrder {
        guard.Step()?;
        let (mw, mh) = {
            let e = edge.b();
            (e.MinWidth, e.MinHeight)
        };
        padding = fmaxN(&[padding, mw as f64, mh as f64]);
    }
    // deltaTo can combine one directional margin or loop offset from each
    // node in the pair. Twice the graph maximum is therefore a proven broad-
    // phase bound for every exact predicate evaluated below.
    padding = fmaxN(&[padding, 2.0 * maxMargin, (NodeGap as f64) + 2.0 * maxLoopOffset]);
    if !transactionFinite(padding) {
        return Err(invariant::New("transaction overlap padding is non-finite"));
    }
    guard.Finish()?;
    Ok(padding)
}

pub(crate) fn transactionFinite(value: f64) -> bool {
    !value.is_nan() && !value.is_infinite()
}

pub(crate) fn transactionNodeHasFiniteBox(node: Option<&NodeRef>) -> bool {
    let Some(node) = node else { return false };
    let Some(tl) = node.tl() else { return false };
    let t = *tl.b();
    let (w, h) = (node.w(), node.h());
    transactionFinite(t.X)
        && transactionFinite(t.Y)
        && transactionFinite(w)
        && transactionFinite(h)
        && transactionFinite(t.X + w)
        && transactionFinite(t.Y + h)
        && w >= 0.0
        && h >= 0.0
}

#[derive(Clone)]
pub(crate) struct transactionSweepNode {
    pub(crate) node: NodeRef,
    pub(crate) index: usize,
    /// Cached sort keys (TopLeft.X, ID) captured when the sweep node is built.
    pub(crate) x: f64,
    pub(crate) id: EntityID,
}

impl transactionSweepNode {
    fn new(node: &NodeRef, index: usize) -> Self {
        transactionSweepNode { node: node.clone(), index, x: node.x(), id: node.id() }
    }
}

fn compareSweepNodes(left: &transactionSweepNode, right: &transactionSweepNode) -> i64 {
    if left.x != right.x {
        if left.x < right.x {
            return -1;
        } else if right.x < left.x {
            return 1;
        } else {
            return 0;
        }
    }
    if left.id != right.id {
        return if left.id < right.id { -1 } else { 1 };
    }
    if left.index != right.index {
        return if left.index < right.index { -1 } else { 1 };
    }
    0
}

type overlapPairs = Map<NodeRef, Set<NodeRef>>;

pub(crate) fn buildTransactionOverlaps(g: &GraphRef, guard: &limits::WorkGuardRef) -> GoResult<(overlapPairs, overlapPairs)> {
    buildTransactionOverlapsWithReferenceLimit(g, guard, maxTransactionOverlapReferences)
}

pub(crate) fn buildTransactionOverlapsWithReferenceLimit(
    g: &GraphRef,
    guard: &limits::WorkGuardRef,
    referenceLimit: i64,
) -> GoResult<(overlapPairs, overlapPairs)> {
    let _padding = transactionSweepPadding(g, guard)?;
    let padding = _padding;
    let gnodes = g.nodes();
    let mut placed: Vec<transactionSweepNode> = Vec::with_capacity(gnodes.len());
    for (index, node) in gnodes.iter().enumerate() {
        guard.Step()?;
        if node.has_tl() && transactionNodeHasFiniteBox(Some(node)) {
            placed.push(transactionSweepNode::new(node, index));
        }
    }
    guard.Step()?;
    gocompat::slices::SortStableFunc(&mut placed, |l, r| compareSweepNodes(l, r));

    let mut existingOverlaps: overlapPairs = Map::default();
    let mut existingExactOverlaps: overlapPairs = Map::default();
    let mut retainedReferences: i64 = 0;
    let mut addPair = |overlaps: &mut overlapPairs, first: &NodeRef, second: &NodeRef| -> GoResult<()> {
        if referenceLimit < 2 || retainedReferences > referenceLimit - 2 {
            return Err(errorf!("TALA transaction overlap references exceed limit {}", referenceLimit));
        }
        overlaps.entry(first.clone()).or_default().insert(second.clone());
        overlaps.entry(second.clone()).or_default().insert(first.clone());
        retainedReferences += 2;
        Ok(())
    };

    let mut active: Vec<NodeRef> = Vec::new();
    for current in &placed {
        guard.Step()?;
        let cur = &current.node;
        let ct = cur.tlv();
        let (cw, ch) = (cur.w(), cur.h());
        let mut kept: Vec<NodeRef> = Vec::with_capacity(active.len() + 1);
        for other in &active {
            guard.Step()?;
            // g.Nodes is a legacy public slice and malformed direct callers can
            // repeat the same pointer.
            if other == cur {
                continue;
            }
            let ot = other.tlv();
            let (ow, oh) = (other.w(), other.h());
            if ot.X + ow + padding <= ct.X {
                continue;
            }
            kept.push(other.clone());
            if ot.Y >= ct.Y + ch + padding || ct.Y >= ot.Y + oh + padding {
                continue;
            }
            if cur.DoesOverlapExact(other) {
                addPair(&mut existingExactOverlaps, cur, other)?;
            }
            // Preserve the old ordered-pair semantics even if a malformed direct
            // caller has asymmetric edge references.
            if cur.doesOverlap(other) || other.doesOverlap(cur) {
                addPair(&mut existingOverlaps, cur, other)?;
            }
        }
        kept.push(cur.clone());
        active = kept;
    }
    guard.Finish()?;
    Ok((existingOverlaps, existingExactOverlaps))
}

impl GraphRef {
    /// newTransactionWithOptionsContext constructs a transaction using a request-scoped
    /// work guard so production stages can share one aggregate budget across all
    /// speculative candidates.
    pub(crate) fn newTransactionWithOptionsContext(
        &self,
        ctx: &Ctx,
        options: TransactionOptions,
        guard: Option<&limits::WorkGuardRef>,
    ) -> GoResult<TransactionRef> {
        let guard = match guard {
            Some(g) => g.clone(),
            None => {
                let (_, g) = ensureTransactionWorkGuard(ctx, "Transaction")?;
                g
            }
        };
        guard.Finish()?;
        let graphState = GraphStateRef::new(GraphState { captureEdgeRoutes: options.AffectEdgeRoutes, ..GraphState::default() });
        graphState.updateContext(self, &guard)?;

        let (existingOverlaps, existingExactOverlaps) = buildTransactionOverlaps(self, &guard)?;

        {
            let mut gs = graphState.bm();
            gs.existingOverlaps = Rc::new(existingOverlaps);
            gs.existingExactOverlaps = Rc::new(existingExactOverlaps);
        }
        let mut txn = Transaction::new(self.clone());
        txn.options = options;
        txn.guard = Some(guard.clone());
        txn.PriorGraphState = Some(graphState);

        guard.Finish()?;
        Ok(TransactionRef::new(txn))
    }
}

impl TransactionRef {
    pub fn UpdateState(&self) -> GoResult<()> {
        // Existing-overlap exceptions intentionally remain anchored to transaction
        // construction. Reusing a transaction updates the rollback point, not which
        // preexisting collisions are allowed.
        let (previous, spare, graph, guard) = {
            let t = self.b();
            (t.PriorGraphState.clone().expect("nil PriorGraphState"), t.spareGraphState.clone(), t.Graph.clone(), t.guard.clone())
        };
        let updated = spare.unwrap_or_else(|| GraphStateRef::new(GraphState::default()));
        {
            let (ct, cr) = {
                let p = previous.b();
                (p.captureTopology, p.captureEdgeRoutes)
            };
            let mut u = updated.bm();
            u.captureTopology = ct;
            u.captureEdgeRoutes = cr;
        }
        let guard = guard.expect("nil pointer dereference: transaction guard");
        if let Err(err) = updated.updateContext(&graph, &guard) {
            // updateContext only mutates the spare snapshot. The prior rollback point
            // remains exact, so a failed refresh can reject the accepted candidate.
            self.Rollback();
            return Err(err);
        }
        {
            let (eo, eeo) = {
                let p = previous.b();
                (p.existingOverlaps.clone(), p.existingExactOverlaps.clone())
            };
            let mut u = updated.bm();
            u.existingOverlaps = eo;
            u.existingExactOverlaps = eeo;
        }
        let mut t = self.bm();
        t.PriorGraphState = Some(updated);
        if t.priorGraphStateShared {
            t.spareGraphState = None;
            t.priorGraphStateShared = false;
        } else {
            t.spareGraphState = Some(previous);
        }
        Ok(())
    }

    pub fn AddOp(&self, f: impl FnMut() -> GoResult<()> + 'static) {
        self.bm().Ops.push(Rc::new(RefCell::new(f)));
    }

    pub(crate) fn repositionContainers(&self, ctx: &Ctx) -> GoResult<()> {
        let (graph, guard) = {
            let t = self.b();
            (t.Graph.clone(), t.guard.clone().unwrap())
        };
        let containers = graph.b().Containers.keys();
        let mut smallestToLargest: Vec<NodeRef> = Vec::with_capacity(containers.len());
        for container in containers {
            guard.Step()?;
            let Some(container) = container else { continue };
            if !container.has_tl() {
                continue;
            }
            smallestToLargest.push(container);
        }
        let keys: Vec<(f64, EntityID, NodeRef)> = smallestToLargest.iter().map(|n| (n.area(), n.id(), n.clone())).collect();
        let mut keys = keys;
        gocompat::sort::Slice(&mut keys, |a, b| {
            if a.0 == b.0 {
                return a.1 < b.1;
            }
            a.0 < b.0
        });

        for (_, _, container) in &keys {
            if let Some(err) = ctx.Err() {
                return Err(err);
            }
            container.wrapChildren();
            guard.Step()?;
        }
        guard.Finish()
    }
}

impl GraphStateRef {
    pub(crate) fn hasOriginalNodeOrder(&self, g: &GraphRef) -> bool {
        let gs = self.b();
        let gb = g.b();
        if gs.originalNodes.len() != gb.Nodes.len() {
            return false;
        }
        for (i, node) in gb.Nodes.iter().enumerate() {
            if gs.originalNodes[i] != *node {
                return false;
            }
        }
        true
    }

    pub(crate) fn geometryChanged(&self, node: &NodeRef) -> bool {
        let gs = self.b();
        let Some(original) = gs.nodeGeometry.get(node) else {
            return true;
        };
        let (tl, w, h) = {
            let n = node.b();
            let bx = n.Box.b();
            (bx.TopLeft.clone(), bx.Width, bx.Height)
        };
        match tl {
            None => original.topLeft.pointer.is_some() || w != original.width || h != original.height,
            Some(tl) => {
                let t = *tl.b();
                original.topLeft.pointer.is_none()
                    || t.X != original.topLeft.value.X
                    || t.Y != original.topLeft.value.Y
                    || w != original.width
                    || h != original.height
            }
        }
    }
}

pub(crate) fn transactionBoxesMayInteract(first: &NodeRef, second: &NodeRef) -> bool {
    if first == second {
        return false;
    }
    let (Some(ftl), Some(stl)) = (first.tl(), second.tl()) else {
        return false;
    };
    let f = *ftl.b();
    let s = *stl.b();
    let (fw, fh) = (first.w(), first.h());
    let (sw, sh) = (second.w(), second.h());
    // This is the same symmetric broad phase used by
    // doesOverlapWithDimensionsContext. NaN values deliberately survive the
    // comparisons and are sent to the normal bad-state path.
    const maxSafeDelta: f64 = 500.0;
    !(f.X > s.X + sw + maxSafeDelta || f.X + fw + maxSafeDelta < s.X || f.Y > s.Y + sh + maxSafeDelta || f.Y + fh + maxSafeDelta < s.Y)
}

impl TransactionRef {
    pub(crate) fn collectFixedOrigins(&self, ctx: &Ctx) -> GoResult<()> {
        let (graph, guard, prior) = {
            let t = self.b();
            (t.Graph.clone(), t.guard.clone().unwrap(), t.PriorGraphState.clone().unwrap())
        };
        self.bm().fixedOrigins.clear();
        if !prior.b().hasFixedTopLeft {
            return guard.Finish();
        }
        for node in graph.nodes() {
            guard.Step()?;
            if let Some(err) = ctx.Err() {
                return Err(err);
            }
            let (Some(tl), Some(ftl)) = (node.tl(), node.fixedTL()) else {
                continue;
            };
            let container = node.container();
            let mut t = self.bm();
            if t.fixedOrigins.contains_key(&container) {
                continue;
            }
            let (tv, fv) = (*tl.b(), *ftl.b());
            t.fixedOrigins.insert(container, geo::Point { X: tv.X - fv.X, Y: tv.Y - fv.Y });
        }
        guard.Finish()
    }

    /// collectOverlapValidationNodes records geometry changes for the legacy
    /// non-exact-to-exact regression check.
    pub(crate) fn collectOverlapValidationNodes(&self, ctx: &Ctx) -> GoResult<bool> {
        let (graph, guard, prior) = {
            let t = self.b();
            (t.Graph.clone(), t.guard.clone().unwrap(), t.PriorGraphState.clone().unwrap())
        };
        if !prior.hasOriginalNodeOrder(&graph) {
            return Ok(false);
        }
        let gnodes = graph.nodes();
        {
            let mut t = self.bm();
            t.dirtyNodes.clear();
            t.dirtyNodeMarks.clear();
            t.dirtyNodeMarks.resize(gnodes.len(), false);
        }

        for (index, node) in gnodes.iter().enumerate() {
            guard.Step()?;
            if let Some(err) = ctx.Err() {
                return Err(err);
            }
            if prior.geometryChanged(node) {
                let mut t = self.bm();
                t.dirtyNodes.insert(node.clone());
                t.dirtyNodeMarks[index] = true;
            }
        }
        guard.Finish()?;
        Ok(true)
    }

    /// buildPostStateOverlapCandidates builds the exact 500-unit broad phase used
    /// by doesOverlapWithDimensionsContext.
    pub(crate) fn buildPostStateOverlapCandidates(&self, ctx: &Ctx, dirtyOnly: bool) -> GoResult<()> {
        let (graph, guard) = {
            let t = self.b();
            (t.Graph.clone(), t.guard.clone().unwrap())
        };
        let gnodes = graph.nodes();
        let nodeCount = gnodes.len();
        if nodeCount as i64 > maxEngineNodes {
            return Err(errorf!("TALA transaction node count exceeds limit {}", maxEngineNodes));
        }
        let mut tb = self.bm();
        let t = &mut *tb;
        t.overlapCandidates.resize_with(nodeCount, Vec::new);
        t.overlapCandidates.truncate(nodeCount);
        for c in t.overlapCandidates.iter_mut() {
            c.clear();
        }
        t.overlapInvalidBoxes.clear();
        t.overlapInvalidBoxes.resize(nodeCount, false);
        // exceptionMarks keeps its previous contents (generation counter).
        t.exceptionMarks.resize(nodeCount, 0);

        let mut rebuildNodeIndex = !dirtyOnly || !t.overlapNodeIndexReady;
        if t.overlapNodeIndices.is_none() {
            t.overlapNodeIndices = Some(Map::default());
            rebuildNodeIndex = true;
        } else if rebuildNodeIndex {
            for indices in t.overlapNodeIndices.as_mut().unwrap().values_mut() {
                indices.clear();
            }
        }
        t.overlapSweepNodes.clear();
        let mut hasInvalidBox = false;
        let mut dirtyCount = 0usize;
        for (index, node) in gnodes.iter().enumerate() {
            guard.Step()?;
            if let Some(err) = ctx.Err() {
                return Err(err);
            }
            if rebuildNodeIndex {
                t.overlapNodeIndices.as_mut().unwrap().entry(node.clone()).or_default().push(index);
            }
            if dirtyOnly && t.dirtyNodeMarks[index] {
                dirtyCount += 1;
            }
            if !node.has_tl() {
                continue;
            }
            if !transactionNodeHasFiniteBox(Some(node)) {
                t.overlapInvalidBoxes[index] = true;
                hasInvalidBox = true;
                continue;
            }
            t.overlapSweepNodes.push(transactionSweepNode::new(node, index));
        }
        t.overlapNodeIndexReady = dirtyOnly;

        let mut retainedReferences: i64 = 0;
        let addPair = |cands: &mut Vec<Vec<usize>>, retained: &mut i64, first: usize, second: usize| -> GoResult<()> {
            if *retained > maxTransactionOverlapReferences - 2 {
                return Err(errorf!("TALA transaction overlap references exceed limit {}", maxTransactionOverlapReferences));
            }
            cands[first].push(second);
            cands[second].push(first);
            *retained += 2;
            Ok(())
        };

        // A sparse geometry trial is cheapest as changed-nodes x graph. When many
        // boxes move, the sweep avoids scanning pairs that are far apart.
        let useDirtyScan = dirtyOnly && dirtyCount * 4 < nodeCount;
        if useDirtyScan {
            for (first, firstNode) in gnodes.iter().enumerate() {
                if !t.dirtyNodeMarks[first] || !firstNode.has_tl() {
                    continue;
                }
                for (second, secondNode) in gnodes.iter().enumerate() {
                    guard.Step()?;
                    if second == first || firstNode == secondNode || !secondNode.has_tl() {
                        continue;
                    }
                    if t.dirtyNodeMarks[second] && second < first {
                        continue;
                    }
                    if !transactionBoxesMayInteract(firstNode, secondNode) {
                        continue;
                    }
                    addPair(&mut t.overlapCandidates, &mut retainedReferences, first, second)?;
                }
            }
        } else {
            gocompat::slices::SortStableFunc(&mut t.overlapSweepNodes, |l, r| compareSweepNodes(l, r));
            t.overlapActiveNodes.clear();
            let sweep = t.overlapSweepNodes.clone();
            for current in &sweep {
                guard.Step()?;
                let cx = current.node.x();
                let mut kept: Vec<transactionSweepNode> = Vec::with_capacity(t.overlapActiveNodes.len() + 1);
                let active = std::mem::take(&mut t.overlapActiveNodes);
                for other in &active {
                    guard.Step()?;
                    // Equality remains active because the legacy broad phase only rejects
                    // a pair when there is a strict gap greater than 500.
                    if other.node.x() + other.node.w() + 500.0 < cx {
                        continue;
                    }
                    kept.push(other.clone());
                    if other.node == current.node || !transactionBoxesMayInteract(&other.node, &current.node) {
                        continue;
                    }
                    if dirtyOnly && !t.dirtyNodeMarks[other.index] && !t.dirtyNodeMarks[current.index] {
                        continue;
                    }
                    addPair(&mut t.overlapCandidates, &mut retainedReferences, other.index, current.index)?;
                }
                kept.push(current.clone());
                t.overlapActiveNodes = kept;
            }

            // Non-finite or negative boxes deliberately follow legacy comparisons,
            // whose NaN behavior cannot be represented by a numeric sweep ordering.
            if hasInvalidBox {
                for first in 0..nodeCount {
                    for second in (first + 1)..nodeCount {
                        if !t.overlapInvalidBoxes[first] && !t.overlapInvalidBoxes[second] {
                            continue;
                        }
                        if dirtyOnly && !t.dirtyNodeMarks[first] && !t.dirtyNodeMarks[second] {
                            continue;
                        }
                        guard.Step()?;
                        let (firstNode, secondNode) = (&gnodes[first], &gnodes[second]);
                        if firstNode == secondNode
                            || !firstNode.has_tl()
                            || !secondNode.has_tl()
                            || !transactionBoxesMayInteract(firstNode, secondNode)
                        {
                            continue;
                        }
                        addPair(&mut t.overlapCandidates, &mut retainedReferences, first, second)?;
                    }
                }
            }
        }

        for candidates in t.overlapCandidates.iter_mut() {
            guard.Step()?;
            candidates.sort_unstable();
        }
        guard.Finish()
    }

    pub(crate) fn markOverlapException(t: &mut Transaction, node: &NodeRef, generation: u32) {
        let Some(indices) = t.overlapNodeIndices.as_ref().and_then(|m| m.get(node)).cloned() else {
            return;
        };
        for index in indices {
            t.exceptionMarks[index] = generation;
        }
    }

    /// markDescendantOverlapExceptions performs the same stable descendant walk as
    /// allDescendantNodesGuarded.
    pub(crate) fn markDescendantOverlapExceptions(
        t: &mut Transaction,
        guard: &limits::WorkGuardRef,
        node: &NodeRef,
        includeClusterNodes: bool,
        generation: u32,
    ) -> GoResult<()> {
        t.descendantGeneration = t.descendantGeneration.wrapping_add(1);
        if t.descendantGeneration == 0 {
            t.descendantSeen.clear();
            t.descendantGeneration = 1;
        }
        let seenGeneration = t.descendantGeneration;
        t.descendantStack.clear();
        t.descendantNodes.clear();
        t.descendantSeen.insert(node.clone(), seenGeneration);

        let (containers, clusters, sequences) = {
            let g = t.Graph.b();
            (g.Containers.clone(), g.Clusters.clone(), g.Sequences.clone())
        };
        let pushChildren = |parent: &NodeRef, stack: &mut Vec<transactionPendingDescendant>| -> GoResult<()> {
            if let Some(sequence) = sequences.get(parent) {
                let snodes = sequence.b().Nodes.clone();
                for i in (0..snodes.len()).rev() {
                    guard.Step()?;
                    stack.push(transactionPendingDescendant { node: snodes[i].clone(), emit: includeClusterNodes });
                }
            }
            if parent.b().isClusterVessel {
                if let Some(cluster) = clusters.get(parent) {
                    let cnodes = cluster.b().Nodes.clone();
                    for i in (0..cnodes.len()).rev() {
                        guard.Step()?;
                        stack.push(transactionPendingDescendant { node: cnodes[i].clone(), emit: includeClusterNodes });
                    }
                }
            }
            if parent.b().isContainer {
                let children = containers.at(&Some(parent.clone()));
                for i in (0..children.len()).rev() {
                    guard.Step()?;
                    stack.push(transactionPendingDescendant { node: children[i].clone(), emit: true });
                }
            }
            Ok(())
        };
        pushChildren(node, &mut t.descendantStack)?;
        while !t.descendantStack.is_empty() {
            guard.Step()?;
            let current = t.descendantStack.pop().unwrap();
            if t.descendantSeen.get(&current.node).copied().unwrap_or(0) == seenGeneration {
                continue;
            }
            t.descendantSeen.insert(current.node.clone(), seenGeneration);
            if current.emit {
                t.descendantNodes.push(current.node.clone());
            }
            pushChildren(&current.node, &mut t.descendantStack)?;
        }
        let descendants = t.descendantNodes.clone();
        for descendant in &descendants {
            guard.Step()?;
            TransactionRef::markOverlapException(t, descendant, generation);
        }
        guard.Finish()
    }

    pub(crate) fn hasBadPostStateOverlap(&self, nodeIndex: usize) -> GoResult<bool> {
        let mut tb = self.bm();
        let t = &mut *tb;
        let guard = t.guard.clone().unwrap();
        let graph = t.Graph.clone();
        let node = graph.b().Nodes[nodeIndex].clone();
        let Some(ntl) = node.tl() else {
            return Ok(false);
        };
        let (hasCluster, isContainer, isVessel, hasContainer, hasSequence) = {
            let n = node.b();
            (n.Cluster.is_some(), n.isContainer, n.isClusterVessel, n.Container.is_some(), n.Sequence.is_some())
        };
        if hasCluster {
            return Ok(false);
        }

        t.exceptionGeneration = t.exceptionGeneration.wrapping_add(1);
        if t.exceptionGeneration == 0 {
            for m in t.exceptionMarks.iter_mut() {
                *m = 0;
            }
            t.exceptionGeneration = 1;
        }
        let generation = t.exceptionGeneration;
        if isContainer || isVessel || graph.b().Sequences.has(&node) {
            TransactionRef::markDescendantOverlapExceptions(t, &guard, &node, true, generation)?;
        }
        if hasContainer || hasCluster || hasSequence {
            let ancestors = graph.ancestorsOfGuarded(&node, &guard)?;
            for ancestor in &ancestors {
                guard.Step()?;
                TransactionRef::markOverlapException(t, ancestor, generation);
            }
        }

        let prior = t.PriorGraphState.clone().unwrap();
        let existing = prior.b().existingOverlaps.clone();
        let pairwiseExceptions = existing.get(&node);
        let nv = *ntl.b();
        let (nw, nh) = (node.w(), node.h());
        let right = nv.X + nw;
        let bottom = nv.Y + nh;
        let candidates = t.overlapCandidates[nodeIndex].clone();
        let gnodes = graph.nodes();
        for otherIndex in candidates {
            guard.Step()?;
            let other = &gnodes[otherIndex];
            let excepted = pairwiseExceptions.map_or(false, |s| s.contains(other));
            if excepted || t.exceptionMarks[otherIndex] == generation {
                continue;
            }
            if !other.has_tl() || !transactionBoxesMayInteract(&node, other) {
                continue;
            }
            let deltaValue = node.deltaToGuarded(Some(other), Some(&ntl), Some(&guard))?;
            let delta = deltaValue as f64;
            let o = other.tlv();
            let (ow, oh) = (other.w(), other.h());
            if nv.X < o.X + ow + delta && right + delta > o.X && nv.Y < o.Y + oh + delta && bottom + delta > o.Y {
                return Ok(true);
            }
        }
        guard.Finish()?;
        Ok(false)
    }

    pub fn Commit(&self, ctx: &Ctx) -> GoResult<()> {
        // A rejected trial is atomic: callers never observe a half-applied graph.
        let result = self.commitInner(ctx);
        if result.is_err() {
            self.Rollback();
        }
        result
    }

    fn commitInner(&self, ctx: &Ctx) -> GoResult<()> {
        let (guard, ops, options, graph) = {
            let t = self.b();
            (t.guard.clone(), t.Ops.clone(), t.options, t.Graph.clone())
        };
        let Some(guard) = guard else {
            return Err(errorf!("TALA transaction commit requires a work guard"));
        };
        guard.Finish()?;
        if let Some(err) = ctx.Err() {
            return Err(err);
        }
        for op in &ops {
            guard.Step()?;
            if let Some(err) = ctx.Err() {
                return Err(err);
            }
            (op.borrow_mut())()?;
            if let Some(err) = ctx.Err() {
                return Err(err);
            }
            // Syncing clusters will reposition cluster nodes, which may shrink containers
            // But repositioning containers can also expand clusters, since cluster nodes can be containers
            if options.AffectContainers {
                self.repositionContainers(ctx)?;
                let containers = graph.b().Containers.keys();
                for container in containers {
                    guard.Step()?;
                    if let Some(err) = ctx.Err() {
                        return Err(err);
                    }
                    let Some(container) = container else { continue };
                    if !container.has_tl() {
                        continue;
                    }
                    let prior = self.b().PriorGraphState.clone();
                    let bad = graph.isBadStateContext(Some(&container), prior.as_ref(), options.IgnoreContainerEscape, &guard)?;
                    if bad {
                        return Err(ErrInvalidCandidate.err());
                    }
                }
            }
            graph.SyncClusters();
            graph.SyncSequences();
            if let Some(err) = ctx.Err() {
                return Err(err);
            }
        }

        let useDirtyValidation = self.collectOverlapValidationNodes(ctx)?;
        self.collectFixedOrigins(ctx)?;
        // Containment and fixed-position invariants remain graph-wide. Pairwise
        // overlap is validated afterward from one stable post-state sweep.
        let prior = self.b().PriorGraphState.clone();
        let gnodes = graph.nodes();
        for n in &gnodes {
            guard.Step()?;
            if let Some(err) = ctx.Err() {
                return Err(err);
            }
            if !n.has_tl() {
                continue;
            }
            let fixedOrigin = self.b().fixedOrigins.get(&n.container()).copied();
            let bad = graph.isStructurallyBadStateWithFixedOriginContext(
                Some(n),
                prior.as_ref(),
                options.IgnoreContainerEscape,
                fixedOrigin,
                true,
                &guard,
            )?;
            if bad.0 {
                return Err(ErrInvalidCandidate.err());
            }
        }
        self.buildPostStateOverlapCandidates(ctx, useDirtyValidation)?;
        let gnodes = graph.nodes();
        for (nodeIndex, n) in gnodes.iter().enumerate() {
            if self.b().overlapCandidates[nodeIndex].is_empty() {
                continue;
            }
            guard.Step()?;
            if let Some(err) = ctx.Err() {
                return Err(err);
            }
            if !n.has_tl() {
                continue;
            }
            let bad = self.hasBadPostStateOverlap(nodeIndex)?;
            if bad {
                return Err(ErrInvalidCandidate.err());
            }
        }

        // If a non-exact overlap became an exact overlap, that's bad
        let (existing, existingExact) = {
            let p = prior.as_ref().unwrap().b();
            (p.existingOverlaps.clone(), p.existingExactOverlaps.clone())
        };
        let dirtyNodes = self.b().dirtyNodes.clone();
        for (n1, overlaps) in existing.iter() {
            guard.Step()?;
            if let Some(err) = ctx.Err() {
                return Err(err);
            }
            for n2 in overlaps.iter() {
                if useDirtyValidation {
                    let firstChanged = dirtyNodes.contains(n1);
                    let secondChanged = dirtyNodes.contains(n2);
                    if !firstChanged && !secondChanged {
                        continue;
                    }
                }
                guard.Step()?;
                if let Some(err) = ctx.Err() {
                    return Err(err);
                }
                let mut wasExact = false;
                if let Some(ex) = existingExact.get(n1) {
                    if ex.contains(n2) {
                        wasExact = true;
                    }
                }
                if !wasExact && n1.DoesOverlapExact(n2) {
                    return Err(ErrInvalidCandidate.err());
                }
            }
        }

        if let Some(err) = ctx.Err() {
            return Err(err);
        }
        guard.Finish()
    }

    /// Clear only needs to be called if transaction will be reused
    pub fn Clear(&self) {
        self.bm().Ops = Vec::new();
    }

    pub fn Rollback(&self) {
        let (state, graph) = {
            let t = self.b();
            (t.PriorGraphState.clone().expect("nil PriorGraphState"), t.Graph.clone())
        };
        rollbackGraphState(&graph, &state);
    }

    /// CloneGeometryContext creates a nested speculative transaction that may only
    /// move geometry.
    pub fn CloneGeometryContext(&self) -> GoResult<TransactionRef> {
        let (prior, guard, ops, graph, options) = {
            let t = self.b();
            (t.PriorGraphState.clone(), t.guard.clone(), t.Ops.clone(), t.Graph.clone(), t.options)
        };
        let (Some(prior), Some(guard)) = (prior, guard) else {
            return Err(errorf!("TALA geometry transaction clone requires an initialized transaction"));
        };
        if prior.b().captureTopology {
            return Err(errorf!("TALA topology transaction requires an independent clone"));
        }
        guard.Finish()?;
        // The source and clone now have equal read-only ownership of this rollback
        // point.
        self.bm().priorGraphStateShared = true;
        let mut txn = Transaction::new(graph);
        txn.Ops = ops;
        txn.options = options;
        txn.guard = Some(guard);
        txn.PriorGraphState = Some(prior);
        txn.priorGraphStateShared = true;
        Ok(TransactionRef::new(txn))
    }
}

/// Go `Transaction.Rollback` body (shared with `RestoreGraphState`).
pub(crate) fn rollbackGraphState(graph: &GraphRef, stateRef: &GraphStateRef) {
    let state = stateRef.b();
    if state.captureTopology {
        if let Some(g) = &state.graph {
            g.restore(graph);
        }

        for (cluster, snapshot) in state.clusters.iter() {
            *cluster.bm() = snapshot.value.clone();
        }
        for (sequence, snapshot) in state.sequences.iter() {
            *sequence.bm() = snapshot.value.clone();
        }
        for (tree, snapshot) in state.trees.iter() {
            *tree.bm() = snapshot.value.shallowCopy();
        }
        for (abduction, snapshot) in state.edgeAbductions.iter() {
            *abduction.bm() = snapshot.clone();
        }
        for (herd, snapshot) in state.herds.iter() {
            *herd.bm() = snapshot.value.clone();
        }
        for (hierarchy, snapshot) in state.hierarchies.iter() {
            let value = snapshot.value.clone();
            *value.level.bm() = snapshot.level.clone();
            *hierarchy.bm() = value;
        }
        for (edge, snapshot) in state.edges.iter() {
            snapshot.restore(edge);
        }
        for (node, snapshot) in state.nodes.iter() {
            snapshot.restore(node);
        }
        return;
    }

    for (node, geometry) in state.nodeGeometry.iter() {
        let tl = geometry.topLeft.restore();
        let bx = node.boxRef();
        let mut b = bx.bm();
        b.TopLeft = tl;
        b.Width = geometry.width;
        b.Height = geometry.height;
    }
    for (cluster, arrangement) in state.clusterArrangements.iter() {
        let mut c = cluster.bm();
        c.Arrangement = arrangement.clone();
        c.DesiredArrangement = state.clusterDesired.get(cluster).cloned().unwrap_or_default();
        c.Padding = state.clusterPaddings.get(cluster).copied().unwrap_or(0.0);
    }
    if let Some(edgeGeometry) = &state.edgeGeometry {
        for (edge, snapshot) in edgeGeometry.iter() {
            snapshot.restore(edge);
        }
    }
    for (tree, orientation) in state.treeOrientations.iter() {
        tree.bm().Orientation = *orientation;
    }
    // GO-ALIAS: Go copies the snapshot back into the captured slice header's
    // backing array (`copy(originalNodesRef, originalNodes)`) and reinstalls it.
    // Rust vectors do not alias, so the restored order is installed directly.
    graph.bm().Nodes = state.originalNodes.clone();
}
