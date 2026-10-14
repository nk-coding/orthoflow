// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/hierarchy/compound.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;
use gocompat::rand::RandRef;

pub(crate) const maxCompoundBlocks: usize = 64;
pub(crate) const maxCompoundInterfaces: usize = 256;

pub(crate) struct compoundBlock {
    pub(crate) original: NodeRef,
    pub(crate) proxy: NodeRef,
    pub(crate) members: Vec<NodeRef>,
}

/// Index into the blocks slice (Go `*compoundBlock`).
pub(crate) type CB = usize;

#[derive(Clone, Copy)]
pub(crate) struct compoundInterface {
    pub(crate) from: CB,
    pub(crate) to: CB,
    pub(crate) fromOffset: geo::Point,
    pub(crate) toOffset: geo::Point,
}

/// PlaceCompound lays out the outer flow of detailed containers after their
/// interiors have been placed and rescaled, but before edge routing.
pub fn PlaceCompound(ctx: &Ctx, g: &GraphRef, random: Option<&RandRef>) -> GoResult<bool> {
    layoutgraph::Validate(ctx, "PlaceCompound", g)?;
    let rootNodes = g.b().Containers.at(&None);
    if rootNodes.len() < 3 || rootNodes.len() > maxCompoundBlocks {
        return Ok(false);
    }
    let (ctx, guard) = layoutgraph::EnsureTransactionWorkGuard(ctx, "PlaceCompoundTransactions")?;
    let proxy = layoutgraph::NewGraph();
    let r = placeCompoundWithProxy(&ctx, g, random, &guard, &proxy, &rootNodes);
    // Rust-only: the proxy graph owns only proxy nodes/edges (labels are copies);
    // break its Rc cycles so it is freed.
    layoutgraph::Teardown(&proxy);
    r
}

