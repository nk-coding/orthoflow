// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/quality/labels.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

const LABEL_PADDING: f64 = label::PADDING as f64;

/// Plain value view of a `geo.Box` (TopLeft dereferenced; Go would panic on a
/// nil TopLeft at first use, so do we).
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct qbox {
    pub(crate) X: f64,
    pub(crate) Y: f64,
    pub(crate) Width: f64,
    pub(crate) Height: f64,
}

impl qbox {
    #[track_caller]
    pub(crate) fn of(b: &geo::Box) -> qbox {
        let tl = *b.TopLeft.as_ref().expect("nil pointer dereference: box.TopLeft").b();
        qbox { X: tl.X, Y: tl.Y, Width: b.Width, Height: b.Height }
    }
    #[track_caller]
    pub(crate) fn new(tl: Option<geo::PointRef>, w: f64, h: f64) -> qbox {
        let tl = *tl.as_ref().expect("nil pointer dereference: box.TopLeft").b();
        qbox { X: tl.X, Y: tl.Y, Width: w, Height: h }
    }
    pub(crate) fn node(n: &NodeRef) -> qbox {
        qbox::of(&n.boxValue())
    }
}

pub(crate) fn scoreExistingLabelPlacements(g: &GraphRef, guard: &WorkGuardRef) -> GoResult<f64> {
    guard.Step()?;

    let (gnodes, gedges, containers) = {
        let gb = g.b();
        (gb.Nodes.clone(), gb.Edges.clone(), gb.Containers.clone())
    };
    let mut placedBoxes: Vec<qbox> = Vec::new();
    let mut totalScore = 0.0;

    for edge in &gedges {
        guard.Step()?;
        let n = edge.b().Points.len() as i64;
        if edge.b().SourceArrowheadLabel.is_some() {
            chargeEvaluationWork(guard, n)?;
            let pal = labeling::PositionArrowheadLabel(edge, false, &edge.points()).expect("nil arrowhead label");
            placedBoxes.push(qbox::of(&pal.Box));
        }
        if edge.b().TargetArrowheadLabel.is_some() {
            chargeEvaluationWork(guard, n)?;
            let pal = labeling::PositionArrowheadLabel(edge, true, &edge.points()).expect("nil arrowhead label");
            placedBoxes.push(qbox::of(&pal.Box));
        }
    }

    for edge in &gedges {
        guard.Step()?;
        if edge.IsLoop() && edge.b().Label.is_some() {
            chargeEvaluationWork(guard, edge.b().Points.len() as i64)?;
            placedBoxes.push(labelBox(edge));
        }
    }

    for node in &gnodes {
        guard.Step()?;

        if let Some(icon) = node.icon() {
            if !node.IsImage() {
                let ipos = icon.b().Position;
                let iconSize = node.IconSize(ipos);
                let iconBox = qbox::new(Some(node.LabelTopLeft(ipos, iconSize, iconSize)), iconSize, iconSize);
                placedBoxes.push(iconBox);

                let nodeOverlaps = nodeOverlapCount(iconBox, &gnodes, 0.0, guard)?;
                let edgeOverlaps = edgeOverlapCount(iconBox, &gedges, 0.0, guard)?;
                let labelOverlaps = boxOverlapCount(iconBox, &placedBoxes, 0.0, guard)?;
                totalScore += scoreNodeLabelOverlaps(nodeOverlaps, edgeOverlaps, labelOverlaps - 1);
            }
        }

        let Some(lbl) = node.label() else { continue };
        let (lpos, lw, lh) = {
            let l = lbl.b();
            (l.Position, l.Width, l.Height)
        };
        let nodeLabelBox = qbox::new(Some(node.LabelTopLeft(lpos, lw, lh)), lw, lh);

        let mut siblingsAndChildren: Vec<NodeRef> = Vec::new();
        for sibling in &containers.at(&node.containerField()) {
            guard.Step()?;
            if sibling != node {
                siblingsAndChildren.push(sibling.clone());
            }
        }
        if node.IsContainer() {
            for child in &containers.at(&Some(node.clone())) {
                guard.Step()?;
                siblingsAndChildren.push(child.clone());
            }
        }

        let mut ancestors: Vec<NodeRef> = Vec::new();
        let mut current = Some(node.clone());
        let mut depth = 0i64;
        while let Some(cur) = current {
            guard.Step()?;
            if depth >= layoutgraph::MaxTopologyDepth {
                return Err(errorf!("TALA Evaluate container depth exceeds limit {}", layoutgraph::MaxTopologyDepth));
            }
            ancestors.push(cur.clone());
            current = cur.EffectiveContainer();
            depth += 1;
        }

        let siblingOverlaps = nodeOverlapCount(nodeLabelBox, &siblingsAndChildren, LABEL_PADDING, guard)?;
        let ancestorOverlaps = partialNodeOverlapCount(nodeLabelBox, &ancestors, LABEL_PADDING, guard)?;
        let edgeOverlaps = edgeOverlapCount(nodeLabelBox, &gedges, LABEL_PADDING - 1.0, guard)?;
        let labelOverlaps = boxOverlapCount(nodeLabelBox, &placedBoxes, LABEL_PADDING, guard)?;
        totalScore += scoreNodeLabelOverlaps(siblingOverlaps + ancestorOverlaps, edgeOverlaps, labelOverlaps);
        placedBoxes.push(nodeLabelBox);
    }

    let sharedSegments = findSharedSegments(&gedges, guard)?;
    let mut sharedSegmentBoxes: Vec<qbox> = Vec::with_capacity(sharedSegments.len());
    for segment in &sharedSegments {
        guard.Step()?;
        let (start, end) = *segment;
        let mut topLeft = start;
        let mut width = end.X - start.X;
        let mut height = end.Y - start.Y;
        if end.X == start.X {
            width = 2.0 * labeling::SharedSegmentClearance;
            topLeft.X -= labeling::SharedSegmentClearance;
        } else {
            height = 2.0 * labeling::SharedSegmentClearance;
            topLeft.Y -= labeling::SharedSegmentClearance;
        }
        sharedSegmentBoxes.push(qbox { X: topLeft.X, Y: topLeft.Y, Width: width, Height: height });
    }

    for edge in &gedges {
        guard.Step()?;
        if edge.b().Label.is_none() || edge.IsLoop() {
            continue;
        }
        chargeEvaluationWork(guard, edge.b().Points.len() as i64)?;
        let edgeLabelBox = labelBox(edge);

        let mut otherEdges: Vec<EdgeRef> = Vec::with_capacity(gedges.len().saturating_sub(1));
        for other in &gedges {
            guard.Step()?;
            if other != edge {
                otherEdges.push(other.clone());
            }
        }

        let (from, to) = (edge.from(), edge.to());
        let mut ancestors: Vec<NodeRef> = Vec::new();
        let mut nonAncestors: Vec<NodeRef> = Vec::new();
        for node in &gnodes {
            guard.Step()?;
            let fromDescendant = evaluationIsDescendantOf(Some(&from), Some(node), guard)?;
            let toDescendant = evaluationIsDescendantOf(Some(&to), Some(node), guard)?;
            if *node != from && *node != to && (fromDescendant || toDescendant) {
                ancestors.push(node.clone());
            } else {
                nonAncestors.push(node.clone());
            }
        }

        let ancestorOverlapArea = nodeOverlapArea(edgeLabelBox, &ancestors, LABEL_PADDING, true, guard)?;
        let nonAncestorOverlapArea = nodeOverlapArea(edgeLabelBox, &nonAncestors, LABEL_PADDING, false, guard)?;
        let edgeOverlaps = edgeOverlapCount(edgeLabelBox, &otherEdges, LABEL_PADDING, guard)?;
        let labelOverlaps = boxOverlapCount(edgeLabelBox, &placedBoxes, 0.0, guard)?;
        let almostLabelOverlaps = boxOverlapCount(edgeLabelBox, &placedBoxes, LABEL_PADDING, guard)?;
        let sharedSegmentOverlaps = boxOverlapCount(edgeLabelBox, &sharedSegmentBoxes, LABEL_PADDING, guard)?;

        totalScore += scoreEdgeLabelOverlaps(
            edgeLabelBox.Width * edgeLabelBox.Height,
            ancestorOverlapArea + nonAncestorOverlapArea,
            0,
            edgeOverlaps,
            almostLabelOverlaps,
            labelOverlaps,
            sharedSegmentOverlaps,
        );
        placedBoxes.push(edgeLabelBox);
    }

    Ok(1.0 / (1.0 + totalScore))
}

