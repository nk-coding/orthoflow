// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/routing/route_stage_guard.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of routing/route_stage_guard.go.
#[allow(unused_imports)]
use crate::*;
use gocompat::context::{self, Ctx, CtxKey};
use gocompat::math;
use gocompat::{GoError, GoResult, Map, Sentinel};
use layoutgraph::{EdgeRef, GraphRef, LabelRef, NodeRef, RoutingCostState};
use std::rc::Rc;

// Route post-processing contains several intentionally quadratic searches.
// Bound the aggregate work independently of adapter validation because the
// engine also has direct callers.
pub(crate) const maxRouteStageWorkUnits: u64 = 50_000_000;
pub(crate) const routeStageContextCheckStride: u64 = 1024;
// EdgeRoutingStage owns OVG construction, up to three concurrent route
// flavors, and its stage-level discovery/rollback work. This aggregate cap
// is the sum of those independently calibrated envelopes; unlike the nested
// caps, it is shared across every disconnected subgraph.
pub(crate) const maxEdgeRoutingStageWorkUnits: u64 =
    maxOVGWorkUnits + maxRouteSearchFlavors * maxRouteSearchWorkUnits + maxRouteStageWorkUnits;

pub(crate) static errRouteStageWorkLimit: Sentinel = Sentinel::new("TALA route-stage work limit exceeded");

/// routeWorkGuard is shared by every helper in a route post-processing stage.
/// Work is unsigned and checked before addition, so even injected limits and
/// hostile inputs cannot wrap the counter.
///
/// Go's parallel (atomic) path is behaviourally identical to the sequential
/// one; the port runs route flavors sequentially, so only one path exists.
pub struct routeWorkGuard {
    pub(crate) parallel: bool,
    pub(crate) ctx: Ctx,
    pub(crate) done: Option<context::Done>,
    pub(crate) location: String,
    pub(crate) used: u64,
    pub(crate) limit: u64,
}

gocompat::go_ptr!(routeWorkGuardRef, routeWorkGuard);

pub(crate) fn newRouteWorkGuard(ctx: &Ctx, location: &str, limit: u64) -> GoResult<routeWorkGuardRef> {
    let guard = routeWorkGuardRef::new(routeWorkGuard {
        parallel: false,
        ctx: ctx.clone(),
        done: ctx.Done(),
        location: location.to_string(),
        used: 0,
        limit,
    });
    guard.check()?;
    Ok(guard)
}

impl routeWorkGuardRef {
    #[inline]
    pub(crate) fn step(&self) -> GoResult<()> {
        self.add(1)
    }

    #[inline]
    pub(crate) fn add(&self, units: u64) -> GoResult<()> {
        let (used, limit) = {
            let g = self.b();
            (g.used, g.limit)
        };
        if used > limit || units > limit - used {
            self.check()?;
            let location = self.location();
            return Err(GoError::wrap(
                format!("{}: TALA {} work exceeds limit {}", errRouteStageWorkLimit.Error(), location, limit),
                errRouteStageWorkLimit.err(),
            ));
        }
        let previous = used;
        let now = used + units;
        self.bm().used = now;
        if units == 0 || previous / routeStageContextCheckStride != now / routeStageContextCheckStride {
            return self.check();
        }
        Ok(())
    }

    pub(crate) fn enableParallel(&self) {
        self.bm().parallel = true;
    }

    pub(crate) fn check(&self) -> GoResult<()> {
        let err = {
            let g = self.b();
            cachedContextErr(&g.ctx, &g.done)
        };
        if let Some(err) = err {
            return Err(GoError::wrap(format!("{}: {}", self.location(), err), err));
        }
        Ok(())
    }

    pub(crate) fn finish(&self) -> GoResult<()> {
        self.check()
    }

    pub(crate) fn location(&self) -> String {
        self.b().location.clone()
    }

    pub(crate) fn ctx(&self) -> Ctx {
        self.b().ctx.clone()
    }

    pub(crate) fn used(&self) -> u64 {
        self.b().used
    }

    // Step and Finish let routing share its exact aggregate budget with lower
    // layout-domain kernels without exposing the guard's counters or limits.
    pub fn Step(&self) -> GoResult<()> {
        self.step()
    }
    pub fn Finish(&self) -> GoResult<()> {
        self.finish()
    }
}

