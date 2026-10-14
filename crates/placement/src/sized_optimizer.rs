// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/placement/sized_optimizer.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of placement/sized_optimizer.go
use crate::*;
use gocompat::Ctx;
use gocompat::rand::RandRef;
use gocompat::{errorf, math, sort, wrapf, GoResult, Map, Set};
use layoutgraph::{nodeEq, EdgeAbductionRef, EdgeRef, GraphRef, LongDistanceNeighborRequirements, NodeRef};
use limits::OptimizationWorkGuardRef;

pub struct sizedOptimizer {
    pub mutationScratch: optimizerMutationSnapshot,

    pub g: GraphRef,
    /// `None` is Go's nil slice.
    pub edgeAbductions: Option<Vec<EdgeAbductionRef>>,
    /// Port-only: the sized scoring options, built once (Go builds the same
    /// literal at every call).
    pub edgeLengthOpts: placementcost::EdgeLengthOptions,
    pub randGenerator: Option<RandRef>,
    pub fixedOrigin: Option<geo::Point>,
    pub symmetryCost: f64,
    pub cellSize: f64,
    pub obstacles: Vec<geo::Box>,
    pub checkedPositions: Option<Set<geo::Point>>,
    pub placementScratch: placementPointsScratch,
    pub spatialIndex: optimizerSpatialIndex,
}

pub fn iterPlacementsAroundPoint(
    node: &NodeRef,
    mut x: f64,
    mut y: f64,
    minimizingSelf: bool,
    apply: &mut dyn FnMut(f64, f64) -> bool,
) {
    let (width, height, cellSize) = {
        let n = node.b();
        let bx = n.Box.b();
        let cs = n.Graph.as_ref().expect("nil pointer dereference: node.Graph").b().CellSize;
        (bx.Width, bx.Height, cs)
    };
    x *= cellSize;
    y *= cellSize;
    let increment = cellSize;
    if apply(x, y) || !minimizingSelf {
        return;
    }
    let mut currentX = x - width;
    while currentX <= x {
        let mut currentY = y - height;
        while currentY <= y {
            if !(currentX == x && currentY == y) {
                if apply(currentX, currentY) {
                    return;
                }
            }
            currentY += increment;
        }
        currentX += increment;
    }
}

// withHubSpokesSuppressed temporarily removes a hub's spoke edges while fn
// evaluates placements. The original edge slice and ordering are restored on
// success, error, or panic.
pub fn withHubSpokesSuppressed(
    node: &NodeRef,
    spokes: &[NodeRef],
    guard: &OptimizationWorkGuardRef,
    f: impl FnOnce() -> GoResult<()>,
) -> GoResult<()> {
    let originalEdges: Vec<EdgeRef> = node.edges();
    if spokes.len() as i64 > limits::MaxEngineNodes || originalEdges.len() as i64 > limits::MaxEngineEdges {
        return Err(errorf!("TALA LocalOptimize hub suppression inputs exceed engine limits"));
    }
    let mut removed = vec![false; originalEdges.len()];
    let result = (|| -> GoResult<()> {
        for spoke in spokes {
            guard.Step()?;
            for (i, edge) in originalEdges.iter().enumerate() {
                guard.Step()?;
                if removed[i] {
                    continue;
                }
                let (from, to) = {
                    let e = edge.b();
                    (e.From.clone(), e.To.clone())
                };
                if from != *node && to != *node {
                    return Err(errorf!("TALA LocalOptimize found a malformed hub edge"));
                }
                if node.Adjacent(edge) == *spoke {
                    removed[i] = true;
                    break;
                }
            }
        }
        let mut workingEdges: Vec<EdgeRef> = Vec::with_capacity(originalEdges.len());
        for (i, edge) in originalEdges.iter().enumerate() {
            guard.Step()?;
            if !removed[i] {
                workingEdges.push(edge.clone());
            }
        }
        node.bm().Edges = workingEdges;
        f()
    })();
    node.bm().Edges = originalEdges;
    result
}

