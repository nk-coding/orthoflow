// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/packing/binpack.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

pub(crate) const subgraphPadding: f64 = 20.0;
pub(crate) const subgraphSquareDampener: f64 = 0.5;

/// Pack recursively arranges disconnected subgraphs into compact containers.
/// Containers may shrink but never grow, and packing may interleave subgraph
/// bounding boxes while preserving edge lengths. Hierarchy subgraphs remain
/// separate to avoid obstructing routes.
pub fn Pack(ctx: &Ctx, g: &GraphRef, root: Option<&NodeRef>) -> GoResult<()> {
    let guard = newWorkGuard(ctx, limits::MaxBinPackWorkUnits)?;
    packAtomic(ctx, g, root, &guard)
}

pub(crate) fn packAtomic(ctx: &Ctx, g: &GraphRef, root: Option<&NodeRef>, guard: &WorkGuardRef) -> GoResult<()> {
    let ctx = layoutgraph::ContextWithTransactionWorkGuard(ctx, Some(guard));
    let state = layoutgraph::NewGraphStateSnapshot(layoutgraph::GraphStateSnapshotOptions { CaptureEdgeRoutes: true, ..Default::default() });
    state.UpdateWithWorkGuard(g, guard)?;
    // Go defers snapshot.Rollback() on error or panic.
    struct rollbackOnUnwind<'a> {
        g: &'a GraphRef,
        state: &'a layoutgraph::GraphStateRef,
        armed: bool,
    }
    impl Drop for rollbackOnUnwind<'_> {
        fn drop(&mut self) {
            if self.armed {
                layoutgraph::RestoreGraphState(self.g, self.state);
            }
        }
    }
    let mut rollback = rollbackOnUnwind { g, state: &state, armed: true };
    packGuarded(&ctx, g, root, guard)?;
    guard.Finish()?;
    rollback.armed = false;
    Ok(())
}

/// Go `originalPos[n].value` (zero Point when n is missing).
#[inline]
fn origValue(originalPos: &Map<NodeRef, pointerSnapshot>, n: &NodeRef) -> geo::Point {
    originalPos.get(n).map(|s| s.value).unwrap_or_default()
}

#[inline]
fn segment(start: geo::PointRef, end: geo::PointRef) -> geo::Segment {
    geo::Segment::new(start, end)
}

/// Collect the unique edges of the root descendants in `nodes` (in order).
fn descendantEdges(nodes: &[NodeRef], root: Option<&NodeRef>, guard: &WorkGuardRef, edges: &mut Vec<EdgeRef>, unique: &mut Set<EdgeRef>) -> GoResult<()> {
    for n in nodes {
        guard.Step()?;
        if Some(n) == root {
            continue;
        }
        let inside = binPackIsDescendantOf(Some(n), root, guard)?;
        if !inside {
            continue;
        }
        for e in n.edges() {
            guard.Step()?;
            if !unique.contains(&e) {
                unique.insert(e.clone());
                edges.push(e);
            }
        }
    }
    Ok(())
}

fn appendRoutedSegments(edges: &[EdgeRef], guard: &WorkGuardRef, packedSegments: &mut Vec<geo::Segment>) -> GoResult<()> {
    let hSegments = routedEdgeSegments(edges, true, guard)?;
    let vSegments = routedEdgeSegments(edges, false, guard)?;
    for s in hSegments {
        guard.Step()?;
        packedSegments.push(s);
    }
    for s in vSegments {
        guard.Step()?;
        packedSegments.push(s);
    }
    Ok(())
}

fn filterDescendants(ns: &[NodeRef], root: Option<&NodeRef>, guard: &WorkGuardRef) -> GoResult<Vec<NodeRef>> {
    let mut filtered: Vec<NodeRef> = Vec::new();
    for n in ns {
        guard.Step()?;
        if Some(n) == root {
            continue;
        }
        let inside = binPackIsDescendantOf(Some(n), root, guard)?;
        if inside {
            filtered.push(n.clone());
        }
    }
    Ok(filtered)
}

/// Translate every route point of the nodes' edges by the node's displacement
/// from its original position (each edge once).
fn moveEdgesWithNodes(nodes: &[NodeRef], originalPos: &Map<NodeRef, pointerSnapshot>, guard: &WorkGuardRef) -> GoResult<()> {
    let mut moved: Set<EdgeRef> = Set::default();
    for n in nodes {
        guard.Step()?;
        for e in n.edges() {
            guard.Step()?;
            if moved.contains(&e) {
                continue;
            }
            moved.insert(e.clone());
            let orig = origValue(originalPos, n);
            for p in e.points() {
                guard.Step()?;
                let tl = n.tlv();
                let mut pb = p.bm();
                pb.X += tl.X - orig.X;
                pb.Y += tl.Y - orig.Y;
            }
        }
    }
    Ok(())
}