pub(crate) fn labelBox(edge: &EdgeRef) -> qbox {
    let lbl = edge.b().Label.clone().expect("nil pointer dereference: edge.Label");
    let (pos, w, h) = {
        let l = lbl.b();
        (l.Position, l.Width, l.Height)
    };
    qbox::new(edge.LabelTopLeft(pos, w, h), w, h)
}

pub(crate) fn nodeOverlapCount(bx: qbox, nodes: &[NodeRef], padding: f64, guard: &WorkGuardRef) -> GoResult<i64> {
    let mut count = 0;
    for node in nodes {
        guard.Step()?;
        if boxesOverlapWithPadding(bx, qbox::node(node), padding) {
            count += 1;
        }
    }
    Ok(count)
}

pub(crate) fn partialNodeOverlapCount(bx: qbox, nodes: &[NodeRef], padding: f64, guard: &WorkGuardRef) -> GoResult<i64> {
    let mut count = 0;
    for node in nodes {
        guard.Step()?;
        let nb = qbox::node(node);
        if boxesOverlapWithPadding(bx, nb, padding) && !boxCovers(nb, bx) {
            count += 1;
        }
    }
    Ok(count)
}

pub(crate) fn boxOverlapCount(bx: qbox, others: &[qbox], padding: f64, guard: &WorkGuardRef) -> GoResult<i64> {
    let mut count = 0;
    for other in others {
        guard.Step()?;
        if boxesOverlapWithPadding(bx, *other, padding) {
            count += 1;
        }
    }
    Ok(count)
}

