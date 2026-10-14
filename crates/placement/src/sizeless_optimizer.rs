// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/placement/sizeless_optimizer.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of placement/sizeless_optimizer.go
use crate::*;
use gocompat::Ctx;
use gocompat::rand::RandRef;
use gocompat::{errorf, math, wrapf, GoResult, Map};
use layoutgraph::{GraphRef, NodeRef};
use limits::OptimizationWorkGuardRef;

// sizelessOptimizer holds the state during the first part (sizeless) of the optimization routine described in
// Graph Compact Orthogonal Layout Algorithm by Freivalds and Glagolevs
pub struct sizelessOptimizer {
    pub mutationScratch: optimizerMutationSnapshot,

    pub nodes: Vec<NodeRef>,
    pub g: GraphRef,
    pub randGenerator: Option<RandRef>,

    pub occupied: Map<geo::Point, NodeRef>,
}

pub fn newSizelessOptimizer(ctx: &Ctx, g: &GraphRef, randGenerator: Option<&RandRef>) -> GoResult<sizelessOptimizer> {
    let (nodes, edgeCount, cellSize) = {
        let gb = g.b();
        (gb.Nodes.clone(), gb.Edges.len(), gb.CellSize)
    };
    if nodes.len() as i64 > limits::MaxEngineNodes {
        return Err(errorf!("TALA sizelessOptimizer node count exceeds limit {}", limits::MaxEngineNodes));
    }
    if edgeCount as i64 > limits::MaxEngineEdges {
        return Err(errorf!("TALA sizelessOptimizer edge count exceeds limit {}", limits::MaxEngineEdges));
    }
    let guard = limits::NewOptimizationWorkGuard(ctx, "sizelessOptimizer.setup", limits::MaxOptimizationWorkUnits)?;
    for node in &nodes {
        guard.Step()?;
        let Some(ng) = node.graph() else {
            return Err(errorf!("TALA sizelessOptimizer found a node without a graph"));
        };
        let (ne, nn) = {
            let n = node.b();
            (n.Edges.len(), n.Nears.len())
        };
        if ne as i64 > limits::MaxEngineEdges || nn as i64 > limits::MaxEngineNodes {
            return Err(errorf!("TALA sizelessOptimizer node {} adjacency references exceed engine limits", node.DebugID()));
        }
        let ncs = ng.b().CellSize;
        if ncs != cellSize {
            return Err(errorf!("mismatch of cell size, graph={:.6}, node={:.6}", cellSize, ncs));
        }
    }
    let mut optim = sizelessOptimizer {
        mutationScratch: optimizerMutationSnapshot::default(),
        nodes: Vec::with_capacity(nodes.len()),
        g: g.clone(),
        randGenerator: randGenerator.cloned(),
        occupied: Map::default(),
    };
    optim.resetOccupied();
    for node in &nodes {
        guard.Step()?;
        let canOptimize = canOptimizeNodeGuarded(node, g, &guard)?;
        if canOptimize {
            optim.nodes.push(node.clone());
        }
    }
    guard.Finish()?;
    Ok(optim)
}

impl sizelessOptimizer {
    // resetOccupied resets the occpupied positions of the optimizer
    // this is useful if the nodes changed their positions outside `optimize`
    pub fn resetOccupied(&mut self) {
        let gb = self.g.b();
        let mut occupied: Map<geo::Point, NodeRef> = Map::with_capacity_and_hasher(gb.Nodes.len(), Default::default());
        for n in gb.Nodes.iter() {
            let Some(tl) = n.tl() else { continue };
            let p = *tl.b();
            occupied.insert(p, n.clone());
        }
        self.occupied = occupied;
    }

    // optimize optimizes sizeless node placement following the routine describe in
    // Graph Compact Orthogonal Layout Algorithm by Freivalds and Glagolevs
    pub fn optimize(&mut self, ctx: &Ctx, temp: f64) -> GoResult<()> {
        self.optimizeWithLimit(ctx, temp, limits::MaxOptimizationWorkUnits)
    }