impl workBudget for routeWorkGuardRef {
    fn step(&self) -> GoResult<()> {
        routeWorkGuardRef::step(self)
    }
    fn add(&self, units: u64) -> GoResult<()> {
        routeWorkGuardRef::add(self, units)
    }
    fn check(&self) -> GoResult<()> {
        routeWorkGuardRef::check(self)
    }
    fn asRouteWorkGuard(&self) -> Option<&routeWorkGuardRef> {
        Some(self)
    }
}

impl layoutgraph::SharedWorkStepper for routeWorkGuardRef {
    fn Step(&self) -> GoResult<()> {
        self.step()
    }
    fn Finish(&self) -> GoResult<()> {
        self.finish()
    }
}

pub(crate) static routeAggregateWorkKey: CtxKey = CtxKey::new("routeAggregateWorkKey");

/// The context value: Go stores the `workBudget` interface value.
#[derive(Clone)]
pub(crate) struct routeAggregateWork(pub(crate) workBudgetRef);

pub(crate) fn contextWithRouteAggregateWork(ctx: &Ctx, guard: Option<workBudgetRef>) -> Ctx {
    match guard {
        None => ctx.clone(),
        Some(guard) => ctx.WithValue(&routeAggregateWorkKey, routeAggregateWork(guard)),
    }
}

pub(crate) fn routeAggregateWorkFromContext(ctx: &Ctx) -> Option<workBudgetRef> {
    ctx.ValueAs::<routeAggregateWork>(&routeAggregateWorkKey).map(|w| w.0)
}

// ---------------------------------------------------------------------------
// Route mutation snapshots. Go builds these from snapshot.go's pointerSnapshot
// and routeEdgeSnapshot; the port captures the same state directly. Go's
// exactSliceSnapshot (restoring a shared backing array) has no counterpart
// because Rust route vectors never alias.
// ---------------------------------------------------------------------------

/// Every exported field of a `layoutgraph.Edge` value (Go `*edge` copy).
#[derive(Clone)]
pub(crate) struct edgeFieldValues {
    D2ID: Option<String>,
    ID: layoutgraph::EntityID,
    From: NodeRef,
    To: NodeRef,
    Points: Vec<geo::PointRef>,
    SourceArrowhead: layoutgraph::Arrowhead,
    TargetArrowhead: layoutgraph::Arrowhead,
    SourceArrowheadLabel: Option<LabelRef>,
    TargetArrowheadLabel: Option<LabelRef>,
    MinWidth: i64,
    MinHeight: i64,
    Label: Option<LabelRef>,
    LabelPercentage: f64,
    IsCurve: bool,
    FromTableColumnIndex: Option<i64>,
    ToTableColumnIndex: Option<i64>,
    IsInvisible: bool,
    Style: layoutgraph::EdgeStyle,
}

impl edgeFieldValues {
    pub(crate) fn capture(edge: &EdgeRef) -> edgeFieldValues {
        let e = edge.b();
        edgeFieldValues {
            D2ID: e.D2ID.clone(),
            ID: e.ID,
            From: e.From.clone(),
            To: e.To.clone(),
            Points: e.Points.clone(),
            SourceArrowhead: e.SourceArrowhead.clone(),
            TargetArrowhead: e.TargetArrowhead.clone(),
            SourceArrowheadLabel: e.SourceArrowheadLabel.clone(),
            TargetArrowheadLabel: e.TargetArrowheadLabel.clone(),
            MinWidth: e.MinWidth,
            MinHeight: e.MinHeight,
            Label: e.Label.clone(),
            LabelPercentage: e.LabelPercentage,
            IsCurve: e.IsCurve,
            FromTableColumnIndex: e.FromTableColumnIndex,
            ToTableColumnIndex: e.ToTableColumnIndex,
            IsInvisible: e.IsInvisible,
            Style: e.Style.clone(),
        }
    }

    pub(crate) fn apply(&self, edge: &EdgeRef) {
        let mut e = edge.bm();
        e.D2ID = self.D2ID.clone();
        e.ID = self.ID;
        e.From = self.From.clone();
        e.To = self.To.clone();
        e.Points = self.Points.clone();
        e.SourceArrowhead = self.SourceArrowhead.clone();
        e.TargetArrowhead = self.TargetArrowhead.clone();
        e.SourceArrowheadLabel = self.SourceArrowheadLabel.clone();
        e.TargetArrowheadLabel = self.TargetArrowheadLabel.clone();
        e.MinWidth = self.MinWidth;
        e.MinHeight = self.MinHeight;
        e.Label = self.Label.clone();
        e.LabelPercentage = self.LabelPercentage;
        e.IsCurve = self.IsCurve;
        e.FromTableColumnIndex = self.FromTableColumnIndex;
        e.ToTableColumnIndex = self.ToTableColumnIndex;
        e.IsInvisible = self.IsInvisible;
        e.Style = self.Style.clone();
    }
}

