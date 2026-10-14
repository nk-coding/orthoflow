// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/routing/nudge_channels.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of routing/nudge_channels.go.
#[allow(unused_imports)]
use crate::*;
use gocompat::context::Ctx;
use gocompat::math;
use gocompat::{GoResult, Map, Set};
use layoutgraph::{EdgeRef, EdgeSegmentRef, GraphRef};

pub(crate) const channelEpsilon: f64 = 1e-6;

// This optional postpass admits only small channel systems. Quadratic
// discovery must not consume a successful layout's routing budget.
pub(crate) const maxChannelSegments: usize = 256;

pub fn NudgeEdgeChannels(ctx: &Ctx, g: &GraphRef) -> GoResult<()> {
    match nudgeChannelsWithLimit(ctx, g, maxRouteStageWorkUnits) {
        Err(err) if err.Is(&errRouteStageWorkLimit) => Ok(()),
        r => r,
    }
}

pub(crate) struct channelGroup {
    pub(crate) segments: Vec<EdgeSegmentRef>,
    pub(crate) position: f64,
    pub(crate) lower: f64,
    pub(crate) upper: f64,
    pub(crate) fixed: bool,
    pub(crate) nodeClearance: Option<Range>,
}

gocompat::go_ptr!(channelGroupRef, channelGroup);

#[derive(Clone, Copy, Debug)]
pub(crate) struct channelArc {
    pub(crate) from: usize,
    pub(crate) to: usize,
    // Legs of a single route may meet to remove an unnecessary bend.
    pub(crate) separate: bool,
}

#[derive(Default)]
pub(crate) struct channelProblem {
    pub(crate) groups: Vec<channelGroupRef>,
    pub(crate) arcs: Vec<channelArc>,
}

// nudgeChannelsGuarded uses WueOrtho's separation-constraint idea with fixed
// boxes and ports.
pub(crate) fn nudgeChannelsGuarded(g: &GraphRef, guard: &routeWorkGuardRef) -> GoResult<()> {
    let ctx = guard.ctx();
    for horizontal in [true, false] {
        let problem = buildChannelProblem(g, horizontal, guard)?;
        if problem.groups.is_empty() {
            continue;
        }
        let snapshot = captureRouteMutations(Some(g), &[], guard)?;
        let closed = channelInventoryIsClosed(g, &snapshot, guard)?;
        if !closed {
            return Ok(());
        }
        let beforePoints = copyRoutePoints(&g.edges(), guard)?;
        let beforeMetrics = quality::Inspect(&ctx, Some(g))?;
        let changed = applyChannelProblem(g, &problem, horizontal, guard)?;
        if !changed {
            continue;
        }
        let afterMetrics = quality::Inspect(&ctx, Some(g))?;
        if afterMetrics.RouteObstructions > beforeMetrics.RouteObstructions
            || afterMetrics.Crossings > beforeMetrics.Crossings
            || afterMetrics.TextOcclusions > beforeMetrics.TextOcclusions
            || afterMetrics.RouteLength > beforeMetrics.RouteLength + channelEpsilon
        {
            snapshot.restore();
            continue;
        }
        for edge in g.edges() {
            let points = edge.points();
            let before = &beforePoints[&edge];
            let mut moved = false;
            for (i, point) in points.iter().enumerate() {
                if point.get() != before[i] {
                    moved = true;
                    break;
                }
            }
            if !moved {
                continue;
            }
            let points = removeDuplicatePointsGuarded(&points, guard)?;
            edge.bm().Points = points;
        }
    }
    Ok(())
}

// Route-stage snapshots can reach owners outside the graph's direct slices.
pub(crate) fn channelInventoryIsClosed(g: &GraphRef, snapshot: &routeMutationSnapshot, guard: &routeWorkGuardRef) -> GoResult<bool> {
    let mut nodes: Set<layoutgraph::NodeRef> = Set::default();
    let mut edges: Set<EdgeRef> = Set::default();
    for n in g.nodes() {
        guard.step()?;
        nodes.insert(n);
    }
    for e in g.edges() {
        guard.step()?;
        edges.insert(e);
    }
    for (n, _, _, _) in &snapshot.nodes {
        guard.step()?;
        if !nodes.contains(n) {
            return Ok(false);
        }
    }
    for (e, _) in &snapshot.edges {
        guard.step()?;
        if !edges.contains(e) {
            return Ok(false);
        }
    }
    Ok(true)
}