    pub fn optimizeWithLimit(&mut self, ctx: &Ctx, temp: f64, workLimit: u64) -> GoResult<()> {
        let guard = limits::NewOptimizationWorkGuard(ctx, "sizelessOptimizer.optimize", workLimit)?;
        if self.randGenerator.is_none() {
            return Err(errorf!("TALA sizelessOptimizer.optimize requires a random generator"));
        }
        let mut snapshot = std::mem::take(&mut self.mutationScratch);
        let result = self.optimizeWithSnapshot(ctx, temp, &guard, &mut snapshot);
        snapshot.release();
        self.mutationScratch = snapshot;
        result
    }

    fn optimizeWithSnapshot(
        &mut self,
        ctx: &Ctx,
        temp: f64,
        guard: &OptimizationWorkGuardRef,
        snapshot: &mut optimizerMutationSnapshot,
    ) -> GoResult<()> {
        captureOptimizerMutationStateInto(&self.g, guard, snapshot)?;
        let mut occupiedSnapshot: Map<geo::Point, NodeRef> = Map::with_capacity_and_hasher(self.occupied.len(), Default::default());
        for (point, node) in self.occupied.iter() {
            guard.Step()?;
            occupiedSnapshot.insert(*point, node.clone());
        }
        let result = self.optimizeGuarded(ctx, temp, guard).and_then(|_| guard.Finish());
        if result.is_err() {
            snapshot.restore();
            self.occupied = occupiedSnapshot;
        }
        result
    }

    pub fn optimizeGuarded(&mut self, ctx: &Ctx, temp: f64, guard: &OptimizationWorkGuardRef) -> GoResult<()> {
        if self.nodes.len() as i64 > limits::MaxEngineNodes {
            return Err(errorf!("TALA sizelessOptimizer.optimize node count exceeds limit {}", limits::MaxEngineNodes));
        }
        let rng = self.randGenerator.clone().expect("random generator");

        let mut nodeIndices: Vec<usize> = Vec::with_capacity(self.nodes.len());
        for i in 0..self.nodes.len() {
            guard.Step()?;
            nodeIndices.push(i);
        }
        limits::Shuffle(&mut nodeIndices, &mut rng.bm(), &mut guard.bm())?;

        for &nodeIndex in &nodeIndices {
            guard.Step()?;
            let node = self.nodes[nodeIndex].clone();
            let Some(tl) = node.tl() else {
                return Err(errorf!("TALA sizelessOptimizer.optimize found an unpositioned node"));
            };

            // we should always be able to move to the original position
            let p = *tl.b();
            self.occupied.remove(&p);
            let medianPoint = self.medianPointGuarded(&node, temp, guard)?;
            let distance = match self.findClosestUnoccupiedDistanceGuarded(&node, &medianPoint, guard) {
                Ok(d) => d,
                Err(err) => {
                    return Err(wrapf!(err, "could not find an unoccupied distance for {}: {}", node.DebugID(), err));
                }
            };
            let mut points = self.placementPointsGuarded(&node, &medianPoint, distance, guard)?;
            limits::Shuffle(&mut points, &mut rng.bm(), &mut guard.bm())?;

            let moved = self.moveNodeToBestGuarded(ctx, &node, &points, guard)?;

            if !moved {
                let bestSwapCandidate = self.bestSwapCandidateGuarded(ctx, &node, guard)?;
                if let Some(best) = bestSwapCandidate {
                    optimizerSwapPositions(&node, &best, guard)?;
                    let bp = best.tlv();
                    self.occupied.insert(bp, best);
                }
            }
            let np = node.tlv();
            self.occupied.insert(np, node);
        }
        guard.Finish()
    }

