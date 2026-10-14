// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/layoutgraph/placement_cost.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

impl GraphRef {
    /// LookupEdgeLengthCost reads one entry from the graph-owned placement cache.
    pub fn LookupEdgeLengthCost(&self, state: u64) -> (f64, bool) {
        let g = self.b();
        match g.edgeLengthCache.as_ref().and_then(|c| c.get(&state)) {
            Some(&cost) => (cost, true),
            None => (0.0, false),
        }
    }

    /// StoreEdgeLengthCost writes one entry. It is a no-op when this graph has
    /// disabled placement caching.
    pub fn StoreEdgeLengthCost(&self, state: u64, cost: f64) {
        let mut g = self.bm();
        if let Some(cache) = g.edgeLengthCache.as_mut() {
            cache.insert(state, cost);
        }
    }

    /// EdgeLengthCacheEntries reports the size of the graph-owned placement cache.
    pub fn EdgeLengthCacheEntries(&self) -> usize {
        self.b().edgeLengthCache.as_ref().map_or(0, |c| c.len())
    }

    /// ResetPlacementCosts clears route-derived scoring state without replacing
    /// the graph's cache map or changing whether caching is enabled.
    pub fn ResetPlacementCosts(&self) {
        let mut g = self.bm();
        if let Some(cache) = g.edgeLengthCache.as_mut() {
            cache.clear();
        }
        g.crossingCost = 0.0;
        g.turnCost = 0.0;
        g.nonCenterPortCost = 0.0;
    }
}

/// PlacementCostSnapshot is an opaque copy of graph-owned placement scoring
/// state used to roll back a rejected optimizer candidate.
pub struct PlacementCostSnapshot {
    pub(crate) graph: Option<GraphRef>,
    pub(crate) cache: Option<Map<u64, f64>>,
    pub(crate) crossingCost: f64,
    pub(crate) turnCost: f64,
    pub(crate) nonCenterPortCost: f64,
}

gocompat::go_ptr!(PlacementCostSnapshotRef, PlacementCostSnapshot);

impl GraphRef {
    /// SnapshotPlacementCosts copies graph-owned scoring state.
    pub fn SnapshotPlacementCosts(&self) -> PlacementCostSnapshotRef {
        let g = self.b();
        PlacementCostSnapshotRef::new(PlacementCostSnapshot {
            graph: Some(self.clone()),
            cache: g.edgeLengthCache.clone(),
            crossingCost: g.crossingCost,
            turnCost: g.turnCost,
            nonCenterPortCost: g.nonCenterPortCost,
        })
    }
}

impl PlacementCostSnapshotRef {
    /// Restore reinstates the captured costs and the exact cache contents owned by
    /// the graph when the state was captured.
    pub fn Restore(&self) {
        let s = self.b();
        let Some(graph) = &s.graph else { return };
        let mut g = graph.bm();
        g.edgeLengthCache = s.cache.clone();
        g.crossingCost = s.crossingCost;
        g.turnCost = s.turnCost;
        g.nonCenterPortCost = s.nonCenterPortCost;
    }
}