pub(crate) fn packGuarded(ctx: &Ctx, g: &GraphRef, root: Option<&NodeRef>, guard: &WorkGuardRef) -> GoResult<()> {
    let checkCanceled = || guard.Step();
    checkCanceled()?;
    let (edgesPlaced, graphIncidentEdges) = allEdgesHaveCompleteRoutesGuarded(g, root, guard)?;

    let containers = g.b().Containers.clone();
    let rootKey: Option<NodeRef> = root.cloned();
    let rootChildren = containers.at(&rootKey);

    // Pack more nested before less nested
    for n in &rootChildren {
        checkCanceled()?;
        if n.IsContainer() {
            packGuarded(ctx, g, Some(n), guard)?;
        }
    }
    let rootChildren = containers.at(&rootKey);
    // For this container level, split subgraphs into 3 categories:
    // 0. Fixed nodes and their subgraphs cannot move
    // 1. Cross-container subgraphs (have edges to another container) cannot move
    // 2. Contained subgraphs can move
    let mut crossContainerSubgraphs: Vec<Vec<NodeRef>> = Vec::new();
    let mut containedSubgraphs: Vec<Vec<NodeRef>> = Vec::new();
    let mut added: Set<NodeRef> = Set::default();

    let mut ignore: Set<NodeRef> = Set::default();
    if let Some(r) = root {
        ignore.insert(r.clone());
    }

    // we create the fixedNodesSubgraph first to ensure no subgraph with a fixed node is packed
    let mut fixedNodesSubgraph: Vec<NodeRef> = Vec::new();
    for n in &rootChildren {
        checkCanceled()?;
        if n.fixedTL().is_none() {
            continue;
        }

        let reachable = n.AllReachableNodesContext(true, true, false, Some(&ignore), guard)?;
        for rn in reachable {
            guard.Step()?;
            if !added.contains(&rn) {
                fixedNodesSubgraph.push(rn.clone());
            }
            added.insert(rn);
        }
    }

    let visitErr: std::cell::RefCell<Option<GoError>> = std::cell::RefCell::new(None);
    let shouldVisitRoot = |n: &NodeRef| -> bool {
        if visitErr.borrow().is_some() {
            return false;
        }
        match binPackIsDescendantOf(Some(n), root, guard) {
            Ok(inside) => inside,
            Err(e) => {
                *visitErr.borrow_mut() = Some(e);
                false
            }
        }
    };
    let shouldVisitAll = |_: &NodeRef| -> bool { true };
    let shouldVisit: &dyn Fn(&NodeRef) -> bool = if root.is_some() { &shouldVisitRoot } else { &shouldVisitAll };

    // 1. get subgraphs contained in root
    // 2. get subgraphs not contained in root
    // 3. filter out subgraphs contained in root from subgraphs not contained in root
    let mut contained: Set<NodeRef> = Set::default();
    for n in &rootChildren {
        checkCanceled()?;
        if added.contains(n) {
            continue;
        }

        let reachable = n.ReachableNodesContext(shouldVisit, true, true, false, Some(&ignore), guard)?;
        if let Some(e) = visitErr.borrow_mut().take() {
            return Err(e);
        }
        let mut cross = false;
        for rn in &reachable {
            guard.Step()?;
            if rn.b().Hierarchy.is_some() {
                cross = true;
                break;
            }
            let hasExternalEdge = binPackHasExternalConnection(g, rn, root, false, edgesPlaced, guard)?;
            if hasExternalEdge {
                cross = true;
                break;
            }
            let hasExternalNear = binPackHasExternalConnection(g, rn, root, true, false, guard)?;
            if hasExternalNear {
                cross = true;
                break;
            }
        }
        if !cross {
            for rn in &reachable {
                guard.Step()?;
                added.insert(rn.clone());
                contained.insert(rn.clone());
            }
            containedSubgraphs.push(reachable);
        }
    }
    for n in &rootChildren {
        checkCanceled()?;
        if added.contains(n) {
            continue;
        }
        let mut filtered: Vec<NodeRef> = Vec::new();
        let allReachable = n.AllReachableNodesContext(true, false, false, Some(&ignore), guard)?;
        for rn in allReachable {
            guard.Step()?;
            if contained.contains(&rn) {
                continue;
            }
            filtered.push(rn);
        }

        for rn in &filtered {
            guard.Step()?;
            added.insert(rn.clone());
        }
        crossContainerSubgraphs.push(filtered);
    }

    if containedSubgraphs.is_empty() {
        checkCanceled()?;
        return Ok(());
    }

    let mut packed: Vec<Vec<NodeRef>> = crossContainerSubgraphs;
    let mut toPack: std::collections::VecDeque<Vec<NodeRef>> = containedSubgraphs.into();

    if !fixedNodesSubgraph.is_empty() {
        packed.push(fixedNodesSubgraph);
    }

    let rootTL = binPackContainerTopLeft(g, root, &toPack[0], guard)?;

    let originalScore = binPackScoreGuarded(&containers.at(&rootKey), root, guard)?;
    let mut originalPos: Map<NodeRef, pointerSnapshot> = Map::default();
    let mut originalPosOrder: Vec<NodeRef> = Vec::new();
    let mut originalRoute: Map<EdgeRef, Vec<geo::Point>> = Map::default();
    let mut originalRouteOrder: Vec<EdgeRef> = Vec::new();
    let returnIfCanceled = || checkCanceled();

    // Move out of the way
    for ns in toPack.iter() {
        guard.Step()?;
        for n in ns {
            guard.Step()?;
            if !originalPos.contains_key(n) {
                originalPos.insert(n.clone(), snapshotPointer(n.tl().as_ref()));
                originalPosOrder.push(n.clone());
            }
            n.Translate(100000.0, 100000.0);
            if edgesPlaced {
                for e in n.edges() {
                    guard.Step()?;
                    if originalRoute.contains_key(&e) {
                        continue;
                    }
                    let mut route: Vec<geo::Point> = Vec::new();
                    for p in e.points() {
                        guard.Step()?;
                        route.push(*p.b());
                    }
                    originalRoute.insert(e.clone(), route);
                    originalRouteOrder.push(e);
                }
            }
        }
    }

    if packed.is_empty() {
        let (currTL, _) = graphbounds::BoundingBox(&toPack[0], guard)?;
        let currTL = *currTL.b();
        let offsetX = rootTL.get().X - currTL.X;
        let offsetY = rootTL.get().Y - currTL.Y;
        for n in &toPack[0] {
            guard.Step()?;
            n.Translate(offsetX, offsetY);
        }
        if edgesPlaced {
            moveEdgesWithNodes(&toPack[0], &originalPos, guard)?;
        }
        let first = toPack.pop_front().unwrap();
        packed = vec![first];
    }
    returnIfCanceled()?;

    let mut packedRootChildren: Vec<NodeRef> = Vec::new();
    let mut packedSegments: Vec<geo::Segment> = Vec::new();
    let toPackSlice = |tp: &std::collections::VecDeque<Vec<NodeRef>>| -> Vec<Vec<NodeRef>> { tp.iter().cloned().collect() };
    // If edges are already placed, use their actual routes
    // Otherwise consider multiple possible good routes
    if edgesPlaced {
        let mut edges: Vec<EdgeRef> = Vec::new();
        let mut unique: Set<EdgeRef> = Set::default();
        for ns in &packed {
            guard.Step()?;
            descendantEdges(ns, root, guard, &mut edges, &mut unique)?;
        }
        appendRoutedSegments(&edges, guard, &mut packedSegments)?;
        for ns in &packed {
            guard.Step()?;
            for n in ns {
                guard.Step()?;
                if n.containerField().as_ref() == root {
                    packedRootChildren.push(n.clone());
                }
            }
        }
    } else {
        for ns in &packed {
            guard.Step()?;
            for n in ns {
                guard.Step()?;
                if n.containerField().as_ref() == root {
                    packedRootChildren.push(n.clone());
                }
            }
            let (xd, yd) = binPackSmallestDeltas(&toPackSlice(&toPack), guard)?;

            // We only care about segments of packed nodes that are descendants of root
            let filtered = filterDescendants(ns, root, guard)?;
            let segments = estimateRouteSegments(&filtered, g, xd, yd, guard)?;
            for segment in segments {
                guard.Step()?;
                packedSegments.push(segment);
            }
        }
    }

    let mut occupied: Set<geo::Point> = Set::default();

    let mut canMoveLeft = true;
    let mut canMoveRight = true;
    let mut canMoveTop = true;
    let mut canMoveBottom = true;

    if let (true, Some(root)) = (edgesPlaced, root) {
        for e in root.edges() {
            guard.Step()?;
            let pts = e.points();
            let (start, end) = if e.from() == *root { (pts[0].get(), pts[1].get()) } else { (pts[pts.len() - 1].get(), pts[pts.len() - 2].get()) };
            if start.Y == end.Y {
                if start.X < end.X {
                    canMoveRight = false;
                }
                if start.X > end.X {
                    canMoveLeft = false;
                }
            } else if start.X == end.X {
                if start.Y < end.Y {
                    canMoveBottom = false;
                }
                if start.Y > end.Y {
                    canMoveTop = false;
                }
            } else {
                // It's a diagonal line, can't risk any movement
                canMoveRight = false;
                canMoveLeft = false;
                canMoveTop = false;
                canMoveBottom = false;
                break;
            }
        }
    }

    let mut failed = false;
    // place greedily
    while !toPack.is_empty() {
        returnIfCanceled()?;
        let mut candidates: Vec<geo::Point> = Vec::new();
        let mut set: Set<geo::Point> = Set::default();
        let hierarchyBoxes = binPackHierarchyBoxes(&packed, guard)?;

        let tl = rootTL.get();
        if !set.contains(&tl) {
            candidates.push(tl);
            set.insert(tl);
        }

        let mut packedTLX = f64::INFINITY;
        let mut packedTLY = f64::INFINITY;
        for ns in &packed {
            guard.Step()?;
            let (tl, _) = graphbounds::BoundingBox(ns, guard)?;
            let tl = *tl.b();
            packedTLX = math::Min(packedTLX, tl.X);
            packedTLY = math::Min(packedTLY, tl.Y);
            let canPlaceWithin = ns[0].b().Hierarchy.is_none();
            let placementCandidates = placementCandidatesGuarded(ns, root, canPlaceWithin, edgesPlaced, guard)?;
            for p in placementCandidates {
                guard.Step()?;
                if occupied.contains(&p) {
                    continue;
                }
                let insideHierarchy = binPackPointInHierarchy(&hierarchyBoxes, &p, guard)?;
                if insideHierarchy {
                    continue;
                }
                if !set.contains(&p) {
                    candidates.push(p);
                    set.insert(p);
                }
            }
        }

        let currPacking = toPack.pop_front().unwrap();
        let (currPackingTL, currPackingBR) = graphbounds::FixedBoundingBox(&currPacking, guard)?;
        let (currPackingTL, currPackingBR) = (*currPackingTL.b(), *currPackingBR.b());
        let currPackingWidth = currPackingBR.X - currPackingTL.X;
        let currPackingHeight = currPackingBR.Y - currPackingTL.Y;

        if canMoveTop {
            candidates.push(geo::Point { X: packedTLX, Y: packedTLY - currPackingHeight - subgraphPadding });
        }
        if canMoveLeft {
            candidates.push(geo::Point { X: packedTLX - currPackingWidth - subgraphPadding, Y: packedTLY });
        }
        if canMoveTop && canMoveLeft {
            candidates.push(geo::Point { X: packedTLX - currPackingWidth - subgraphPadding, Y: packedTLY - currPackingHeight - subgraphPadding });
        }

        let txn = g.NewRequestTransaction(
            ctx,
            layoutgraph::TransactionOptions { IgnoreContainerEscape: true, AffectEdgeRoutes: edgesPlaced, ..Default::default() },
        )?;

        let (currTL, _) = graphbounds::BoundingBox(&currPacking, guard)?;
        let currTL = *currTL.b();
        let mut packedWithCurr: Vec<NodeRef> = Vec::with_capacity(packedRootChildren.len() + currPacking.len());
        for node in &packedRootChildren {
            guard.Step()?;
            packedWithCurr.push(node.clone());
        }
        for node in &currPacking {
            guard.Step()?;
            packedWithCurr.push(node.clone());
        }
        let mut bestScore = f64::INFINITY;

        for n in &currPacking {
            guard.Step()?;
            let og = origValue(&originalPos, n);
            if !set.contains(&og) {
                candidates.push(og);
                set.insert(og);
            }
        }
        let (packedXD, packedYD) = binPackSmallestDeltas(&packed, guard)?;

        let mut bestCandidate: Option<geo::Point> = None;
        for p in &candidates {
            returnIfCanceled()?;
            txn.Clear();
            {
                let p = *p;
                let currPacking = currPacking.clone();
                let guard = guard.clone();
                txn.AddOp(move || -> GoResult<()> {
                    let offsetX = p.X - currTL.X;
                    let offsetY = p.Y - currTL.Y;
                    for n in &currPacking {
                        guard.Step()?;
                        n.Translate(offsetX, offsetY);
                    }
                    Ok(())
                });
            }
            if let Err(err) = txn.Commit(ctx) {
                if layoutgraph::IsCandidateRejection(Some(&err)) {
                    continue;
                }
                checkCanceled()?;
                return Err(err);
            }
            // Not allowed to expand out of containers
            if let Some(rootNode) = root {
                let mut packedInContainer: Vec<NodeRef> = Vec::new();
                for n in &packedWithCurr {
                    if let Err(e) = guard.Step() {
                        txn.Rollback();
                        return Err(e);
                    }
                    if n.containerField().as_ref() == root {
                        packedInContainer.push(n.clone());
                    }
                }
                let (tl, br) = match graphbounds::BoundingBox(&packedInContainer, guard) {
                    Ok(v) => v,
                    Err(e) => {
                        txn.Rollback();
                        return Err(e);
                    }
                };
                let (tl, br) = (*tl.b(), *br.b());
                let (w, h) = (br.X - tl.X, br.Y - tl.Y);
                let innerBox = rootNode.InnerBox();
                if w > innerBox.Width || h > innerBox.Height {
                    txn.Rollback();
                    continue;
                }
            }
            if !edgesPlaced {
                // Not allowed for any node to go into the bounding box of a placed hierarchy
                let mut is = false;
                for n in &currPacking {
                    let (tl, br) = match graphbounds::NodeBoundingBox(Some(n), None, guard) {
                        Ok(v) => v,
                        Err(e) => {
                            txn.Rollback();
                            return Err(e);
                        }
                    };
                    let topLeftInside = match binPackPointInHierarchy(&hierarchyBoxes, &tl.get(), guard) {
                        Ok(v) => v,
                        Err(e) => {
                            txn.Rollback();
                            return Err(e);
                        }
                    };
                    let bottomRightInside = match binPackPointInHierarchy(&hierarchyBoxes, &br.get(), guard) {
                        Ok(v) => v,
                        Err(e) => {
                            txn.Rollback();
                            return Err(e);
                        }
                    };
                    if topLeftInside || bottomRightInside {
                        is = true;
                        break;
                    }
                }
                if is {
                    txn.Rollback();
                    continue;
                }
            }
            let blocked = match blocksRoutesWithDeltasGuarded(g, &currPacking, &packed, &packedSegments, packedXD, packedYD, guard) {
                Ok(v) => v,
                Err(e) => {
                    txn.Rollback();
                    return Err(e);
                }
            };
            if blocked {
                txn.Rollback();
                continue;
            }
            let score = match binPackScoreGuarded(&packedWithCurr, root, guard) {
                Ok(v) => v,
                Err(e) => {
                    txn.Rollback();
                    return Err(e);
                }
            };
            if score < bestScore {
                bestScore = score;
                bestCandidate = Some(*p);
            }
            txn.Rollback();
        }
        returnIfCanceled()?;

        let Some(bestCandidate) = bestCandidate else {
            failed = true;
            break;
        };

        txn.Clear();
        {
            let currPacking = currPacking.clone();
            let guard = guard.clone();
            let origs: Map<NodeRef, pointerSnapshot> = currPacking.iter().filter_map(|n| originalPos.get(n).map(|s| (n.clone(), s.clone()))).collect();
            txn.AddOp(move || -> GoResult<()> {
                let offsetX = bestCandidate.X - currTL.X;
                let offsetY = bestCandidate.Y - currTL.Y;
                for n in &currPacking {
                    guard.Step()?;
                    n.Translate(offsetX, offsetY);
                }
                if edgesPlaced {
                    moveEdgesWithNodes(&currPacking, &origs, &guard)?;
                }
                Ok(())
            });
        }
        if let Err(err) = txn.Commit(ctx) {
            checkCanceled()?;
            return Err(err);
        }
        occupied.insert(bestCandidate);

        for node in &currPacking {
            guard.Step()?;
            packedRootChildren.push(node.clone());
        }
        packed.push(currPacking.clone());
        let (xd, yd) = binPackSmallestDeltas(&toPackSlice(&toPack), guard)?;

        if edgesPlaced {
            let mut edges: Vec<EdgeRef> = Vec::new();
            let mut unique: Set<EdgeRef> = Set::default();
            descendantEdges(&currPacking, root, guard, &mut edges, &mut unique)?;
            appendRoutedSegments(&edges, guard, &mut packedSegments)?;
        } else {
            let filtered = filterDescendants(&currPacking, root, guard)?;
            let segments = estimateRouteSegments(&filtered, g, xd, yd, guard)?;
            for segment in segments {
                guard.Step()?;
                packedSegments.push(segment);
            }
        }
    }
    returnIfCanceled()?;

    let afterBinPackScore = if failed { f64::INFINITY } else { binPackScoreGuarded(&containers.at(&rootKey), root, guard)? };
    if originalScore < afterBinPackScore {
        for node in &originalPosOrder {
            guard.Step()?;
            let restored = originalPos[node].restore();
            node.set_tl(restored);
        }
        binPackSyncClusters(g, guard)?;
        binPackSyncSequences(g, guard)?;
        failed = true;
    } else if let Some(root) = root.filter(|r| r.clusterField().is_none()) {
        // record state to revert to if fitting to the children's packed position causes a bad state
        // we need to save positions of packed children not in toPack
        let descendants = g.AllDescendantNodesWithWorkGuard(Some(root), true, guard)?;
        for node in &descendants {
            guard.Step()?;
            if !originalPos.contains_key(node) {
                originalPos.insert(node.clone(), snapshotPointer(node.tl().as_ref()));
                originalPosOrder.push(node.clone());
            }
        }

        let rootX = root.x();
        let rootY = root.y();
        let (rootWidth, rootHeight) = (root.w(), root.h());
        let originalRootBox = geo::Box { TopLeft: Some(geo::NewPoint(rootX, rootY)), Width: rootWidth, Height: rootHeight };

        binPackWrapChildren(root, guard)?;
        let mut routedContainerHandled = false;
        if edgesPlaced {
            let decision = binPackCanUseRoutedContainerBox(g, root, &originalRootBox, &graphIncidentEdges, guard)?;
            match decision {
                routedContainerDeferToSideConstraints => {}
                routedContainerUseProposedBox => {
                    routedContainerHandled = true;
                }
                routedContainerKeepOriginalBox => {
                    routedContainerHandled = true;
                    // Unsupported shapes, detached endpoints, and route geometry cut
                    // by the proposed shrink retain the exact routed box.
                    root.set_x(rootX);
                    root.set_y(rootY);
                    root.set_w(rootWidth);
                    root.set_h(rootHeight);
                }
            }
        }
        if !routedContainerHandled {
            if !canMoveLeft {
                root.set_x(rootX);
            }
            if !canMoveTop {
                root.set_y(rootY);
            }
            if !canMoveRight {
                root.set_w(rootWidth);
            }
            if !canMoveBottom {
                root.set_h(rootHeight);
            }
            // wrapChildren preserves the aspect ratio of circles and other square
            // shapes. Restoring one route-constrained side independently can break
            // that invariant, so retain the larger constrained dimension on both
            // axes.
            if root.shape().AspectRatio1() {
                let size = math::Max(root.w(), root.h());
                root.set_w(size);
                root.set_h(size);
            }
        }

        let mut childrenFit = true;
        let innerBox = root.InnerBox();
        for child in containers.at(&Some(root.clone())) {
            guard.Step()?;
            if !layoutgraph::Covers(&innerBox, &child.boxValue()) {
                childrenFit = false;
                break;
            }
        }
        let badState = g.IsBadStateWithWorkGuard(Some(root), None, false, guard)?;
        if !childrenFit || badState {
            for node in &originalPosOrder {
                guard.Step()?;
                let restored = originalPos[node].restore();
                node.set_tl(restored);
            }
            root.set_x(rootX);
            root.set_y(rootY);
            root.set_w(rootWidth);
            root.set_h(rootHeight);
            failed = true;
        }
    }
    // Transactions don't deal with edge routes, so rollback manually
    if failed {
        for edge in &originalRouteOrder {
            guard.Step()?;
            let route = &originalRoute[edge];
            let points = edge.points();
            for (i, p) in route.iter().enumerate() {
                guard.Step()?;
                let mut pb = points[i].bm();
                pb.X = p.X;
                pb.Y = p.Y;
            }
        }
    }
    guard.Finish()
}