/// Go `candidate := *edge; &candidate`: a new edge holding a shallow value copy.
pub(crate) fn edgeValueCopy(edge: &EdgeRef) -> EdgeRef {
    let (from, to) = (edge.from(), edge.to());
    let copy = layoutgraph::NewEdge(&from, &to);
    edgeFieldValues::capture(edge).apply(&copy);
    copy
}

pub(crate) struct routeLabelSnapshot {
    pointer: Option<LabelRef>,
    value: Option<layoutgraph::Label>,
}

impl routeLabelSnapshot {
    fn new(pointer: &Option<LabelRef>) -> routeLabelSnapshot {
        routeLabelSnapshot { pointer: pointer.clone(), value: pointer.as_ref().map(|l| l.b().clone()) }
    }
    fn restore(&self) {
        if let (Some(p), Some(v)) = (&self.pointer, &self.value) {
            *p.bm() = v.clone();
        }
    }
}

pub(crate) struct routeEdgeSnapshot {
    value: edgeFieldValues,
    pointValues: Vec<(geo::PointRef, geo::Point)>,
    label: routeLabelSnapshot,
    sourceArrowheadLabel: routeLabelSnapshot,
    targetArrowheadLabel: routeLabelSnapshot,
}

impl routeEdgeSnapshot {
    fn restore(&self, edge: &EdgeRef) {
        self.label.restore();
        self.sourceArrowheadLabel.restore();
        self.targetArrowheadLabel.restore();
        for (pointer, value) in &self.pointValues {
            pointer.set(*value);
        }
        self.value.apply(edge);
    }
}

pub(crate) struct routeMutationSnapshot {
    graph: Option<GraphRef>,
    cellSize: f64,
    costs: RoutingCostState,
    /// Go `map[*Node]pointerSnapshot[geo.Point]` (+ nodeGraphs), kept in
    /// capture order (Go restores in map order; restores are independent).
    pub(crate) nodes: Vec<(NodeRef, Option<geo::PointRef>, Option<geo::Point>, Option<GraphRef>)>,
    pub(crate) nodeSet: Map<NodeRef, ()>,
    pub(crate) edges: Vec<(EdgeRef, routeEdgeSnapshot)>,
    pub(crate) edgeSet: Map<EdgeRef, ()>,
}