pub fn newSizedOptimizer(
    ctx: &Ctx,
    g: &GraphRef,
    root: Option<&NodeRef>,
    abductions: Option<&[EdgeAbductionRef]>,
    randGenerator: Option<&RandRef>,
    obstacles: &[geo::Box],
) -> GoResult<sizedOptimizer> {
    let (nodes, edgeCount, cellSize) = {
        let gb = g.b();
        (gb.Nodes.clone(), gb.Edges.len(), gb.CellSize)
    };
    if nodes.len() as i64 > limits::MaxEngineNodes {
        return Err(errorf!("TALA LocalOptimize node count exceeds limit {}", limits::MaxEngineNodes));
    }
    if edgeCount as i64 > limits::MaxEngineEdges {
        return Err(errorf!("TALA LocalOptimize edge count exceeds limit {}", limits::MaxEngineEdges));
    }
    let abductionSlice: &[EdgeAbductionRef] = abductions.unwrap_or(&[]);
    if abductionSlice.len() as i64 > limits::MaxEngineEdges {
        return Err(errorf!("TALA LocalOptimize edge abduction count exceeds limit {}", limits::MaxEngineEdges));
    }
    if obstacles.len() as i64 > layoutgraph::MaxTopologyReferences {
        return Err(errorf!(
            "TALA LocalOptimize obstacle count exceeds limit {}",
            layoutgraph::MaxTopologyReferences
        ));
    }
    if cellSize < 1.0 || math::IsNaN(cellSize) || math::IsInf(cellSize, 0) || math::Trunc(cellSize) != cellSize {
        return Err(errorf!("TALA LocalOptimize requires a finite positive integer cell size"));
    }
    let guard = limits::NewOptimizationWorkGuard(ctx, "LocalOptimizeSetup", limits::MaxOptimizationWorkUnits)?;
    for node in &nodes {
        guard.Step()?;
        let Some(ng) = node.graph() else {
            return Err(errorf!("TALA LocalOptimize found a node without a graph"));
        };
        let (ne, nn) = {
            let n = node.b();
            (n.Edges.len(), n.Nears.len())
        };
        if ne as i64 > limits::MaxEngineEdges || nn as i64 > limits::MaxEngineNodes {
            return Err(errorf!("TALA LocalOptimize node {} adjacency references exceed engine limits", node.DebugID()));
        }
        if ng.b().CellSize != cellSize {
            return Err(errorf!("cell size mismatch for node {}", node.DebugID()));
        }
    }
    let fixedOrigin = g.ContainerFixedOrigin(root).map(|p| *p.b());
    let optim = sizedOptimizer {
        mutationScratch: optimizerMutationSnapshot::default(),
        g: g.clone(),
        edgeAbductions: abductions.map(|a| a.to_vec()),
        edgeLengthOpts: edgeLengthOptions(abductions, true),
        randGenerator: randGenerator.cloned(),
        fixedOrigin,
        symmetryCost: cellSize,
        cellSize,
        obstacles: obstacles.to_vec(),
        checkedPositions: None,
        placementScratch: placementPointsScratch::default(),
        spatialIndex: optimizerSpatialIndex::default(),
    };
    for n in &nodes {
        guard.Step()?;

        // Pre-process all edge abductions that affect this node
        let mut abductedEdges: Set<EdgeRef> = Set::default();
        let mut adjReplacements: Map<NodeRef, NodeRef> = Map::default();
        for ea in abductionSlice {
            guard.Step()?;
            let ea = ea.b();
            if nodeEq(ea.CurrentFrom.as_ref(), Some(n)) || nodeEq(ea.CurrentTo.as_ref(), Some(n)) {
                if ea.OriginallyFrom.is_some() || ea.OriginallyTo.is_some() {
                    abductedEdges.insert(ea.Edge.clone());
                    continue;
                }
                let adj = n.Adjacent(&ea.Edge);
                if nodeEq(ea.CurrentFrom.as_ref(), Some(n)) && ea.OriginallyTo.is_some() {
                    adjReplacements.insert(adj, ea.OriginallyTo.clone().unwrap());
                } else if nodeEq(ea.CurrentTo.as_ref(), Some(n)) && ea.OriginallyFrom.is_some() {
                    adjReplacements.insert(adj, ea.OriginallyFrom.clone().unwrap());
                }
            }
        }

        let cellSizeInt = math::F64ToInt(optim.cellSize);
        let mut spaceReqs: Map<NodeRef, LongDistanceNeighborRequirements> = Map::default();

        let edges = n.edges();
        for edge in &edges {
            guard.Step()?;
            let (from, to, minWidth, minHeight) = {
                let e = edge.b();
                (e.From.clone(), e.To.clone(), e.MinWidth, e.MinHeight)
            };
            if from != *n && to != *n {
                return Err(errorf!("TALA LocalOptimize found a malformed incident edge on {}", n.DebugID()));
            }
            if abductedEdges.contains(edge) {
                continue;
            }

            let mut adj = n.Adjacent(edge);
            if let Some(replacement) = adjReplacements.get(&adj) {
                adj = replacement.clone();
            }

            let requirements = spaceReqs.entry(adj).or_default();
            requirements.EdgeCount += 1;
            if minWidth > requirements.MaxWidth {
                requirements.MaxWidth = minWidth;
            }
            if minHeight > requirements.MaxHeight {
                requirements.MaxHeight = minHeight;
            }
        }

        // Go ranges over the map and stops at the first qualifying entry; the
        // outcome (assigning the whole map) does not depend on the order, only
        // the number of guard steps until the hit does.
        let mut assign = false;
        for requirements in spaceReqs.values() {
            guard.Step()?;
            if requirements.EdgeCount < 3 {
                continue;
            }
            if requirements.MaxWidth <= cellSizeInt && requirements.MaxHeight <= cellSizeInt {
                continue;
            }
            assign = true;
            break;
        }
        if assign {
            n.bm().LongDistanceNeighborRequirements = spaceReqs;
        }
    }

    guard.Finish()?;
    Ok(optim)
}

// placementPointsScratch holds reusable candidate-generation memory.
#[derive(Default)]
pub struct placementPointsScratch {
    pub seen: Set<u64>,
    pub points: Vec<geo::Point>,
}

impl sizedOptimizer {
    #[inline]
    fn abductionSlice(&self) -> &[EdgeAbductionRef] {
        self.edgeAbductions.as_deref().unwrap_or(&[])
    }

    pub fn optimize(&mut self, ctx: &Ctx, temp: f64) -> GoResult<bool> {
        self.optimizeWithLimit(ctx, temp, limits::MaxOptimizationWorkUnits)
    }

