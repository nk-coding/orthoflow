// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/placement/optimizer_spatial_index.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of placement/optimizer_spatial_index.go
use crate::*;
use gocompat::{errorf, math, GoResult, Map};
use layoutgraph::{GraphRef, NodeRef};
use limits::OptimizationWorkGuardRef;

pub const optimizerSpatialIndexMinNodes: usize = 96;

// optimizerSpatialEntry retains graph order separately from the X ordering
// used by the interval tree. Candidate queries sort the small result set back
// into graph order before running the legacy directional overlap predicate.
#[derive(Clone, Copy, Debug, Default)]
pub struct optimizerSpatialEntry {
    pub graphIndex: usize,
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct optimizerOccupancyEntry {
    pub graphIndex: usize,
    pub generation: u64,
}

// optimizerSpatialIndex is rebuilt once for each node considered by the sized
// optimizer. Candidate evaluation may move that node repeatedly, but all other
// boxes remain fixed; queries skip the moving node by identity. This replaces
// the candidate-by-graph scans without requiring updates for every trial point.
#[derive(Default)]
pub struct optimizerSpatialIndex {
    pub graph: Option<GraphRef>,
    pub nodeCount: usize,
    pub occupancyUsable: bool,
    pub spatialUsable: bool,
    pub entries: Vec<optimizerSpatialEntry>,
    pub maxRight: Vec<f64>,
    pub candidates: Vec<usize>,

    pub occupied: Map<geo::Point, optimizerOccupancyEntry>,
    pub occupancyGeneration: u64,
}

impl sizedOptimizer {
    pub fn rebuildSpatialIndex(&mut self, guard: &OptimizationWorkGuardRef) -> GoResult<()> {
        let g = self.g.clone();
        self.spatialIndex.rebuild(&g, guard)
    }

    #[inline]
    pub fn indexedIsOccupied(&self, point: &geo::Point, guard: &OptimizationWorkGuardRef) -> GoResult<(Option<NodeRef>, bool)> {
        self.spatialIndex.isOccupied(&self.g, point, guard)
    }

    #[inline]
    pub fn indexedDoesOverlap(
        &mut self,
        node: &NodeRef,
        point: &geo::PointRef,
        exceptions: &[NodeRef],
        guard: &OptimizationWorkGuardRef,
    ) -> GoResult<bool> {
        self.spatialIndex.doesOverlap(node, point, exceptions, guard)
    }

    pub fn nodeMayMoveDescendants(&self, node: &NodeRef) -> bool {
        node.IsContainer() || node.IsClusterVessel() || self.g.b().Sequences.has(node)
    }

