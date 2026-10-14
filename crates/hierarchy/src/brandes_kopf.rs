// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/hierarchy/brandes_kopf.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

/// Just a `tuple` to index the results of a given vertical/horizontal alignment
#[derive(Clone, Copy)]
pub(crate) struct alignmentDirection {
    pub(crate) vertical: geo::Orientation,
    pub(crate) horizontal: geo::Orientation,
}

/// Index of an alignmentNode in its arena (Go `*alignmentNode`).
pub(crate) type AN = usize;

pub(crate) struct alignmentNode {
    /// embedded *placementNode
    pub(crate) pn: PN,
    pub(crate) prevSibling: Option<AN>,
    pub(crate) root: AN,
    pub(crate) alignedWith: AN,
    pub(crate) sink: AN,
    pub(crate) medianNeighbors: Vec<AN>,
    pub(crate) shift: f64,
    pub(crate) x: f64,
    pub(crate) blockSize: f64,
    pub(crate) rightPad: f64,
    pub(crate) leftPad: f64,
}

pub(crate) fn alignHierarchy(ctx: &Ctx, a: &mut placementArena, g: &GraphRef, byLevel: &byLevelMap) -> GoResult<()> {
    let guard = limits::NewWorkGuard(ctx, "AlignHierarchy", limits::MaxEngineWorkUnits)?;
    let TL = alignmentDirection { vertical: geo::Top, horizontal: geo::Left };
    let TR = alignmentDirection { vertical: geo::Top, horizontal: geo::Right };
    let BR = alignmentDirection { vertical: geo::Bottom, horizontal: geo::Right };
    let BL = alignmentDirection { vertical: geo::Bottom, horizontal: geo::Left };

    let alignments = [TL, TR, BL, BR];

    let conflicts = markConflicts(ctx, a, byLevel)?;
    let mut minX = [0.0f64; 4];
    let mut maxX = [0.0f64; 4];
    let mut minWidth = math::Inf(1);
    let mut minWidthAlignment: usize = usize::MAX;
    let mut xs: Map<PN, Vec<f64>> = Map::default();
    let mut xsOrder: Vec<PN> = Vec::new();
    for (di, d) in alignments.iter().enumerate() {
        guard.Step()?;
        minX[di] = math::Inf(1);
        maxX[di] = math::Inf(-1);
        let (mut ans, alignmentNodes) = createAlignmentNodes(ctx, a, byLevel, d.vertical, d.horizontal)?;
        verticalAlignment(ctx, a, &mut ans, &alignmentNodes, &conflicts, d.horizontal)?;
        horizontalCompaction(ctx, a, &mut ans, &alignmentNodes, d.horizontal)?;
        for &n in &alignmentNodes {
            guard.Step()?;
            let an = &ans[n];
            let root = &ans[an.root];
            let x = an.x + (root.blockSize / 2.0) - (a.gn(an.pn).w() / 2.0);
            let entry = xs.entry(an.pn).or_insert_with(|| {
                xsOrder.push(an.pn);
                Vec::new()
            });
            entry.push(x);
            minX[di] = math::Min(minX[di], an.x);
            maxX[di] = math::Max(maxX[di], an.x + an.blockSize);
        }
        let width = maxX[di] - minX[di];
        if width < minWidth {
            minWidth = width;
            minWidthAlignment = di;
        }
    }

    // balance
    let mut shift = [0.0f64; 4];
    for (di, d) in alignments.iter().enumerate() {
        guard.Step()?;
        if d.horizontal == geo::Left {
            shift[di] = minX[minWidthAlignment] - minX[di];
        } else {
            shift[di] = maxX[minWidthAlignment] - maxX[di];
        }
    }

    // final placement
    let mut updates: Vec<(NodeRef, f64)> = Vec::with_capacity(xs.len());
    for pn in &xsOrder {
        guard.Step()?;
        let x = xs.get_mut(pn).unwrap();
        for i in 0..x.len() {
            guard.Step()?;
            x[i] += shift[i];
        }
        updates.push((a.gn(*pn), math::Round(median(x))));
    }
    guard.Finish()?;
    for (node, x) in &updates {
        node.set_x(*x);
    }
    syncContainers(g, &guard)
}

