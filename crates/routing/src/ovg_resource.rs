// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/routing/ovg_resource.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of routing/ovg_resource.go.
#[allow(unused_imports)]
use crate::*;
use gocompat::context::{Ctx, Done};
use gocompat::{math, GoError, GoResult, Map, Sentinel, Set};
use layoutgraph::{GraphRef, NodeRef};
use std::rc::Rc;

/// OVG construction derives a visibility graph whose size can be much larger
/// than the input graph. These limits bound the memory and CPU cost of one
/// routing attempt independently of the adapter's input limits.
pub(crate) const maxOVGIntersectionCandidates: u64 = 1_000_000;
pub(crate) const maxOVGNodes: u64 = 200_000;
pub(crate) const maxOVGEdges: u64 = 500_000;
/// maxOVGWorkUnits bounds actual OVG construction and query work, including
/// index construction.
pub(crate) const maxOVGWorkUnits: u64 = 250_000_000;

pub static errOVGResourceLimit: Sentinel = Sentinel::new("TALA OVG resource limit exceeded");

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ovgBuildLimits {
    pub(crate) intersectionCandidates: u64,
    pub(crate) nodes: u64,
    pub(crate) edges: u64,
    pub(crate) work: u64,
}

pub(crate) fn defaultOVGBuildLimits() -> ovgBuildLimits {
    ovgBuildLimits {
        intersectionCandidates: maxOVGIntersectionCandidates,
        nodes: maxOVGNodes,
        edges: maxOVGEdges,
        work: maxOVGWorkUnits,
    }
}

pub struct ovgBuildGuard {
    pub(crate) ctx: Ctx,
    pub(crate) done: Option<Done>,
    /// aggregate is shared by all OVGs and route-search flavors in one public
    /// routing operation. EdgeRoutingStage also reuses this entire guard across
    /// subgraphs, making its resource counters stage-wide rather than per graph.
    pub(crate) aggregate: Option<workBudgetRef>,

    pub(crate) limits: ovgBuildLimits,
    pub(crate) candidates: u64,
    pub(crate) nodes: u64,
    pub(crate) edges: u64,
    pub(crate) work: u64,
}

gocompat::go_ptr!(ovgBuildGuardRef, ovgBuildGuard);

pub(crate) fn newOVGBuildGuard(ctx: Option<&Ctx>, limits: ovgBuildLimits) -> GoResult<ovgBuildGuardRef> {
    let ctx = match ctx {
        None => gocompat::context::Background(),
        Some(c) => c.clone(),
    };
    let guard = ovgBuildGuard {
        done: ctx.Done(),
        aggregate: routeAggregateWorkFromContext(&ctx),
        ctx,
        limits,
        candidates: 0,
        nodes: 0,
        edges: 0,
        work: 0,
    };
    guard.check()?;
    Ok(ovgBuildGuardRef::new(guard))
}

fn ovgLimitErr(msg: String) -> GoError {
    GoError::wrap(format!("{}: {}", errOVGResourceLimit.Error(), msg), errOVGResourceLimit.err())
}

pub(crate) fn ovgResourceOverflow(resource: &str) -> GoError {
    ovgLimitErr(format!("{} arithmetic overflow", resource))
}

pub(crate) fn ovgResourceExceeded(resource: &str, requested: u64, limit: u64) -> GoError {
    ovgLimitErr(format!("{} {} exceeds limit {}", resource, requested, limit))
}

pub(crate) fn checkedOVGIntersectionCount(xCount: usize, yCount: usize) -> GoResult<u64> {
    let (count, ok) = limits::CheckedMulUint64(xCount as u64, yCount as u64);
    if !ok {
        return Err(ovgResourceOverflow("intersection candidate count"));
    }
    Ok(count)
}

pub(crate) fn checkedUint64ToInt(value: u64, intLimit: u64) -> GoResult<usize> {
    if value > intLimit {
        return Err(ovgResourceExceeded("allocation capacity", value, intLimit));
    }
    Ok(value as usize)
}

pub(crate) fn checkedOVGSliceCapacity(lengths: &[usize]) -> GoResult<usize> {
    let mut total = 0u64;
    for &length in lengths {
        let (next, ok) = limits::CheckedAddUint64(total, length as u64);
        if !ok {
            return Err(ovgResourceOverflow("allocation capacity"));
        }
        total = next;
    }
    checkedUint64ToInt(total, maxIntAsUint64())
}