pub(crate) fn nodeOverlapArea(bx: qbox, nodes: &[NodeRef], padding: f64, partial: bool, guard: &WorkGuardRef) -> GoResult<f64> {
    let mut area = 0.0;
    for node in nodes {
        guard.Step()?;
        let nb = qbox::node(node);
        if !boxesOverlapWithPadding(bx, nb, padding) || partial && boxCovers(nb, bx) {
            continue;
        }
        if padding == 0.0 || boxesOverlapWithPadding(bx, nb, 0.0) {
            area += boxOverlapArea(bx, nb);
        }
        if !node.IsContainer() {
            area += bx.Width * bx.Height;
        }
    }
    Ok(area)
}

pub(crate) fn edgeOverlapCount(bx: qbox, edges: &[EdgeRef], padding: f64, guard: &WorkGuardRef) -> GoResult<i64> {
    let mut count = 0;
    for edge in edges {
        let e = edge.b();
        for pointIndex in 1..e.Points.len() {
            guard.Step()?;
            let a = *e.Points[pointIndex - 1].b();
            let b = *e.Points[pointIndex].b();
            if boxOverlapsLine(bx, &a, &b, padding) {
                count += 1;
            }
        }
    }
    Ok(count)
}

pub(crate) fn boxesOverlapWithPadding(first: qbox, second: qbox, padding: f64) -> bool {
    let firstRight = first.X + first.Width;
    let secondRight = second.X + second.Width;
    if first.X >= secondRight + padding || second.X >= firstRight + padding {
        return false;
    }
    let firstBottom = first.Y + first.Height;
    let secondBottom = second.Y + second.Height;
    first.Y < secondBottom + padding && second.Y < firstBottom + padding
}

pub(crate) fn boxCovers(outer: qbox, inner: qbox) -> bool {
    inner.X >= outer.X
        && inner.Y >= outer.Y
        && inner.X + inner.Width <= outer.X + outer.Width
        && inner.Y + inner.Height <= outer.Y + outer.Height
}

pub(crate) fn boxOverlapArea(first: qbox, second: qbox) -> f64 {
    let mut xs = [first.X, first.X + first.Width, second.X, second.X + second.Width];
    let mut ys = [first.Y, first.Y + first.Height, second.Y, second.Y + second.Height];
    gocompat::slices::Sort(&mut xs);
    gocompat::slices::Sort(&mut ys);
    (xs[2] - xs[1]) * (ys[2] - ys[1])
}

