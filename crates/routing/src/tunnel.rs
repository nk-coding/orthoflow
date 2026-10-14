// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/routing/tunnel.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of routing/tunnel.go.
#[allow(unused_imports)]
use crate::*;
use gocompat::{math, GoResult, Map, Set};
use layoutgraph::{GraphRef, NodeRef};

pub struct TunnelEntry {
    pub Node: NodeRef,
    pub OVGNode: OVGNodeRef,
}

pub fn NewTunnelEntry(node: &NodeRef, ovgNode: &OVGNodeRef) -> TunnelEntry {
    // Tunnel endpoints are unrestricted ports. Register their owner when the
    // entry is created so every production port uses the same per-owner metadata
    // representation before OVG canonicalization.
    ovgNode.addPortMetadata(node, ovgPortMetadata::default());
    TunnelEntry { Node: node.clone(), OVGNode: ovgNode.clone() }
}

pub struct Tunnel {
    pub EntryA: TunnelEntry,
    pub EntryB: TunnelEntry,
}

pub fn NewTunnel(entryA: TunnelEntry, entryB: TunnelEntry) -> Tunnel {
    Tunnel { EntryA: entryA, EntryB: entryB }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Range {
    pub(crate) start: f64,
    pub(crate) end: f64,
}

impl Range {
    #[inline]
    pub(crate) fn length(&self) -> f64 {
        self.end - self.start
    }
}

/// The step/check/isDescendant callbacks Go passes to tunnelRangesBetweenChecked.
pub(crate) trait tunnelRangeGuard {
    fn tstep(&mut self) -> GoResult<()>;
    fn tcheck(&mut self) -> GoResult<()>;
    fn tisDescendant(&mut self, descendant: Option<&NodeRef>, ancestor: Option<&NodeRef>) -> GoResult<bool>;
}

impl tunnelRangeGuard for ovgBuildGuard {
    fn tstep(&mut self) -> GoResult<()> {
        self.step()
    }
    fn tcheck(&mut self) -> GoResult<()> {
        self.check()
    }
    fn tisDescendant(&mut self, descendant: Option<&NodeRef>, ancestor: Option<&NodeRef>) -> GoResult<bool> {
        self.isDescendantOf(descendant, ancestor)
    }
}

struct routeGuardTunnel<'a>(&'a routeWorkGuardRef);

impl<'a> tunnelRangeGuard for routeGuardTunnel<'a> {
    fn tstep(&mut self) -> GoResult<()> {
        self.0.step()
    }
    fn tcheck(&mut self) -> GoResult<()> {
        self.0.check()
    }
    fn tisDescendant(&mut self, descendant: Option<&NodeRef>, ancestor: Option<&NodeRef>) -> GoResult<bool> {
        isDescendantOfWithRouteGuard(descendant, ancestor, self.0)
    }
}

pub(crate) fn tunnelRangesBetween(g: &GraphRef, nodeA: &NodeRef, nodeB: &NodeRef, filterOutShortTunnels: bool, guard: &mut ovgBuildGuard) -> GoResult<(Option<Vec<Range>>, bool)> {
    guard.check()?;
    tunnelRangesBetweenChecked(g, nodeA, nodeB, filterOutShortTunnels, guard)
}

pub(crate) fn tunnelRangesBetweenGuarded(g: &GraphRef, nodeA: &NodeRef, nodeB: &NodeRef, filterOutShortTunnels: bool, guard: &routeWorkGuardRef) -> GoResult<(Option<Vec<Range>>, bool)> {
    tunnelRangesBetweenChecked(g, nodeA, nodeB, filterOutShortTunnels, &mut routeGuardTunnel(guard))
}

pub(crate) fn isDescendantOfWithRouteGuard(descendant: Option<&NodeRef>, ancestor: Option<&NodeRef>, guard: &routeWorkGuardRef) -> GoResult<bool> {
    routeSearchIsDescendantOf(guard, descendant, ancestor)
}