    pub fn optimizeWithLimit(&mut self, ctx: &Ctx, temp: f64, workLimit: u64) -> GoResult<bool> {
        let (ctx, _) = layoutgraph::EnsureTransactionWorkGuard(ctx, "LocalOptimizeTransactions")?;
        let guard = limits::NewOptimizationWorkGuard(&ctx, "LocalOptimize", workLimit)?;
        if self.randGenerator.is_none() {
            return Err(errorf!("TALA LocalOptimize requires a random generator"));
        }
        let mut snapshot = std::mem::take(&mut self.mutationScratch);
        let result = (|| -> GoResult<bool> {
            captureOptimizerMutationStateInto(&self.g, &guard, &mut snapshot)?;
            let result = self.optimizeGuarded(&ctx, temp, &guard).and_then(|changed| {
                guard.Finish()?;
                Ok(changed)
            });
            if result.is_err() {
                snapshot.restore();
            }
            result
        })();
        snapshot.release();
        self.mutationScratch = snapshot;
        result
    }

    pub fn optimizeGuarded(&mut self, ctx: &Ctx, temp: f64, guard: &OptimizationWorkGuardRef) -> GoResult<bool> {
        let mut changed = false;
        let rng = self.randGenerator.clone().expect("random generator");

        let nodeCount = self.g.b().Nodes.len();
        if nodeCount as i64 > limits::MaxEngineNodes {
            return Err(errorf!("TALA LocalOptimize node count exceeds limit {}", limits::MaxEngineNodes));
        }
        let mut nodeIndices: Vec<usize> = Vec::with_capacity(nodeCount);
        for i in 0..nodeCount {
            guard.Step()?;
            nodeIndices.push(i);
        }
        limits::Shuffle(&mut nodeIndices, &mut rng.bm(), &mut guard.bm())?;

        for &nodeIndex in &nodeIndices {
            guard.Step()?;
            let node = self.g.b().Nodes[nodeIndex].clone();
            let (hasTL, hasFixed, width, height, noEdges) = {
                let n = node.b();
                let bx = n.Box.b();
                (bx.TopLeft.is_some(), n.FixedTopLeft.is_some(), bx.Width, bx.Height, n.Edges.is_empty())
            };
            if !hasTL {
                return Err(errorf!("TALA LocalOptimize found an unpositioned graph node"));
            }
            if hasFixed {
                continue;
            }
            if width > 100.0 * self.cellSize || height > 100.0 * self.cellSize {
                // leave giant node where it is, we won't find an unoccupied point within 100 cellSize
                continue;
            }
            if noEdges {
                let mut hasUsableNear = false;
                let (nears, container) = {
                    let n = node.b();
                    (n.Nears.iter().cloned().collect::<Vec<_>>(), n.Container.clone())
                };
                for near in &nears {
                    guard.Step()?;
                    let isDescendant = optimizerIsDescendantOf(Some(near), container.as_ref(), guard)?;
                    if isDescendant {
                        hasUsableNear = true;
                        break;
                    }
                }
                if !hasUsableNear {
                    continue;
                }
            }
            if self.g.b().NodeToTree.has(&node) {
                continue;
            }

            let protrudingChildren = self.protrudingChildrenGuarded(&node, guard)?;
            let minimizingSelf = protrudingChildren.is_empty();

            let medianPoint = self.medianPointGuarded(&node, temp, &protrudingChildren, guard)?;
            // All candidate placements for this node see the same boxes for every
            // other graph node. Build one ordered broad-phase index and reuse it until
            // the optimizer commits this node's final position.
            self.rebuildSpatialIndex(guard)?;

            // Making and using a cache takes many allocs. Only worth it when this node has to search through many siblings for overlap
            let mut checkedPositionsCache: Option<Set<geo::Point>> = None;
            if nodeIndices.len() > 10 {
                let mut m = self.checkedPositions.take().unwrap_or_else(|| Set::with_capacity_and_hasher(64, Default::default()));
                m.clear();
                checkedPositionsCache = Some(m);
            }
            let result = self.optimizeNode(ctx, temp, &node, &medianPoint, &protrudingChildren, minimizingSelf, &mut checkedPositionsCache, &rng, guard);
            if let Some(m) = checkedPositionsCache {
                self.checkedPositions = Some(m);
            }
            if result? {
                changed = true;
            }
        }
        guard.Finish()?;
        Ok(changed)
    }