pub(crate) fn nudgeChannelsWithLimit(ctx: &Ctx, g: &GraphRef, limit: u64) -> GoResult<()> {
    if let Some(err) = ctx.Err() {
        return Err(err);
    }
    if channelInputTooLarge(g) {
        // Admission scans at most maxChannelSegments edge references. Check
        // cancellation again before returning without the atomic stage.
        return match ctx.Err() {
            Some(err) => Err(err),
            None => Ok(()),
        };
    }
    runAtomicRouteStage(ctx, "NudgeChannels", g, &[], limit, |guard| nudgeChannelsGuarded(g, guard))
}

// Oversized inputs are an optional no-op.
pub(crate) fn channelInputTooLarge(g: &GraphRef) -> bool {
    let (nodes, edges) = {
        let gb = g.b();
        (gb.Nodes.len(), gb.Edges.clone())
    };
    if nodes > maxChannelSegments || edges.len() > maxChannelSegments {
        return true;
    }
    let mut remaining = maxChannelSegments;
    for edge in &edges {
        let n = edge.b().Points.len();
        if n > remaining {
            return true;
        }
        remaining -= n;
    }
    false
}

#[inline]
pub(crate) fn channelCoordinate(p: &geo::Point, horizontal: bool) -> f64 {
    if horizontal {
        p.X
    } else {
        p.Y
    }
}