pub(crate) fn tunnelRangesBetweenChecked(
    g: &GraphRef,
    nodeA: &NodeRef,
    nodeB: &NodeRef,
    filterOutShortTunnels: bool,
    guard: &mut dyn tunnelRangeGuard,
) -> GoResult<(Option<Vec<Range>>, bool)> {
    guard.tstep()?;
    let visibleHorizontally = nodeA.VisibilityGraphCandidate(true, false, true, nodeB, 0.0);
    if !visibleHorizontally {
        guard.tstep()?;
        let visibleVertically = nodeA.VisibilityGraphCandidate(false, false, true, nodeB, 0.0);
        if !visibleVertically {
            return Ok((None, false));
        }
    }

    let (atl, aw, ah) = (nodeA.tlv(), nodeA.w(), nodeA.h());
    let (btl, bw, bh) = (nodeB.tlv(), nodeB.w(), nodeB.h());
    let mut ranges: Vec<Range>;
    {
        let (start, end);
        if visibleHorizontally {
            start = math::Max(atl.Y, btl.Y);
            end = math::Min(atl.Y + ah, btl.Y + bh);
        } else {
            let mut nodeARight = atl.X + aw;
            let mut nodeBRight = btl.X + bw;

            // sequence nodes have shorter ranges along Top/Bottom (except last node in sequence)
            if let Some(s) = nodeA.sequenceField() {
                if s.Last() != *nodeA {
                    nodeARight -= shape::STEP_WEDGE_WIDTH;
                }
            }
            if let Some(s) = nodeB.sequenceField() {
                if s.Last() != *nodeB {
                    nodeBRight -= shape::STEP_WEDGE_WIDTH;
                }
            }

            start = math::Max(atl.X, btl.X);
            end = math::Min(nodeARight, nodeBRight);
        }
        guard.tstep()?;
        ranges = vec![Range { start, end }];
    }

    let gNodes = g.b().Nodes.clone();
    for otherN in &gNodes {
        guard.tstep()?;
        if otherN == nodeA || otherN == nodeB {
            continue;
        }
        let otherBelowA = guard.tisDescendant(Some(otherN), Some(nodeA))?;
        let aBelowOther = guard.tisDescendant(Some(nodeA), Some(otherN))?;
        let otherBelowB = guard.tisDescendant(Some(otherN), Some(nodeB))?;
        let bBelowOther = guard.tisDescendant(Some(nodeB), Some(otherN))?;
        if otherBelowA || aBelowOther || otherBelowB || bBelowOther {
            continue;
        }

        // We're reusing a function that assumes order, so the different order matters
        if otherN.IsBlocked(nodeA, nodeB, true, visibleHorizontally) || otherN.IsBlocked(nodeB, nodeA, true, visibleHorizontally) {
            return Ok((None, false));
        }
        let otl = otherN.tlv();
        let (ow, oh) = (otherN.w(), otherN.h());
        // Skip ones not in between
        if visibleHorizontally {
            if !((atl.X < otl.X && otl.X < btl.X) || (btl.X < otl.X && otl.X < atl.X)) {
                continue;
            }
        } else {
            if !((atl.Y < otl.Y && otl.Y < btl.Y) || (btl.Y < otl.Y && otl.Y < atl.Y)) {
                continue;
            }
        }

        let mut newRanges: Vec<Range> = Vec::new();
        // For every range, a partial obstruction can do one of the following
        // (a) delete it entirely (it obscures the whole range)
        // (b) split it into 2 (it obscures a middle subrange of a range)
        // (c) shorten it (it obscures a non-middle subrange)
        // (d) nothing (it doesn't overlap with any ranges)
        for r in &ranges {
            guard.tstep()?;
            let (lo, size) = if visibleHorizontally { (otl.Y, oh) } else { (otl.X, ow) };
            // (a)
            if lo <= r.start && (lo + size) >= r.end {
                continue;
            }
            // (b)
            if lo > r.start && (lo + size) < r.end {
                newRanges.push(Range { start: r.start, end: lo });
                newRanges.push(Range { start: lo + size, end: r.end });
                continue;
            }
            // (c) obscures start
            if lo <= r.start && (lo + size) < r.end && (lo + size) > r.start {
                newRanges.push(Range { start: lo + size, end: r.end });
                continue;
            }
            // (c) obscures end
            if lo > r.start && (lo + size) >= r.end && lo < r.end {
                newRanges.push(Range { start: r.start, end: lo });
                continue;
            }
            // (d)
            newRanges.push(*r);
        }
        ranges = Vec::new();
        guard.tcheck()?;
        // Filter out the ranges that are too short for even one tunnel
        for s in &newRanges {
            guard.tstep()?;
            if !filterOutShortTunnels || s.length() >= segmentSpacingBuffer {
                ranges.push(*s);
            }
        }
    }

    Ok((Some(ranges), visibleHorizontally))
}

