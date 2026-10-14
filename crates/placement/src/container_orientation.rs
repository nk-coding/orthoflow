// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/placement/container_orientation.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.
//
// Port of placement/container_orientation.go
#[allow(unused_imports)]
use crate::*;
use gocompat::math;
use gocompat::{Ctx, CtxKey, GoResult, Map, Set};
use layoutgraph::{GraphRef, NodeRef};

/// Go `containerOrientationDisabled{}` context key.
pub(crate) static containerOrientationDisabled: CtxKey = CtxKey::new("placement.containerOrientationDisabled");

// A container footprint can change the later placement of its siblings. Until
// those later operations validate self-loop envelopes, leave loop-bearing
// layouts on their existing placement path. This scan happens once per attempt.
pub(crate) fn orientationContext(ctx: &Ctx, g: &GraphRef) -> GoResult<Ctx> {
    let guard = limits::NewWorkGuard(ctx, "ContainerOrientationPreflight", limits::MaxEngineWorkUnits)?;
    for e in g.edges() {
        guard.Step()?;
        if e.IsLoop() {
            return Ok(ctx.WithValue(&containerOrientationDisabled, true));
        }
    }
    guard.Finish()?;
    Ok(ctx.clone())
}

// orientSourceInterior aligns the internal flow of a small source container
// with its surrounding flow, before the parent is fitted around the result.
// The optional move is local: it does not add another complete layout attempt.
pub(crate) fn orientSourceInterior(ctx: &Ctx, g: &GraphRef, root: Option<&NodeRef>, obstacles: &[geo::Box]) -> GoResult<()> {
    if let Some(err) = ctx.Err() {
        return Err(err);
    }
    if ctx.ValueAs::<bool>(&containerOrientationDisabled) == Some(true) {
        return Ok(());
    }
    let Some(root) = root else {
        return Ok(());
    };
    let gn = g.b().Nodes.len();
    if gn < 3 || gn > 16 || !obstacles.is_empty() {
        return Ok(());
    }
    {
        let r = root.b();
        if r.FixedTopLeft.is_some() || r.Cluster.is_some() || r.Sequence.is_some() || r.ForceHierarchy {
            return Ok(());
        }
    }
    if root.g().Direction(Some(root)) != geo::NONE {
        return Ok(());
    }
    let direction = g.Direction(root.OwningContainer().as_ref());
    if !direction.IsVertical() || root.b().Edges.len() < 3 {
        return Ok(());
    }
    let guard = limits::NewWorkGuard(ctx, "ContainerOrientation", limits::MaxEngineWorkUnits)?;
    let mut destinations: Set<NodeRef> = Set::default();
    for e in root.edges() {
        guard.Step()?;
        let (from, to, ok) = e.DirectedEndpoints();
        if !ok || from.as_ref() != Some(root) || to.as_ref() == Some(root) {
            return Ok(());
        }
        destinations.insert(to.expect("nil directed endpoint"));
    }
    if destinations.len() < 2 {
        return Ok(());
    }
    let gnodes = g.nodes();
    let mut local: Map<NodeRef, bool> = gocompat::NewMapCap(gnodes.len());
    for n in &gnodes {
        if n.IsContainer() {
            return Ok(());
        }
        {
            let nb = n.b();
            if nb.Hierarchy.is_some() || nb.FixedTopLeft.is_some() || nb.Sequence.is_some() {
                return Ok(());
            }
        }
        if g.IsTreeSentinel(Some(n)) || g.IsSequenceVessel(n) || n.b().HerdAssignment.is_some() {
            return Ok(());
        }
        // Self-loop envelopes do not turn with the boxes. Keep their established
        // placement until orientation can preserve that extra route clearance.
        for (_, offset) in n.b().LoopOffsets.iter() {
            if *offset > 0.0 {
                return Ok(());
            }
        }
        local.insert(n.clone(), true);
    }
    for n in &gnodes {
        for near in n.b().Nears.iter() {
            if !local.get(near).copied().unwrap_or(false) {
                return Ok(());
            }
        }
    }
    let flow = interiorFlow(g, &local, &guard)?;
    // A quarter-turn needs a clear transverse flow to improve. Disconnected
    // contents, balanced cycles and already vertical compositions are left alone.
    if flow.across <= flow.along + 1e-9 || flow.x.abs() <= 1e-9 {
        return Ok(());
    }
    let mut turn = 1.0;
    if (flow.x < 0.0) != (direction == geo::Top) {
        turn = -1.0;
    }

    let (beforeWidth, beforeHeight) = orientationFootprintSize(g, root);
    let txn = g.NewRequestTransaction(ctx, layoutgraph::TransactionOptions { IgnoreContainerEscape: true, ..Default::default() })?;
    {
        let ctx = ctx.clone();
        let g = g.clone();
        let root = root.clone();
        let guard = guard.clone();
        let local = local.clone();
        txn.AddOp(move || -> GoResult<()> {
            let gnodes = g.nodes();
            let mut centers: Map<NodeRef, geo::Point> = gocompat::NewMapCap(gnodes.len());
            for n in &gnodes {
                centers.insert(n.clone(), *n.Center().b());
            }
            for n in &gnodes {
                guard.Step()?;
                if let Some(c) = g.clusters().get(n) {
                    let flipped = c.b().Arrangement.Flip();
                    c.bm().Arrangement = flipped.clone();
                    c.bm().DesiredArrangement = flipped;
                    // Preserve the group's logical member order, but recalculate spacing for
                    // its new axis, including icon and outside-label room.
                    let padding = grouping::PaddingBetween(&c, true);
                    c.bm().Padding = padding;
                    c.Resize(n);
                }
                let p = centers.get(n).copied().unwrap_or_default();
                n.set_tl(Some(geo::NewPoint((-turn * p.Y - n.w() / 2.0).round(), (turn * p.X - n.h() / 2.0).round())));
            }
            g.SyncNestedGeometry();
            let oldCell = g.b().CellSize;
            g.bm().CellSize = 1.0;
            let result = (|| -> GoResult<()> {
                for axis in [horizontalAxis, verticalAxis] {
                    compaction(&ctx, &g, compactionOptions { axis, includeSizes: true, factor: 1.0, transition: true, ..Default::default() })?;
                }
                g.SyncNestedGeometry();
                let (tl, br) = g.BoundingBox();
                let (tl, br) = (*tl.expect("nil bounding box").b(), *br.expect("nil bounding box").b());
                let (width, height) = orientationFootprintSize(&g, &root);
                let maxSize = limits::MaxGraphSize as f64;
                if br.X - tl.X > maxSize || br.Y - tl.Y > maxSize || !(width >= 0.0 && height >= 0.0 && width <= maxSize && height <= maxSize) {
                    return Err(layoutgraph::ErrInvalidCandidate.err());
                }
                // Upright boxes can expand substantially during a quarter-turn. Allow
                // at most one additional original footprint, including the containing
                // shape, label room, and authored minimum dimensions.
                let rejected = width * height > 2.0 * beforeWidth * beforeHeight;
                if rejected {
                    return Err(layoutgraph::ErrNonImprovingCandidate.err());
                }
                let after = interiorFlow(&g, &local, &guard)?;
                let mut sign = 1.0;
                if direction == geo::Top {
                    sign = -1.0;
                }
                if sign * after.y <= sign * flow.y + 1e-9 {
                    return Err(layoutgraph::ErrNonImprovingCandidate.err());
                }
                guard.Finish()
            })();
            g.bm().CellSize = oldCell;
            result
        });
    }
    // Commit validates node clearances and restores positions, dimensions and
    // cluster metadata on rejection or error. Root fitting happens after success.
    if let Err(err) = txn.Commit(ctx) {
        if layoutgraph::IsCandidateRejection(Some(&err)) {
            return Ok(());
        }
        return Err(err);
    }
    Ok(())
}

