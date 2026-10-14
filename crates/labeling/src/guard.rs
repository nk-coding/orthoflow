// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/labeling/guard.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

/// Label placement is intentionally bounded independently of adapter input validation.
pub(crate) const maxLabelPlacementWorkUnits: i64 = limits::MaxLabelPlacementWorkUnits;
pub(crate) const maxLabelPlacementAncestryDepth: i64 = layoutgraph::MaxTopologyDepth;
pub(crate) const labelPlacementContextCheckStride: i64 = 64;

const LABEL_PADDING: f64 = label::PADDING as f64;

/// labelPlacementWorkGuard makes every unit of quadratic/candidate work both
/// cancellable and bounded.
pub(crate) struct labelPlacementWorkGuard {
    pub(crate) ctx: Ctx,
    pub(crate) location: String,
    pub(crate) used: i64,
    pub(crate) limit: i64,
}

pub(crate) fn newLabelPlacementWorkGuard(ctx: &Ctx, location: &str, limit: i64) -> GoResult<labelPlacementWorkGuard> {
    if limit < 0 {
        return Err(errorf!("TALA {} work limit must not be negative", location));
    }
    let guard = labelPlacementWorkGuard { ctx: ctx.clone(), location: location.to_string(), used: 0, limit };
    guard.check()?;
    Ok(guard)
}

impl labelPlacementWorkGuard {
    pub(crate) fn step(&mut self) -> GoResult<()> {
        self.add(1)
    }

    pub(crate) fn add(&mut self, units: i64) -> GoResult<()> {
        if units < 0 {
            return Err(errorf!("TALA {} received a negative work estimate", self.location));
        }
        if self.used < 0 || self.used > self.limit || units > self.limit - self.used {
            self.check()?;
            return Err(errorf!("TALA {} work exceeds limit {}", self.location, self.limit));
        }
        let previous = self.used;
        self.used += units;
        if units == 0 || previous / labelPlacementContextCheckStride != self.used / labelPlacementContextCheckStride {
            return self.check();
        }
        Ok(())
    }

    pub(crate) fn check(&self) -> GoResult<()> {
        if let Some(err) = self.ctx.Err() {
            return Err(wrapf!(err, "{}: {}", self.location, err));
        }
        Ok(())
    }
}

pub(crate) struct labelPositionSnapshot {
    pub(crate) label: layoutgraph::LabelRef,
    pub(crate) position: label::Position,
}

pub(crate) struct iconPositionSnapshot {
    pub(crate) icon: layoutgraph::IconRef,
    pub(crate) position: label::Position,
}

pub(crate) struct edgeLabelPercentageSnapshot {
    pub(crate) edge: EdgeRef,
    pub(crate) percentage: f64,
}

#[derive(Default)]
pub(crate) struct labelPlacementSnapshot {
    pub(crate) labels: Vec<labelPositionSnapshot>,
    pub(crate) icons: Vec<iconPositionSnapshot>,
    pub(crate) edgePercentages: Vec<edgeLabelPercentageSnapshot>,
}

/// captureLabelPlacement records only the state this algorithm owns.
pub(crate) fn captureLabelPlacement(g: Option<&GraphRef>, extraEdges: &[EdgeRef]) -> labelPlacementSnapshot {
    let mut snapshot = labelPlacementSnapshot::default();
    let mut seenLabels: Set<layoutgraph::LabelRef> = Set::default();
    let mut seenIcons: Set<layoutgraph::IconRef> = Set::default();
    let mut seenEdges: Set<EdgeRef> = Set::default();

    fn captureLabel(snapshot: &mut labelPlacementSnapshot, seen: &mut Set<layoutgraph::LabelRef>, value: Option<layoutgraph::LabelRef>) {
        let Some(value) = value else { return };
        if seen.contains(&value) {
            return;
        }
        seen.insert(value.clone());
        let position = value.b().Position;
        snapshot.labels.push(labelPositionSnapshot { label: value, position });
    }
    let mut captureEdge = |snapshot: &mut labelPlacementSnapshot, seenLabels: &mut Set<layoutgraph::LabelRef>, edge: &EdgeRef| {
        captureLabel(snapshot, seenLabels, edge.b().Label.clone());
        if seenEdges.contains(edge) {
            return;
        }
        seenEdges.insert(edge.clone());
        let percentage = edge.b().LabelPercentage;
        snapshot.edgePercentages.push(edgeLabelPercentageSnapshot { edge: edge.clone(), percentage });
    };

    if let Some(g) = g {
        let (nodes, edges) = {
            let gb = g.b();
            (gb.Nodes.clone(), gb.Edges.clone())
        };
        for node in &nodes {
            captureLabel(&mut snapshot, &mut seenLabels, node.label());
            if let Some(icon) = node.icon() {
                if !seenIcons.contains(&icon) {
                    seenIcons.insert(icon.clone());
                    let position = icon.b().Position;
                    snapshot.icons.push(iconPositionSnapshot { icon, position });
                }
            }
        }
        for edge in &edges {
            captureEdge(&mut snapshot, &mut seenLabels, edge);
        }
    }
    for edge in extraEdges {
        captureEdge(&mut snapshot, &mut seenLabels, edge);
    }
    snapshot
}