/// placementCandidates returns the candidate points in the graph that are suitable to pack more nodes/subgraphs at
/// if considerWithin is true, candidate points include points inside the bounding box of the graph
pub(crate) fn placementCandidatesGuarded(nodes: &[NodeRef], root: Option<&NodeRef>, considerWithin: bool, edgesPlaced: bool, guard: &WorkGuardRef) -> GoResult<Vec<geo::Point>> {
    let mut candidatePoints: Vec<geo::Point> = Vec::new();
    let mut inContainer: Vec<NodeRef> = Vec::new();
    let p = |x: f64, y: f64| geo::Point { X: x, Y: y };
    for n in nodes {
        guard.Step()?;
        // Avoid placement points that'd move into different container
        if n.containerField().as_ref() != root {
            continue;
        }
        inContainer.push(n.clone());
        if !considerWithin {
            continue;
        }
        let mut sideDelta = subgraphPadding;
        if n.IsTable() {
            sideDelta = layoutgraph::TableNodeGap as f64;
        }
        let tl = n.tlv();
        let (w, h) = (n.w(), n.h());
        if edgesPlaced {
            candidatePoints.push(p(tl.X + w + sideDelta, tl.Y));
            candidatePoints.push(p(tl.X, tl.Y + h + subgraphPadding));
            candidatePoints.push(p(tl.X + w + sideDelta, tl.Y + h));
        } else {
            // Avoid placement points that are likely to cause obstructions
            let mut hasRight = false;
            let mut hasBottom = false;
            let mut hasBottomRight = false;
            for e in n.edges() {
                guard.Step()?;
                let adj = n.Adjacent(&e);
                match n.Orientation(&adj) {
                    geo::Left => hasRight = true,
                    geo::Top => hasBottom = true,
                    geo::TopLeft => hasBottomRight = true,
                    _ => {}
                }
                // Go's `break` here only leaves the switch.
            }

            if !hasRight {
                candidatePoints.push(p(tl.X + w + sideDelta, tl.Y));
            }
            if !hasBottom {
                candidatePoints.push(p(tl.X, tl.Y + h + subgraphPadding));
            }
            if !hasBottomRight {
                candidatePoints.push(p(tl.X + w + sideDelta, tl.Y + h));
            }
        }
    }

    let (graphTL, graphBR) = graphbounds::FixedBoundingBox(&inContainer, guard)?;
    let (graphTL, graphBR) = (*graphTL.b(), *graphBR.b());

    candidatePoints.push(p(graphBR.X, graphTL.Y));
    candidatePoints.push(p(graphBR.X, graphBR.Y));
    candidatePoints.push(p(graphTL.X, graphBR.Y));
    candidatePoints.push(p(graphBR.X + subgraphPadding, graphTL.Y));
    candidatePoints.push(p(graphBR.X + subgraphPadding, graphBR.Y));
    candidatePoints.push(p(graphTL.X, graphBR.Y + subgraphPadding));
    guard.Finish()?;
    Ok(candidatePoints)
}