    pub fn indexedCanMove(&mut self, node: &NodeRef, point: &geo::PointRef, guard: &OptimizationWorkGuardRef) -> GoResult<bool> {
        if node.graph().is_none() {
            return Err(errorf!("TALA {} movement check requires a node, graph, and point", guard.Location()));
        }
        let p = *point.b();
        if nonNilEquals(&node.tlv(), &p) {
            return Ok(true);
        }
        // Candidate scoring moves descendants together with a container, cluster, or
        // sequence vessel. Their indexed boxes would become stale after the first
        // trial point, so retain the legacy scan for those comparatively rare nodes.
        if self.nodeMayMoveDescendants(node) {
            return optimizerCanMove(node, point, true, guard);
        }
        let (_, occupied) = self.indexedIsOccupied(&p, guard)?;
        if occupied {
            return Ok(false);
        }
        let overlaps = self.indexedDoesOverlap(node, point, &[], guard)?;
        Ok(!overlaps)
    }
}

#[inline]
pub fn optimizerIndexFinite(value: f64) -> bool {
    !math::IsNaN(value) && !math::IsInf(value, 0)
}

pub fn optimizerIndexFiniteBox(tl: Option<geo::Point>, width: f64, height: f64) -> bool {
    let Some(tl) = tl else { return false };
    if width < 0.0 || height < 0.0 {
        return false;
    }
    optimizerIndexFinite(tl.X)
        && optimizerIndexFinite(tl.Y)
        && optimizerIndexFinite(width)
        && optimizerIndexFinite(height)
        && optimizerIndexFinite(tl.X + width)
        && optimizerIndexFinite(tl.Y + height)
}

impl optimizerSpatialIndex {
    pub fn rebuild(&mut self, g: &GraphRef, guard: &OptimizationWorkGuardRef) -> GoResult<()> {
        let gb = g.b();
        if gb.Nodes.len() as i64 > limits::MaxEngineNodes {
            return Err(errorf!("TALA {} spatial index node count exceeds limit {}", guard.Location(), limits::MaxEngineNodes));
        }

        self.graph = Some(g.clone());
        self.nodeCount = gb.Nodes.len();
        self.occupancyUsable = true;
        self.spatialUsable = gb.Nodes.len() >= optimizerSpatialIndexMinNodes;
        self.entries.clear();
        self.candidates.clear();

        self.occupancyGeneration = self.occupancyGeneration.wrapping_add(1);
        // Avoid clearing map buckets on every optimizer node while bounding stale
        // coordinate retention when accepted moves continually introduce new keys.
        let maxRetained = std::cmp::max(64, 4 * gb.Nodes.len());
        if self.occupancyGeneration == 0 || self.occupied.len() > maxRetained {
            self.occupied.clear();
            self.occupancyGeneration = 1;
        }

        for (graphIndex, node) in gb.Nodes.iter().enumerate() {
            guard.Step()?;
            let (tl, width, height) = {
                let n = node.b();
                let bx = n.Box.b();
                (bx.TopLeft.as_ref().map(|p| *p.b()), bx.Width, bx.Height)
            };
            let Some(point) = tl else { continue };
            if !optimizerIndexFinite(point.X) || !optimizerIndexFinite(point.Y) {
                self.occupancyUsable = false;
                self.spatialUsable = false;
                continue;
            }
            let generation = self.occupancyGeneration;
            let entry = self.occupied.get(&point).copied();
            match entry {
                Some(e) if e.generation == generation => {}
                _ => {
                    self.occupied.insert(point, optimizerOccupancyEntry { graphIndex, generation });
                }
            }

            if !self.spatialUsable {
                continue;
            }
            if !optimizerIndexFiniteBox(tl, width, height) {
                // The legacy predicates have intentionally unusual behavior for NaN,
                // infinities, and negative dimensions. Retain that behavior by using
                // their full scan rather than forcing those values into an ordering.
                self.spatialUsable = false;
                continue;
            }
            self.entries.push(optimizerSpatialEntry {
                graphIndex,
                left: point.X,
                top: point.Y,
                right: point.X + width,
                bottom: point.Y + height,
            });
        }
        drop(gb);

        if !self.spatialUsable || self.entries.is_empty() {
            return guard.Finish();
        }
        guard.AddSort(self.entries.len() as i64)?;
        // (left, graphIndex) is a total order on finite lefts and unique graph
        // indices, so any stable sort yields Go's order.
        self.entries.sort_by(|a, b| {
            if a.left != b.left {
                if a.left < b.left {
                    return std::cmp::Ordering::Less;
                }
                if b.left < a.left {
                    return std::cmp::Ordering::Greater;
                }
                return std::cmp::Ordering::Equal;
            }
            a.graphIndex.cmp(&b.graphIndex)
        });

        let requiredTreeSize = 4 * self.entries.len();
        self.maxRight.clear();
        self.maxRight.resize(requiredTreeSize, 0.0);
        self.buildMaxRight(1, 0, self.entries.len());
        guard.Finish()
    }

    pub fn buildMaxRight(&mut self, treeIndex: usize, low: usize, high: usize) -> f64 {
        if high - low == 1 {
            self.maxRight[treeIndex] = self.entries[low].right;
            return self.maxRight[treeIndex];
        }
        let middle = low + (high - low) / 2;
        let leftMax = self.buildMaxRight(treeIndex * 2, low, middle);
        let rightMax = self.buildMaxRight(treeIndex * 2 + 1, middle, high);
        self.maxRight[treeIndex] = math::BuiltinMax(leftMax, rightMax);
        self.maxRight[treeIndex]
    }

    /// Fills `self.candidates` (sorted graph indices).
    pub fn query(&mut self, left: f64, top: f64, right: f64, bottom: f64, guard: &OptimizationWorkGuardRef) -> GoResult<()> {
        self.candidates.clear();
        if !self.spatialUsable || self.entries.is_empty() {
            return guard.Finish();
        }
        self.queryTree(1, 0, self.entries.len(), left, top, right, bottom, guard)?;
        guard.AddSort(self.candidates.len() as i64)?;
        // Graph indices are unique, so any sort matches sort.Ints.
        self.candidates.sort_unstable();
        guard.Finish()
    }