impl labelPlacementSnapshot {
    pub(crate) fn restore(&self) {
        for state in &self.labels {
            state.label.bm().Position = state.position;
        }
        for state in &self.icons {
            state.icon.bm().Position = state.position;
        }
        for state in &self.edgePercentages {
            state.edge.bm().LabelPercentage = state.percentage;
        }
    }
}

pub(crate) fn nodeOverlapCount(node: &NodeRef, nodes: &[NodeRef], delta: f64, guard: &mut labelPlacementWorkGuard) -> GoResult<i64> {
    let mut count = 0;
    for otherNode in nodes {
        guard.step()?;
        if node.LabelBoxesOverlap(otherNode, delta) {
            count += 1;
        }
    }
    Ok(count)
}

pub(crate) fn partialNodeOverlapCount(node: &NodeRef, nodes: &[NodeRef], delta: f64, guard: &mut labelPlacementWorkGuard) -> GoResult<i64> {
    let mut count = 0;
    for otherNode in nodes {
        guard.step()?;
        if node.LabelBoxesOverlap(otherNode, delta) && !otherNode.Covers(node) {
            count += 1;
        }
    }
    Ok(count)
}

pub(crate) fn nodeOverlapArea(
    node: &NodeRef,
    nodes: &[NodeRef],
    delta: f64,
    partial: bool,
    guard: &mut labelPlacementWorkGuard,
) -> GoResult<(f64, i64)> {
    let mut area = 0.0;
    let mut overlapCount = 0;
    for otherNode in nodes {
        guard.step()?;
        if !node.LabelBoxesOverlap(otherNode, delta) || (partial && otherNode.Covers(node)) {
            continue;
        }
        if delta == 0.0 || node.DoesOverlapExact(otherNode) {
            area += node.LabelOverlapArea(otherNode);
            overlapCount += 1;
        }
        if !otherNode.IsContainer() {
            area += node.Area();
            overlapCount += 1;
        }
    }
    Ok((area, overlapCount))
}

pub(crate) fn edgeOverlapCount(node: &NodeRef, edges: &[EdgeRef], delta: f64, guard: &mut labelPlacementWorkGuard) -> GoResult<i64> {
    let mut count = 0;
    for edge in edges {
        guard.step()?;
        let points = edge.points();
        for pointIndex in 1..points.len() {
            guard.step()?;
            let a = *points[pointIndex - 1].b();
            let b = *points[pointIndex].b();
            if node.OverlapsLine(&a, &b, delta) {
                count += 1;
            }
        }
    }
    Ok(count)
}

pub(crate) fn collectLabelPlacementAncestors(node: &NodeRef, guard: &mut labelPlacementWorkGuard) -> GoResult<Vec<NodeRef>> {
    let mut ancestors = Vec::with_capacity(4);
    let mut current = Some(node.clone());
    let mut depth = 0i64;
    while let Some(cur) = current {
        if depth >= maxLabelPlacementAncestryDepth {
            return Err(errorf!("TALA {} container depth exceeds limit {}", guard.location, maxLabelPlacementAncestryDepth));
        }
        guard.step()?;
        ancestors.push(cur.clone());
        current = cur.EffectiveContainer();
        depth += 1;
    }
    Ok(ancestors)
}

