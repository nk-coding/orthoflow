// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/routing/standalone_route_guard.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of routing/standalone_route_guard.go.
#[allow(unused_imports)]
use crate::*;
use gocompat::math;
use gocompat::sort;
use gocompat::{GoResult, Map, Set};
use labeling::PositionedArrowheadLabel;
use layoutgraph::{EdgeRef, GraphRef, NodeRef};

impl routeWorkGuardRef {
    pub(crate) fn reserveSort(&self, length: usize) -> GoResult<()> {
        if length < 2 {
            return self.step();
        }
        // Go's comparison sort is O(n log n). Reserve that worst-case work before
        // entering the non-interruptible standard-library sort.
        let mut width = 1usize;
        while width < length {
            self.add(length as u64)?;
            if width > length / 2 {
                break;
            }
            width *= 2;
        }
        self.check()
    }
}

pub(crate) fn portEdgesGuarded(edges: &[EdgeRef], node: &NodeRef, guard: &dyn workBudget) -> GoResult<Map<geo::Point, Vec<EdgeRef>>> {
    let mut portEdges: Map<geo::Point, Vec<EdgeRef>> = Map::default();
    for edge in edges {
        guard.step()?;
        let port = if edge.from() == *node {
            edge.SourcePort()
        } else if edge.to() == *node {
            edge.TargetPort()
        } else {
            continue;
        };
        if let Some(port) = port {
            portEdges.entry(port.get()).or_default().push(edge.clone());
        }
    }
    Ok(portEdges)
}