    /// Body of Go's per-node loop after the cache setup; returns whether the
    /// node changed the layout.
    fn optimizeNode(
        &mut self,
        ctx: &Ctx,
        temp: f64,
        node: &NodeRef,
        medianPoint: &geo::Point,
        protrudingChildren: &[NodeRef],
        minimizingSelf: bool,
        checked: &mut Option<Set<geo::Point>>,
        rng: &RandRef,
        guard: &OptimizationWorkGuardRef,
    ) -> GoResult<bool> {
        let mut changed = false;
        let d = self.findClosestUnoccupiedDistanceGuarded(node, medianPoint, minimizingSelf, checked.as_mut(), guard)?;

        let mut scratch = std::mem::take(&mut self.placementScratch);
        let moved = (|| -> GoResult<bool> {
            self.fillPlacementPointsGuarded(node, medianPoint, d, minimizingSelf, &mut scratch, guard)?;
            limits::Shuffle(&mut scratch.points, &mut rng.bm(), &mut guard.bm())?;

            // we want mustImprove to be true for the last 10 optimize calls where temp is 0
            self.moveNodeToBestGuarded(ctx, node, &scratch.points, temp == 0.0, guard)
        })();
        self.placementScratch = scratch;
        let moved = moved?;

        if !moved {
            let bestSwapCandidate = self.bestSwapCandidateGuarded(ctx, node, guard)?;
            if let Some(best) = bestSwapCandidate {
                changed = true;
                optimizerSwapPositions(node, &best, guard)?;
                self.syncHerdFencesGuarded(guard)?;
            } else {
                chargeOptimizerTranspose(&self.g, node, self.edgeAbductions.as_deref(), guard)?;
                let g = self.g.clone();
                let abductions = self.edgeAbductions.clone();
                let ok = transpose(ctx, &g, node, abductions.as_deref())?;
                guard.Step()?;
                if ok {
                    changed = true;
                } else {
                    // The spokes don't need to move because in the next iteration they'll move
                    let spokes = self.g.b().Hubs.get(node);
                    if let Some(spokes) = spokes {
                        if temp != 0.0 {
                            withHubSpokesSuppressed(node, &spokes, guard, || -> GoResult<()> {
                                let medianPoint = self.medianPointGuarded(node, temp, protrudingChildren, guard)?;

                                let d = self.findClosestUnoccupiedDistanceGuarded(
                                    node,
                                    &medianPoint,
                                    minimizingSelf,
                                    checked.as_mut(),
                                    guard,
                                )?;
                                let mut scratch = std::mem::take(&mut self.placementScratch);
                                let r = (|| -> GoResult<()> {
                                    self.fillPlacementPointsGuarded(node, &medianPoint, d, minimizingSelf, &mut scratch, guard)?;
                                    limits::Shuffle(&mut scratch.points, &mut rng.bm(), &mut guard.bm())?;
                                    self.moveNodeToBestGuarded(ctx, node, &scratch.points, temp == 0.0, guard)?;
                                    Ok(())
                                })();
                                self.placementScratch = scratch;
                                r
                            })?;
                        }
                    }
                }
            }
        } else {
            self.syncHerdFencesGuarded(guard)?;
            changed = true;
        }
        Ok(changed)
    }

    pub fn medianPointGuarded(
        &self,
        node: &NodeRef,
        temp: f64,
        protrudingChildren: &[NodeRef],
        guard: &OptimizationWorkGuardRef,
    ) -> GoResult<geo::Point> {
        let width = node.w() / self.cellSize;
        let height = node.h() / self.cellSize;
        let (mut medianX, mut medianY) = optimizerMedianToNeighbors(node, true, self.abductionSlice(), guard)?;

        let fixedOrigin = optimizerFixedOrigin(Some(&self.g), node.OwningContainer().as_ref(), guard)?;
        if let Some(fo) = &fixedOrigin {
            // we can't have node positions past the fixedOrigin so set a floor
            if medianX < fo.X / self.cellSize {
                medianX = fo.X / self.cellSize;
                // normally the random movement is within [-temp*width, temp*width],
                // but anything left of the fixed origin is invalid so make it [0, 2*temp*width]
                medianX += temp * width;
            }
            if medianY < fo.Y / self.cellSize {
                medianY = fo.Y / self.cellSize;
                medianY += temp * height;
            }
        }

        let rng = self.randGenerator.as_ref().expect("random generator");
        let r1 = rng.bm().Float64();
        medianX += (-1.0 * temp * width) + r1 * (2.0 * temp * width);
        let r2 = rng.bm().Float64();
        medianY += (-1.0 * temp * height) + r2 * (2.0 * temp * height);

        if !protrudingChildren.is_empty() {
            let (childrenMedianX, childrenMedianY) = optimizerMedian(protrudingChildren, true, guard)?;
            let tl = node.tlv();
            medianX -= childrenMedianX - tl.X / self.cellSize;
            medianY -= childrenMedianY - tl.Y / self.cellSize;
        }

        if let Some(fo) = &fixedOrigin {
            medianX = math::Max(medianX, fo.X / self.cellSize);
            medianY = math::Max(medianY, fo.Y / self.cellSize);
        }

        Ok(geo::Point { X: math::Round(medianX * self.cellSize), Y: math::Round(medianY * self.cellSize) })
    }

    pub fn protrudingChildrenGuarded(&self, node: &NodeRef, guard: &OptimizationWorkGuardRef) -> GoResult<Vec<NodeRef>> {
        // If this container were being placed, its median would be the bottom-most node, but that doesn't reflect where we want the node to be
        // We want the node to be placed such that the median of its children with protruding edges is at the calculated median
        let mut protrudingChildren: Vec<NodeRef> = Vec::new();
        for ea in self.abductionSlice() {
            guard.Step()?;
            let ea = ea.b();
            if nodeEq(ea.CurrentFrom.as_ref(), Some(node)) && ea.OriginallyFrom.is_some() {
                protrudingChildren.push(ea.OriginallyFrom.clone().unwrap());
            }
            if nodeEq(ea.CurrentTo.as_ref(), Some(node)) && ea.OriginallyTo.is_some() {
                protrudingChildren.push(ea.OriginallyTo.clone().unwrap());
            }
        }
        Ok(protrudingChildren)
    }