pub(crate) fn buildChannelProblem(g: &GraphRef, horizontal: bool, guard: &routeWorkGuardRef) -> GoResult<channelProblem> {
    let mut result = channelProblem::default();
    let edges = g.edges();
    let nodes = g.nodes();
    let segments = edgeSegmentsGuarded(&edges, !horizontal, guard)?;
    let mut fixedPoints: Set<geo::PointRef> = Set::default();
    for n in &nodes {
        if let Some(tl) = n.tl() {
            fixedPoints.insert(tl);
        }
    }
    let nodeToTree = g.b().NodeToTree.clone();
    let mut lockedEdges: Map<EdgeRef, bool> = Map::default();
    for e in &edges {
        guard.step()?;
        let points = e.points();
        if points.len() < 2 {
            continue;
        }
        fixedPoints.insert(points[0].clone());
        fixedPoints.insert(points[points.len() - 1].clone());
        let fromTree = nodeToTree.has(&e.from());
        let toTree = nodeToTree.has(&e.to());
        let mut locked = e.b().IsCurve
            || e.IsLoop()
            || fromTree
            || toTree
            || (isSpecialEdgeForBalancing(g, e) && !hasFixedBalancingPorts(e));
        for i in 1..points.len() {
            guard.step()?;
            let (a, b) = (points[i - 1].get(), points[i].get());
            if (a.X != b.X && a.Y != b.Y) || a == b {
                locked = true;
            }
        }
        lockedEdges.insert(e.clone(), locked);
    }
    let mut parent: Vec<usize> = (0..segments.len()).collect();
    fn root(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    let segValues: Vec<geo::Segment> = segments.iter().map(|s| s.b().Segment.clone()).collect();
    for i in 0..segments.len() {
        let s = &segValues[i];
        for j in 0..i {
            guard.step()?;
            let other = &segValues[j];
            if channelCoordinate(&s.Start.get(), horizontal) == channelCoordinate(&other.Start.get(), horizontal)
                && s.Overlaps(other, horizontal, segmentSpacingBuffer)
            {
                let ri = root(&mut parent, i);
                let rj = root(&mut parent, j);
                parent[ri] = rj;
            }
        }
    }
    let mut byRoot: Map<usize, channelGroupRef> = Map::default();
    let mut groups: Vec<channelGroupRef> = Vec::new();
    for (i, s) in segments.iter().enumerate() {
        guard.step()?;
        let r = root(&mut parent, i);
        let group = match byRoot.get(&r) {
            Some(group) => group.clone(),
            None => {
                let group = channelGroupRef::new(channelGroup {
                    segments: Vec::new(),
                    position: channelCoordinate(&segValues[i].Start.get(), horizontal),
                    lower: 0.0,
                    upper: 0.0,
                    fixed: false,
                    nodeClearance: None,
                });
                byRoot.insert(r, group.clone());
                groups.push(group.clone());
                group
            }
        };
        let (start, end) = (segValues[i].Start.clone(), segValues[i].End.clone());
        {
            let mut gb = group.bm();
            gb.segments.push(s.clone());
            gb.fixed = gb.fixed
                || lockedEdges.get(&s.b().Owner()).copied().unwrap_or(false)
                || fixedPoints.contains(&start)
                || fixedPoints.contains(&end);
        }
        let (sv, ev) = (start.get(), end.get());
        for n in &nodes {
            guard.step()?;
            if n.w() != 1.0 || n.h() != 1.0 {
                continue;
            }
            let tl = n.tlv();
            for p in [sv, ev] {
                if (p.X == tl.X || p.X == tl.X + 1.0) && (p.Y == tl.Y || p.Y == tl.Y + 1.0) {
                    group.bm().fixed = true;
                }
            }
        }
    }
    let mut fixed = nodeSegmentsGuarded(&nodes, !horizontal, guard)?;
    let nodeWalls = fixed.clone();
    for group in &groups {
        if group.b().fixed {
            continue;
        }
        let mut clearance = Range { start: math::Inf(-1), end: math::Inf(1) };
        let (segs, position) = {
            let gb = group.b();
            (gb.segments.clone(), gb.position)
        };
        for segment in &segs {
            let seg = segment.b().Segment.clone();
            let (lower, upper) = routeSegmentBounds(&seg, &nodeWalls, segmentSpacingBuffer, guard)?;
            if lower != math::Inf(-1) {
                clearance.start = math::Max(clearance.start, lower + math::Min(position - lower, segmentSpacingBuffer));
            }
            if upper != math::Inf(1) {
                clearance.end = math::Min(clearance.end, upper - math::Min(upper - position, segmentSpacingBuffer));
            }
        }
        group.bm().nodeClearance = Some(clearance);
    }
    let appendFixed = |fixed: &mut Vec<geo::Segment>, group: &channelGroupRef| {
        for s in group.b().segments.iter() {
            fixed.push(s.b().Segment.clone());
        }
    };
    for group in &groups {
        if group.b().fixed {
            appendFixed(&mut fixed, group);
        }
    }
    // A real wall must bound both sides.
    for _pass in 0..2 {
        for group in &groups {
            if group.b().fixed {
                continue;
            }
            let (segs, position) = {
                let gb = group.b();
                (gb.segments.clone(), gb.position)
            };
            let (mut lower, mut upper) = (math::Inf(-1), math::Inf(1));
            for s in &segs {
                let seg = s.b().Segment.clone();
                let (lo, hi) = routeSegmentBounds(&seg, &fixed, segmentSpacingBuffer, guard)?;
                lower = math::Max(lower, lo);
                upper = math::Min(upper, hi);
            }
            {
                let mut gb = group.bm();
                gb.lower = lower;
                gb.upper = upper;
            }
            if lower.is_infinite() || upper.is_infinite() || lower > position || upper < position || lower >= upper {
                group.bm().fixed = true;
                appendFixed(&mut fixed, group);
            }
        }
    }
    for group in &groups {
        if !group.b().fixed {
            result.groups.push(group.clone());
        }
    }
    stableSortRouteValues(&mut result.groups, |a, b| a.b().position < b.b().position, guard)?;
    for i in 0..result.groups.len() {
        let a = result.groups[i].clone();
        for j in i + 1..result.groups.len() {
            let b = result.groups[j].clone();
            let mut linked = false;
            let mut sameOwner = true;
            let (aSegs, bSegs) = (a.b().segments.clone(), b.b().segments.clone());
            let owner = aSegs[0].b().Owner();
            for s in &aSegs {
                sameOwner = sameOwner && s.b().Owner() == owner;
            }
            for s in &bSegs {
                sameOwner = sameOwner && s.b().Owner() == owner;
            }
            let mut buffer = segmentSpacingBuffer;
            if sameOwner {
                buffer = 0.0;
            }
            for s in &aSegs {
                for t in &bSegs {
                    guard.step()?;
                    linked = linked || s.b().Segment.Overlaps(&t.b().Segment, horizontal, buffer);
                }
            }
            if linked {
                result.arcs.push(channelArc { from: i, to: j, separate: !sameOwner });
            }
        }
    }
    Ok(result)
}

// solveChannelGap computes the earliest and latest feasible solutions of the
// same difference constraints. Their midpoint is feasible by convexity.
pub(crate) fn solveChannelGap(p: &channelProblem, gap: f64, guard: &routeWorkGuardRef) -> GoResult<(Vec<f64>, Vec<f64>, bool)> {
    let n = p.groups.len();
    guard.add(n as u64 * 2)?;
    let mut lo = vec![0.0; n];
    let mut hi = vec![0.0; n];
    for (i, group) in p.groups.iter().enumerate() {
        let gb = group.b();
        lo[i] = gb.lower + gap;
        hi[i] = gb.upper - gap;
        if let Some(clearance) = &gb.nodeClearance {
            lo[i] = math::Max(lo[i], clearance.start);
            hi[i] = math::Min(hi[i], clearance.end);
        }
    }
    for arc in &p.arcs {
        guard.step()?;
        let separation = if arc.separate { gap } else { 0.0 };
        lo[arc.to] = math::Max(lo[arc.to], lo[arc.from] + separation);
    }
    for arc in p.arcs.iter().rev() {
        guard.step()?;
        let separation = if arc.separate { gap } else { 0.0 };
        hi[arc.from] = math::Min(hi[arc.from], hi[arc.to] - separation);
    }
    for i in 0..lo.len() {
        if lo[i] > hi[i] {
            return Ok((Vec::new(), Vec::new(), false));
        }
    }
    Ok((lo, hi, true))
}

pub(crate) fn channelGap(p: &channelProblem, positions: &[f64]) -> f64 {
    let mut gap = math::Inf(1);
    for (i, group) in p.groups.iter().enumerate() {
        let gb = group.b();
        gap = math::Min(gap, math::Min(positions[i] - gb.lower, gb.upper - positions[i]));
    }
    for arc in &p.arcs {
        if arc.separate {
            gap = math::Min(gap, positions[arc.to] - positions[arc.from]);
        }
    }
    gap
}

/// Go `map[*geo.Point]*geo.Point`; candidate points are fresh values.
pub(crate) type channelProposal = Map<geo::PointRef, geo::Point>;

pub(crate) fn applyChannelProblem(g: &GraphRef, p: &channelProblem, horizontal: bool, guard: &routeWorkGuardRef) -> GoResult<bool> {
    let mut parent: Vec<usize> = (0..p.groups.len()).collect();
    fn root(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    for arc in &p.arcs {
        let rt = root(&mut parent, arc.to);
        let rf = root(&mut parent, arc.from);
        parent[rt] = rf;
    }
    let mut components: Map<usize, Vec<usize>> = Map::default();
    let mut order: Vec<usize> = Vec::new();
    for i in 0..p.groups.len() {
        let id = root(&mut parent, i);
        if !components.contains_key(&id) {
            order.push(id);
        }
        components.entry(id).or_default().push(i);
    }
    let mut changed = false;
    for id in &order {
        let mut local = channelProblem::default();
        let mut indices: Map<usize, usize> = Map::default();
        for &i in &components[id] {
            indices.insert(i, local.groups.len());
            local.groups.push(p.groups[i].clone());
        }
        for arc in &p.arcs {
            if let Some(&from) = indices.get(&arc.from) {
                local.arcs.push(channelArc { from, to: indices.get(&arc.to).copied().unwrap_or(0), separate: arc.separate });
            }
        }
        let positions: Vec<f64> = local.groups.iter().map(|group| group.b().position).collect();
        let oldGap = math::Max(0.0, channelGap(&local, &positions));
        let (mut left, mut right) = (oldGap, math::Inf(1));
        for group in &local.groups {
            let gb = group.b();
            right = math::Min(right, (gb.upper - gb.lower) / 2.0);
        }
        let mut iteration = 0;
        while iteration < 32 && right - left > channelEpsilon {
            let mid = (left + right) / 2.0;
            let (_, _, feasible) = solveChannelGap(&local, mid, guard)?;
            if feasible {
                left = mid;
            } else {
                right = mid;
            }
            iteration += 1;
        }
        let mut bestLength = channelWireLength(g, None, guard)?;
        let mut bestGap = oldGap;
        let mut best: Option<channelProposal> = None;
        for gap in [oldGap, left] {
            let (earliest, latest, feasible) = solveChannelGap(&local, gap, guard)?;
            if !feasible {
                continue;
            }
            for fraction in [0.0, 1.0, 0.5] {
                let mut candidate = vec![0.0; positions.len()];
                for i in 0..candidate.len() {
                    candidate[i] = earliest[i] * (1.0 - fraction) + latest[i] * fraction;
                }
                let (proposal, compatible) = channelPointProposal(&local, &candidate, horizontal, guard)?;
                if !compatible {
                    continue;
                }
                let length = channelWireLength(g, Some(&proposal), guard)?;
                let actualGap = channelGap(&local, &candidate);
                // Pixel-level rounding changes alone are not a spacing benefit.
                if length > bestLength + channelEpsilon || ((length - bestLength).abs() <= channelEpsilon && actualGap < bestGap + 1.0) {
                    continue;
                }
                let safe = channelProposalSafe(g, &proposal, guard)?;
                if !safe {
                    continue;
                }
                best = Some(proposal);
                bestLength = length;
                bestGap = actualGap;
            }
        }
        if let Some(best) = best {
            // Go ranges over the map; the targets are distinct pointers.
            for (original, candidate) in best.iter() {
                guard.step()?;
                original.set(*candidate);
            }
            changed = true;
        }
    }
    Ok(changed)
}

pub(crate) fn channelPointProposal(
    p: &channelProblem,
    positions: &[f64],
    horizontal: bool,
    guard: &routeWorkGuardRef,
) -> GoResult<(channelProposal, bool)> {
    let mut proposal: channelProposal = Map::default();
    for (i, group) in p.groups.iter().enumerate() {
        let segs = group.b().segments.clone();
        for s in &segs {
            let (start, end) = {
                let sb = s.b();
                (sb.Segment.Start.clone(), sb.Segment.End.clone())
            };
            for point in [start, end] {
                guard.step()?;
                let mut candidate = point.get();
                if horizontal {
                    candidate.X = positions[i];
                } else {
                    candidate.Y = positions[i];
                }
                if let Some(old) = proposal.get(&point) {
                    if *old != candidate {
                        return Ok((Map::default(), false));
                    }
                }
                proposal.insert(point, candidate);
            }
        }
    }
    Ok((proposal, true))
}

fn proposed(proposal: Option<&channelProposal>, p: &geo::PointRef) -> geo::Point {
    match proposal.and_then(|m| m.get(p)) {
        Some(v) => *v,
        None => p.get(),
    }
}

pub(crate) fn channelWireLength(g: &GraphRef, proposal: Option<&channelProposal>, guard: &routeWorkGuardRef) -> GoResult<f64> {
    let mut length = 0.0;
    for e in g.edges() {
        let points = e.points();
        for i in 1..points.len() {
            guard.step()?;
            let a = proposed(proposal, &points[i - 1]);
            let b = proposed(proposal, &points[i]);
            length += math::Hypot(b.X - a.X, b.Y - a.Y);
        }
    }
    Ok(length)
}

fn segmentsIntersect(a: &geo::Point, b: &geo::Point, c: &geo::Point, d: &geo::Point) -> bool {
    geo::IntersectionPointV(a, b, c, d).is_some()
}

pub(crate) fn channelProposalSafe(g: &GraphRef, proposal: &channelProposal, guard: &routeWorkGuardRef) -> GoResult<bool> {
    let edges = g.edges();
    let mut candidates: Vec<Vec<geo::Point>> = Vec::with_capacity(edges.len());
    let mut originals: Vec<Vec<geo::Point>> = Vec::with_capacity(edges.len());
    let mut changed = vec![false; edges.len()];
    for (index, e) in edges.iter().enumerate() {
        let points = e.points();
        let original = pointValues(&points);
        let mut candidate = original.clone();
        for (i, point) in points.iter().enumerate() {
            guard.step()?;
            if let Some(p) = proposal.get(point) {
                candidate[i] = *p;
                changed[index] = changed[index] || *p != original[i];
            }
        }
        candidates.push(candidate);
        originals.push(original);
        if !changed[index] {
            continue;
        }
        if e.b().IsCurve {
            return Ok(false);
        }
        let candidate = &candidates[index];
        let original = &originals[index];
        let safe = changedRouteIsClear(g, e, candidate, original, true, guard)?;
        if !safe {
            return Ok(false);
        }
        let (mut beforeLength, mut afterLength) = (0.0, 0.0);
        let mut nonzero: Vec<geo::Point> = Vec::new();
        for (i, point) in candidate.iter().enumerate() {
            guard.step()?;
            if point.X.is_nan() || point.Y.is_nan() || point.X.is_infinite() || point.Y.is_infinite() {
                return Ok(false);
            }
            if i > 0 {
                let (old, previous) = (original[i - 1], candidate[i - 1]);
                let before = math::Hypot(original[i].X - old.X, original[i].Y - old.Y);
                let after = math::Hypot(point.X - previous.X, point.Y - previous.Y);
                beforeLength += before;
                afterLength += after;
                if after > channelEpsilon && after + channelEpsilon < math::Min(before, segmentSpacingBuffer) {
                    return Ok(false);
                }
            }
            if nonzero.is_empty() || nonzero[nonzero.len() - 1] != *point {
                nonzero.push(*point);
            }
        }
        if afterLength > beforeLength + channelEpsilon {
            return Ok(false);
        }
        for i in 2..nonzero.len() {
            let (a, b, c) = (nonzero[i - 2], nonzero[i - 1], nonzero[i]);
            if (b.X - a.X) * (c.X - b.X) + (b.Y - a.Y) * (c.Y - b.Y) < 0.0 {
                return Ok(false);
            }
        }
        let mut i = 0usize;
        while i + 1 < nonzero.len() {
            let mut j = i + 2;
            while j + 1 < nonzero.len() {
                guard.step()?;
                if segmentsIntersect(&nonzero[i], &nonzero[i + 1], &nonzero[j], &nonzero[j + 1]) {
                    return Ok(false);
                }
                j += 1;
            }
            i += 1;
        }
    }
    for i in 0..edges.len() {
        for j in i + 1..edges.len() {
            if !changed[i] && !changed[j] {
                continue;
            }
            if edges[i].b().IsCurve || edges[j].b().IsCurve {
                return Ok(false);
            }
            let (e, other) = (&originals[i], &originals[j]);
            let (ci, cj) = (&candidates[i], &candidates[j]);
            let (mut beforeCrossings, mut afterCrossings) = (0i64, 0i64);
            let mut a = 0usize;
            while a + 1 < e.len() {
                let mut b = 0usize;
                while b + 1 < other.len() {
                    guard.step()?;
                    if nonSharedCrossingValues(e, other, a, b) {
                        beforeCrossings += 1;
                    }
                    if nonSharedCrossingValues(ci, cj, a, b) {
                        afterCrossings += 1;
                    }
                    let old = balanceCollinearOverlap(&e[a], &e[a + 1], &other[b], &other[b + 1]);
                    let new = balanceCollinearOverlap(&ci[a], &ci[a + 1], &cj[b], &cj[b + 1]);
                    if old == 0.0 && new > 0.0 {
                        return Ok(false);
                    }
                    let oldContact = segmentsIntersect(&e[a], &e[a + 1], &other[b], &other[b + 1]);
                    let newContact = segmentsIntersect(&ci[a], &ci[a + 1], &cj[b], &cj[b + 1]);
                    if !oldContact && newContact {
                        return Ok(false);
                    }
                    b += 1;
                }
                a += 1;
            }
            if afterCrossings > beforeCrossings {
                return Ok(false);
            }
        }
    }
    Ok(true)
}