pub(crate) fn isLabelPlacementDescendantOf(
    maybeDescendant: Option<&NodeRef>,
    maybeAncestor: Option<&NodeRef>,
    guard: &mut labelPlacementWorkGuard,
) -> GoResult<bool> {
    let mut maybeDescendant = maybeDescendant.cloned();
    let mut depth = 0i64;
    loop {
        if layoutgraph::nodeEq(maybeAncestor, maybeDescendant.as_ref()) {
            return Ok(true);
        }
        let Some(d) = maybeDescendant.clone() else { return Ok(false) };
        if depth >= maxLabelPlacementAncestryDepth {
            return Err(errorf!("TALA {} ancestry depth exceeds limit {}", guard.location, maxLabelPlacementAncestryDepth));
        }
        guard.step()?;
        let (container, cluster, sequence) = {
            let n = d.b();
            (n.Container.clone(), n.Cluster.clone(), n.Sequence.clone())
        };
        maybeDescendant = if container.is_some() {
            container
        } else if let Some(c) = cluster {
            let v = c.b().Vessel.clone();
            Some(v)
        } else if let Some(s) = sequence {
            let v = s.b().Vessel.clone();
            Some(v)
        } else {
            None
        };
        depth += 1;
    }
}

pub(crate) fn clusterBoundaryNodePlacement(
    node: &NodeRef,
    nodes: &[NodeRef],
    orientation: geo::Orientation,
    guard: &mut labelPlacementWorkGuard,
) -> GoResult<bool> {
    for other in nodes {
        guard.step()?;
        if other == node || !other.has_tl() {
            continue;
        }
        let o = other.tlv();
        let n = node.tlv();
        if orientation == geo::Left {
            if o.X < n.X {
                return Ok(false);
            }
        } else if orientation == geo::Top {
            if o.Y < n.Y {
                return Ok(false);
            }
        } else if orientation == geo::Right {
            if o.X + other.w() > n.X + node.w() {
                return Ok(false);
            }
        } else if orientation == geo::Bottom {
            if o.Y + other.h() > n.Y + node.h() {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

pub(crate) fn clusterNodeBoundingBoxPlacement(
    node: &NodeRef,
    nodes: &[NodeRef],
    guard: &mut labelPlacementWorkGuard,
) -> GoResult<(geo::Point, geo::Point)> {
    if !node.has_tl() {
        return Err(errorf!("TALA {} cluster contains an unplaced node", guard.location));
    }
    guard.step()?;
    let mut tl = node.tlv();
    let mut br = geo::Point { X: math::Round(tl.X + node.w()), Y: math::Round(tl.Y + node.h()) };
    let (dx, dy) = node.ModifierElementAdjustments();
    if dx != 0.0 || dy != 0.0 {
        tl.Y -= dy;
        br.X += dx;
    }
    tl.X -= node.loopOffset(geo::Left);
    tl.Y -= node.loopOffset(geo::Top);
    br.X += node.loopOffset(geo::Right);
    br.Y += node.loopOffset(geo::Bottom);

    if let Some(lbl) = node.label() {
        let (lpos, lw, lh) = {
            let l = lbl.b();
            (l.Position, l.Width, l.Height)
        };
        if lpos.IsOutside() {
            let labelTopLeft = *node.LabelTopLeft(lpos, lw, lh).b();
            let boundaryPadding = LABEL_PADDING;
            let outsidePadding = 2.0 * LABEL_PADDING;
            if labelTopLeft.X < tl.X {
                let boundary = clusterBoundaryNodePlacement(node, nodes, geo::Left, guard)?;
                let padding = if boundary { boundaryPadding } else { outsidePadding };
                tl.X = math::Floor(labelTopLeft.X - padding);
            }
            if labelTopLeft.Y < tl.Y {
                let boundary = clusterBoundaryNodePlacement(node, nodes, geo::Top, guard)?;
                let padding = if boundary { boundaryPadding } else { outsidePadding };
                tl.Y = math::Floor(labelTopLeft.Y - padding);
            }
            if labelTopLeft.X > br.X {
                let boundary = clusterBoundaryNodePlacement(node, nodes, geo::Right, guard)?;
                let padding = if boundary { boundaryPadding } else { outsidePadding };
                br.X = math::Ceil(labelTopLeft.X + lw + padding);
            }
            if labelTopLeft.Y > br.Y {
                let boundary = clusterBoundaryNodePlacement(node, nodes, geo::Bottom, guard)?;
                let padding = if boundary { boundaryPadding } else { outsidePadding };
                br.Y = math::Ceil(labelTopLeft.Y + lh + padding);
            }
        }
    }
    if let Some(icon) = node.icon() {
        let ipos = icon.b().Position;
        if !node.IsImage() && ipos.IsOutside() {
            let iconSize = layoutgraph::MaxIconSize as f64;
            let iconTopLeft = *node.LabelTopLeft(ipos, iconSize, iconSize).b();
            let outsidePadding = 2.0 * LABEL_PADDING;
            tl.X = math::Min(tl.X, math::Floor(iconTopLeft.X - outsidePadding));
            tl.Y = math::Min(tl.Y, math::Floor(iconTopLeft.Y - outsidePadding));
            br.X = math::Max(br.X, math::Ceil(iconTopLeft.X + iconSize + outsidePadding));
            br.Y = math::Max(br.Y, math::Ceil(iconTopLeft.Y + iconSize + outsidePadding));
        }
    }
    Ok((tl, br))
}

pub(crate) fn clusterLabelPlacementOrientation(
    node: &NodeRef,
    member: &NodeRef,
    cluster: &layoutgraph::ClusterRef,
    guard: &mut labelPlacementWorkGuard,
) -> GoResult<(geo::Orientation, i64)> {
    let cnodes = cluster.b().Nodes.clone();
    let mut clusterTopLeft = geo::Point { X: math::Inf(-1), Y: math::Inf(-1) };
    let mut clusterBottomRight = geo::Point { X: math::Inf(1), Y: math::Inf(1) };
    if cnodes.len() > 0 {
        clusterTopLeft = geo::Point { X: math::Inf(1), Y: math::Inf(1) };
        clusterBottomRight = geo::Point { X: math::Inf(-1), Y: math::Inf(-1) };
        for clusterNode in &cnodes {
            let (topLeft, bottomRight) = clusterNodeBoundingBoxPlacement(clusterNode, &cnodes, guard)?;
            clusterTopLeft.X = math::Min(clusterTopLeft.X, topLeft.X);
            clusterTopLeft.Y = math::Min(clusterTopLeft.Y, topLeft.Y);
            clusterBottomRight.X = math::Max(clusterBottomRight.X, bottomRight.X);
            clusterBottomRight.Y = math::Max(clusterBottomRight.Y, bottomRight.Y);
        }
    }
    let shell = layoutgraph::NewNode(0, clusterBottomRight.X - clusterTopLeft.X, clusterBottomRight.Y - clusterTopLeft.Y);
    shell.set_tl(Some(geo::PointRef::new(clusterTopLeft)));
    let orientation = node.Orientation(&shell);
    let mut index = -1;
    for (candidateIndex, candidate) in cnodes.iter().enumerate() {
        guard.step()?;
        if candidate == member {
            index = candidateIndex as i64;
            break;
        }
    }
    Ok((orientation, index))
}

pub(crate) fn routeLengthPlacement(edge: &EdgeRef, guard: &mut labelPlacementWorkGuard) -> GoResult<f64> {
    let mut length = 0.0;
    let points = edge.points();
    for index in 1..points.len() {
        guard.step()?;
        let previous = *points[index - 1].b();
        let current = *points[index].b();
        length += geo::EuclideanDistance(previous.X, previous.Y, current.X, current.Y);
    }
    Ok(length)
}

pub(crate) fn edgeLabelTopLeft(
    edge: &EdgeRef,
    position: label::Position,
    width: f64,
    height: f64,
    guard: &mut labelPlacementWorkGuard,
) -> GoResult<Option<geo::PointRef>> {
    let n = edge.b().Points.len() as i64;
    for _pass in 0..2 {
        guard.add(n)?;
    }
    let point = edge.LabelTopLeft(position, width, height);
    guard.check()?;
    Ok(point)
}

pub(crate) fn positionedArrowheadLabel(
    edge: &EdgeRef,
    isTarget: bool,
    guard: &mut labelPlacementWorkGuard,
) -> GoResult<Option<PositionedArrowheadLabel>> {
    let n = edge.b().Points.len() as i64;
    for _pass in 0..3 {
        guard.add(n)?;
    }
    let positioned = PositionArrowheadLabel(edge, isTarget, &edge.points());
    guard.check()?;
    Ok(positioned)
}

#[derive(Clone)]
pub(crate) struct labelEdgeSortItem {
    pub(crate) edge: EdgeRef,
    pub(crate) length: f64,
    pub(crate) npoints: usize,
}

pub(crate) fn sortLabelPlacementEdges(edges: &[EdgeRef], guard: &mut labelPlacementWorkGuard) -> GoResult<Vec<EdgeRef>> {
    let mut items: Vec<labelEdgeSortItem> = Vec::with_capacity(edges.len());
    for edge in edges {
        guard.step()?;
        let length = routeLengthPlacement(edge, guard)?;
        items.push(labelEdgeSortItem { edge: edge.clone(), length, npoints: edge.b().Points.len() });
    }
    let mut buffer: Vec<labelEdgeSortItem> = items.clone();
    let less = |left: &labelEdgeSortItem, right: &labelEdgeSortItem| -> bool {
        if right.npoints == 2 && left.npoints > 2 {
            return true;
        }
        if left.npoints == 2 && right.npoints > 2 {
            return false;
        }
        left.length < right.length
    };
    let n = items.len();
    let mut width = 1usize;
    while width < n {
        let mut start = 0;
        while start < n {
            let middle = std::cmp::min(start + width, n);
            let end = std::cmp::min(start + 2 * width, n);
            let (mut left, mut right, mut output) = (start, middle, start);
            while left < middle && right < end {
                guard.step()?;
                if less(&items[right], &items[left]) {
                    buffer[output] = items[right].clone();
                    right += 1;
                } else {
                    buffer[output] = items[left].clone();
                    left += 1;
                }
                output += 1;
            }
            while left < middle {
                guard.step()?;
                buffer[output] = items[left].clone();
                left += 1;
                output += 1;
            }
            while right < end {
                guard.step()?;
                buffer[output] = items[right].clone();
                right += 1;
                output += 1;
            }
            start += 2 * width;
        }
        std::mem::swap(&mut items, &mut buffer);
        if width > n / 2 {
            break;
        }
        width *= 2;
    }
    let sorted = items.into_iter().map(|i| i.edge).collect();
    guard.check()?;
    Ok(sorted)
}

/// Segments are plain values here (Go's pointers are never shared outside).
pub(crate) fn sortSharedSegmentGroup(
    segments: &mut Vec<(geo::Point, geo::Point)>,
    coordinate: &dyn Fn(&geo::Point) -> f64,
    checkWork: &mut Option<&mut dyn FnMut() -> GoResult<()>>,
) -> GoResult<()> {
    let n = segments.len();
    if n < 2 {
        return Ok(());
    }
    let less = |left: &(geo::Point, geo::Point), right: &(geo::Point, geo::Point)| -> bool {
        if coordinate(&left.0) == coordinate(&right.0) {
            return coordinate(&left.1) < coordinate(&right.1);
        }
        coordinate(&left.0) < coordinate(&right.0)
    };
    let mut items = segments.clone();
    let mut buffer = segments.clone();
    let mut width = 1usize;
    while width < n {
        let mut start = 0;
        while start < n {
            let middle = std::cmp::min(start + width, n);
            let end = std::cmp::min(start + 2 * width, n);
            let (mut left, mut right, mut output) = (start, middle, start);
            while left < middle && right < end {
                if let Some(cw) = checkWork.as_mut() {
                    cw()?;
                }
                if less(&items[right], &items[left]) {
                    buffer[output] = items[right];
                    right += 1;
                } else {
                    buffer[output] = items[left];
                    left += 1;
                }
                output += 1;
            }
            while left < middle {
                if let Some(cw) = checkWork.as_mut() {
                    cw()?;
                }
                buffer[output] = items[left];
                left += 1;
                output += 1;
            }
            while right < end {
                if let Some(cw) = checkWork.as_mut() {
                    cw()?;
                }
                buffer[output] = items[right];
                right += 1;
                output += 1;
            }
            start += 2 * width;
        }
        std::mem::swap(&mut items, &mut buffer);
        if width > n / 2 {
            break;
        }
        width *= 2;
    }
    *segments = items;
    Ok(())
}