pub(crate) fn boxOverlapsLine(bx: qbox, first: &geo::Point, second: &geo::Point, padding: f64) -> bool {
    let contains = |point: &geo::Point| -> bool {
        bx.X - padding <= point.X
            && bx.X + bx.Width + padding >= point.X
            && bx.Y - padding <= point.Y
            && bx.Y + bx.Height + padding >= point.Y
    };
    if contains(first) || contains(second) {
        return true;
    }

    let left = bx.X - padding;
    let right = bx.X + bx.Width + padding;
    let top = bx.Y - padding;
    let bottom = bx.Y + bx.Height + padding;
    let topLeft = geo::Point { X: left, Y: top };
    let topRight = geo::Point { X: right, Y: top };
    let bottomRight = geo::Point { X: right, Y: bottom };
    let bottomLeft = geo::Point { X: left, Y: bottom };
    segmentsIntersect(&topLeft, &topRight, first, second)
        || segmentsIntersect(&topRight, &bottomRight, first, second)
        || segmentsIntersect(&bottomRight, &bottomLeft, first, second)
        || segmentsIntersect(&bottomLeft, &topLeft, first, second)
}

pub(crate) fn segmentsIntersect(firstStart: &geo::Point, firstEnd: &geo::Point, secondStart: &geo::Point, secondEnd: &geo::Point) -> bool {
    let secondStartSide = orientation(firstStart, firstEnd, secondStart);
    let secondEndSide = orientation(firstStart, firstEnd, secondEnd);
    let firstStartSide = orientation(secondStart, secondEnd, firstStart);
    let firstEndSide = orientation(secondStart, secondEnd, firstEnd);
    if secondStartSide == 0.0 && secondEndSide == 0.0 && firstStartSide == 0.0 && firstEndSide == 0.0 {
        return closedIntervalsOverlap(firstStart.X, firstEnd.X, secondStart.X, secondEnd.X)
            && closedIntervalsOverlap(firstStart.Y, firstEnd.Y, secondStart.Y, secondEnd.Y);
    }
    straddlesLine(secondStartSide, secondEndSide) && straddlesLine(firstStartSide, firstEndSide)
}

pub(crate) fn straddlesLine(first: f64, second: f64) -> bool {
    first == 0.0 || second == 0.0 || (first < 0.0) != (second < 0.0)
}

pub(crate) fn closedIntervalsOverlap(firstStart: f64, firstEnd: f64, secondStart: f64, secondEnd: f64) -> bool {
    let (firstMin, firstMax) = (math::Min(firstStart, firstEnd), math::Max(firstStart, firstEnd));
    let (secondMin, secondMax) = (math::Min(secondStart, secondEnd), math::Max(secondStart, secondEnd));
    firstMin <= secondMax && secondMin <= firstMax
}

pub(crate) fn scoreNodeLabelOverlaps(nodeOverlaps: i64, edgeOverlaps: i64, labelOverlaps: i64) -> f64 {
    (nodeOverlaps + edgeOverlaps + 2 * labelOverlaps) as f64
}

pub(crate) fn scoreEdgeLabelOverlaps(
    labelArea: f64,
    nodeOverlapArea: f64,
    nodeOverlapCount: i64,
    edgeOverlapCount: i64,
    almostLabelOverlapCount: i64,
    labelOverlapCount: i64,
    sharedSegmentOverlapCount: i64,
) -> f64 {
    let mut score = nodeOverlapArea / labelArea * 2.0;
    score += labelOverlapCount as f64 * 10.0;
    score += almostLabelOverlapCount as f64;
    score += nodeOverlapCount as f64 * 2.0;
    score += edgeOverlapCount as f64 * 2.0;
    score += sharedSegmentOverlapCount as f64;
    score
}

type seg = (geo::Point, geo::Point);