    pub fn findClosestUnoccupiedDistanceGuarded(
        &mut self,
        node: &NodeRef,
        p: &geo::Point,
        minimizingSelf: bool,
        mut checked: Option<&mut Set<geo::Point>>,
        guard: &OptimizationWorkGuardRef,
    ) -> GoResult<f64> {
        if node.graph().is_none() || !node.has_tl() {
            return Err(errorf!("TALA {} placement search requires a positioned node, graph, and point", guard.Location()));
        }
        if self.cellSize <= 0.0 || math::IsNaN(self.cellSize) || math::IsInf(self.cellSize, 0) {
            return Err(errorf!("TALA {} placement search requires a finite positive cell size", guard.Location()));
        }
        if let Some(c) = checked.as_deref() {
            if c.len() as i64 > maxOptimizerPlacementCandidates {
                return Err(optimizationResourceLimit(format!(
                    "TALA {} checked placement count exceeds limit {}",
                    guard.Location(),
                    maxOptimizerPlacementCandidates
                )));
            }
        }
        let cellSize = self.cellSize;
        let (nodeWidth, nodeHeight) = (node.w(), node.h());

        // For each candidate “ring” (curr is in grid units, then scaled by cellSize)
        let mut curr = 0.0f64;
        while curr <= 100.0 {
            guard.Step()?;
            let currCell = curr * cellSize;

            let mut x = currCell;
            let mut y = 0.0f64;

            while x >= -currCell {
                guard.Step()?;
                // Check the candidate at (x, -y) if y is nonzero.
                if y != 0.0 {
                    let candidate = geo::Point {
                        X: roundToNearestCellSize(p.X + x, cellSize),
                        Y: roundToNearestCellSize(p.Y - y, cellSize),
                    };
                    // Enforce fixed-origin constraint if needed.
                    if self.fixedOrigin.map_or(true, |fo| candidate.X >= fo.X && candidate.Y >= fo.Y) {
                        let unoccupied = self.findUnoccupiedGuarded(
                            x,
                            -y,
                            nodeWidth,
                            nodeHeight,
                            minimizingSelf,
                            node,
                            p,
                            checked.as_deref_mut(),
                            guard,
                        )?;
                        if unoccupied {
                            return Ok(curr);
                        }
                    }
                }
                // Check candidate at (x, y)
                let candidate = geo::Point {
                    X: roundToNearestCellSize(p.X + x, cellSize),
                    Y: roundToNearestCellSize(p.Y + y, cellSize),
                };
                if self.fixedOrigin.map_or(true, |fo| candidate.X >= fo.X && candidate.Y >= fo.Y) {
                    let unoccupied = self.findUnoccupiedGuarded(
                        x,
                        y,
                        nodeWidth,
                        nodeHeight,
                        minimizingSelf,
                        node,
                        p,
                        checked.as_deref_mut(),
                        guard,
                    )?;
                    if unoccupied {
                        return Ok(curr);
                    }
                }

                // Update y: if x > 0, increment y by cellSize; otherwise, decrement y.
                if x > 0.0 {
                    y += cellSize;
                } else {
                    y -= cellSize;
                }
                x -= cellSize;
            }
            curr += 1.0;
        }
        Err(errorf!(
            "could not find closest unoccupied distance for p=&{{{} {}}}, node={}",
            p.X,
            p.Y,
            node.DebugID()
        ))
    }

    // findUnoccupied is a tailored closest-unoccupied-distance search over placements around a node.
    // It is called often enough to warrant a specialized implementation.
    pub fn findUnoccupiedGuarded(
        &mut self,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        minimizingSelf: bool,
        node: &NodeRef,
        p: &geo::Point,
        mut checked: Option<&mut Set<geo::Point>>,
        guard: &OptimizationWorkGuardRef,
    ) -> GoResult<bool> {
        guard.Step()?;
        let mut skip = false;
        let offset = geo::Point { X: x, Y: y };
        if let Some(c) = checked.as_deref_mut() {
            if c.contains(&offset) {
                skip = true;
            } else {
                if c.len() as i64 >= maxOptimizerPlacementCandidates {
                    return Err(optimizationResourceLimit(format!(
                        "TALA {} checked placement count exceeds limit {}",
                        guard.Location(),
                        maxOptimizerPlacementCandidates
                    )));
                }
                c.insert(offset);
            }
        }

        if !skip {
            let occupied = self.isPointOccupiedGuarded(x, y, p, node, guard)?;
            if !occupied {
                return Ok(true);
            }
        }

        if !minimizingSelf {
            return Ok(false);
        }

        let mut i = x - width;
        while i <= x {
            let mut j = y - height;
            while j <= y {
                guard.Step()?;
                // Already attempted above
                if i == x && j == y {
                    j += self.cellSize;
                    continue;
                }
                if let Some(c) = checked.as_deref_mut() {
                    let offset = geo::Point { X: i, Y: j };
                    if c.contains(&offset) {
                        j += self.cellSize;
                        continue;
                    }
                    if c.len() as i64 >= maxOptimizerPlacementCandidates {
                        return Err(optimizationResourceLimit(format!(
                            "TALA {} checked placement count exceeds limit {}",
                            guard.Location(),
                            maxOptimizerPlacementCandidates
                        )));
                    }
                    c.insert(offset);
                }
                let occupied = self.isPointOccupiedGuarded(i, j, p, node, guard)?;
                if !occupied {
                    return Ok(true);
                }
                j += self.cellSize;
            }
            i += self.cellSize;
        }
        Ok(false)
    }

    pub fn isPointOccupiedGuarded(
        &mut self,
        x: f64,
        y: f64,
        p: &geo::Point,
        node: &NodeRef,
        guard: &OptimizationWorkGuardRef,
    ) -> GoResult<bool> {
        let point = geo::Point {
            X: roundToNearestCellSize(p.X + x, self.cellSize),
            Y: roundToNearestCellSize(p.Y + y, self.cellSize),
        };
        if let Some(fo) = &self.fixedOrigin {
            if point.X < fo.X || point.Y < fo.Y {
                return Ok(true);
            }
        }
        let (_, occupied) = self.indexedIsOccupied(&point, guard)?;
        if !occupied {
            let pointRef = geo::PointRef::new(point);
            let overlaps = self.indexedDoesOverlap(node, &pointRef, std::slice::from_ref(node), guard)?;
            if !overlaps {
                return Ok(false);
            }
        }
        Ok(true)
    }