/// Reposition and resize containers based on the position of the aligned children.
pub(crate) fn syncContainers(g: &GraphRef, guard: &WorkGuardRef) -> GoResult<()> {
    let mut containers: Vec<NodeRef> = Vec::new();
    let mut queue: std::collections::VecDeque<NodeRef> = g.b().Nodes.iter().cloned().collect();
    while let Some(node) = queue.front().cloned() {
        guard.Step()?;
        if node.IsContainer() {
            containers.push(node.clone());
            queue.extend(g.b().Containers.at(&Some(node.clone())));
        }
        queue.pop_front();
    }

    for i in (0..containers.len()).rev() {
        guard.Step()?;
        containers[i].WrapChildren();
    }
    guard.Finish()
}

pub(crate) fn median(numbers: &[f64]) -> f64 {
    if numbers.is_empty() {
        return 0.0;
    } else if numbers.len() == 1 {
        return numbers[0];
    }
    let mut ordered = numbers.to_vec();
    gocompat::slices::Sort(&mut ordered);
    if ordered.len() % 2 == 1 {
        return ordered[ordered.len() / 2];
    }
    let middle = ordered.len() / 2;
    ordered[middle - 1] / 2.0 + ordered[middle] / 2.0
}

pub(crate) fn newAlignmentNode(a: &placementArena, ans: &mut Vec<alignmentNode>, pn: PN) -> AN {
    let blockSize = a.gn(pn).w();
    let idx = ans.len();
    ans.push(alignmentNode {
        pn,
        prevSibling: None,
        root: idx,
        alignedWith: idx,
        sink: idx,
        medianNeighbors: Vec::new(),
        shift: 0.0,
        x: 0.0,
        blockSize,
        rightPad: 0.0,
        leftPad: 0.0,
    });
    idx
}

/// Given layered nodes, creates a slice of alignment nodes sorted in the desired direction.
/// Returns (arena, ordered node indices).
pub(crate) fn createAlignmentNodes(
    ctx: &Ctx,
    a: &mut placementArena,
    byLevel: &byLevelMap,
    vertical: geo::Orientation,
    horizontal: geo::Orientation,
) -> GoResult<(Vec<alignmentNode>, Vec<AN>)> {
    let guard = limits::NewWorkGuard(ctx, "CreateAlignmentNodes", limits::MaxEngineWorkUnits)?;
    let mut ans: Vec<alignmentNode> = Vec::new();
    let mut placementToAlignment: Map<PN, AN> = Map::default();
    let mut nodes: Vec<AN> = Vec::new();
    for l in 0..byLevel.len() as i64 {
        guard.Step()?;
        let lvl = a.levelSlice(byLevel, l);
        let leaves = leafNodesContext(a, &mut ans, &lvl, &guard)?;
        for node in leaves {
            guard.Step()?;
            nodes.push(node);
            placementToAlignment.insert(ans[node].pn, node);
        }
    }

    sortAlignmentNodes(a, &ans, &mut nodes, vertical, horizontal);
    guard.Finish()?;

    let mut x = math::Inf(-1);
    let mut shift = math::Inf(1);
    if horizontal == geo::Right {
        x = math::Inf(1);
        shift = math::Inf(-1);
    }
    let mut previous: Option<AN> = None;
    for &node in &nodes {
        guard.Step()?;
        if let Some(p) = previous {
            if a.nodes[ans[node].pn].level != a.nodes[ans[p].pn].level {
                previous = None;
            }
        }
        let medianNeighbors = medianNeighborsContext(a, &ans, node, vertical, horizontal, &placementToAlignment, &guard)?;
        let n = &mut ans[node];
        n.medianNeighbors = medianNeighbors;
        n.prevSibling = previous;
        n.x = x;
        n.shift = shift;
        previous = Some(node);
    }
    guard.Finish()?;
    Ok((ans, nodes))
}

pub(crate) fn absInt(a: i64) -> i64 {
    if a < 0 {
        -a
    } else {
        a
    }
}