    pub fn medianPointGuarded(&self, node: &NodeRef, temp: f64, guard: &OptimizationWorkGuardRef) -> GoResult<geo::Point> {
        let (mut medianX, mut medianY) = optimizerMedianToNeighbors(node, false, &[], guard)?;

        let cell = self.g.b().CellSize;
        let fixedOrigin = optimizerFixedOrigin(Some(&self.g), node.OwningContainer().as_ref(), guard)?;
        if let Some(fo) = &fixedOrigin {
            if cell <= 0.0 || math::IsNaN(cell) || math::IsInf(cell, 0) {
                return Err(errorf!("TALA {} fixed-origin placement requires a finite positive cell size", guard.Location()));
            }
            // we can't have node positions past the fixedOrigin so set a floor
            if medianX < fo.X / cell {
                medianX = fo.X / cell;
                medianX += temp;
            }
            if medianY < fo.Y / cell {
                medianY = fo.Y / cell;
                medianY += temp;
            }
        }

        let rng = self.randGenerator.as_ref().expect("random generator");
        let r1 = rng.bm().Float64();
        medianX += -temp + r1 * (2.0 * temp);
        let r2 = rng.bm().Float64();
        medianY += -temp + r2 * (2.0 * temp);

        if let Some(fo) = &fixedOrigin {
            medianX = math::Max(medianX, fo.X / cell);
            medianY = math::Max(medianY, fo.Y / cell);
        }

        Ok(geo::Point { X: math::Round(medianX), Y: math::Round(medianY) })
    }

    // FindClosestUnoccupiedDistance finds the closest distance from `medianPoint` which is not occupied by any other node
    pub fn FindClosestUnoccupiedDistance(&self, ctx: &Ctx, node: &NodeRef, medianPoint: &geo::Point) -> GoResult<f64> {
        let guard = limits::NewOptimizationWorkGuard(ctx, "sizelessOptimizer.optimize", limits::MaxOptimizationWorkUnits)?;
        self.findClosestUnoccupiedDistanceGuarded(node, medianPoint, &guard)
    }

    /// Fixed origin rounded to grid units (Go's pastOrigin closure input).
    fn gridFixedOrigin(&self, node: &NodeRef, guard: &OptimizationWorkGuardRef) -> GoResult<Option<geo::Point>> {
        let g = node.g();
        let mut fixedOrigin = optimizerFixedOrigin(Some(&g), node.OwningContainer().as_ref(), guard)?;
        if let Some(fo) = fixedOrigin.as_mut() {
            let cs = g.b().CellSize;
            if cs <= 0.0 || math::IsNaN(cs) || math::IsInf(cs, 0) {
                return Err(errorf!("TALA {} fixed-origin placement requires a finite positive cell size", guard.Location()));
            }
            fo.X = math::Round(fo.X / cs);
            fo.Y = math::Round(fo.Y / cs);
        }
        Ok(fixedOrigin)
    }

    pub fn findClosestUnoccupiedDistanceGuarded(
        &self,
        node: &NodeRef,
        medianPoint: &geo::Point,
        guard: &OptimizationWorkGuardRef,
    ) -> GoResult<f64> {
        if node.graph().is_none() {
            return Err(errorf!("TALA {} placement search requires a node, graph, and median", guard.Location()));
        }
        let fixedOrigin = self.gridFixedOrigin(node, guard)?;
        let pastOrigin = |x: f64, y: f64| match &fixedOrigin {
            Some(fo) => x < fo.X || y < fo.Y,
            None => false,
        };

        let mut curr = 0.0f64;
        while curr <= 100.0 {
            guard.Step()?;
            let (mut x, mut y) = (curr, 0.0f64);
            while x >= -curr {
                guard.Step()?;
                if y != 0.0 {
                    let p = geo::Point { X: medianPoint.X + x, Y: medianPoint.Y - y };
                    if !pastOrigin(p.X, p.Y) && !self.isOccupied(&p) {
                        return Ok(curr);
                    }
                }
                let p = geo::Point { X: medianPoint.X + x, Y: medianPoint.Y + y };
                if !pastOrigin(p.X, p.Y) && !self.isOccupied(&p) {
                    return Ok(curr);
                }
                if x > 0.0 {
                    y += 1.0;
                } else {
                    y -= 1.0;
                }
                x -= 1.0;
            }
            curr += 1.0;
        }
        Err(errorf!("no unoccupied points within d=100 of p=&{{{} {}}}", medianPoint.X, medianPoint.Y))
    }

    #[inline]
    pub fn isOccupied(&self, p: &geo::Point) -> bool {
        self.occupied.contains_key(p)
    }