    // fillPlacementPointsGuarded fills caller-owned scratch so the optimizer can
    // reuse candidate storage across its generate-shuffle-score loop.
    pub fn fillPlacementPointsGuarded(
        &self,
        node: &NodeRef,
        median: &geo::Point,
        minUnocc: f64,
        minimizingSelf: bool,
        scratch: &mut placementPointsScratch,
        guard: &OptimizationWorkGuardRef,
    ) -> GoResult<()> {
        if !node.has_tl() {
            return Err(errorf!("TALA {} placement generation requires a positioned node and median", guard.Location()));
        }
        if minUnocc < 0.0 || minUnocc > 100.0 || math::IsNaN(minUnocc) || math::IsInf(minUnocc, 0) {
            return Err(errorf!("TALA {} placement distance must be finite and within [0, 100]", guard.Location()));
        }
        if self.cellSize <= 0.0 || math::IsNaN(self.cellSize) || math::IsInf(self.cellSize, 0) {
            return Err(errorf!("TALA {} placement generation requires a finite positive cell size", guard.Location()));
        }

        // Clear data structures for reuse
        for _ in 0..scratch.seen.len() {
            guard.Step()?;
        }
        scratch.seen.clear();
        scratch.points.clear();

        let dist = minUnocc + 1.0;
        let cellSize = self.cellSize;
        let fixedOrigin = self.fixedOrigin;

        let addErr: std::cell::RefCell<Option<gocompat::GoError>> = std::cell::RefCell::new(None);
        {
            let seen = &mut scratch.seen;
            let points = &mut scratch.points;
            let mut add = |offX: f64, offY: f64| -> bool {
                if addErr.borrow().is_some() {
                    return true;
                }
                if let Err(err) = guard.Step() {
                    *addErr.borrow_mut() = Some(err);
                    return true;
                }
                let pX = roundToNearestCellSize(median.X + offX, cellSize);
                let pY = roundToNearestCellSize(median.Y + offY, cellSize);

                // Early rejection based on fixed origin
                if let Some(fo) = &fixedOrigin {
                    if pX < fo.X || pY < fo.Y {
                        return false;
                    }
                }

                // Encode coordinates to a single 64-bit value for map lookup
                let ix = math::F64ToInt(pX / cellSize);
                let iy = math::F64ToInt(pY / cellSize);
                let key = ((ix as u32 as u64) << 32) | (iy as u32 as u64);

                if seen.contains(&key) {
                    return false;
                }
                if points.len() as i64 >= maxOptimizerPlacementCandidates {
                    *addErr.borrow_mut() = Some(optimizationResourceLimit(format!(
                        "TALA LocalOptimize placement candidates exceed limit {}",
                        maxOptimizerPlacementCandidates
                    )));
                    return true;
                }

                seen.insert(key);
                points.push(geo::Point { X: pX, Y: pY });
                false
            };

            // Generate placement points in diamond pattern
            let mut x = dist;
            while x >= -dist {
                guard.Step()?;
                let maxY = dist - math::Abs(x);
                let mut y = 0.0f64;
                while y <= maxY {
                    guard.Step()?;
                    if y != 0.0 {
                        iterPlacementsAroundPoint(node, x, -y, minimizingSelf, &mut add);
                    }
                    iterPlacementsAroundPoint(node, x, y, minimizingSelf, &mut add);
                    if let Some(err) = addErr.borrow_mut().take() {
                        return Err(err);
                    }
                    y += 1.0;
                }
                x -= 1.0;
            }

            // Process long-distance neighbor requirements.
            let data = {
                let n = node.b();
                if n.LongDistanceNeighborRequirements.is_empty() {
                    None
                } else {
                    Some(n.LongDistanceNeighborRequirements.clone())
                }
            };
            if let Some(data) = data {
                if data.len() as i64 > layoutgraph::MaxTopologyReferences {
                    return Err(errorf!(
                        "TALA {} long-distance neighbor references exceed limit {}",
                        guard.Location(),
                        layoutgraph::MaxTopologyReferences
                    ));
                }
                let cell = math::F64ToInt(cellSize) as f64;

                let mut neighbors: Vec<NodeRef> = Vec::with_capacity(data.len());
                for adjacent in data.keys() {
                    guard.Step()?;
                    neighbors.push(adjacent.clone());
                }
                guard.AddSort(neighbors.len() as i64)?;
                sort::SlicesSortFunc(&mut neighbors, |a, b| sort::cmpCompare(a.IDValue(), b.IDValue()));

                let (nodeWidth, nodeHeight) = (node.w(), node.h());
                for adj in &neighbors {
                    guard.Step()?;
                    let requirements = data[adj];
                    let Some(atl) = adj.tl() else {
                        return Err(errorf!("TALA {} found an unpositioned long-distance neighbor", guard.Location()));
                    };
                    let atl = *atl.b();
                    let maxW = requirements.MaxWidth;
                    let maxH = requirements.MaxHeight;

                    let ax = atl.X - median.X;
                    let ay = atl.Y - median.Y;

                    // Add specific placement points for width
                    if maxW > math::F64ToInt(cell) {
                        add(math::Floor((ax - nodeWidth - maxW as f64) / cell) * cell, ay);
                        add(math::Ceil((ax + adj.w() + maxW as f64) / cell) * cell, ay);
                    }

                    // Add specific placement points for height
                    if maxH > math::F64ToInt(cell) {
                        add(ax, math::Floor((ay - nodeHeight - maxH as f64) / cell) * cell);
                        add(ax, math::Ceil((ay + adj.h() + maxH as f64) / cell) * cell);
                    }
                    if let Some(err) = addErr.borrow_mut().take() {
                        return Err(err);
                    }
                }
            }
        }

        // Add current position for herd assignments
        if node.b().HerdAssignment.is_some() {
            if scratch.points.len() as i64 >= maxOptimizerPlacementCandidates {
                return Err(optimizationResourceLimit(format!(
                    "TALA LocalOptimize placement candidates exceed limit {}",
                    maxOptimizerPlacementCandidates
                )));
            }
            scratch.points.push(node.tlv());
        }

        // Preserve the original work charge even when the internal caller consumes
        // the scratch slice directly.
        guard.Add(scratch.points.len() as u64)
    }