/// blocksRoutesWithDeltasGuarded checks if the given `packing` placement overlaps with some desired routes in `packed`
/// or if `packed` nodes overlap with routes in `packing`.
pub(crate) fn blocksRoutesWithDeltasGuarded(
    g: &GraphRef,
    packing: &[NodeRef],
    packed: &[Vec<NodeRef>],
    packedSegments: &[geo::Segment],
    xd: f64,
    yd: f64,
    guard: &WorkGuardRef,
) -> GoResult<bool> {
    let packingSegments = estimateRouteSegments(packing, g, xd, yd, guard)?;
    let packingSegments: Vec<(geo::Point, geo::Point)> = packingSegments.iter().map(|s| (s.Start.get(), s.End.get())).collect();
    let nodeGap = layoutgraph::NodeGap as f64;

    // check if `packed` nodes could block routes routes in `packing`
    for nodes in packed {
        guard.Step()?;
        for n in nodes {
            guard.Step()?;
            for (start, end) in &packingSegments {
                guard.Step()?;
                if n.OverlapsLine(start, end, nodeGap) {
                    guard.Finish()?;
                    return Ok(true);
                }
            }
        }
    }
    // check if `packing` nodes could block routes in `packed`
    for n in packing {
        guard.Step()?;
        for seg in packedSegments {
            guard.Step()?;
            if n.OverlapsLine(&seg.Start.get(), &seg.End.get(), nodeGap) {
                guard.Finish()?;
                return Ok(true);
            }
        }
    }

    guard.Finish()?;
    Ok(false)
}