    // placementPointsGuarded generates points up to minUnoccupiedDistance+1 from medianPoint.
    // The resulting points contain only valid positions (unoccupied and positive if there are fixed position nodes)
    pub fn placementPointsGuarded(
        &self,
        node: &NodeRef,
        medianPoint: &geo::Point,
        minUnoccupiedDistance: f64,
        guard: &OptimizationWorkGuardRef,
    ) -> GoResult<Vec<geo::Point>> {
        if node.graph().is_none() {
            return Err(errorf!("TALA {} placement generation requires a node, graph, and median", guard.Location()));
        }
        if minUnoccupiedDistance < 0.0
            || minUnoccupiedDistance > 100.0
            || math::IsNaN(minUnoccupiedDistance)
            || math::IsInf(minUnoccupiedDistance, 0)
        {
            return Err(errorf!("TALA {} placement distance must be finite and within [0, 100]", guard.Location()));
        }
        let distance = minUnoccupiedDistance + 1.0;
        let capacity = numPointsWithinManhattanDistance(distance) + 1;
        if capacity < 0 || capacity > maxOptimizerPlacementCandidates {
            return Err(optimizationResourceLimit(format!(
                "TALA sizelessOptimizer.optimize placement candidates exceed limit {}",
                maxOptimizerPlacementCandidates
            )));
        }
        let mut points: Vec<geo::Point> = Vec::with_capacity(capacity as usize);

        let fixedOrigin = self.gridFixedOrigin(node, guard)?;
        let pastOrigin = |x: f64, y: f64| match &fixedOrigin {
            Some(fo) => x < fo.X || y < fo.Y,
            None => false,
        };

        let mut x = distance;
        while x >= -distance {
            guard.Step()?;
            let mut y = 0.0f64;
            while y <= distance - math::Abs(x) {
                guard.Step()?;
                if y != 0.0 {
                    let point = geo::Point { X: medianPoint.X + x, Y: medianPoint.Y - y };
                    if !pastOrigin(point.X, point.Y) && !self.isOccupied(&point) {
                        points.push(point);
                    }
                }
                let point = geo::Point { X: medianPoint.X + x, Y: medianPoint.Y + y };
                if !pastOrigin(point.X, point.Y) && !self.isOccupied(&point) {
                    points.push(point);
                }
                y += 1.0;
            }
            x -= 1.0;
        }

        Ok(points)
    }