    #[inline]
    fn scoreExtras(&self, ctx: &Ctx, node: &NodeRef) -> GoResult<(f64, f64)> {
        let abd = self.edgeAbductions.as_deref();
        let columnCrossingCost = placementcost::ColumnCrossingCost(ctx, node, abd)?;
        let symmetry = placementcost::NodeSymmetry(ctx, node, abd)?;
        Ok((columnCrossingCost, symmetry))
    }

    pub fn moveNodeToBestGuarded(
        &mut self,
        ctx: &Ctx,
        node: &NodeRef,
        points: &[geo::Point],
        mustImprove: bool,
        guard: &OptimizationWorkGuardRef,
    ) -> GoResult<bool> {
        let abductionsLen = self.abductionSlice().len();
        let mut scorer = placementcost::NewNodeEdgeLengthScorer(node, self.edgeLengthOpts.clone());
        let result = (|| -> GoResult<bool> {
            let symmetryCost = self.cellSize * node.b().Edges.len() as f64;
            let mut leastDistance = math::Inf(1);
            if mustImprove {
                chargeOptimizerScoring(node, abductionsLen, true, guard)?;
                leastDistance = scorer.Score(ctx)?;
                let (columnCrossingCost, symmetry) = self.scoreExtras(ctx, node)?;
                leastDistance += columnCrossingCost;
                leastDistance -= symmetry * symmetryCost;
            }
            let current = node.tlv();
            let mut leastDistancePoint = current;
            let (currentNodeX, currentNodeY) = (current.X, current.Y);
            let movement = captureOptimizerCandidateMovement(node, guard)?;
            let originalPositions = &movement.positions;
            let fail = |err| {
                restoreNodePositions(originalPositions);
                Err(err)
            };

            for point in points {
                if let Err(err) = guard.Step() {
                    return fail(err);
                }
                let (x, y) = (point.X, point.Y);
                if let Some(fo) = &self.fixedOrigin {
                    if x < fo.X || y < fo.Y {
                        continue;
                    }
                }
                // If the candidate position is the original one, we should be able to move back to it (if we moved somewhere else in this routine)
                let mut canMove = x == currentNodeX && y == currentNodeY;
                if !canMove {
                    let pointRef = geo::PointRef::new(*point);
                    canMove = match self.indexedCanMove(node, &pointRef, guard) {
                        Ok(v) => v,
                        Err(err) => return fail(err),
                    };
                }
                if canMove {
                    if let Err(err) = movement.moveAbs(x, y, guard) {
                        return fail(err);
                    }
                    if let Err(err) = chargeOptimizerScoring(node, abductionsLen, true, guard) {
                        return fail(err);
                    }

                    let mut edgeLength = match scorer.Score(ctx) {
                        Ok(v) => v,
                        Err(err) => return fail(err),
                    };
                    if !math::IsInf(leastDistance, 1)
                        && geo::PrecisionCompare(edgeLength - symmetryCost, leastDistance, geo::PRECISION) == 1
                    {
                        // no need to continue, this is clearly worse
                        continue;
                    }
                    let (columnCrossingCost, symmetry) = match self.scoreExtras(ctx, node) {
                        Ok(v) => v,
                        Err(err) => return fail(err),
                    };
                    edgeLength += columnCrossingCost;
                    edgeLength -= symmetry * symmetryCost;

                    if !self.obstacles.is_empty() {
                        let nodeBox = node.boxValue();
                        for o in &self.obstacles {
                            if let Err(err) = guard.Step() {
                                return fail(err);
                            }
                            let otl = *o.TL().b();
                            if otl.X == -(layoutgraph::ContainerPadding as f64) && otl.Y == -(layoutgraph::ContainerPadding as f64) {
                                continue;
                            }
                            if nodeBox.Overlaps(o) {
                                edgeLength += 3.0 * self.g.TurnCost();
                            }
                        }
                    }

                    match geo::PrecisionCompare(edgeLength, leastDistance, geo::PRECISION) {
                        -1 => {
                            leastDistance = edgeLength;
                            leastDistancePoint = *point;
                        }
                        0 => {
                            if point.X == currentNodeX && point.Y == currentNodeY {
                                leastDistance = edgeLength;
                                leastDistancePoint = *point;
                            }
                        }
                        _ => {}
                    }
                }
            }

            if let Some(err) = ctx.Err() {
                return fail(wrapf!(err, "EdgeLength: {}", err));
            }
            if math::IsInf(leastDistance, 1) {
                return fail(errorf!("sizedOptimizer: could not find any placement"));
            }

            if let Err(err) = movement.moveAbs(leastDistancePoint.X, leastDistancePoint.Y, guard) {
                return fail(err);
            }

            let after = node.tlv();
            Ok(after.X != currentNodeX || after.Y != currentNodeY)
        })();
        scorer.Close();
        result
    }