/// Go map[float64][]*geo.Segment iteration order is random; the result is only
/// counted against (order-independent), so first-insertion order is used.
fn groupPush(groups: &mut Vec<(f64, Vec<seg>)>, index: &mut Map<u64, usize>, coord: f64, s: seg) {
    let key = if coord.is_nan() {
        None
    } else if coord == 0.0 {
        Some(0u64)
    } else {
        Some(coord.to_bits())
    };
    match key {
        Some(k) => {
            if let Some(&i) = index.get(&k) {
                groups[i].1.push(s);
            } else {
                index.insert(k, groups.len());
                groups.push((coord, vec![s]));
            }
        }
        None => groups.push((coord, vec![s])),
    }
}

pub(crate) fn findSharedSegments(edges: &[EdgeRef], guard: &WorkGuardRef) -> GoResult<Vec<seg>> {
    guard.Step()?;
    let mut vertical: Vec<(f64, Vec<seg>)> = Vec::new();
    let mut horizontal: Vec<(f64, Vec<seg>)> = Vec::new();
    let mut vIndex: Map<u64, usize> = Map::default();
    let mut hIndex: Map<u64, usize> = Map::default();
    for edge in edges {
        guard.Step()?;
        let points = edge.points();
        let n = points.len() as i64;
        let mut i = 0i64;
        while i < n - 1 {
            guard.Step()?;
            let mut start = *points[i as usize].b();
            let mut end = *points[i as usize + 1].b();
            i += 1;
            if start.X == end.X {
                if end.Y < start.Y {
                    std::mem::swap(&mut end.Y, &mut start.Y);
                }
                groupPush(&mut vertical, &mut vIndex, start.X, (start, end));
            } else if start.Y == end.Y {
                if end.X < start.X {
                    std::mem::swap(&mut end.X, &mut start.X);
                }
                groupPush(&mut horizontal, &mut hIndex, start.Y, (start, end));
            }
        }
    }

    let mut shared: Vec<seg> = Vec::with_capacity(std::cmp::max(vertical.len(), horizontal.len()));
    let mut addGroups = |groups: &mut Vec<(f64, Vec<seg>)>, horizontal: bool| -> GoResult<()> {
        let coordinate = |p: &geo::Point| -> f64 {
            if horizontal {
                p.X
            } else {
                p.Y
            }
        };
        let setCoordinate = |p: &mut geo::Point, v: f64| {
            if horizontal {
                p.X = v;
            } else {
                p.Y = v;
            }
        };
        for (_, segments) in groups.iter_mut() {
            guard.Step()?;
            if segments.len() == 1 {
                continue;
            }
            sortSharedSegments(segments, &coordinate, guard)?;

            let mut previous = segments[0];
            let mut overlap: Option<seg> = None;
            for i in 1..segments.len() {
                guard.Step()?;
                let current = segments[i];
                if coordinate(&previous.1) > coordinate(&current.0) {
                    match overlap.as_mut() {
                        None => {
                            let mut o = (current.0, current.0);
                            setCoordinate(&mut o.1, math::Min(coordinate(&previous.1), coordinate(&current.1)));
                            overlap = Some(o);
                        }
                        Some(o) => {
                            let minimumOverlap = math::Min(coordinate(&previous.1), coordinate(&current.1));
                            let v = math::Max(coordinate(&o.1), minimumOverlap);
                            setCoordinate(&mut o.1, v);
                        }
                    }
                } else if let Some(o) = overlap {
                    shared.push(o);
                    overlap = None;
                }
                if coordinate(&current.1) > coordinate(&previous.1) {
                    previous = current;
                }
            }
            if let Some(o) = overlap {
                shared.push(o);
            }
        }
        Ok(())
    };
    addGroups(&mut vertical, false)?;
    addGroups(&mut horizontal, true)?;
    Ok(shared)
}

pub(crate) fn sortSharedSegments(segments: &mut Vec<seg>, coordinate: &dyn Fn(&geo::Point) -> f64, guard: &WorkGuardRef) -> GoResult<()> {
    let n = segments.len();
    if n < 2 {
        return Ok(());
    }
    let less = |left: &seg, right: &seg| -> bool {
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
                guard.Step()?;
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
                guard.Step()?;
                buffer[output] = items[left];
                left += 1;
                output += 1;
            }
            while right < end {
                guard.Step()?;
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