fn placeCompoundWithProxy(
    ctx: &Ctx,
    g: &GraphRef,
    random: Option<&RandRef>,
    guard: &WorkGuardRef,
    proxy: &GraphRef,
    rootNodes: &[NodeRef],
) -> GoResult<bool> {
    let ctx = ctx.clone();
    let guard = guard.clone();
    let dir = g.Direction(None);
    proxy.b().Directions.set(None, dir);
    let mut blocks: Vec<compoundBlock> = Vec::with_capacity(rootNodes.len());
    let mut byRoot: Map<NodeRef, CB> = Map::default();
    for root in rootNodes {
        guard.Step()?;
        let n = layoutgraph::NewNode(root.id(), root.w(), root.h());
        n.set_tl(Some(root.tl().expect("nil pointer dereference: root.TopLeft").Copy()));
        proxy.AddNewNodeToContainer(None, &n);
        byRoot.insert(root.clone(), blocks.len());
        blocks.push(compoundBlock { original: root.clone(), proxy: n, members: Vec::new() });
    }
    let gnodes = g.b().Nodes.clone();
    let mut owner: Map<NodeRef, CB> = Map::default();
    for node in &gnodes {
        guard.Step()?;
        if node.b().FixedTopLeft.is_some() || g.IsSequenceVessel(node) || node.IsClusterVessel() {
            return Ok(false);
        }
        let mut root = node.clone();
        loop {
            let c = root.b().Container.clone();
            let Some(c) = c else { break };
            guard.Step()?;
            root = c;
        }
        let Some(&block) = byRoot.get(&root) else {
            return Ok(false);
        };
        blocks[block].members.push(node.clone());
        owner.insert(node.clone(), block);
    }
    let treeNodes = g.b().NodeToTree.keys();
    for node in &treeNodes {
        guard.Step()?;
        if !owner.contains_key(node) {
            return Ok(false);
        }
    }
    let mut interfaces: Vec<compoundInterface> = Vec::new();
    let mut detailed = false;
    let gedges = g.b().Edges.clone();
    for edge in &gedges {
        guard.Step()?;
        let (ef, et) = (edge.from(), edge.to());
        let (Some(&from), Some(&to)) = (owner.get(&ef), owner.get(&et)) else {
            return Ok(false);
        };
        if from == to {
            detailed = detailed || (blocks[from].original.IsContainer() && ef != et);
            continue;
        }
        if interfaces.len() == maxCompoundInterfaces {
            return Ok(false);
        }
        let e = proxy.Connect(&blocks[from].proxy, &blocks[to].proxy);
        {
            let src = edge.b();
            let mut eb = e.bm();
            eb.ID = src.ID;
            eb.SourceArrowhead = src.SourceArrowhead.clone();
            eb.TargetArrowhead = src.TargetArrowhead.clone();
            if let Some(l) = &src.Label {
                let copied = l.b().clone();
                eb.Label = Some(layoutgraph::LabelRef::new(copied));
            }
        }
        let (fci, tci) = {
            let src = edge.b();
            (src.FromTableColumnIndex, src.ToTableColumnIndex)
        };
        interfaces.push(compoundInterface {
            from,
            to,
            fromOffset: compoundEndpointOffset(&ef, &blocks[from].original, fci),
            toOffset: compoundEndpointOffset(&et, &blocks[to].original, tci),
        });
    }
    if !detailed || !compoundConnected(&blocks, &interfaces) {
        return Ok(false);
    }
    let outerHierarchy = build(&ctx, proxy, true, &Candidates(proxy), None)?;
    let Some(outerHierarchy) = outerHierarchy else {
        return Ok(false);
    };
    let (forward, other) = countEdgeDirection(&outerHierarchy, proxy);
    if outerHierarchy.b().LevelCount < 2 || forward == 0 || (forward as f64) < 1.5 * other as f64 {
        return Ok(false);
    }
    let pnodes = proxy.b().Nodes.clone();
    for node in &pnodes {
        guard.Step()?;
        node.bm().Hierarchy = Some(outerHierarchy.clone());
    }
    Place(&ctx, proxy, None, random)?;
    orderCompoundInterfaces(&blocks, &interfaces, proxy.Direction(None).IsHorizontal(), &guard)?;
    let (tl, br) = pnodes.BoundingBox();
    let (tl, br) = (*tl.unwrap().b(), *br.unwrap().b());
    if br.X - tl.X > limits::MaxGraphSize as f64 || br.Y - tl.Y > limits::MaxGraphSize as f64 {
        return Ok(false);
    }
    let state = layoutgraph::NewGraphStateSnapshot(layoutgraph::GraphStateSnapshotOptions {
        CaptureTopology: true,
        CaptureEdgeRoutes: true,
    });
    state.UpdateWithWorkGuard(g, &guard)?;
    let r = (|| -> GoResult<bool> {
        let rootNodes = g.b().Containers.at(&None);
        let (originalTL, _) = rootNodes.BoundingBox();
        let originalTL = *originalTL.unwrap().b();
        let h = layoutgraph::NewHierarchy();
        h.bm().LevelCount = pnodes[0].b().Hierarchy.clone().unwrap().b().LevelCount;
        for block in &blocks {
            guard.Step()?;
            let dx = block.proxy.x() - tl.X + originalTL.X - block.original.x();
            let dy = block.proxy.y() - tl.Y + originalTL.Y - block.original.y();
            for member in &block.members {
                guard.Step()?;
                member.Translate(dx, dy);
            }
            if let Some(previous) = block.original.b().Hierarchy.clone() {
                previous.Levels().delete(&block.original);
            }
            block.original.bm().Hierarchy = Some(h.clone());
            h.Levels().set(block.original.clone(), block.proxy.HierarchyLevel());
        }
        guard.Finish()?;
        Ok(true)
    })();
    if r.is_err() {
        layoutgraph::RestoreGraphState(g, &state);
    }
    r
}

pub(crate) fn compoundEndpointOffset(endpoint: &NodeRef, block: &NodeRef, column: Option<i64>) -> geo::Point {
    let mut center = *endpoint.Center().b();
    if let Some(column) = column {
        let (port, ok) = nodeshape::TableColumnPortValue(&*endpoint.shape(), geo::Right, column);
        if ok {
            center = port;
        }
    }
    let btl = block.tlv();
    geo::Point {
        X: math::Max(0.0, math::Min(block.w(), center.X - btl.X)),
        Y: math::Max(0.0, math::Min(block.h(), center.Y - btl.Y)),
    }
}

pub(crate) fn compoundConnected(blocks: &[compoundBlock], interfaces: &[compoundInterface]) -> bool {
    let mut adjacent: Map<CB, Vec<CB>> = Map::default();
    for edge in interfaces {
        adjacent.entry(edge.from).or_default().push(edge.to);
        adjacent.entry(edge.to).or_default().push(edge.from);
    }
    let mut seen: Set<CB> = Set::default();
    seen.insert(0);
    let mut queue: std::collections::VecDeque<CB> = std::collections::VecDeque::from([0]);
    while let Some(block) = queue.pop_front() {
        if let Some(nexts) = adjacent.get(&block) {
            for &next in nexts {
                if seen.insert(next) {
                    queue.push_back(next);
                }
            }
        }
    }
    seen.len() == blocks.len()
}