// interiorFlow gives each distinct directed adjacency one vote, normalized by
// its Manhattan length so a single long edge cannot dominate the orientation.
#[derive(Clone, Copy, Default, Debug)]
pub(crate) struct containerFlow {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) across: f64,
    pub(crate) along: f64,
}

pub(crate) fn interiorFlow(g: &GraphRef, local: &Map<NodeRef, bool>, guard: &limits::WorkGuardRef) -> GoResult<containerFlow> {
    let mut flow = containerFlow::default();
    let mut seen: Set<(NodeRef, NodeRef)> = Set::default();
    let isLocal = |n: &NodeRef| local.get(n).copied().unwrap_or(false);
    for e in g.edges() {
        guard.Step()?;
        let (from, to, ok) = e.DirectedEndpoints();
        if !ok {
            continue;
        }
        let (Some(from), Some(to)) = (from, to) else {
            // Go: local[nil] is false
            continue;
        };
        if from == to || !isLocal(&from) || !isLocal(&to) {
            continue;
        }
        let key = (from.clone(), to.clone());
        if seen.contains(&key) {
            continue;
        }
        seen.insert(key);
        let (a, b) = (*from.Center().b(), *to.Center().b());
        let (dx, dy) = (b.X - a.X, b.Y - a.Y);
        let length = dx.abs() + dy.abs();
        if length == 0.0 {
            continue;
        }
        flow.x += dx / length;
        flow.across += dx.abs() / length;
        flow.y += dy / length;
        flow.along += dy.abs() / length;
    }
    guard.Finish()?;
    Ok(flow)
}

// Use the actual fitting path, including label room, shape geometry and authored
// minimum dimensions. Its only mutations are Width/Height; restore both before
// returning so optional admission does not resize the parent early.
pub(crate) fn orientationFitsContainer(g: &GraphRef, root: &NodeRef) -> bool {
    let (width, height) = orientationFootprintSize(g, root);
    let maxSize = limits::MaxGraphSize as f64;
    width >= 0.0 && height >= 0.0 && width <= maxSize && height <= maxSize
}

// Fitting mutates only the containing node's dimensions. Restore those values
// so observing the original or optional footprint never resizes the parent early.
pub(crate) fn orientationFootprintSize(g: &GraphRef, root: &NodeRef) -> (f64, f64) {
    let (oldWidth, oldHeight) = (root.w(), root.h());
    root.FitToGraph(g, g.ContainerPadding(Some(root), false));
    let (width, height) = (root.w(), root.h());
    root.set_w(oldWidth);
    root.set_h(oldHeight);
    (width, height)
}