/// estimateRouteSegments generates all L-shaped and S-shaped estimates between
/// connected nodes, sharing the complete BinPack operation's guard.
pub(crate) fn estimateRouteSegments(nodes: &[NodeRef], g: &GraphRef, smallestXGap: f64, smallestYGap: f64, guard: &WorkGuardRef) -> GoResult<Vec<geo::Segment>> {
    let mut segments: Vec<geo::Segment> = Vec::with_capacity(nodes.len() * 6);
    let mut unique: Set<EdgeRef> = Set::default();
    for n in nodes {
        guard.Step()?;
        for e in n.edges() {
            guard.Step()?;
            if unique.contains(&e) {
                continue;
            }
            unique.insert(e.clone());
            let edgeSegments = estimateEdgeSegments(&e, g, smallestXGap, smallestYGap, guard)?;
            segments.extend(edgeSegments);
        }
    }
    guard.Finish()?;
    Ok(segments)
}

pub(crate) fn routedEdgeSegments(edges: &[EdgeRef], isHorizontal: bool, guard: &WorkGuardRef) -> GoResult<Vec<geo::Segment>> {
    let mut out: Vec<geo::Segment> = Vec::new();
    for edge in edges {
        guard.Step()?;
        let points = edge.points();
        let mut index = 0usize;
        while index + 1 < points.len() {
            guard.Step()?;
            let (first, second) = (&points[index], &points[index + 1]);
            let (f, s) = (first.get(), second.get());
            let mut se: Option<(geo::PointRef, geo::PointRef)> = None;
            if isHorizontal {
                if f.Y == s.Y {
                    if f.X < s.X {
                        se = Some((first.clone(), second.clone()));
                    } else {
                        se = Some((second.clone(), first.clone()));
                    }
                }
            } else if f.X == s.X {
                if f.Y < s.Y {
                    se = Some((first.clone(), second.clone()));
                } else {
                    se = Some((second.clone(), first.clone()));
                }
            }
            if let Some((start, end)) = se {
                // GO-ALIAS: the segment shares the edge's route points.
                out.push(segment(start, end));
            }
            index += 1;
        }
    }
    guard.Finish()?;
    Ok(out)
}