// captureRouteMutations records every route reachable through the collections
// used by post-processing.
pub(crate) fn captureRouteMutations(
    g: Option<&GraphRef>,
    extraEdges: &[EdgeRef],
    guard: &routeWorkGuardRef,
) -> GoResult<routeMutationSnapshot> {
    let mut snapshot = routeMutationSnapshot {
        graph: g.cloned(),
        cellSize: 0.0,
        costs: RoutingCostState::default(),
        nodes: Vec::new(),
        nodeSet: Map::default(),
        edges: Vec::new(),
        edgeSet: Map::default(),
    };
    if let Some(g) = g {
        snapshot.cellSize = g.b().CellSize;
        snapshot.costs = g.RoutingCosts();
    }
    let mut nodeQueue: Vec<NodeRef> = Vec::new();
    fn captureNode(
        snapshot: &mut routeMutationSnapshot,
        nodeQueue: &mut Vec<NodeRef>,
        node: Option<&NodeRef>,
        guard: &routeWorkGuardRef,
    ) -> GoResult<()> {
        guard.step()?;
        let Some(node) = node else { return Ok(()) };
        if snapshot.nodeSet.contains_key(node) {
            return Ok(());
        }
        let tl = node.tl();
        let value = tl.as_ref().map(|p| *p.b());
        snapshot.nodes.push((node.clone(), tl, value, node.graph()));
        snapshot.nodeSet.insert(node.clone(), ());
        nodeQueue.push(node.clone());
        Ok(())
    }
    fn capture(
        snapshot: &mut routeMutationSnapshot,
        nodeQueue: &mut Vec<NodeRef>,
        edge: &EdgeRef,
        guard: &routeWorkGuardRef,
    ) -> GoResult<()> {
        // Charge every reference before de-duplication.
        guard.step()?;
        if snapshot.edgeSet.contains_key(edge) {
            return Ok(());
        }
        let points = edge.points();
        let mut pointValues = Vec::with_capacity(points.len());
        for point in &points {
            guard.step()?;
            pointValues.push((point.clone(), *point.b()));
        }
        let (label, source, target) = {
            let e = edge.b();
            (
                routeLabelSnapshot::new(&e.Label),
                routeLabelSnapshot::new(&e.SourceArrowheadLabel),
                routeLabelSnapshot::new(&e.TargetArrowheadLabel),
            )
        };
        snapshot.edges.push((
            edge.clone(),
            routeEdgeSnapshot {
                value: edgeFieldValues::capture(edge),
                pointValues,
                label,
                sourceArrowheadLabel: source,
                targetArrowheadLabel: target,
            },
        ));
        snapshot.edgeSet.insert(edge.clone(), ());
        let (from, to) = (edge.from(), edge.to());
        captureNode(snapshot, nodeQueue, Some(&from), guard)?;
        captureNode(snapshot, nodeQueue, Some(&to), guard)
    }
    if let Some(g) = g {
        for edge in g.edges() {
            capture(&mut snapshot, &mut nodeQueue, &edge, guard)?;
        }
        for node in g.nodes() {
            captureNode(&mut snapshot, &mut nodeQueue, Some(&node), guard)?;
            for edge in node.edges() {
                capture(&mut snapshot, &mut nodeQueue, &edge, guard)?;
            }
        }
    }
    for edge in extraEdges {
        capture(&mut snapshot, &mut nodeQueue, edge, guard)?;
    }
    // A route stage temporarily rewrites graph references not just for graph
    // members but also for container, cluster, and sequence descendants.
    let mut index = 0;
    while index < nodeQueue.len() {
        let node = nodeQueue[index].clone();
        index += 1;
        let nodeGraph = match node.graph() {
            Some(ng) => ng,
            None => match g {
                Some(g) => g.clone(),
                None => continue,
            },
        };
        if node.IsContainer() {
            let children = nodeGraph.b().Containers.get(&Some(node.clone())).unwrap_or_default();
            for child in &children {
                captureNode(&mut snapshot, &mut nodeQueue, Some(child), guard)?;
            }
        }
        if node.IsClusterVessel() {
            let cluster = nodeGraph.b().Clusters.get(&node);
            if let Some(cluster) = cluster {
                let nodes = cluster.b().Nodes.clone();
                for child in &nodes {
                    captureNode(&mut snapshot, &mut nodeQueue, Some(child), guard)?;
                }
            }
        }
        let sequence = nodeGraph.b().Sequences.get(&node);
        if let Some(sequence) = sequence {
            let nodes = sequence.b().Nodes.clone();
            for child in &nodes {
                captureNode(&mut snapshot, &mut nodeQueue, Some(child), guard)?;
            }
        }
    }
    guard.finish()?;
    Ok(snapshot)
}

impl routeMutationSnapshot {
    pub(crate) fn restore(&self) {
        if let Some(g) = &self.graph {
            g.bm().CellSize = self.cellSize;
            g.RestoreRoutingCosts(self.costs);
        }
        for (node, pointer, value, graph) in &self.nodes {
            if let (Some(p), Some(v)) = (pointer, value) {
                p.set(*v);
            }
            node.set_tl(pointer.clone());
            node.bm().Graph = graph.clone();
        }
        for (edge, state) in &self.edges {
            state.restore(edge);
        }
    }

    pub(crate) fn hasNode(&self, node: &NodeRef) -> bool {
        self.nodeSet.contains_key(node)
    }
}