pub(crate) fn leafNodesContext(a: &mut placementArena, ans: &mut Vec<alignmentNode>, nodes: &[PN], guard: &WorkGuardRef) -> GoResult<Vec<AN>> {
    let mut leaves: Vec<AN> = Vec::new();
    for i in 0..nodes.len() {
        guard.Step()?;
        let node = nodes[i];
        if !a.nodes[node].isContainer {
            leaves.push(newAlignmentNode(a, ans, node));
            continue;
        }
        let children = a.children(node);
        let descendants = leafNodesContext(a, ans, &children, guard)?;

        let pickChildForEdge = |a: &placementArena, ans: &Vec<alignmentNode>, edgeEndpoint: PN, possible: &[AN]| -> GoResult<AN> {
            let mut best = possible[0];
            let erank = a.nodes[edgeEndpoint].rank;
            let mut minDiff = absInt(erank - a.nodes[ans[best].pn].rank);
            for &cand in possible {
                guard.Step()?;
                let diff = absInt(erank - a.nodes[ans[cand].pn].rank);
                if diff < minDiff {
                    minDiff = diff;
                    best = cand;
                }
            }
            Ok(best)
        };

        let aboves = a.nodes[node].aboves.clone();
        for above in aboves {
            guard.Step()?;
            setRemove(&mut a.nodes[above].belows, node);
            let bestChild = pickChildForEdge(a, ans, above, &descendants)?;
            let bpn = ans[bestChild].pn;
            setInsert(&mut a.nodes[above].belows, bpn);
            setInsert(&mut a.nodes[bpn].aboves, above);
        }

        let belows = a.nodes[node].belows.clone();
        for below in belows {
            guard.Step()?;
            setRemove(&mut a.nodes[below].aboves, node);
            let bestChild = pickChildForEdge(a, ans, below, &descendants)?;
            let bpn = ans[bestChild].pn;
            setInsert(&mut a.nodes[below].aboves, bpn);
            setInsert(&mut a.nodes[bpn].belows, below);
        }

        leaves.extend(descendants);
    }
    let (leftNode, rightNode) = (leaves[0], leaves[leaves.len() - 1]);
    let lp = containerPadding(a, Some(ans[leftNode].pn)).Left();
    ans[leftNode].leftPad += lp;
    let rp = containerPadding(a, Some(ans[rightNode].pn)).Right();
    ans[rightNode].rightPad += rp;
    Ok(leaves)
}

pub(crate) fn containerPadding(a: &placementArena, pn: Option<PN>) -> layoutgraph::Spacing {
    let Some(pn) = pn else {
        return layoutgraph::UniformSpacing(layoutgraph::ContainerPadding as f64);
    };
    if a.nodes[pn].isDummy {
        return layoutgraph::UniformSpacing(layoutgraph::ContainerPadding as f64);
    }
    let container = a.nodes[pn].container.and_then(|c| a.nodes[c].graphNode.clone());
    a.gn(pn).g().ContainerPadding(container.as_ref(), true)
}

pub(crate) fn sortAlignmentNodes(a: &placementArena, ans: &[alignmentNode], nodes: &mut Vec<AN>, vertical: geo::Orientation, horizontal: geo::Orientation) {
    let lv = |n: AN| a.nodes[ans[n].pn].level;
    let rk = |n: AN| a.nodes[ans[n].pn].rank;
    if vertical == geo::Top && horizontal == geo::Left {
        gocompat::sort::SortSlice(nodes, |&i, &j| if lv(i) == lv(j) { rk(i) < rk(j) } else { lv(i) < lv(j) });
    } else if vertical == geo::Top && horizontal == geo::Right {
        gocompat::sort::SortSlice(nodes, |&i, &j| if lv(i) == lv(j) { rk(j) < rk(i) } else { lv(i) < lv(j) });
    } else if vertical == geo::Bottom && horizontal == geo::Left {
        gocompat::sort::SortSlice(nodes, |&i, &j| if lv(i) == lv(j) { rk(i) < rk(j) } else { lv(j) < lv(i) });
    } else if vertical == geo::Bottom && horizontal == geo::Right {
        gocompat::sort::SortSlice(nodes, |&i, &j| if lv(i) == lv(j) { rk(j) < rk(i) } else { lv(j) < lv(i) });
    }
}