pub(crate) fn maxIntAsUint64() -> u64 {
    i64::MAX as u64
}

/// checkedOVGEdgeCapacity computes the exact sweep upper bound used for slice
/// preallocation.
pub(crate) fn checkedOVGEdgeCapacity(existing: u64, nodeCount: u64, horizontalLines: u64, verticalLines: u64, edgeLimit: u64, intLimit: u64) -> GoResult<usize> {
    if existing > edgeLimit {
        return Err(ovgResourceExceeded("edge count", existing, edgeLimit));
    }

    let (twiceNodes, ok) = limits::CheckedMulUint64(nodeCount, 2);
    if !ok {
        return Err(ovgResourceOverflow("edge capacity"));
    }
    let (lineCount, ok) = limits::CheckedAddUint64(horizontalLines, verticalLines);
    if !ok || twiceNodes < lineCount {
        return Err(ovgResourceOverflow("edge capacity"));
    }
    let newEdges = twiceNodes - lineCount;
    let (mut capacity, ok) = limits::CheckedAddUint64(existing, newEdges);
    if !ok {
        return Err(ovgResourceOverflow("edge capacity"));
    }
    if capacity > edgeLimit {
        capacity = edgeLimit;
    }
    checkedUint64ToInt(capacity, intLimit)
}

impl ovgBuildGuard {
    #[inline]
    pub(crate) fn check(&self) -> GoResult<()> {
        if let Some(err) = cachedContextErr(&self.ctx, &self.done) {
            return Err(GoError::wrap(format!("EdgeRouting: {}", err), err));
        }
        Ok(())
    }

    fn reserveField(&mut self, resource: &str, which: u8, amount: u64, limit: u64) -> GoResult<()> {
        self.check()?;
        let used = match which {
            0 => self.candidates,
            1 => self.nodes,
            _ => self.edges,
        };
        let (next, ok) = limits::CheckedAddUint64(used, amount);
        if !ok {
            return Err(ovgResourceOverflow(resource));
        }
        if next > limit {
            return Err(ovgResourceExceeded(resource, next, limit));
        }
        match which {
            0 => self.candidates = next,
            1 => self.nodes = next,
            _ => self.edges = next,
        }
        Ok(())
    }

    pub(crate) fn reserveCandidates(&mut self, amount: u64) -> GoResult<()> {
        let limit = self.limits.intersectionCandidates;
        self.reserveField("intersection candidate count", 0, amount, limit)
    }

    pub(crate) fn reserveNodes(&mut self, amount: u64) -> GoResult<()> {
        let limit = self.limits.nodes;
        self.reserveField("node count", 1, amount, limit)
    }

    pub(crate) fn reserveEdges(&mut self, amount: u64) -> GoResult<()> {
        let limit = self.limits.edges;
        self.reserveField("edge count", 2, amount, limit)
    }

    pub(crate) fn reserveWork(&mut self, amount: u64) -> GoResult<()> {
        let (next, ok) = limits::CheckedAddUint64(self.work, amount);
        if !ok {
            self.check()?;
            return Err(ovgResourceOverflow("work units"));
        }
        if next > self.limits.work {
            self.check()?;
            return Err(ovgResourceExceeded("work units", next, self.limits.work));
        }
        // Synthetic contexts without Done retain per-operation Err polling. Standard
        // contexts poll at a bounded stride.
        if self.done.is_none() || amount == 0 || self.work / routeStageContextCheckStride != next / routeStageContextCheckStride {
            self.check()?;
        }
        self.work = next;
        if let Some(agg) = &self.aggregate {
            return agg.add(amount);
        }
        Ok(())
    }

    #[inline]
    pub(crate) fn step(&mut self) -> GoResult<()> {
        self.reserveWork(1)
    }

    pub(crate) fn reserveSortWork(&mut self, length: usize) -> GoResult<()> {
        if length < 2 {
            return self.check();
        }
        let mut levels = 0u64;
        let mut remaining = (length - 1) as u64;
        while remaining > 0 {
            levels += 1;
            remaining >>= 1;
        }
        let (work, ok) = limits::CheckedMulUint64(length as u64, levels);
        if !ok {
            return Err(ovgResourceOverflow("sort work units"));
        }
        self.reserveWork(work)
    }