pub(crate) fn validateRouteStageGeometry(g: &GraphRef, extraEdges: &[EdgeRef], guard: &routeWorkGuardRef) -> GoResult<()> {
    if extraEdges.len() as i64 > layoutgraph::MaxTopologyReferences {
        return Err(GoError::new(format!(
            "TALA {} extra edge references exceed limit {}",
            guard.location(),
            layoutgraph::MaxTopologyReferences
        )));
    }
    let finite = |value: f64| !value.is_nan() && !value.is_infinite();
    let mut seenNodes: Map<NodeRef, ()> = Map::default();
    let mut graphNodes: Map<NodeRef, ()> = Map::default();
    let validateNode = |seenNodes: &mut Map<NodeRef, ()>, node: &NodeRef, description: &dyn Fn() -> String| -> GoResult<()> {
        guard.step()?;
        if seenNodes.contains_key(node) {
            return Ok(());
        }
        seenNodes.insert(node.clone(), ());
        if !node.has_tl() {
            return Err(GoError::new(format!("TALA {} {} has no position", guard.location(), description())));
        }
        Ok(())
    };
    for (index, node) in g.nodes().iter().enumerate() {
        validateNode(&mut seenNodes, node, &|| format!("graph node at index {}", index))?;
        graphNodes.insert(node.clone(), ());
    }

    let mut seen: Map<EdgeRef, ()> = Map::default();
    let mut routePointCapacity: u64 = 0;
    let mut validateEdge = |seenNodes: &mut Map<NodeRef, ()>, edge: &EdgeRef, requireGraphEndpoints: bool| -> GoResult<()> {
        guard.step()?;
        if seen.contains_key(edge) {
            return Ok(());
        }
        seen.insert(edge.clone(), ());
        let id = edge.IDValue();
        let (from, to) = (edge.from(), edge.to());
        if requireGraphEndpoints {
            if !graphNodes.contains_key(&from) {
                return Err(GoError::new(format!("graph edge {} source node does not belong to the graph", id)));
            }
            if !graphNodes.contains_key(&to) {
                return Err(GoError::new(format!("graph edge {} target node does not belong to the graph", id)));
            }
        }
        let points = edge.points();
        // Go checks cap(edge.Points); Rust vectors have no spare capacity semantics.
        let capacity = points.len() as u64;
        let maxPoints = layoutgraph::MaxRoutePoints as u64;
        if routePointCapacity > maxPoints || capacity > maxPoints - routePointCapacity {
            return Err(GoError::new(format!(
                "TALA {} route point capacity exceeds limit {}",
                guard.location(),
                layoutgraph::MaxRoutePoints
            )));
        }
        routePointCapacity += capacity;
        if points.len() == 1 {
            return Err(invariant::Errorf(format!(
                "edge {} has an incomplete route: expected at least two points, got 1",
                id
            )));
        }
        {
            let e = edge.b();
            for (name, l) in [
                ("label", &e.Label),
                ("source arrowhead label", &e.SourceArrowheadLabel),
                ("target arrowhead label", &e.TargetArrowheadLabel),
            ] {
                if let Some(l) = l {
                    let pos = l.b().Position;
                    if pos != label::Unset && !pos.IsEdgePosition() {
                        return Err(invariant::Errorf(format!("edge {} {} has an invalid edge position", id, name)));
                    }
                }
            }
        }
        validateNode(seenNodes, &from, &|| format!("edge {} source", id))?;
        validateNode(seenNodes, &to, &|| format!("edge {} target", id))?;
        for point in &points {
            guard.step()?;
            let p = *point.b();
            if !finite(p.X) || !finite(p.Y) {
                return Err(GoError::new(format!(
                    "TALA {} edge {} has a non-finite route point",
                    guard.location(), id
                )));
            }
        }
        Ok(())
    };
    for edge in g.edges() {
        validateEdge(&mut seenNodes, &edge, true)?;
    }
    for node in g.nodes() {
        for edge in node.edges() {
            validateEdge(&mut seenNodes, &edge, false)?;
        }
    }
    for edge in extraEdges {
        validateEdge(&mut seenNodes, edge, false)?;
    }
    guard.finish()
}

// routeStageGraphBoundingBox preserves Graph.BoundingBox geometry while
// charging the stage aggregate for the existing outside-label extremity scans
// and every edge-route scan.
pub(crate) fn routeStageGraphBoundingBox(g: &GraphRef, guard: &routeWorkGuardRef) -> GoResult<(geo::PointRef, geo::PointRef)> {
    let (topLeft, bottomRight) = graphbounds::FixedBoundingBox(&g.nodes(), guard)?;
    let (mut minX, mut minY) = (topLeft.X(), topLeft.Y());
    let (mut maxX, mut maxY) = (bottomRight.X(), bottomRight.Y());
    for edge in g.edges() {
        guard.step()?;
        let (n, hasLabel, hasSource, hasTarget) = {
            let e = edge.b();
            (
                e.Points.len() as u64,
                e.Label.as_ref().map_or(false, |l| l.b().Position != label::Unset),
                e.SourceArrowheadLabel.is_some(),
                e.TargetArrowheadLabel.is_some(),
            )
        };
        guard.add(n)?;
        let chargeRouteScans = |scans: usize| -> GoResult<()> {
            for _ in 0..scans {
                guard.add(n)?;
            }
            Ok(())
        };
        if n != 0 && hasLabel {
            chargeRouteScans(2)?;
        }
        if n != 0 && hasSource {
            chargeRouteScans(3)?;
        }
        if n != 0 && hasTarget {
            chargeRouteScans(3)?;
        }
        let (edgeTopLeft, edgeBottomRight) = edge.BoundingBoxValues();
        if !edgeTopLeft.X.is_infinite() {
            minX = math::Min(minX, edgeTopLeft.X);
            minY = math::Min(minY, edgeTopLeft.Y);
            maxX = math::Max(maxX, edgeBottomRight.X);
            maxY = math::Max(maxY, edgeBottomRight.Y);
        }
    }
    guard.finish()?;
    Ok((geo::NewPoint(minX.round(), minY.round()), geo::NewPoint(maxX.round(), maxY.round())))
}