pub(crate) fn edgeHasDuplicateInGuarded(edge: &EdgeRef, edges: &[EdgeRef], guard: &dyn workBudget) -> GoResult<bool> {
    for otherEdge in edges {
        guard.step()?;
        if otherEdge.IsDuplicateOf(edge) {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(crate) fn tryStraightEdgeFallbackGuarded(g: &GraphRef, edge: &EdgeRef, guard: &routeWorkGuardRef) -> GoResult<()> {
    guard.step()?;
    let originalCost = estimateRouteCostGuarded(&g.edges(), edge, guard)?;
    let (fromPort, toPort, mut lineCost) = match routeLineGuarded(g, edge, &g.edges(), &[], guard) {
        Ok(v) => v,
        Err(err) => {
            if layoutgraph::IsCandidateRejection(Some(&err)) {
                // A straight replacement is optional. Preserve the current route
                // when no valid line exists.
                return Ok(());
            }
            return Err(err);
        }
    };

    let points = pointValues(&edge.b().Points);
    if points.len() == 4 && geo::EuclideanDistance(points[1].X, points[1].Y, points[2].X, points[2].Y) <= lowerJitterThreshold {
        lineCost /= nonOrthogonalFactor;
    }
    guard.step()?;
    if lineCost < originalCost {
        edge.bm().Points = vec![fromPort, toPort];
    }
    Ok(())
}

pub(crate) fn filterEdgeAncestorsGuarded(edge: &EdgeRef, nodes: &[NodeRef], guard: &dyn workBudget) -> GoResult<Vec<NodeRef>> {
    let (from, to) = (edge.from(), edge.to());
    let mut nonAncestors: Vec<NodeRef> = Vec::with_capacity(nodes.len());
    for node in nodes {
        guard.step()?;
        if *node != from && *node != to && (from.IsDescendantOf(Some(node)) || to.IsDescendantOf(Some(node))) {
            continue;
        }
        nonAncestors.push(node.clone());
    }
    Ok(nonAncestors)
}

/// Go returns `map[*Node]bool` sets; only membership is ever read.
pub(crate) fn sourceAndTargetClusterNodesGuarded(
    g: &GraphRef,
    source: &NodeRef,
    target: &NodeRef,
    guard: &dyn workBudget,
) -> GoResult<(Set<NodeRef>, Set<NodeRef>)> {
    let mut sourceClusterNodes: Set<NodeRef> = Set::default();
    let mut targetClusterNodes: Set<NodeRef> = Set::default();

    // Go ranges over the cluster map; the result and the step count do not
    // depend on the order.
    let clusters = g.b().Clusters.values();
    for cluster in clusters {
        guard.step()?;
        let nodes = cluster.b().Nodes.clone();
        let mut isSource = false;
        let mut isTarget = false;
        for clusterNode in &nodes {
            guard.step()?;
            if clusterNode == source {
                isSource = true;
            }
            if clusterNode == target {
                isTarget = true;
            }
        }
        if isSource {
            for clusterNode in &nodes {
                guard.step()?;
                if clusterNode != source {
                    sourceClusterNodes.insert(clusterNode.clone());
                }
            }
        }
        if isTarget {
            for clusterNode in &nodes {
                guard.step()?;
                if clusterNode != target {
                    targetClusterNodes.insert(clusterNode.clone());
                }
            }
        }
    }
    Ok((sourceClusterNodes, targetClusterNodes))
}

pub(crate) fn overlappingEdgesGuarded(
    start: &geo::Point,
    end: &geo::Point,
    otherEdges: &[EdgeRef],
    guard: &dyn workBudget,
) -> GoResult<Vec<EdgeRef>> {
    let mut overlappingEdges: Vec<EdgeRef> = Vec::new();
    for edge in otherEdges {
        guard.step()?;
        let points = pointValues(&edge.b().Points);
        let mut index = 0usize;
        while index + 1 < points.len() {
            guard.step()?;
            if isVerticalOrHorizontalOverlap(start, end, &points[index], &points[index + 1]) {
                overlappingEdges.push(edge.clone());
                break;
            }
            index += 1;
        }
    }
    Ok(overlappingEdges)
}

pub(crate) fn edgeCanOverlapEdgesGuarded(
    edge: &EdgeRef,
    otherEdges: &[EdgeRef],
    sourceClusterNodes: Option<&Set<NodeRef>>,
    targetClusterNodes: Option<&Set<NodeRef>>,
    guard: &dyn workBudget,
) -> GoResult<bool> {
    if otherEdges.is_empty() {
        // add(0) polls the context, so the general path below performs eleven
        // identical polls before edgeCanOverlapEdges returns true.
        if let Some(searchGuard) = guard.asRouteSearchWorkGuard() {
            if searchGuard.b().done.is_some() {
                searchGuard.check()?;
                return Ok(true);
            }
        }
    }
    // edgeCanOverlapEdges makes a bounded number of full passes over otherEdges.
    for _pass in 0..10 {
        guard.add(otherEdges.len() as u64)?;
    }
    guard.check()?;
    Ok(edgeCanOverlapEdges(edge, otherEdges, sourceClusterNodes, targetClusterNodes))
}

/// Go `&layoutgraph.Node{Box: box, Graph: graph}`.
pub(crate) fn fakeBoxNode(box_: &geo::Box, graph: Option<GraphRef>) -> NodeRef {
    let mut n = layoutgraph::Node::default();
    n.Box = geo::BoxRef::new(box_.clone());
    n.Graph = graph;
    NodeRef::new(n)
}

pub(crate) fn positionedArrowheadLabelCostGuarded(
    positioned: &PositionedArrowheadLabel,
    nodes: &[NodeRef],
    labels: &[PositionedArrowheadLabel],
    routes: &[RouteRef],
    edges: &[EdgeRef],
    guard: &dyn workBudget,
) -> GoResult<f64> {
    for other in labels {
        guard.step()?;
        if positioned.Edge == other.Edge && positioned.IsTarget == other.IsTarget {
            continue;
        }
        if positioned.Box.Overlaps(&other.Box) {
            if positioned.Text == other.Text {
                continue;
            }
            return Ok(math::Inf(1));
        }
    }

    let graph = positioned.Edge.from().g();
    let fakeLabelNode = fakeBoxNode(&positioned.Box, Some(graph.clone()));
    let mut overlapCount: i64 = 0;
    for node in nodes {
        guard.step()?;
        if fakeLabelNode.LabelBoxesOverlap(node, label::PADDING as f64) {
            overlapCount += 1;
        }
    }
    let mut penalty = 4.0 * graph.TurnCost() * overlapCount as f64;

    let mut overlappingEdgeCount: i64 = 0;
    for route in routes {
        guard.step()?;
        let r = route.b();
        if r.GEdge == positioned.Edge {
            continue;
        }
        for index in 1..r.OVGNodes.len() {
            guard.step()?;
            if fakeLabelNode.OverlapsLine(&r.OVGNodes[index - 1].P(), &r.OVGNodes[index].P(), 0.0) {
                overlappingEdgeCount += 1;
                break;
            }
        }
    }
    for edge in edges {
        guard.step()?;
        if *edge == positioned.Edge {
            continue;
        }
        let points = pointValues(&edge.b().Points);
        for index in 1..points.len() {
            guard.step()?;
            if fakeLabelNode.OverlapsLine(&points[index - 1], &points[index], 0.0) {
                overlappingEdgeCount += 1;
                break;
            }
        }
    }
    penalty += graph.TurnCost() * overlappingEdgeCount as f64;
    Ok(penalty)
}

pub(crate) fn edgeIsStraightGuarded(edge: &EdgeRef, guard: &routeWorkGuardRef) -> GoResult<bool> {
    let points = pointValues(&edge.b().Points);
    if points.len() < 2 {
        guard.step()?;
        return Ok(false);
    }
    let mut first = points[0];
    let mut second = points[1];
    for index in 2..points.len() {
        guard.step()?;
        if orientation(&first, &second, &points[index]) != 0.0 {
            return Ok(false);
        }
        first = second;
        second = points[index];
    }
    Ok(true)
}

pub(crate) fn edgeHasOverlappingEndGuarded(edge: &EdgeRef, guard: &routeWorkGuardRef) -> GoResult<bool> {
    let points = pointValues(&edge.b().Points);
    let start = points[0];
    let end = points[points.len() - 1];
    for fromEdge in edge.from().edges() {
        guard.step()?;
        if fromEdge != *edge {
            let fp = pointValues(&fromEdge.b().Points);
            if nonNilEquals(&fp[0], &start) || nonNilEquals(&fp[fp.len() - 1], &start) {
                return Ok(true);
            }
        }
    }
    for toEdge in edge.to().edges() {
        guard.step()?;
        if toEdge != *edge {
            let tp = pointValues(&toEdge.b().Points);
            if nonNilEquals(&tp[0], &end) || nonNilEquals(&tp[tp.len() - 1], &end) {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

pub(crate) fn reorderDuplicatesInEdgesGuarded(edges: &[EdgeRef], guard: &routeWorkGuardRef) -> GoResult<()> {
    for edge in edges {
        guard.step()?;
        if edge.HasTableColumn() || edge.b().Label.is_none() {
            continue;
        }
        let straight = edgeIsStraightGuarded(edge, guard)?;
        if !straight {
            continue;
        }
        let hasOverlappingEnd = edgeHasOverlappingEndGuarded(edge, guard)?;

        let mut duplicates: Vec<usize> = Vec::new();
        for (index, other) in edges.iter().enumerate() {
            guard.step()?;
            if other == edge || !other.IsDuplicateOf(edge) {
                continue;
            }
            let straight = edgeIsStraightGuarded(other, guard)?;
            if !straight {
                continue;
            }
            let otherHasOverlappingEnd = if hasOverlappingEnd { true } else { edgeHasOverlappingEndGuarded(other, guard)? };
            if otherHasOverlappingEnd {
                let (mut sourceArrow, mut targetArrow) = {
                    let e = edge.b();
                    (e.SourceArrowhead.clone(), e.TargetArrowhead.clone())
                };
                if other.from() == edge.to() {
                    std::mem::swap(&mut sourceArrow, &mut targetArrow);
                }
                let o = other.b();
                if o.TargetArrowhead == targetArrow && o.SourceArrowhead == sourceArrow {
                    duplicates.push(index);
                }
            } else {
                duplicates.push(index);
            }
        }

        if duplicates.len() <= 1 {
            continue;
        }
        let points = pointValues(&edge.b().Points);
        let start = points[0];
        let end = points[points.len() - 1];
        let dx = end.X - start.X;
        let dy = end.Y - start.Y;
        let mut sourceOrder: Vec<EdgeRef> = Vec::with_capacity(duplicates.len() + 1);
        let mut targetOrder: Vec<EdgeRef> = Vec::with_capacity(duplicates.len() + 1);
        sourceOrder.push(edge.clone());
        targetOrder.push(edge.clone());
        for &index in &duplicates {
            guard.step()?;
            sourceOrder.push(edges[index].clone());
            targetOrder.push(edges[index].clone());
        }

        guard.reserveSort(sourceOrder.len())?;
        guard.reserveSort(targetOrder.len())?;
        sortDuplicateOrders(&mut sourceOrder, &mut targetOrder, dy > dx);

        let mut consistentOrder = true;
        for index in 0..sourceOrder.len() {
            guard.step()?;
            if sourceOrder[index] != targetOrder[index] {
                consistentOrder = false;
                break;
            }
        }
        if !consistentOrder {
            continue;
        }
        let mut currentIndex: i64 = -1;
        for index in 0..sourceOrder.len() {
            guard.step()?;
            if sourceOrder[index] == *edge {
                currentIndex = index as i64;
                break;
            }
        }
        if currentIndex == 0 || currentIndex == sourceOrder.len() as i64 - 1 {
            continue;
        }
        let first = sourceOrder[0].clone();
        let last = sourceOrder[sourceOrder.len() - 1].clone();
        if first.b().Label.is_some() && last.b().Label.is_some() {
            continue;
        }
        guard.step()?;

        if last.b().Label.is_none() {
            swapEdgePoints(edge, &last);
            if last.from() == edge.to() {
                reverseEdgeRouteGuarded(edge, guard)?;
                reverseEdgeRouteGuarded(&last, guard)?;
            }
        } else if first.b().Label.is_none() {
            swapEdgePoints(edge, &first);
            if first.from() == edge.to() {
                reverseEdgeRouteGuarded(edge, guard)?;
                reverseEdgeRouteGuarded(&first, guard)?;
            }
        }
    }
    guard.finish()
}

/// Go `a.Points, b.Points = b.Points, a.Points`.
pub(crate) fn swapEdgePoints(a: &EdgeRef, b: &EdgeRef) {
    if a == b {
        return;
    }
    let pa = std::mem::take(&mut a.bm().Points);
    let pb = std::mem::replace(&mut b.bm().Points, pa);
    a.bm().Points = pb;
}

/// The two `sort.Slice` calls shared by ReorderDuplicates and its guarded
/// standalone variant.
pub(crate) fn sortDuplicateOrders(sourceOrder: &mut Vec<EdgeRef>, targetOrder: &mut Vec<EdgeRef>, byX: bool) {
    let first = |e: &EdgeRef| e.b().Points[0].get();
    let last = |e: &EdgeRef| {
        let e = e.b();
        e.Points[e.Points.len() - 1].get()
    };
    if byX {
        sort::SortSlice(sourceOrder, |a, b| first(a).X < first(b).X);
        sort::SortSlice(targetOrder, |a, b| last(a).X < last(b).X);
    } else {
        sort::SortSlice(sourceOrder, |a, b| first(a).Y < first(b).Y);
        sort::SortSlice(targetOrder, |a, b| last(a).Y < last(b).Y);
    }
}

pub(crate) fn reverseEdgeRouteGuarded(edge: &EdgeRef, guard: &routeWorkGuardRef) -> GoResult<()> {
    let n = edge.b().Points.len();
    if n == 0 {
        return Ok(());
    }
    let (mut left, mut right) = (0usize, n - 1);
    while left < right {
        guard.step()?;
        edge.bm().Points.swap(left, right);
        left += 1;
        right -= 1;
    }
    Ok(())
}

pub(crate) fn traceToShapeBorderGuarded(edge: Option<&EdgeRef>, guard: &routeWorkGuardRef) -> GoResult<()> {
    guard.step()?;
    let Some(edge) = edge else { return Ok(()) };
    if edge.b().Points.len() < 2 {
        return Ok(());
    }
    // Go restores both endpoint positions only when the trace panics; the port
    // lets panics propagate.
    traceToShapeBorder(edge);
    guard.step()
}