    pub fn moveNodeToBestGuarded(
        &self,
        ctx: &Ctx,
        node: &NodeRef,
        points: &[geo::Point],
        guard: &OptimizationWorkGuardRef,
    ) -> GoResult<bool> {
        let mut scorer = placementcost::NewNodeEdgeLengthScorer(node, edgeLengthOptions(None, false));
        let result = (|| -> GoResult<bool> {
            let mut leastDistance = math::Inf(1);
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
                if let Err(err) = movement.moveAbs(x, y, guard) {
                    return fail(err);
                }
                if let Err(err) = chargeOptimizerScoring(node, 0, false, guard) {
                    return fail(err);
                }
                let edgeLength = match scorer.Score(ctx) {
                    Ok(v) => v,
                    Err(err) => return fail(err),
                };
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

            if let Some(err) = ctx.Err() {
                return fail(wrapf!(err, "EdgeLength: {}", err));
            }
            if math::IsInf(leastDistance, 1) {
                return fail(errorf!("sizelessOptimizer.moveNodeToBest: could not find any placement"));
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

    pub fn bestSwapCandidateGuarded(&self, ctx: &Ctx, node: &NodeRef, guard: &OptimizationWorkGuardRef) -> GoResult<Option<NodeRef>> {
        let opts = edgeLengthOptions(None, false);
        chargeOptimizerScoring(node, 0, false, guard)?;
        let mut minSwapL1 = placementcost::NodeEdgeLength(ctx, node, &opts)?;
        let mut bestSwapCandidate: Option<NodeRef> = None;

        let mut swapCandidates = self.swapCandidatesGuarded(node, guard)?;
        {
            let rng = self.randGenerator.as_ref().expect("random generator");
            limits::Shuffle(&mut swapCandidates, &mut rng.bm(), &mut guard.bm())?;
        }

        for swapCandidate in &swapCandidates {
            guard.Step()?;
            chargeOptimizerScoring(swapCandidate, 0, false, guard)?;
            let currentL2 = placementcost::NodeEdgeLength(ctx, swapCandidate, &opts)?;

            let mut swappedL1 = 0.0f64;
            let mut swappedL2 = 0.0f64;
            withOptimizerPositionsSwapped(node, swapCandidate, guard, || -> GoResult<()> {
                chargeOptimizerScoring(node, 0, false, guard)?;
                swappedL1 = placementcost::NodeEdgeLength(ctx, node, &opts)?;
                // No need to compute it if we already know it won't be a good swap.
                if geo::PrecisionCompare(swappedL1, minSwapL1, geo::PRECISION) < 0 {
                    chargeOptimizerScoring(swapCandidate, 0, false, guard)?;
                    swappedL2 = placementcost::NodeEdgeLength(ctx, swapCandidate, &opts)?;
                }
                Ok(())
            })?;

            // Both positions have to be better than before
            if geo::PrecisionCompare(swappedL1, minSwapL1, geo::PRECISION) < 0
                && geo::PrecisionCompare(swappedL2, currentL2, geo::PRECISION) <= 0
            {
                minSwapL1 = swappedL1;
                bestSwapCandidate = Some(swapCandidate.clone());
            }
        }

        if let Some(err) = ctx.Err() {
            return Err(wrapf!(err, "EdgeLength: {}", err));
        }
        Ok(bestSwapCandidate)
    }

    // swapCandidatesGuarded finds the optimizable nodes adjacent to node.
    pub fn swapCandidatesGuarded(&self, node: &NodeRef, guard: &OptimizationWorkGuardRef) -> GoResult<Vec<NodeRef>> {
        let mut adjacent: Vec<NodeRef> = Vec::with_capacity(4);
        let tl = node.tlv();

        let adjacentNode = |xDiff: f64, yDiff: f64| -> GoResult<Option<NodeRef>> {
            guard.Step()?;
            let p = geo::Point { X: tl.X + xDiff, Y: tl.Y + yDiff };
            if let Some(n) = self.occupied.get(&p) {
                let canOptimize = canOptimizeNodeGuarded(n, &self.g, guard)?;
                if canOptimize {
                    return Ok(Some(n.clone()));
                }
            }
            Ok(None)
        };

        // Go records the error and keeps probing; the stage fails either way.
        let mut adjacentErr = None;
        for (dx, dy) in [(0.0, -1.0), (0.0, 1.0), (-1.0, 0.0), (1.0, 0.0)] {
            match adjacentNode(dx, dy) {
                Ok(Some(n)) => adjacent.push(n),
                Ok(None) => {}
                Err(err) => adjacentErr = Some(err),
            }
        }
        if let Some(err) = adjacentErr {
            return Err(err);
        }

        Ok(adjacent)
    }
}

// canOptimizeNodeGuarded reports whether a node can be optimized.
// In order to be optimizable, a node must have edges, or at least `nears`, can't have a fixed position and can't be in a tree
pub fn canOptimizeNodeGuarded(node: &NodeRef, g: &GraphRef, guard: &OptimizationWorkGuardRef) -> GoResult<bool> {
    let (noEdges, hasFixed) = {
        let n = node.b();
        (n.Edges.is_empty(), n.FixedTopLeft.is_some())
    };
    if noEdges {
        let mut hasUsableNear = false;
        let (nears, container) = {
            let n = node.b();
            (n.Nears.iter().cloned().collect::<Vec<_>>(), n.Container.clone())
        };
        for near in &nears {
            guard.Step()?;
            let usable = optimizerIsDescendantOf(Some(near), container.as_ref(), guard)?;
            if usable {
                hasUsableNear = true;
                break;
            }
        }
        if !hasUsableNear {
            return Ok(false);
        }
    }
    if hasFixed {
        return Ok(false);
    }
    let inTree = g.b().NodeToTree.has(node);
    Ok(!inTree)
}
