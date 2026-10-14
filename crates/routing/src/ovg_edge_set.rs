// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/routing/ovg_edge_set.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of routing/ovg_edge_set.go.
#[allow(unused_imports)]
use crate::*;
use gocompat::{GoResult, Map, Set};

/// Go map key for an `OVGEdge` value: (From pointer, To pointer, Distance).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct ovgEdgeValueKey(usize, usize, u64);

fn edgeValueKey(e: &OVGEdge) -> ovgEdgeValueKey {
    ovgEdgeValueKey(e.From.addr(), e.To.addr(), f64key(e.Distance))
}

/// Keeps OVG edges as one set for vertical edges and other set of horizontal ones.
/// Can quickly check for intersections/overlaps
///
/// Go stores `OVGEdge` values; OVG edges are immutable, so the port stores the
/// shared `OVGEdgeRef` (identical observable behavior).
pub struct ovgEdgeSet {
    pub(crate) horizontalEdges: Map<u64, Vec<OVGEdgeRef>>,
    pub(crate) verticalEdges: Map<u64, Vec<OVGEdgeRef>>,
    pub(crate) horizontals: Vec<f64>,
    pub(crate) verticals: Vec<f64>,
    edges: Set<ovgEdgeValueKey>,
}

pub(crate) fn newOvgEdgeSet() -> ovgEdgeSet {
    ovgEdgeSet {
        horizontalEdges: gocompat::NewMapCap(16),
        verticalEdges: gocompat::NewMapCap(16),
        horizontals: Vec::with_capacity(16),
        verticals: Vec::with_capacity(16),
        edges: gocompat::NewSetCap(16),
    }
}


#[inline]
fn edgeMinCoord(e: &OVGEdge, isVertical: bool) -> f64 {
    let (f, t) = (e.From.P(), e.To.P());
    if isVertical {
        if f.Y < t.Y {
            return f.Y;
        }
        t.Y
    } else {
        if f.X < t.X {
            return f.X;
        }
        t.X
    }
}

impl ovgEdgeSet {
    /// insertSortedFloat64 inserts a value into a sorted slice while maintaining order
    pub(crate) fn insertSortedFloat64(slice: &mut Vec<f64>, val: f64) {
        let i = gocompat::sort::SearchFloat64s(slice, val) as usize;
        if i < slice.len() && slice[i] == val {
            return; // Already exists
        }
        slice.insert(i, val);
    }

    /// insertSortedEdge inserts an edge into a sorted edge slice while maintaining order
    /// Edges are sorted by their minimum coordinate (Y for vertical, X for horizontal)
    pub(crate) fn insertSortedEdge(edges: &mut Vec<OVGEdgeRef>, edge: OVGEdgeRef, isVertical: bool) {
        let edgeMin = edgeMinCoord(&edge, isVertical);
        let i = gocompat::sort::Search(edges.len() as i64, |j| edgeMinCoord(&edges[j as usize], isVertical) >= edgeMin) as usize;
        edges.insert(i, edge);
    }

    pub(crate) fn add(&mut self, edge: &OVGEdgeRef) {
        let key = edgeValueKey(edge);
        if self.edges.contains(&key) {
            return;
        }
        self.edges.insert(key);

        if edge.isVertical() {
            let x = edge.From.X();
            let k = f64key(x);
            if !self.verticalEdges.contains_key(&k) {
                self.verticalEdges.insert(k, Vec::with_capacity(16));
                Self::insertSortedFloat64(&mut self.verticals, x);
            }
            let edges = self.verticalEdges.get_mut(&k).unwrap();
            Self::insertSortedEdge(edges, edge.clone(), true);
        } else if edge.isHorizontal() {
            let y = edge.From.Y();
            let k = f64key(y);
            if !self.horizontalEdges.contains_key(&k) {
                self.horizontalEdges.insert(k, Vec::with_capacity(16));
                Self::insertSortedFloat64(&mut self.horizontals, y);
            }
            let edges = self.horizontalEdges.get_mut(&k).unwrap();
            Self::insertSortedEdge(edges, edge.clone(), false);
        }
    }

    /// addGuarded reserves every sorted-slice shift before mutating the set. The
    /// duplicate lookup itself is also accounted, and a guard failure leaves the set
    /// unchanged.
    pub(crate) fn addGuarded(&mut self, edge: &OVGEdgeRef, guard: &dyn workBudget) -> GoResult<()> {
        guard.step()?;
        if self.edges.contains(&edgeValueKey(edge)) {
            return Ok(());
        }
        if edge.isVertical() {
            let k = f64key(edge.From.X());
            if !self.verticalEdges.contains_key(&k) {
                guard.add(self.verticals.len() as u64 + 1)?;
            }
            guard.add(self.verticalEdges.get(&k).map_or(0, |v| v.len()) as u64 + 1)?;
        } else if edge.isHorizontal() {
            let k = f64key(edge.From.Y());
            if !self.horizontalEdges.contains_key(&k) {
                guard.add(self.horizontals.len() as u64 + 1)?;
            }
            guard.add(self.horizontalEdges.get(&k).map_or(0, |v| v.len()) as u64 + 1)?;
        }
        self.add(edge);
        guard.check()
    }

    pub(crate) fn intersectsWithGuarded(&self, edge: &OVGEdgeRef, guard: &dyn workBudget) -> GoResult<bool> {
        self.intersectsWithChecked(edge, Some(guard))
    }