/// estimateEdgeSegments generates horizontal and vertical segments for the given edge.
/// It only generates the segments for diagonal edges, as these are the ones that can have S/L-shaped routes.
/// There's an exception for cluster vessels, in which is is allowed to be vertical or horizontal.
pub(crate) fn estimateEdgeSegments(e: &EdgeRef, g: &GraphRef, smallestXGap: f64, smallestYGap: f64, guard: &WorkGuardRef) -> GoResult<Vec<geo::Segment>> {
    guard.Step()?;
    let (eFrom, eTo) = (e.from(), e.to());
    if !eFrom.has_tl() || !eTo.has_tl() {
        return Err(invariant::New("BinPack segment estimate encountered an incomplete edge"));
    }
    let o = eFrom.Orientation(&eTo);

    // Straight lines only matter if they're long (can potentially fit other subgraphs)
    if !o.IsDiagonal() && !eFrom.IsClusterVessel() && !eTo.IsClusterVessel() {
        let from = eFrom.Center();
        let to = eTo.Center();
        let (f, t) = (from.get(), to.get());

        if o.IsHorizontal() {
            if (f.X - t.X).abs() > smallestXGap {
                return Ok(vec![segment(from, to)]);
            }
        }

        if o.IsVertical() {
            if (f.Y - t.Y).abs() > smallestYGap {
                return Ok(vec![segment(from, to)]);
            }
        }

        return Ok(Vec::new());
    }

    // for clusters, as they can be properly aligned vertically or horizontally
    // generate the segments using the endpoints depending on the arrangement
    let clusterEndpoints = |vessel: &NodeRef| -> Vec<geo::PointRef> {
        let c = g.b().Clusters.get(vessel).expect("nil pointer dereference: cluster");
        let (vessel, arrangement) = {
            let cb = c.b();
            (cb.Vessel.clone(), cb.Arrangement.clone())
        };
        let center = vessel.Center().get();
        let (tl, br) = vessel.BoundingBox(None);
        let (tl, br) = (tl.get(), br.get());
        if arrangement == layoutgraph::Row {
            return vec![geo::NewPoint(tl.X, center.Y), geo::NewPoint(br.X, center.Y)];
        }
        if arrangement == layoutgraph::Column {
            return vec![geo::NewPoint(center.X, tl.Y), geo::NewPoint(center.X, br.Y)];
        }
        Vec::new()
    };
    // for non-cluster nodes, it generates 10 segments
    let mut segments: Vec<geo::Segment> = Vec::with_capacity(6);
    let fromPoints = if eFrom.IsClusterVessel() { clusterEndpoints(&eFrom) } else { vec![eFrom.Center()] };
    let toPoints = if eTo.IsClusterVessel() { clusterEndpoints(&eTo) } else { vec![eTo.Center()] };

    for from in &fromPoints {
        for to in &toPoints {
            guard.Step()?;
            let (f, t) = (from.get(), to.get());
            // L-shaped estimate (segments 1-4 in the method doc above)
            segments.push(segment(from.clone(), geo::NewPoint(f.X, t.Y)));
            segments.push(segment(geo::NewPoint(f.X, t.Y), to.clone()));
            segments.push(segment(from.clone(), geo::NewPoint(t.X, f.Y)));
            segments.push(segment(geo::NewPoint(t.X, f.Y), to.clone()));
            // S-shaped estimate (segments 5-10 in the method doc above)
            let midX = math::Round((f.X + t.X) / 2.0);
            segments.push(segment(geo::NewPoint(midX, f.Y), geo::NewPoint(midX, t.Y)));
            let midY = math::Round((f.Y + t.Y) / 2.0);
            segments.push(segment(geo::NewPoint(f.X, midY), geo::NewPoint(t.X, midY)));
        }
    }

    guard.Finish()?;
    Ok(segments)
}