    pub fn queryTree(
        &mut self,
        treeIndex: usize,
        low: usize,
        high: usize,
        left: f64,
        top: f64,
        right: f64,
        bottom: f64,
        guard: &OptimizationWorkGuardRef,
    ) -> GoResult<()> {
        guard.Step()?;
        if low >= high || self.maxRight[treeIndex] < left || self.entries[low].left > right {
            return Ok(());
        }
        if high - low == 1 {
            let entry = self.entries[low];
            if entry.left <= right && entry.right >= left && entry.top <= bottom && entry.bottom >= top {
                self.candidates.push(entry.graphIndex);
            }
            return Ok(());
        }

        let middle = low + (high - low) / 2;
        self.queryTree(treeIndex * 2, low, middle, left, top, right, bottom, guard)?;
        self.queryTree(treeIndex * 2 + 1, middle, high, left, top, right, bottom, guard)
    }

    #[inline]
    fn sameGraph(&self, g: &GraphRef) -> bool {
        matches!(&self.graph, Some(x) if x == g)
    }

    pub fn isOccupied(&self, g: &GraphRef, point: &geo::Point, guard: &OptimizationWorkGuardRef) -> GoResult<(Option<NodeRef>, bool)> {
        let nodeCount = g.b().Nodes.len();
        if !self.sameGraph(g)
            || self.nodeCount != nodeCount
            || !self.occupancyUsable
            || !optimizerIndexFinite(point.X)
            || !optimizerIndexFinite(point.Y)
        {
            return optimizerIsOccupied(g, point, guard);
        }
        guard.Step()?;
        let entry = self.occupied.get(point).copied();
        let entry = match entry {
            Some(e) if e.generation == self.occupancyGeneration => e,
            _ => {
                guard.Finish()?;
                return Ok((None, false));
            }
        };
        if entry.graphIndex >= nodeCount {
            return Err(errorf!("TALA {} spatial occupancy index is stale", guard.Location()));
        }
        let n = g.b().Nodes[entry.graphIndex].clone();
        guard.Finish()?;
        Ok((Some(n), true))
    }

    pub fn doesOverlap(
        &mut self,
        node: &NodeRef,
        point: &geo::PointRef,
        exceptions: &[NodeRef],
        guard: &OptimizationWorkGuardRef,
    ) -> GoResult<bool> {
        let Some(g) = node.graph() else {
            return Err(errorf!("TALA {} overlap check requires a node, graph, and point", guard.Location()));
        };
        let nodeCount = g.b().Nodes.len();
        if nodeCount as i64 > limits::MaxEngineNodes || exceptions.len() as i64 > limits::MaxEngineNodes {
            return Err(errorf!("TALA {} overlap inputs exceed node limit {}", guard.Location(), limits::MaxEngineNodes));
        }
        let p = *point.b();
        let (width, height, edgeCount) = {
            let n = node.b();
            let bx = n.Box.b();
            (bx.Width, bx.Height, n.Edges.len())
        };
        if !self.sameGraph(&g)
            || self.nodeCount != nodeCount
            || !self.spatialUsable
            || !optimizerIndexFinite(p.X)
            || !optimizerIndexFinite(p.Y)
            || !optimizerIndexFinite(width)
            || !optimizerIndexFinite(height)
            || width < 0.0
            || height < 0.0
            || !optimizerIndexFinite(p.X + width)
            || !optimizerIndexFinite(p.Y + height)
        {
            return optimizerDoesOverlap(node, point, exceptions, guard);
        }

        self.query(
            p.X - maxSafeOverlapDelta,
            p.Y - maxSafeOverlapDelta,
            p.X + width + maxSafeOverlapDelta,
            p.Y + height + maxSafeOverlapDelta,
            guard,
        )?;
        let right = p.X + width;
        let bottom = p.Y + height;
        let gb = g.b();
        for &graphIndex in self.candidates.iter() {
            guard.Step()?;
            let otherNode = &gb.Nodes[graphIndex];
            if otherNode == node {
                continue;
            }
            let mut excluded = false;
            for exception in exceptions {
                guard.Step()?;
                if exception == otherNode {
                    excluded = true;
                    break;
                }
            }
            if excluded {
                continue;
            }
            let (ox, oy, ow, oh) = {
                let o = otherNode.b();
                let bx = o.Box.b();
                let Some(tl) = bx.TopLeft.as_ref() else { continue };
                let tl = *tl.b();
                (tl.X, tl.Y, bx.Width, bx.Height)
            };
            // The interval query is deliberately only a broad phase. Retain the
            // historical directional spacing calculation and strict comparisons.
            guard.Add(edgeCount as u64)?;
            let delta = node.DeltaTo(otherNode, Some(point)) as f64;
            if p.X < ox + ow + delta && right + delta > ox && p.Y < oy + oh + delta && bottom + delta > oy {
                return Ok(true);
            }
        }
        drop(gb);
        guard.Finish()?;
        Ok(false)
    }
}