pub(crate) fn medianNeighborsContext(
    a: &placementArena,
    ans: &[alignmentNode],
    n: AN,
    vertical: geo::Orientation,
    horizontal: geo::Orientation,
    placementToAlignment: &Map<PN, AN>,
    guard: &WorkGuardRef,
) -> GoResult<Vec<AN>> {
    let mut neighbors: Vec<AN> = Vec::new();
    let pn = &a.nodes[ans[n].pn];

    if vertical == geo::Top {
        for &above in &pn.aboves {
            guard.Step()?;
            if a.nodes[above].level == pn.level - 1 {
                neighbors.push(*placementToAlignment.get(&above).expect("nil pointer dereference: alignment node"));
            }
        }
    } else {
        for &below in &pn.belows {
            guard.Step()?;
            if a.nodes[below].level == pn.level + 1 {
                neighbors.push(*placementToAlignment.get(&below).expect("nil pointer dereference: alignment node"));
            }
        }
    }

    if neighbors.is_empty() {
        return Ok(neighbors);
    }

    let xOf: Vec<(AN, f64)> = neighbors.iter().map(|&m| (m, a.gn(ans[m].pn).x())).collect();
    let xm: Map<AN, f64> = xOf.into_iter().collect();
    gocompat::sort::SortSlice(&mut neighbors, |i, j| xm[i] < xm[j]);

    let mid = (neighbors.len() + 1) as f64 / 2.0;
    if neighbors.len() % 2 == 1 {
        let medianIndex = mid as i64 - 1;
        Ok(vec![neighbors[medianIndex as usize]])
    } else {
        let mut leftMedian = math::Floor(mid) as i64 - 1;
        let mut rightMedian = math::Ceil(mid) as i64 - 1;
        if horizontal == geo::Right {
            std::mem::swap(&mut leftMedian, &mut rightMedian);
        }
        Ok(vec![neighbors[leftMedian as usize], neighbors[rightMedian as usize]])
    }
}

pub(crate) fn allAbove(a: &placementArena, pn: PN) -> Vec<PN> {
    a.nodes[pn].aboves.clone()
}

pub(crate) fn markConflicts(ctx: &Ctx, a: &placementArena, byLevel: &byLevelMap) -> GoResult<Set<(PN, PN)>> {
    let guard = limits::NewWorkGuard(ctx, "MarkHierarchyConflicts", limits::MaxEngineWorkUnits)?;
    let mut conflicts: Set<(PN, PN)> = Set::default();

    let nLevels = byLevel.len() as i64;
    let mut level = 1i64;
    while level < nLevels - 1 {
        guard.Step()?;
        let nextLevel = level + 1;
        let next = a.levelSlice(byLevel, nextLevel);
        let cur = a.levelSlice(byLevel, level);
        let mut k0: i64 = 0;
        let mut l = 0usize;
        for (l1, &pn) in next.iter().enumerate() {
            guard.Step()?;
            let mut k1: i64 = 0;
            if l1 == next.len() - 1 {
                let last = cur.len() - 1;
                k1 = a.nodes[cur[last]].rank;
            } else if a.nodes[pn].isDummy {
                for above in allAbove(a, pn) {
                    guard.Step()?;
                    if !a.nodes[above].isDummy {
                        continue;
                    }
                    k1 = a.nodes[above].rank;
                }
            } else {
                continue;
            }
            while l <= l1 {
                guard.Step()?;
                let n = next[l];
                for &above in &a.nodes[n].aboves {
                    guard.Step()?;
                    let r = a.nodes[above].rank;
                    if r < k0 || r > k1 {
                        conflicts.insert((n, above));
                        conflicts.insert((above, n));
                    }
                }
                l += 1;
            }
            k0 = k1;
        }
        level += 1;
    }

    guard.Finish()?;
    Ok(conflicts)
}