/// orderCompoundInterfaces refines the center-based layered ordering using actual boundary offsets.
pub(crate) fn orderCompoundInterfaces(blocks: &[compoundBlock], interfaces: &[compoundInterface], horizontal: bool, guard: &WorkGuardRef) -> GoResult<()> {
    let mut levels: Map<i64, Vec<CB>> = Map::default();
    for (i, block) in blocks.iter().enumerate() {
        levels.entry(block.proxy.HierarchyLevel()).or_default().push(i);
    }
    let getCoord = |b: CB| -> (f64, f64) {
        let p = &blocks[b].proxy;
        if horizontal {
            (p.y(), p.h())
        } else {
            (p.x(), p.w())
        }
    };
    let setCoord = |b: CB, v: f64| {
        let p = &blocks[b].proxy;
        if horizontal {
            p.set_y(v);
        } else {
            p.set_x(v);
        }
    };
    let levelCount = blocks[0].proxy.b().Hierarchy.clone().unwrap().b().LevelCount;
    for level in 0..levelCount {
        let mut row = levels.get(&level).cloned().unwrap_or_default();
        gocompat::slices::SortFunc(&mut row, |&x, &y| -> i64 {
            let (ax, _) = getCoord(x);
            let (bx, _) = getCoord(y);
            if ax < bx {
                return -1;
            }
            if ax > bx {
                return 1;
            }
            0
        });
        levels.insert(level, row);
    }
    let mut score = compoundInterfaceScore(blocks, interfaces, horizontal, guard)?;
    for _pass in 0..2 {
        let mut improved = false;
        for level in 0..levelCount {
            let mut row = levels.get(&level).cloned().unwrap_or_default();
            let mut i = 0;
            while i + 1 < row.len() {
                guard.Step()?;
                let (a, b) = (row[i], row[i + 1]);
                let (oldA, aw) = getCoord(a);
                let (oldB, bw) = getCoord(b);
                setCoord(a, oldB + bw - aw);
                setCoord(b, oldA);
                let candidate = compoundInterfaceScore(blocks, interfaces, horizontal, guard)?;
                if candidate.crossings < score.crossings || (candidate.crossings == score.crossings && candidate.span < score.span) {
                    score = candidate;
                    row[i] = b;
                    row[i + 1] = a;
                    improved = true;
                } else {
                    setCoord(a, oldA);
                    setCoord(b, oldB);
                }
                i += 1;
            }
            levels.insert(level, row);
        }
        if !improved {
            break;
        }
    }
    guard.Finish()
}

#[derive(Clone, Copy, Default)]
pub(crate) struct compoundScore {
    pub(crate) crossings: i64,
    pub(crate) span: f64,
}

pub(crate) fn compoundInterfaceScore(blocks: &[compoundBlock], interfaces: &[compoundInterface], horizontal: bool, guard: &WorkGuardRef) -> GoResult<compoundScore> {
    let mut score = compoundScore::default();
    let mut segments: Vec<(geo::Point, geo::Point)> = Vec::with_capacity(interfaces.len());
    for edge in interfaces {
        guard.Step()?;
        let (from, to) = (&blocks[edge.from].proxy, &blocks[edge.to].proxy);
        let (ftl, ttl) = (from.tlv(), to.tlv());
        let mut a = geo::Point { X: ftl.X + edge.fromOffset.X, Y: ftl.Y + edge.fromOffset.Y };
        let mut b = geo::Point { X: ttl.X + edge.toOffset.X, Y: ttl.Y + edge.toOffset.Y };
        if horizontal {
            if from.Center().b().X < to.Center().b().X {
                a.X = ftl.X + from.w();
                b.X = ttl.X;
            } else {
                a.X = ftl.X;
                b.X = ttl.X + to.w();
            }
            score.span += math::Abs(a.Y - b.Y);
        } else {
            if from.Center().b().Y < to.Center().b().Y {
                a.Y = ftl.Y + from.h();
                b.Y = ttl.Y;
            } else {
                a.Y = ftl.Y;
                b.Y = ttl.Y + to.h();
            }
            score.span += math::Abs(a.X - b.X);
        }
        segments.push((a, b));
    }
    let orientation = |a: geo::Point, b: geo::Point, c: geo::Point| -> f64 { (b.X - a.X) * (c.Y - a.Y) - (b.Y - a.Y) * (c.X - a.X) };
    for i in 0..segments.len() {
        let sa = segments[i];
        for sb in &segments[i + 1..] {
            guard.Step()?;
            if orientation(sa.0, sa.1, sb.0) * orientation(sa.0, sa.1, sb.1) < 0.0
                && orientation(sb.0, sb.1, sa.0) * orientation(sb.0, sb.1, sa.1) < 0.0
            {
                score.crossings += 1;
            }
        }
    }
    Ok(score)
}