pub(crate) fn buildTunnelsBetween(g: &GraphRef, nodeA: &NodeRef, nodeB: &NodeRef, guard: &mut ovgBuildGuard) -> GoResult<Vec<Tunnel>> {
    guard.check()?;
    let aBelowB = guard.isDescendantOf(Some(nodeA), Some(nodeB))?;
    let bBelowA = guard.isDescendantOf(Some(nodeB), Some(nodeA))?;
    if aBelowB || bBelowA {
        return Ok(Vec::new());
    }
    let (ranges, visibleHorizontally) = tunnelRangesBetween(g, nodeA, nodeB, true, guard)?;
    let ranges = match ranges {
        None => return Ok(Vec::new()),
        Some(r) => r,
    };

    let mut numEdgesToFit = 0.0f64;
    for otherE in &nodeA.edges() {
        guard.step()?;
        if nodeA.Adjacent(otherE) == *nodeB {
            numEdgesToFit += 1.0;
        }
    }

    let (atl, aw, ah) = (nodeA.tlv(), nodeA.w(), nodeA.h());
    let (btl, bw, bh) = (nodeB.tlv(), nodeB.w(), nodeB.h());
    let mut tunnels: Vec<Tunnel> = Vec::new();
    for r in &ranges {
        guard.step()?;
        let numTunnelsFit = math::Min(math::Floor((r.end - r.start) / segmentSpacingBuffer), numEdgesToFit);
        let mut i = 1.0f64;
        while i <= numTunnelsFit {
            guard.step()?;
            let val = math::Round(r.start + i * (r.end - r.start) / (numTunnelsFit + 1.0));
            let p = if visibleHorizontally { geo::Point { X: 0.0, Y: val } } else { geo::Point { X: val, Y: 0.0 } };
            let nodeAEntry = guard.newDerivedNode(p)?;
            let nodeBEntry = guard.newDerivedNode(p)?;
            guard.step()?;
            let tunnel = NewTunnel(NewTunnelEntry(nodeA, &nodeAEntry), NewTunnelEntry(nodeB, &nodeBEntry));
            if visibleHorizontally {
                if atl.X > btl.X {
                    tunnel.EntryA.OVGNode.bm().Point.X = atl.X;
                    tunnel.EntryB.OVGNode.bm().Point.X = btl.X + bw;
                } else {
                    tunnel.EntryA.OVGNode.bm().Point.X = atl.X + aw;
                    tunnel.EntryB.OVGNode.bm().Point.X = btl.X;
                }
            } else {
                if atl.Y > btl.Y {
                    tunnel.EntryA.OVGNode.bm().Point.Y = atl.Y;
                    tunnel.EntryB.OVGNode.bm().Point.Y = btl.Y + bh;
                } else {
                    tunnel.EntryA.OVGNode.bm().Point.Y = atl.Y + ah;
                    tunnel.EntryB.OVGNode.bm().Point.Y = btl.Y;
                }
            }
            tunnels.push(tunnel);
            numEdgesToFit -= 1.0;
            i += 1.0;
        }
    }
    Ok(tunnels)
}

pub(crate) fn buildTunnels(g: &GraphRef, guard: &mut ovgBuildGuard) -> GoResult<Vec<Tunnel>> {
    guard.check()?;
    let mut out: Vec<Tunnel> = Vec::new();
    let mut specialNodes: Set<NodeRef> = Set::default();
    let (clusters, nodeToTree, gNodes) = {
        let gb = g.b();
        (gb.Clusters.values(), gb.NodeToTree.keys(), gb.Nodes.clone())
    };
    for c in &clusters {
        guard.step()?;
        for cn in &c.b().Nodes {
            guard.step()?;
            specialNodes.insert(cn.clone());
        }
    }
    for n in &nodeToTree {
        guard.step()?;
        specialNodes.insert(n.clone());
    }

    let mut searched: Map<NodeRef, Set<NodeRef>> = Map::default();
    for n in &gNodes {
        guard.step()?;
        if specialNodes.contains(n) {
            continue;
        }
        if n.IsTable() {
            // no tunnels for tables
            continue;
        }
        searched.entry(n.clone()).or_default();
        for e in &n.edges() {
            guard.step()?;
            let adj = n.Adjacent(e);
            if adj == *n {
                continue;
            }
            if specialNodes.contains(&adj) {
                continue;
            }
            searched.entry(adj.clone()).or_default();
            if searched.get(n).unwrap().contains(&adj) {
                continue;
            }
            searched.get_mut(n).unwrap().insert(adj.clone());
            searched.get_mut(&adj).unwrap().insert(n.clone());

            let tunnels = buildTunnelsBetween(g, n, &adj, guard)?;
            out.extend(tunnels);
        }
    }

    Ok(out)
}
