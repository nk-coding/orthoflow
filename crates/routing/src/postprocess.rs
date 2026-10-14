// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/routing/postprocess.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of routing/postprocess.go.
#[allow(unused_imports)]
use crate::*;
use gocompat::context::Ctx;
use gocompat::math;
use gocompat::{GoError, GoResult, Map, Set};
use layoutgraph::{ClusterRef, EdgeRef, EdgeSegmentRef, GraphRef};

pub fn BalanceEdgeSegments(ctx: &Ctx, g: &GraphRef) -> GoResult<()> {
    balanceEdgeSegmentsWithLimit(ctx, g, maxRouteStageWorkUnits)
}

pub(crate) fn balanceEdgeSegmentsWithLimit(ctx: &Ctx, g: &GraphRef, workLimit: u64) -> GoResult<()> {
    runAtomicRouteStage(ctx, "BalanceEdgeSegments", g, &[], workLimit, |guard| balanceEdgeSegmentsGuarded(g, guard))
}

pub(crate) fn balanceEdgeSegmentsGuarded(g: &GraphRef, guard: &routeWorkGuardRef) -> GoResult<()> {
    // Special nodes get special routing, don't mess with them
    let mut specialEdges: Vec<EdgeRef> = Vec::new();
    let mut regularEdges: Vec<EdgeRef> = Vec::new();
    for e in g.edges() {
        guard.step()?;
        if e.b().Points.len() < 2 {
            return Err(GoError::new(format!(
                "TALA BalanceEdgeSegments edge {} has an incomplete route",
                e.IDValue()
            )));
        }
        if isSpecialEdgeForBalancing(g, &e) {
            specialEdges.push(e);
            continue;
        }
        regularEdges.push(e);
    }

    balanceRegularEdgesGuarded(g, &specialEdges, &regularEdges, guard)?;
    balancePortInteriorsGuarded(g, guard)
}

// isSpecialEdgeForBalancing identifies routes whose ports or rule-generated
// geometry balancing must leave unchanged.
pub(crate) fn isSpecialEdgeForBalancing(g: &GraphRef, e: &EdgeRef) -> bool {
    {
        let nodeToTree = g.b().NodeToTree.clone();
        if nodeToTree.has(&e.from()) {
            return true;
        }
        if nodeToTree.has(&e.to()) {
            return true;
        }
    }
    if e.HasTableColumn() || e.IsLoop() {
        return true;
    }
    // We want edges locked to diamond ports strictly.
    if e.from().shape().GetType() == shape::DIAMOND_TYPE || e.to().shape().GetType() == shape::DIAMOND_TYPE {
        return true;
    }
    // When node A (width even) aligns with node B (width odd), the centers can
    // end up misaligned by 1.
    let points = e.points();
    let first = points[0].get();
    let last = points[points.len() - 1].get();
    (first.X - last.X).abs() == 1.0 || (first.Y - last.Y).abs() == 1.0
}

/// Go map key `layoutgraph.EdgeSegment` (value): the segment's point pointers
/// and its owner pointer.
type edgeSegmentKey = (usize, usize, usize);

fn segmentKey(s: &EdgeSegmentRef) -> edgeSegmentKey {
    let s = s.b();
    (s.Segment.Start.addr(), s.Segment.End.addr(), s.Owner().addr())
}

/// Go map-key equality on `Range` (a struct of two float64 fields).
fn rangeKey(r: &Range) -> (u64, u64) {
    let k = |f: f64| if f == 0.0 { 0u64 } else { f.to_bits() };
    (k(r.start), k(r.end))
}

fn segStart(s: &EdgeSegmentRef) -> geo::Point {
    s.b().Segment.Start.get()
}