    pub(crate) fn addNode(&mut self, ovg: &mut OVG, node: &OVGNodeRef) -> GoResult<OVGNodeRef> {
        self.step()?;
        if let Some(occupant) = ovg.OccupiedPoints.get(&node.P()) {
            return Ok(occupant.clone());
        }
        self.reserveNodes(1)?;
        ovg.AddNodeUnchecked(node);
        Ok(node.clone())
    }

    pub(crate) fn addPoint(&mut self, ovg: &mut OVG, point: geo::Point) -> GoResult<OVGNodeRef> {
        self.step()?;
        if let Some(occupant) = ovg.OccupiedPoints.get(&point) {
            return Ok(occupant.clone());
        }
        self.reserveNodes(1)?;
        let node = NewOVGNode(point);
        ovg.AddNodeUnchecked(&node);
        Ok(node)
    }

    /// newDerivedNode accounts for a temporary OVG node before allocating it. The
    /// node may later also consume a slot in one or more OVGs; those memberships are
    /// deliberately counted separately by addNode.
    pub(crate) fn newDerivedNode(&mut self, point: geo::Point) -> GoResult<OVGNodeRef> {
        self.step()?;
        self.reserveNodes(1)?;
        Ok(NewOVGNode(point))
    }

    pub(crate) fn newCandidateNode(&mut self, point: geo::Point) -> GoResult<OVGNodeRef> {
        self.step()?;
        Ok(NewOVGNode(point))
    }

    pub(crate) fn addNodeUnchecked(&mut self, ovg: &mut OVG, node: &OVGNodeRef) -> GoResult<()> {
        self.step()?;
        self.reserveNodes(1)?;
        ovg.AddNodeUnchecked(node);
        Ok(())
    }

    pub(crate) fn connect(&mut self, ovg: &mut OVG, nodeA: &OVGNodeRef, nodeB: &OVGNodeRef) -> GoResult<Option<OVGEdgeRef>> {
        self.step()?;
        {
            let (a, b) = (nodeA.b(), nodeB.b());
            if a.IsNodeCenter && !b.isPort() || b.IsNodeCenter && !a.isPort() {
                return Ok(None);
            }
        }
        self.reserveEdges(1)?;
        Ok(ovg.Connect(nodeA, nodeB))
    }