    /// NodeEdgeLength + column crossing − symmetry, as bestSwapCandidate scores
    /// a node.
    fn swapScore(&self, ctx: &Ctx, node: &NodeRef) -> GoResult<f64> {
        let mut l = placementcost::NodeEdgeLength(ctx, node, &self.edgeLengthOpts)?;
        let (columnCrossingCost, symmetry) = self.scoreExtras(ctx, node)?;
        l += columnCrossingCost;
        l -= symmetry * self.symmetryCost * node.b().Edges.len() as f64;
        Ok(l)
    }

    pub fn bestSwapCandidateGuarded(&mut self, ctx: &Ctx, node: &NodeRef, guard: &OptimizationWorkGuardRef) -> GoResult<Option<NodeRef>> {
        let mut minSwapL = math::Inf(1);
        let abductionsLen = self.abductionSlice().len();
        let rng = self.randGenerator.clone().expect("random generator");

        chargeOptimizerScoring(node, abductionsLen, true, guard)?;
        let currentL1 = self.swapScore(ctx, node)?;
        let mut bestSwapCandidate: Option<NodeRef> = None;

        let nodes = self.g.b().Nodes.clone();
        let mut swapCandidateIndices: Vec<usize> = Vec::with_capacity(nodes.len());
        for i in 0..nodes.len() {
            guard.Step()?;
            swapCandidateIndices.push(i);
        }
        limits::Shuffle(&mut swapCandidateIndices, &mut rng.bm(), &mut guard.bm())?;

        for &swapCandidateIndex in &swapCandidateIndices {
            guard.Step()?;
            let swapCandidate = &nodes[swapCandidateIndex];
            if swapCandidate == node {
                continue;
            }
            if swapCandidate.b().FixedTopLeft.is_some() {
                continue;
            }
            if !swapCandidate.IsAdjacentTo(node, true) {
                continue;
            }
            if self.g.b().NodeToTree.has(swapCandidate) {
                continue;
            }
            let nodeTL = node.tl().expect("nil pointer dereference: node.TopLeft");
            let firstOverlap = self.indexedDoesOverlap(swapCandidate, &nodeTL, std::slice::from_ref(node), guard)?;
            let candTL = swapCandidate.tl().expect("nil pointer dereference: node.TopLeft");
            let secondOverlap = self.indexedDoesOverlap(node, &candTL, std::slice::from_ref(swapCandidate), guard)?;
            if firstOverlap || secondOverlap {
                continue;
            }

            chargeOptimizerScoring(swapCandidate, abductionsLen, true, guard)?;
            let currentL2 = self.swapScore(ctx, swapCandidate)?;

            let mut swappedL1 = 0.0f64;
            let mut swappedL2 = 0.0f64;
            let mut validSwap = false;
            {
                let this = &*self;
                withOptimizerPositionsSwapped(node, swapCandidate, guard, || -> GoResult<()> {
                    // Have to check for overlap again, since swapping two of different sizes
                    // may cause one to overlap with the other -- so, no exceptions in this check
                    let candTL = swapCandidate.tl().expect("nil pointer dereference: node.TopLeft");
                    let firstOverlap = optimizerDoesOverlap(swapCandidate, &candTL, &[], guard)?;
                    let nodeTL = node.tl().expect("nil pointer dereference: node.TopLeft");
                    let secondOverlap = optimizerDoesOverlap(node, &nodeTL, &[], guard)?;
                    if firstOverlap || secondOverlap {
                        return Ok(());
                    }
                    validSwap = true;

                    chargeOptimizerScoring(node, abductionsLen, true, guard)?;
                    swappedL1 = this.swapScore(ctx, node)?;

                    // No need to compute it if we already know it won't be a good swap.
                    if geo::PrecisionCompare(swappedL1, currentL1, geo::PRECISION) < 0 {
                        chargeOptimizerScoring(swapCandidate, abductionsLen, true, guard)?;
                        swappedL2 = this.swapScore(ctx, swapCandidate)?;
                    }
                    Ok(())
                })?;
            }
            if !validSwap {
                continue;
            }

            if geo::PrecisionCompare(swappedL1, currentL1, geo::PRECISION) < 0
                && geo::PrecisionCompare(swappedL1 + swappedL2, currentL1 + currentL2, geo::PRECISION) < 0
                && geo::PrecisionCompare(swappedL1 + swappedL2, minSwapL, geo::PRECISION) < 0
            {
                minSwapL = swappedL1 + swappedL2;
                bestSwapCandidate = Some(swapCandidate.clone());
            }
        }

        if let Some(err) = ctx.Err() {
            return Err(wrapf!(err, "EdgeLength: {}", err));
        }
        Ok(bestSwapCandidate)
    }

    pub fn syncHerdFencesGuarded(&self, guard: &OptimizationWorkGuardRef) -> GoResult<()> {
        // Preserve the legacy bounding-box semantics (outside labels, modifier
        // elements, loop offsets, routed edges, and fixed origins). Precharging the
        // bounded quadratic scan keeps it under the shared operation budget.
        let (nodes, edges) = {
            let gb = self.g.b();
            (gb.Nodes.len() as u64, gb.Edges.len() as u64)
        };
        guard.AddProduct(nodes, nodes + 1)?;
        guard.Add(edges)?;
        proximity::SyncHerdFences(&self.g);
        guard.Finish()
    }
}