pub(crate) fn balanceRegularEdgesGuarded(
    g: &GraphRef,
    specialEdges: &[EdgeRef],
    regularEdges: &[EdgeRef],
    guard: &routeWorkGuardRef,
) -> GoResult<()> {
    // Row and diamond ports fix the endpoint and its short approach segment,
    // while longer interior corridors can still be balanced.
    let mut fixedPortPoints: Set<geo::PointRef> = Set::default();
    for edge in regularEdges {
        guard.step()?;
        if !hasFixedBalancingPorts(edge) {
            continue;
        }
        let points = edge.points();
        for point in &points[..points.len().min(2)] {
            fixedPortPoints.insert(point.clone());
        }
        let from = if points.len() >= 2 { points.len() - 2 } else { 0 };
        for point in &points[from..] {
            fixedPortPoints.insert(point.clone());
        }
    }
    let nodes = g.nodes();
    let isConnectedToUnitNode = |point: &geo::PointRef| -> GoResult<bool> {
        let p = point.get();
        for node in &nodes {
            guard.step()?;
            if node.w() == 1.0 && node.h() == 1.0 {
                let tl = node.tlv();
                // Check if point is on the node's border
                if (p.X == tl.X || p.X == tl.X + 1.0) && (p.Y == tl.Y || p.Y == tl.Y + 1.0) {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    };

    let getSharedCluster = |edgeA: &EdgeRef, edgeB: &EdgeRef| -> Option<ClusterRef> {
        let fromA = edgeA.from().clusterField();
        let toA = edgeA.to().clusterField();

        let fromB = edgeB.from().clusterField();
        let toB = edgeB.to().clusterField();
        if let Some(fa) = &fromA {
            if Some(fa) == fromB.as_ref() || Some(fa) == toB.as_ref() {
                return fromA;
            }
        }
        if let Some(ta) = &toA {
            if Some(ta) == fromB.as_ref() || Some(ta) == toB.as_ref() {
                return toA;
            }
        }
        None
    };

    let eq = |a: f64, b: f64| (a - b).abs() <= 1.0;

    for isHorizontal in [true, false] {
        let mut lockedSegments = nodeSegmentsGuarded(&nodes, !isHorizontal, guard)?;
        let mut lockedEdgeSegments: Map<edgeSegmentKey, bool> = Map::default();
        // When we balance vertical segments horizontally, we get the vertical segments
        let allEdgeSegments = edgeSegmentsGuarded(regularEdges, !isHorizontal, guard)?;

        let mut edgeSegments: Vec<EdgeSegmentRef> = Vec::with_capacity(allEdgeSegments.len());
        let mut fixedSegments: Vec<geo::Segment> = Vec::new();
        for segment in &allEdgeSegments {
            let (start, end) = {
                let s = segment.b();
                (s.Segment.Start.clone(), s.Segment.End.clone())
            };
            if fixedPortPoints.contains(&start) || fixedPortPoints.contains(&end) {
                fixedSegments.push(segment.b().Segment.clone());
                continue;
            }
            let startConnected = isConnectedToUnitNode(&start)?;
            let endConnected = isConnectedToUnitNode(&end)?;
            if !startConnected && !endConnected {
                edgeSegments.push(segment.clone());
            }
        }
        // Special edges shouldn't be moved, but they're still considered for bounds checking
        let mut specialEdgeSegments = fixedSegments;
        let guardedSpecialEdgeSegments = edgeSegmentsGuarded(specialEdges, !isHorizontal, guard)?;
        for specialEdgeSegment in &guardedSpecialEdgeSegments {
            guard.step()?;
            specialEdgeSegments.push(specialEdgeSegment.b().Segment.clone());
        }
        // Regular edges shared with special edges should also be locked
        for s in &edgeSegments {
            let mut k = 0usize;
            // Go ranges over the slice header captured at loop start.
            let n = specialEdgeSegments.len();
            while k < n {
                guard.step()?;
                let ses = specialEdgeSegments[k].clone();
                k += 1;
                let sseg = s.b().Segment.clone();
                if !sseg.Overlaps(&ses, isHorizontal, 1.0) {
                    continue;
                }
                let (a, b) = (sseg.Start.get(), ses.Start.get());
                let shared = if isHorizontal { eq(a.X, b.X) } else { eq(a.Y, b.Y) };
                if shared {
                    specialEdgeSegments.push(sseg);
                    lockedEdgeSegments.insert(segmentKey(s), true);
                    break;
                }
            }
        }
        lockedSegments.extend(specialEdgeSegments.iter().cloned());
        while lockedEdgeSegments.len() != edgeSegments.len() {
            guard.step()?;
            let mut minRange = Range { start: math::Inf(-1), end: math::Inf(1) };
            // Go map[Range][]*EdgeSegment; only lookups by key matter.
            let mut segmentRanges: Map<(u64, u64), Vec<EdgeSegmentRef>> = Map::default();

            for es in &edgeSegments {
                guard.step()?;
                if lockedEdgeSegments.contains_key(&segmentKey(es)) {
                    continue;
                }
                let seg = es.b().Segment.clone();
                let (mut floor, mut ceil) = routeSegmentBounds(&seg, &lockedSegments, segmentSpacingBuffer, guard)?;
                let start = seg.Start.get();
                if isHorizontal {
                    if floor == math::Inf(-1) {
                        floor = start.X - unboundedSegmentMove;
                    }
                    if ceil == math::Inf(1) {
                        ceil = start.X + unboundedSegmentMove;
                    }
                } else {
                    if floor == math::Inf(-1) {
                        floor = start.Y - unboundedSegmentMove;
                    }
                    if ceil == math::Inf(1) {
                        ceil = start.Y + unboundedSegmentMove;
                    }
                }

                let range = Range { start: floor, end: ceil };

                segmentRanges.entry(rangeKey(&range)).or_default().push(es.clone());
                if range.length() < minRange.length() {
                    minRange = range;
                }
            }

            // For all the segments which share the same min range, we want to further split them up by overlap
            let minSegments = segmentRanges.get(&rangeKey(&minRange)).cloned().unwrap_or_default();
            for s in &minSegments {
                guard.step()?;
                if lockedEdgeSegments.contains_key(&segmentKey(s)) {
                    continue;
                }
                let mut balanceBatch: Vec<EdgeSegmentRef> = vec![s.clone()];
                let mut balanceBatchSet: Set<EdgeSegmentRef> = Set::default();
                balanceBatchSet.insert(s.clone());
                let sOwner = s.b().Owner();
                let sSeg = s.b().Segment.clone();
                for otherS in &minSegments {
                    guard.step()?;
                    if s == otherS {
                        continue;
                    }
                    if lockedEdgeSegments.contains_key(&segmentKey(otherS)) {
                        continue;
                    }
                    let mut buffer: f64 = segmentSpacingBuffer;
                    // Segments of the same edge don't need to maintain a buffer from each other
                    if sOwner == otherS.b().Owner() {
                        buffer = 0.0;
                    }
                    if sSeg.Overlaps(&otherS.b().Segment, isHorizontal, buffer) {
                        balanceBatch.push(otherS.clone());
                        balanceBatchSet.insert(otherS.clone());
                    }
                }
                // Even if the same range is not shared, if a segment shares a route with another segment (even partially), it should be balanced together to maintain shared route
                for otherS in &edgeSegments {
                    guard.step()?;
                    if balanceBatchSet.contains(otherS) {
                        continue;
                    }
                    if lockedEdgeSegments.contains_key(&segmentKey(otherS)) {
                        continue;
                    }
                    let otherOwner = otherS.b().Owner();
                    let otherSeg = otherS.b().Segment.clone();
                    if let Some(cluster) = getSharedCluster(&sOwner, &otherOwner) {
                        let (arrangement, desired) = {
                            let c = cluster.b();
                            (c.Arrangement.clone(), c.DesiredArrangement.clone())
                        };
                        if arrangement == desired {
                            let (a, b) = (sSeg.Start.get(), otherSeg.Start.get());
                            let shouldBalanceTogether = if isHorizontal {
                                arrangement == layoutgraph::Column && eq(a.X, b.X)
                            } else {
                                arrangement == layoutgraph::Row && eq(a.Y, b.Y)
                            };
                            if shouldBalanceTogether {
                                balanceBatch.push(otherS.clone());
                                balanceBatchSet.insert(otherS.clone());
                                continue;
                            }
                        }
                    }
                    if !sSeg.Overlaps(&otherSeg, isHorizontal, 1.0) {
                        continue;
                    }
                    let (a, b) = (sSeg.Start.get(), otherSeg.Start.get());
                    let shared = if isHorizontal { eq(a.X, b.X) } else { eq(a.Y, b.Y) };
                    if shared {
                        balanceBatch.push(otherS.clone());
                        balanceBatchSet.insert(otherS.clone());
                    }
                }
                stableSortRouteValues(
                    &mut balanceBatch,
                    |a, b| {
                        let (sa, sb) = (segStart(a), segStart(b));
                        let (primaryDimensionI, primaryDimensionJ, secondaryDimensionI, secondaryDimensionJ) =
                            if isHorizontal { (sa.X, sb.X, sa.Y, sb.Y) } else { (sa.Y, sb.Y, sa.X, sb.X) };
                        if primaryDimensionI == primaryDimensionJ {
                            return secondaryDimensionI < secondaryDimensionJ;
                        }
                        primaryDimensionI < primaryDimensionJ
                    },
                    guard,
                )?;
                // Among the overlapping segments, some may be on shared routes. They're shared intentionally, so we want to keep them shared.
                let mut distincts: Set<u64> = Set::default();
                for s in &balanceBatch {
                    guard.step()?;
                    let st = segStart(s);
                    let v = if isHorizontal { st.X.round() } else { st.Y.round() };
                    distincts.insert(if v == 0.0 { 0 } else { v.to_bits() });
                }

                // Now we take the overlapping segments with the same range of movement, and rebalance them
                let distributionVals = evenlyDistributeGuarded(minRange.start, minRange.end, distincts.len(), guard)?;
                let mut proposed: Vec<f64> = vec![0.0; balanceBatch.len()];
                for (i, segment) in balanceBatch.iter().enumerate() {
                    guard.step()?;
                    let st = segStart(segment);
                    proposed[i] = st.Y;
                    if isHorizontal {
                        proposed[i] = st.X;
                    }
                }

                let mut segmentIndex = 0usize;
                for valIndex in 0..distributionVals.len() {
                    guard.step()?;
                    let mut sharedCount = 0usize;
                    while segmentIndex + sharedCount < balanceBatch.len() {
                        guard.step()?;
                        let (a, b) = (segStart(&balanceBatch[segmentIndex]), segStart(&balanceBatch[segmentIndex + sharedCount]));
                        let same = if isHorizontal { eq(a.X, b.X) } else { eq(a.Y, b.Y) };
                        if same {
                            sharedCount += 1;
                        } else {
                            break;
                        }
                    }

                    let val = distributionVals[valIndex];
                    for i in segmentIndex..segmentIndex + sharedCount {
                        guard.step()?;
                        proposed[i] = val;
                    }
                    segmentIndex += sharedCount;
                }
                let mut order =
                    checkBalanceOrder(&balanceBatch, &balanceBatchSet, &allEdgeSegments, &proposed, isHorizontal, guard)?;
                if order != balanceOrderContactChanged {
                    let specialOrder = checkBalanceOrder(
                        &balanceBatch,
                        &balanceBatchSet,
                        &guardedSpecialEdgeSegments,
                        &proposed,
                        isHorizontal,
                        guard,
                    )?;
                    order = order.max(specialOrder);
                }
                let mut accept = order == balanceOrderPreserved;
                if order == balanceOrderReversed {
                    accept = balanceReversalRemovesCrossings(g, &balanceBatch, &proposed, isHorizontal, guard)?;
                }
                if accept {
                    for (i, segment) in balanceBatch.iter().enumerate() {
                        guard.step()?;
                        let (start, end) = {
                            let s = segment.b();
                            (s.Segment.Start.clone(), s.Segment.End.clone())
                        };
                        if isHorizontal {
                            start.bm().X = proposed[i];
                            end.bm().X = proposed[i];
                        } else {
                            start.bm().Y = proposed[i];
                            end.bm().Y = proposed[i];
                        }
                    }
                }
                for s in &balanceBatch {
                    guard.step()?;
                    lockedEdgeSegments.insert(segmentKey(s), true);
                    lockedSegments.push(s.b().Segment.clone());
                }
            }
        }
    }
    for e in regularEdges {
        let points = removeDuplicatePointsGuarded(&e.points(), guard)?;
        e.bm().Points = points;
    }
    Ok(())
}

// FixClusterEdgeBranching resolves route branching at cluster boundaries.
pub fn FixClusterEdgeBranching(ctx: &Ctx, g: &GraphRef) -> GoResult<()> {
    fixClusterEdgeBranchingWithLimit(ctx, g, maxRouteStageWorkUnits)
}

pub(crate) fn fixClusterEdgeBranchingWithLimit(ctx: &Ctx, g: &GraphRef, workLimit: u64) -> GoResult<()> {
    runAtomicRouteStage(ctx, "FixClusterEdgeBranching", g, &[], workLimit, |guard| fixClusterEdgeBranchingGuarded(g, guard))
}

pub(crate) fn fixClusterEdgeBranchingGuarded(g: &GraphRef, guard: &routeWorkGuardRef) -> GoResult<()> {
    let graphNodes = g.nodes();
    for n in &graphNodes {
        guard.step()?;
        let nEdges = n.edges();
        if nEdges.len() <= 1 {
            continue;
        }
        if n.clusterField().is_some() {
            continue;
        }
        let mut isFrom: Map<ClusterRef, bool> = Map::default();
        // Go map[*Cluster][]*Edge, ranged in random order. The port uses first
        // insertion order (see DEVIATIONS.md).
        let mut clusterOrder: Vec<ClusterRef> = Vec::new();
        let mut clusterEdges: Map<ClusterRef, Vec<EdgeRef>> = Map::default();
        for e in &nEdges {
            guard.step()?;
            if e.b().Points.len() != 4 {
                continue;
            }
            let adj = n.Adjacent(e);
            if let Some(cluster) = adj.clusterField() {
                let (desired, arrangement) = {
                    let c = cluster.b();
                    (c.DesiredArrangement.clone(), c.Arrangement.clone())
                };
                if desired != arrangement {
                    continue;
                }
                if e.from() == *n {
                    isFrom.insert(cluster.clone(), true);
                } else {
                    isFrom.insert(cluster.clone(), false);
                }
                if !clusterEdges.contains_key(&cluster) {
                    clusterOrder.push(cluster.clone());
                    clusterEdges.insert(cluster.clone(), vec![e.clone()]);
                } else {
                    clusterEdges.get_mut(&cluster).unwrap().push(e.clone());
                }
            }
        }

        for cluster in &clusterOrder {
            let edges = clusterEdges[cluster].clone();
            guard.step()?;
            let isFromCluster = isFrom.get(cluster).copied().unwrap_or(false);
            let isRow = cluster.b().Arrangement == layoutgraph::Row;
            let mut backOfCenterFound = false;
            let mut frontOfCenterFound = false;
            for e in &edges {
                guard.step()?;
                let p = pointValues(&e.b().Points);
                if isRow {
                    if isFromCluster {
                        if p[3].X < p[0].X {
                            backOfCenterFound = true;
                        }
                        if p[3].X > p[0].X {
                            frontOfCenterFound = true;
                        }
                    } else {
                        if p[0].X < p[3].X {
                            backOfCenterFound = true;
                        }
                        if p[0].X > p[3].X {
                            frontOfCenterFound = true;
                        }
                    }
                } else if isFromCluster {
                    if p[3].Y < p[0].Y {
                        backOfCenterFound = true;
                    }
                    if p[3].Y > p[0].Y {
                        frontOfCenterFound = true;
                    }
                } else {
                    if p[0].Y < p[3].Y {
                        backOfCenterFound = true;
                    }
                    if p[0].Y > p[3].Y {
                        frontOfCenterFound = true;
                    }
                }
            }
            if !backOfCenterFound || !frontOfCenterFound {
                continue;
            }

            let mut originalP1: Map<EdgeRef, geo::PointRef> = Map::default();
            let mut originalP2: Map<EdgeRef, geo::PointRef> = Map::default();
            let mut potentialMovePoints: Vec<geo::PointRef> = Vec::new();
            for e in &edges {
                guard.step()?;
                let points = e.points();
                if isFromCluster {
                    potentialMovePoints.push(points[1].clone());
                } else {
                    potentialMovePoints.push(points[2].clone());
                }
                originalP1.insert(e.clone(), points[1].Copy());
                originalP2.insert(e.clone(), points[2].Copy());
            }

            let mut bestPoint: Option<geo::PointRef> = None;
            let mut bestPointCost: f64 = 0.0;

            let allEdges = g.edges();
            'OUTER: for p in &potentialMovePoints {
                guard.step()?;
                for e in &edges {
                    guard.step()?;
                    let mut eb = e.bm();
                    eb.Points[1] = originalP1[e].Copy();
                    eb.Points[2] = originalP2[e].Copy();
                }
                let mut cost = 0.0;
                for e in &edges {
                    guard.step()?;
                    let originalCost = estimateRouteCostGuarded(&allEdges, e, guard)?;
                    if isFromCluster {
                        if e.b().Points[1] == *p {
                            cost += originalCost;
                            continue;
                        }
                    } else if e.b().Points[2] == *p {
                        cost += originalCost;
                        continue;
                    }
                    let pv = p.get();
                    if isFromCluster {
                        let p2 = {
                            let mut eb = e.bm();
                            eb.Points[1] = p.Copy();
                            eb.Points[2].clone()
                        };
                        if isRow {
                            p2.bm().Y = pv.Y;
                        } else {
                            p2.bm().X = pv.X;
                        }
                    } else {
                        let p1 = {
                            let mut eb = e.bm();
                            eb.Points[2] = p.Copy();
                            eb.Points[1].clone()
                        };
                        if isRow {
                            p1.bm().Y = pv.Y;
                        } else {
                            p1.bm().X = pv.X;
                        }
                    }

                    let mut revert = false;
                    let intersectsNode = routeIntersectsNodeGuarded(&graphNodes, e, guard)?;
                    if intersectsNode {
                        revert = true;
                    }
                    if !revert {
                        let newCost = estimateRouteCostGuarded(&allEdges, e, guard)?;
                        cost += newCost;
                        if newCost > originalCost {
                            revert = true;
                        }
                    }
                    if revert {
                        continue 'OUTER;
                    }
                }
                if bestPoint.is_none() || cost < bestPointCost {
                    bestPoint = Some(p.clone());
                    bestPointCost = cost;
                }
            }

            for e in &edges {
                guard.step()?;
                let mut eb = e.bm();
                eb.Points[1] = originalP1[e].Copy();
                eb.Points[2] = originalP2[e].Copy();
            }

            if let Some(bestPoint) = bestPoint {
                let bv = bestPoint.get();
                for e in &edges {
                    guard.step()?;
                    if isFromCluster {
                        let p2 = {
                            let mut eb = e.bm();
                            eb.Points[1] = bestPoint.Copy();
                            eb.Points[2].clone()
                        };
                        if isRow {
                            p2.bm().Y = bv.Y;
                        } else {
                            p2.bm().X = bv.X;
                        }
                    } else {
                        let p1 = {
                            let mut eb = e.bm();
                            eb.Points[2] = bestPoint.Copy();
                            eb.Points[1].clone()
                        };
                        if isRow {
                            p1.bm().Y = bv.Y;
                        } else {
                            p1.bm().X = bv.X;
                        }
                    }
                }
            }
        }
    }

    guard.finish()
}