pub(crate) fn verticalAlignment(
    ctx: &Ctx,
    a: &placementArena,
    ans: &mut Vec<alignmentNode>,
    nodes: &[AN],
    conflicts: &Set<(PN, PN)>,
    horizontal: geo::Orientation,
) -> GoResult<()> {
    let guard = limits::NewWorkGuard(ctx, "VerticalAlignment", limits::MaxEngineWorkUnits)?;
    let rank = |ans: &Vec<alignmentNode>, m: AN| a.nodes[ans[m].pn].rank;

    let mut lastAlignedRank = i64::MAX;
    for &n in nodes {
        guard.Step()?;
        if ans[n].prevSibling.is_none() {
            if horizontal == geo::Left {
                lastAlignedRank = i64::MIN;
            } else {
                lastAlignedRank = i64::MAX;
            }
        }

        let medianNeighbors = ans[n].medianNeighbors.clone();
        for m in medianNeighbors {
            guard.Step()?;
            if conflicts.contains(&(ans[n].pn, ans[m].pn)) {
                continue;
            } else if ans[n].alignedWith != n {
                continue;
            } else if horizontal == geo::Left && lastAlignedRank >= rank(ans, m) {
                continue;
            } else if horizontal == geo::Right && lastAlignedRank <= rank(ans, m) {
                continue;
            }
            ans[m].alignedWith = n;
            let mroot = ans[m].root;
            ans[n].root = mroot;
            ans[n].alignedWith = mroot;
            let (nb, nl, nr) = (ans[n].blockSize, ans[n].leftPad, ans[n].rightPad);
            let r = &mut ans[mroot];
            r.blockSize = math::Max(r.blockSize, nb);
            r.leftPad = math::Max(r.leftPad, nl);
            r.rightPad = math::Max(r.rightPad, nr);
            lastAlignedRank = rank(ans, m);
        }
    }
    guard.Finish()
}

pub(crate) fn horizontalCompaction(
    ctx: &Ctx,
    a: &placementArena,
    ans: &mut Vec<alignmentNode>,
    nodes: &[AN],
    horizontalDirection: geo::Orientation,
) -> GoResult<()> {
    let guard = limits::NewWorkGuard(ctx, "HorizontalCompaction", limits::MaxEngineWorkUnits)?;
    for &n in nodes {
        guard.Step()?;
        if ans[n].root == n {
            placeBlock(a, ans, n, horizontalDirection, &guard)?;
        }
    }
    for &n in nodes {
        guard.Step()?;
        let root = ans[n].root;
        ans[n].x = ans[root].x;
        let sinkShift = ans[ans[n].sink].shift;
        if root == n && !math::IsInf(sinkShift, 0) {
            ans[n].x += sinkShift;
        }
    }
    guard.Finish()
}

pub(crate) fn placeBlock(a: &placementArena, ans: &mut Vec<alignmentNode>, root: AN, horizontal: geo::Orientation, guard: &WorkGuardRef) -> GoResult<()> {
    guard.Step()?;
    if !math::IsInf(ans[root].x, 0) {
        return Ok(());
    }
    ans[root].x = 0.0;
    let mut n = root;
    loop {
        guard.Step()?;
        if let Some(ps) = ans[n].prevSibling {
            let prevRoot = ans[ps].root;
            placeBlock(a, ans, prevRoot, horizontal, guard)?;
            if ans[root].sink == root {
                ans[root].sink = ans[prevRoot].sink;
            }
            let delta = distanceFromPreviousRoot(a, ans, prevRoot, root, horizontal);
            if ans[root].sink != ans[prevRoot].sink {
                let ps = ans[prevRoot].sink;
                if horizontal == geo::Left {
                    let v = ans[root].x - ans[prevRoot].x - ans[prevRoot].blockSize - delta;
                    ans[ps].shift = math::Min(ans[ps].shift, v);
                } else {
                    let v = ans[root].x - ans[prevRoot].x + ans[root].blockSize + delta;
                    ans[ps].shift = math::Max(ans[ps].shift, v);
                }
            } else if horizontal == geo::Left {
                ans[root].x = math::Max(ans[root].x, ans[prevRoot].x + ans[prevRoot].blockSize + delta);
            } else {
                ans[root].x = math::Min(ans[root].x, ans[prevRoot].x - ans[root].blockSize - delta);
            }
        }
        n = ans[n].alignedWith;
        if n == root {
            break;
        }
    }
    Ok(())
}

/// distanceFromPreviousRoot combines padding in the given direction.
pub(crate) fn distanceFromPreviousRoot(a: &placementArena, ans: &[alignmentNode], previous: AN, current: AN, horizontal: geo::Orientation) -> f64 {
    let mut pad = siblingSpacing;
    let cur = &ans[current];
    if a.nodes[cur.pn].isDummy || cur.prevSibling.map_or(false, |p| a.nodes[ans[p].pn].isDummy) {
        pad = siblingDummySpacing;
    }
    if horizontal == geo::Left {
        pad += ans[previous].rightPad + cur.leftPad;
    } else {
        pad += ans[previous].leftPad + cur.rightPad;
    }
    pad
}