    pub(crate) fn intersectsWithChecked(&self, edge: &OVGEdge, guard: Option<&dyn workBudget>) -> GoResult<bool> {
        let step = || -> GoResult<()> {
            match guard {
                None => Ok(()),
                Some(g) => g.step(),
            }
        };
        let (ef, et) = (edge.From.P(), edge.To.P());
        if ef.Y == et.Y {
            // horizontal
            let (mut minX, mut maxX) = (ef.X, et.X);
            if maxX < minX {
                std::mem::swap(&mut minX, &mut maxX);
            }

            // check for overlaps
            for e in self.horizontalEdges.get(&f64key(ef.Y)).map_or(&[][..], |v| v.as_slice()) {
                step()?;
                let (f, t) = (e.From.P(), e.To.P());
                if f.X < t.X {
                    if maxX <= f.X {
                        // horizontalEdges is ordered by minX, so no following edges have a minX less than this one
                        break;
                    }
                    if t.X <= minX {
                        continue;
                    }
                } else {
                    if maxX <= t.X {
                        break;
                    }
                    if f.X <= minX {
                        continue;
                    }
                }
                if !edge.sharePoints(e) {
                    return Ok(true);
                }
            }

            let y = ef.Y;
            // The axes are sorted. Charge the skipped prefix as before, but begin
            // geometry checks at the first line that can cross this segment.
            let verticals = &self.verticals;
            let start = gocompat::sort::Search(verticals.len() as i64, |i| !(verticals[i as usize] < minX)) as usize;
            chargeSkippedRouteWork(guard, start)?;
            // check for crossings
            for &lineCoordinate in &verticals[start..] {
                step()?;
                if lineCoordinate < minX {
                    continue;
                }
                if maxX < lineCoordinate {
                    break;
                }
                for e in self.verticalEdges.get(&f64key(lineCoordinate)).map_or(&[][..], |v| v.as_slice()) {
                    step()?;
                    let (f, t) = (e.From.P(), e.To.P());
                    if f.Y < t.Y {
                        if y < f.Y {
                            break;
                        }
                        if t.Y < y {
                            continue;
                        }
                    } else {
                        if y < t.Y {
                            break;
                        }
                        if f.Y < y {
                            continue;
                        }
                    }
                    if !edge.sharePoints(e) {
                        return Ok(true);
                    }
                }
            }
        } else if ef.X == et.X {
            // vertical
            let (mut minY, mut maxY) = (ef.Y, et.Y);
            if maxY < minY {
                std::mem::swap(&mut minY, &mut maxY);
            }
            // check for overlaps
            for e in self.verticalEdges.get(&f64key(ef.X)).map_or(&[][..], |v| v.as_slice()) {
                step()?;
                let (f, t) = (e.From.P(), e.To.P());
                if f.Y < t.Y {
                    if maxY <= f.Y {
                        // verticalEdges is ordered by minY, so no following edges have a minY less than this one
                        break;
                    }
                    if t.Y <= minY {
                        continue;
                    }
                } else {
                    if maxY <= t.Y {
                        break;
                    }
                    if f.Y <= minY {
                        continue;
                    }
                }
                if !edge.sharePoints(e) {
                    return Ok(true);
                }
            }

            let x = ef.X;
            let horizontals = &self.horizontals;
            let start = gocompat::sort::Search(horizontals.len() as i64, |i| !(horizontals[i as usize] < minY)) as usize;
            chargeSkippedRouteWork(guard, start)?;
            for &lineCoordinate in &horizontals[start..] {
                step()?;
                if lineCoordinate < minY {
                    continue;
                }
                if maxY < lineCoordinate {
                    break;
                }
                for e in self.horizontalEdges.get(&f64key(lineCoordinate)).map_or(&[][..], |v| v.as_slice()) {
                    step()?;
                    let (f, t) = (e.From.P(), e.To.P());
                    if f.X < t.X {
                        if x < f.X {
                            break;
                        }
                        if t.X < x {
                            continue;
                        }
                    } else {
                        if x < t.X {
                            break;
                        }
                        if f.X < x {
                            continue;
                        }
                    }
                    if !edge.sharePoints(e) {
                        return Ok(true);
                    }
                }
            }
        }
        Ok(false)
    }

    pub(crate) fn overlappingEdgesGuarded(&self, edge: &OVGEdgeRef, guard: &dyn workBudget) -> GoResult<Vec<OVGEdgeRef>> {
        self.overlappingEdgesChecked(edge, Some(guard))
    }

    pub(crate) fn overlappingEdgesChecked(&self, edge: &OVGEdge, guard: Option<&dyn workBudget>) -> GoResult<Vec<OVGEdgeRef>> {
        let mut overlapping: Vec<OVGEdgeRef> = Vec::new();
        let (ef, et) = (edge.From.P(), edge.To.P());
        let maybeOverlap: &[OVGEdgeRef] = if ef.X == et.X {
            self.verticalEdges.get(&f64key(ef.X)).map_or(&[], |v| v.as_slice())
        } else if ef.Y == et.Y {
            self.horizontalEdges.get(&f64key(ef.Y)).map_or(&[], |v| v.as_slice())
        } else {
            &[]
        };

        for e in maybeOverlap {
            if let Some(g) = guard {
                g.step()?;
            }
            if intersects(&ef, &et, &e.From.P(), &e.To.P()) {
                overlapping.push(e.clone());
            }
        }
        Ok(overlapping)
    }
}