    pub(crate) fn pointNearGraphNode(&mut self, graph: &GraphRef, point: &geo::Point) -> GoResult<bool> {
        let nodes = graph.b().Nodes.clone();
        for node in &nodes {
            self.step()?;
            // It is valid for visibility nodes to lie on container boundaries.
            if node.IsContainer() {
                continue;
            }
            if node.IsPointNear(point) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub(crate) fn tightBoundingBox(&mut self, nodes: &[NodeRef]) -> GoResult<(Option<geo::Point>, Option<geo::Point>)> {
        if nodes.is_empty() {
            self.check()?;
            return Ok((Some(geo::Point { X: math::Inf(-1), Y: math::Inf(-1) }), Some(geo::Point { X: math::Inf(1), Y: math::Inf(1) })));
        }
        if !nodes[0].has_tl() {
            self.check()?;
            return Ok((None, None));
        }

        let tl0 = nodes[0].tlv();
        let mut minX = tl0.X;
        let mut minY = tl0.Y;
        let mut maxX = tl0.X + nodes[0].w();
        let mut maxY = tl0.Y + nodes[0].h();
        for node in nodes {
            self.step()?;
            if !node.has_tl() {
                return Ok((None, None));
            }
            let tl = node.tlv();
            minX = math::Min(minX, tl.X);
            minY = math::Min(minY, tl.Y);
            maxX = math::Max(maxX, tl.X + node.w());
            maxY = math::Max(maxY, tl.Y + node.h());
        }
        self.check()?;
        Ok((Some(geo::Point { X: minX, Y: minY }), Some(geo::Point { X: maxX, Y: maxY })))
    }

    pub(crate) fn ovgBoundingBox(&mut self, ovg: &OVG) -> GoResult<(geo::Point, geo::Point)> {
        if ovg.Nodes.is_empty() {
            self.check()?;
            return Ok((geo::Point { X: math::Inf(-1), Y: math::Inf(-1) }, geo::Point { X: math::Inf(1), Y: math::Inf(1) }));
        }
        let p0 = ovg.Nodes[0].P();
        let mut minX = p0.X;
        let mut minY = p0.Y;
        let mut maxX = p0.X;
        let mut maxY = p0.Y;
        for node in &ovg.Nodes {
            self.step()?;
            let p = node.P();
            minX = math::Min(minX, p.X);
            minY = math::Min(minY, p.Y);
            maxX = math::Max(maxX, p.X);
            maxY = math::Max(maxY, p.Y);
        }
        self.check()?;
        Ok((geo::Point { X: minX, Y: minY }, geo::Point { X: maxX, Y: maxY }))
    }

    pub(crate) fn portsByOrientation(&mut self, ovg: &OVG, owner: &NodeRef, orientation: geo::Orientation) -> GoResult<Vec<OVGNodeRef>> {
        let empty = Vec::new();
        let all = ovg.Ports.get(owner).unwrap_or(&empty);
        let mut ports: Vec<OVGNodeRef> = Vec::with_capacity(all.len());
        let mut seen: Set<OVGNodeRef> = gocompat::NewSetCap(all.len());
        for port in all {
            self.step()?;
            if seen.contains(port) {
                continue;
            }
            seen.insert(port.clone());
            if port.hasPortDirection(owner, orientation) {
                ports.push(port.clone());
            }
        }
        self.check()?;
        Ok(ports)
    }

    pub(crate) fn passesThroughAllowingPorts(&mut self, node: &NodeRef, p1: &geo::Point, p2: &geo::Point, direction: geo::Orientation, ports: &[OVGNodeRef]) -> GoResult<bool> {
        let passes = |n: &NodeRef| n.PassesThrough(Some(&geo::PointRef::new(*p1)), Some(&geo::PointRef::new(*p2)));
        if ports.is_empty() {
            let r = passes(node);
            self.check()?;
            return Ok(r);
        }
        // A port can exempt a segment only when it points outwards in the supplied
        // direction.
        let allowsPort = if direction == geo::Top {
            p1.X == p2.X && p1.Y > p2.Y
        } else if direction == geo::Bottom {
            p1.X == p2.X && p1.Y < p2.Y
        } else if direction == geo::Left {
            p1.Y == p2.Y && p1.X > p2.X
        } else if direction == geo::Right {
            p1.Y == p2.Y && p1.X < p2.X
        } else {
            false
        };
        if allowsPort {
            for port in ports {
                self.step()?;
                let pp = port.P();
                if nonNilEquals(&pp, p1) || nonNilEquals(&pp, p2) {
                    return Ok(false);
                }
            }
        }
        let r = passes(node);
        self.check()?;
        Ok(r)
    }

    pub(crate) fn isDescendantOf(&mut self, descendant: Option<&NodeRef>, ancestor: Option<&NodeRef>) -> GoResult<bool> {
        let mut current: Option<NodeRef> = descendant.cloned();
        loop {
            self.step()?;
            if ancestor == current.as_ref() {
                return Ok(true);
            }
            let c = match &current {
                None => return Ok(false),
                Some(c) => c.clone(),
            };
            let cb = c.b();
            if let Some(n) = &cb.Container {
                let n = n.clone();
                drop(cb);
                current = Some(n);
            } else if let Some(cl) = &cb.Cluster {
                let v = cl.b().Vessel.clone();
                drop(cb);
                current = Some(v);
            } else if let Some(s) = &cb.Sequence {
                let v = s.b().Vessel.clone();
                drop(cb);
                current = Some(v);
            } else {
                return Ok(ancestor.is_none());
            }
        }
    }

    pub(crate) fn hasFixedAncestor(&mut self, node: &NodeRef) -> GoResult<bool> {
        let mut current = Some(node.clone());
        while let Some(c) = current {
            self.step()?;
            if c.b().FixedTopLeft.is_some() {
                return Ok(true);
            }
            current = c.OwningContainer();
        }
        Ok(false)
    }

    pub(crate) fn sameNodes(&mut self, a: &[NodeRef], b: &[NodeRef]) -> GoResult<bool> {
        if a.len() != b.len() {
            self.check()?;
            return Ok(false);
        }
        for i in 0..a.len() {
            self.step()?;
            if a[i] != b[i] {
                return Ok(false);
            }
        }
        self.check()?;
        Ok(true)
    }

    pub(crate) fn isRestrictedSequencePort(&mut self, node: &OVGNodeRef) -> GoResult<bool> {
        let owners = node.portOwners();
        if owners.is_empty() {
            self.check()?;
            return Ok(false);
        }
        for (owner, metadata) in &owners {
            self.step()?;
            let seq = match owner.sequenceField() {
                None => return Ok(false),
                Some(s) => s,
            };
            if metadata.directions.any(|direction| {
                if direction == geo::Left {
                    seq.First() == *owner
                } else if direction == geo::Right {
                    seq.Last() == *owner
                } else {
                    true
                }
            }) {
                return Ok(false);
            }
        }
        self.check()?;
        Ok(true)
    }

    pub(crate) fn containerRDFSOrder(&mut self, graph: &GraphRef, root: Option<&NodeRef>) -> GoResult<Vec<NodeRef>> {
        let mut order: Vec<NodeRef> = Vec::new();
        if let Some(r) = root {
            if !r.IsContainer() {
                self.check()?;
                return Ok(order);
            }
        }
        let children = graph.b().Containers.b().get(&root.cloned()).cloned().unwrap_or_default();
        for child in children.iter().rev() {
            self.step()?;

            if child.IsContainer() {
                let descendants = self.containerRDFSOrder(graph, Some(child))?;
                order.extend(descendants);
                order.push(child.clone());
                continue;
            }
            if child.IsClusterVessel() {
                let cluster = graph.b().Clusters.b().get(child).cloned().expect("cluster vessel without cluster");
                let clusterNodes = cluster.b().Nodes.clone();
                for clusterNode in clusterNodes.iter().rev() {
                    self.step()?;

                    if !clusterNode.IsContainer() {
                        continue;
                    }
                    let descendants = self.containerRDFSOrder(graph, Some(clusterNode))?;
                    order.extend(descendants);
                    order.push(clusterNode.clone());
                }
            }
        }
        self.check()?;
        Ok(order)
    }

    /// reserveHierarchyTransform accounts for a complete, non-interruptible
    /// coordinate transform before it mutates graph or OVG state.
    pub(crate) fn reserveHierarchyTransform(&mut self, ovg: &OVG, graphNodeCount: usize) -> GoResult<()> {
        self.reserveWork(graphNodeCount as u64)?;
        for node in &ovg.Nodes {
            self.step()?; // preflight scan
            self.reserveWork(2)?; // point transform and reindex
            let n = node.b().portOwnersByNode.len();
            for _ in 0..n {
                self.step()?; // preflight scan
                self.reserveWork(1)?; // direction transform
            }
        }
        self.check()
    }

    pub(crate) fn hasCoordinateWithin(&mut self, sortedValues: &[f64], coordinate: f64, distance: f64) -> GoResult<bool> {
        let (mut low, mut high) = (0usize, sortedValues.len());
        let threshold = coordinate - distance;
        while low < high {
            self.step()?;
            let mid = (low + high) >> 1;
            if sortedValues[mid] < threshold {
                low = mid + 1;
            } else {
                high = mid;
            }
        }
        let r = low < sortedValues.len() && sortedValues[low] <= coordinate + distance;
        self.check()?;
        Ok(r)
    }
}

/// ovgPointProximityIndex is a build-local broad phase for the Cartesian
/// intersection grid.
pub(crate) struct ovgPointProximityIndex {
    pub(crate) byX: Map<u64, Vec<NodeRef>>,
}

pub(crate) fn newOVGPointProximityIndex(graph: &GraphRef, xs: &[f64], guard: &mut ovgBuildGuard) -> GoResult<ovgPointProximityIndex> {
    let mut index = ovgPointProximityIndex { byX: gocompat::NewMapCap(xs.len()) };
    let nodes = graph.b().Nodes.clone();
    // Precompute node extents (pure reads).
    let ext: Vec<(bool, f64, f64)> = nodes.iter().map(|n| if n.IsContainer() { (true, 0.0, 0.0) } else { (false, n.x(), n.w()) }).collect();
    for &x in xs {
        guard.step()?;
        for (i, node) in nodes.iter().enumerate() {
            guard.step()?;
            let (isContainer, nx, nw) = ext[i];
            if isContainer {
                continue;
            }
            if nx - layoutgraph::MinRouteNodeClearance <= x && nx + nw + layoutgraph::MinRouteNodeClearance >= x {
                index.byX.entry(f64key(x)).or_default().push(node.clone());
            }
        }
    }
    guard.check()?;
    Ok(index)
}

impl ovgPointProximityIndex {
    pub(crate) fn pointNear(&self, x: f64, y: f64, guard: &mut ovgBuildGuard) -> GoResult<bool> {
        guard.step()?;
        let point = geo::Point { X: x, Y: y };
        if let Some(nodes) = self.byX.get(&f64key(x)) {
            for node in nodes {
                guard.step()?;
                if node.IsPointNear(&point) {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
}

pub(crate) struct fixedOverlapsCacheEntry {
    pub(crate) graph: GraphRef,
    pub(crate) nodes: Vec<NodeRef>,
    pub(crate) overlaps: Rc<Set<NodeRef>>,
}

impl OVG {
    pub(crate) fn fixedOverlapsForBuild(&mut self, graph: &GraphRef, nodes: &[NodeRef], guard: &mut ovgBuildGuard) -> GoResult<Rc<Set<NodeRef>>> {
        for entry in &self.fixedOverlapsCache {
            guard.step()?;
            let same = guard.sameNodes(&entry.nodes, nodes)?;
            if entry.graph == *graph && same {
                return Ok(entry.overlaps.clone());
            }
        }

        let mut fixedNodes: Vec<NodeRef> = Vec::with_capacity(nodes.len() / 4);
        for node in nodes {
            guard.step()?;
            let fixed = guard.hasFixedAncestor(node)?;
            if fixed {
                fixedNodes.push(node.clone());
            }
        }

        let mut overlaps: Set<NodeRef> = Set::default();
        for node in &fixedNodes {
            if overlaps.contains(node) {
                continue;
            }
            for other in nodes {
                guard.step()?;
                let nodeBelowOther = guard.isDescendantOf(Some(node), Some(other))?;
                let otherBelowNode = guard.isDescendantOf(Some(other), Some(node))?;
                if node == other || nodeBelowOther || otherBelowNode {
                    continue;
                }
                if nodeOverlapsBox(node, other) {
                    overlaps.insert(node.clone());
                    let fixed = guard.hasFixedAncestor(other)?;
                    if fixed {
                        overlaps.insert(other.clone());
                    }
                    break;
                }
            }
        }

        let overlaps = Rc::new(overlaps);
        self.fixedOverlapsCache.push(fixedOverlapsCacheEntry { graph: graph.clone(), nodes: nodes.to_vec(), overlaps: overlaps.clone() });
        Ok(overlaps)
    }
}

pub(crate) struct ovgPortIndex {
    pub(crate) xByY: Map<u64, Vec<f64>>,
    pub(crate) yByX: Map<u64, Vec<f64>>,
    pub(crate) owners: Vec<NodeRef>,
    pub(crate) ownersByX: Map<u64, Vec<usize>>,
    pub(crate) ownersByY: Map<u64, Vec<usize>>,
    pub(crate) verticalBlockers: Map<u64, Vec<NodeRef>>,
    pub(crate) horizontalBlockers: Map<u64, Vec<NodeRef>>,
}

pub(crate) fn newOVGPortIndex(ports: &Map<NodeRef, Vec<OVGNodeRef>>, graph: Option<&GraphRef>, fixedOverlaps: &Set<NodeRef>, guard: &mut ovgBuildGuard) -> GoResult<ovgPortIndex> {
    let mut index = ovgPortIndex {
        xByY: Map::default(),
        yByX: Map::default(),
        owners: Vec::with_capacity(ports.len()),
        ownersByX: Map::default(),
        ownersByY: Map::default(),
        verticalBlockers: Map::default(),
        horizontalBlockers: Map::default(),
    };
    // Deterministic owner order (Go iterates its map in random order; only work
    // accounting depends on it).
    for (owner, nodePorts) in ports.iter() {
        guard.step()?;
        let ownerIndex = index.owners.len();
        index.owners.push(owner.clone());
        for port in nodePorts {
            guard.step()?;
            let p = port.P();
            index.xByY.entry(f64key(p.Y)).or_default().push(p.X);
            index.yByX.entry(f64key(p.X)).or_default().push(p.Y);
            index.ownersByX.entry(f64key(p.X)).or_default().push(ownerIndex);
            index.ownersByY.entry(f64key(p.Y)).or_default().push(ownerIndex);
        }
    }
    for values in index.xByY.values_mut() {
        guard.reserveSortWork(values.len())?;
        gocompat::sort::Float64s(values);
    }
    for values in index.yByX.values_mut() {
        guard.reserveSortWork(values.len())?;
        gocompat::sort::Float64s(values);
    }

    // A port-to-candidate segment is axis aligned. Precompute the graph-order
    // blockers that can intersect each port axis.
    if let Some(graph) = graph {
        let nodes = graph.b().Nodes.clone();
        // (eligible, left, right, top, bottom)
        let ext: Vec<(bool, f64, f64, f64, f64)> = nodes
            .iter()
            .map(|node| {
                if !node.has_tl() || node.IsContainer() || fixedOverlaps.contains(node) {
                    return (false, 0.0, 0.0, 0.0, 0.0);
                }
                let tl = node.tlv();
                let (w, h) = (node.w(), node.h());
                (true, math::Min(tl.X, tl.X + w), math::Max(tl.X, tl.X + w), math::Min(tl.Y, tl.Y + h), math::Max(tl.Y, tl.Y + h))
            })
            .collect();
        let xs: Vec<u64> = index.ownersByX.keys().cloned().collect();
        for xk in xs {
            guard.step()?;
            let x = f64::from_bits(xk);
            for (i, node) in nodes.iter().enumerate() {
                guard.step()?;
                let (ok, left, right, _, _) = ext[i];
                if !ok {
                    continue;
                }
                if left <= x && x <= right {
                    index.verticalBlockers.entry(xk).or_default().push(node.clone());
                }
            }
        }
        let ys: Vec<u64> = index.ownersByY.keys().cloned().collect();
        for yk in ys {
            guard.step()?;
            let y = f64::from_bits(yk);
            for (i, node) in nodes.iter().enumerate() {
                guard.step()?;
                let (ok, _, _, top, bottom) = ext[i];
                if !ok {
                    continue;
                }
                if top <= y && y <= bottom {
                    index.horizontalBlockers.entry(yk).or_default().push(node.clone());
                }
            }
        }
    }
    Ok(index)
}

pub(crate) struct ovgAlignedOwners<'a> {
    pub(crate) byX: &'a [usize],
    pub(crate) byY: &'a [usize],
}

impl ovgPortIndex {
    /// Owner indexes are appended in ascending order during index construction.
    /// Merge the two axis lists so a query visits only aligned owners, once each,
    /// in the same order as a full owners scan.
    pub(crate) fn alignedOwners(&self, x: f64, y: f64) -> ovgAlignedOwners<'_> {
        ovgAlignedOwners {
            byX: self.ownersByX.get(&f64key(x)).map_or(&[], |v| v.as_slice()),
            byY: self.ownersByY.get(&f64key(y)).map_or(&[], |v| v.as_slice()),
        }
    }

    pub(crate) fn tooClose(&self, x: f64, y: f64, distance: f64, guard: &mut ovgBuildGuard) -> GoResult<bool> {
        let tooClose = guard.hasCoordinateWithin(self.xByY.get(&f64key(y)).map_or(&[], |v| v.as_slice()), x, distance)?;
        if tooClose {
            return Ok(tooClose);
        }
        guard.hasCoordinateWithin(self.yByX.get(&f64key(x)).map_or(&[], |v| v.as_slice()), y, distance)
    }
}

impl<'a> ovgAlignedOwners<'a> {
    pub(crate) fn next(&mut self, guard: &mut ovgBuildGuard) -> GoResult<(usize, bool)> {
        guard.step()?;
        if self.byX.is_empty() && self.byY.is_empty() {
            return Ok((0, false));
        }
        let owner = if self.byY.is_empty() || !self.byX.is_empty() && self.byX[0] < self.byY[0] { self.byX[0] } else { self.byY[0] };
        while !self.byX.is_empty() && self.byX[0] == owner {
            guard.step()?;
            self.byX = &self.byX[1..];
        }
        while !self.byY.is_empty() && self.byY[0] == owner {
            guard.step()?;
            self.byY = &self.byY[1..];
        }
        Ok((owner, true))
    }
}