// runAtomicRouteStage provides one error boundary for each whole stage.
pub(crate) fn runAtomicRouteStage(
    ctx: &Ctx,
    location: &str,
    g: &GraphRef,
    extraEdges: &[EdgeRef],
    workLimit: u64,
    f: impl FnOnce(&routeWorkGuardRef) -> GoResult<()>,
) -> GoResult<()> {
    layoutgraph::Validate(ctx, location, g)?;
    runAtomicRouteStageAfterPreflight(ctx, location, g, extraEdges, workLimit, f)
}

pub(crate) fn runAtomicRouteStageAfterPreflight(
    ctx: &Ctx,
    location: &str,
    g: &GraphRef,
    extraEdges: &[EdgeRef],
    workLimit: u64,
    f: impl FnOnce(&routeWorkGuardRef) -> GoResult<()>,
) -> GoResult<()> {
    let guard = newRouteWorkGuard(ctx, location, workLimit)?;
    runAtomicRouteStageWithGuard(g, extraEdges, &guard, f)
}

// runAtomicRouteStageWithGuard lets a whole pipeline stage use the same work
// budget for its own discovery kernels and the exact mutation boundary.
pub(crate) fn runAtomicRouteStageWithGuard(
    g: &GraphRef,
    extraEdges: &[EdgeRef],
    guard: &routeWorkGuardRef,
    f: impl FnOnce(&routeWorkGuardRef) -> GoResult<()>,
) -> GoResult<()> {
    validateRouteStageGeometry(g, extraEdges, guard)?;
    runAtomicRouteStageWithValidatedGeometry(g, extraEdges, guard, f)
}

pub(crate) fn runAtomicRouteStageWithValidatedGeometry(
    g: &GraphRef,
    extraEdges: &[EdgeRef],
    guard: &routeWorkGuardRef,
    f: impl FnOnce(&routeWorkGuardRef) -> GoResult<()>,
) -> GoResult<()> {
    let snapshot = captureRouteMutations(Some(g), extraEdges, guard)?;
    let result = f(guard).and_then(|_| guard.finish());
    if result.is_err() {
        snapshot.restore();
    }
    result
}

// stableSortRouteValues is a cancellable bottom-up stable merge sort.
pub(crate) fn stableSortRouteValues<T: Clone>(
    values: &mut [T],
    mut less: impl FnMut(&T, &T) -> bool,
    guard: &routeWorkGuardRef,
) -> GoResult<()> {
    let n = values.len();
    if n < 2 {
        return guard.step();
    }
    let mut temporary: Vec<T> = values.to_vec();
    let mut width = 1usize;
    while width < n {
        let mut start = 0usize;
        while start < n {
            let middle = (start + width).min(n);
            let end = (start + 2 * width).min(n);
            let (mut left, mut right, mut output) = (start, middle, start);
            while left < middle && right < end {
                guard.step()?;
                // Prefer the left value when equivalent to preserve stability.
                if less(&values[right], &values[left]) {
                    temporary[output] = values[right].clone();
                    right += 1;
                } else {
                    temporary[output] = values[left].clone();
                    left += 1;
                }
                output += 1;
            }
            while left < middle {
                guard.step()?;
                temporary[output] = values[left].clone();
                left += 1;
                output += 1;
            }
            while right < end {
                guard.step()?;
                temporary[output] = values[right].clone();
                right += 1;
                output += 1;
            }
            start += 2 * width;
        }
        guard.add(n as u64)?;
        values.clone_from_slice(&temporary);
        if width > n / 2 {
            break;
        }
        width *= 2;
    }
    Ok(())
}
