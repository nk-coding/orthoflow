// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/placementcost/symmetry.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

/// NodesSymmetry sums local symmetry scores for nodes in order.
pub fn NodesSymmetry(ctx: &Ctx, nodes: &[NodeRef], edgeAbductions: Option<&[EdgeAbductionRef]>) -> GoResult<f64> {
    ctxCheck(ctx)?;
    let mut sum = 0.0;
    for (i, node) in nodes.iter().enumerate() {
        scoringCancellationError(ctx, i as i64)?;
        let symmetry = nodeSymmetry(ctx, node, edgeAbductions, true)?;
        sum += symmetry;
    }
    ctxCheck(ctx)?;
    Ok(sum)
}

/// NodeSymmetry evaluates a node and the neighboring symmetry it contributes to.
pub fn NodeSymmetry(ctx: &Ctx, node: &NodeRef, edgeAbductions: Option<&[EdgeAbductionRef]>) -> GoResult<f64> {
    nodeSymmetry(ctx, node, edgeAbductions, true)
}

/// Small insertion-ordered pointer set (Go `map[*Node]struct{}` used only for
/// membership).
#[derive(Default)]
struct nodeSet(Set<NodeRef>);

impl nodeSet {
    #[inline]
    fn has(&self, n: &NodeRef) -> bool {
        self.0.contains(n)
    }
    #[inline]
    fn add(&mut self, n: NodeRef) {
        self.0.insert(n);
    }
}

pub(crate) fn nodeSymmetry(ctx: &Ctx, node: &NodeRef, edgeAbductions: Option<&[EdgeAbductionRef]>, checkNeighbors: bool) -> GoResult<f64> {
    ctxCheck(ctx)?;
    let mut score = 0.0;
    let mut maxScore: i64 = 0;
    let abds: &[EdgeAbductionRef] = edgeAbductions.unwrap_or(&[]);
    let graph = node.g();

    let mut usedEdgeAbductions = vec![false; abds.len()];
    let edges = node.edges();
    let mut dedupedAdjacentNodes: Vec<NodeRef> = Vec::with_capacity(edges.len());
    let mut added = nodeSet::default();

    // We consider container children with protruding edges like neighbors for symmetry, their score matters for the container score
    if node.IsContainer() {
        let mut childNodes: Vec<NodeRef> = Vec::new();
        // Within node placement, we use edge abductions to discern children; otherwise, the AdjacentNodes are correct
        if edgeAbductions.is_none() {
            let children = graph.b().Containers.at(&Some(node.clone()));
            for (i, child) in children.iter().enumerate() {
                scoringCancellationError(ctx, i as i64)?;
                let childEdges = child.edges();
                for (j, e) in childEdges.iter().enumerate() {
                    scoringCancellationError(ctx, j as i64)?;
                    let adj = child.Adjacent(e);
                    // We only want to consider symmetry of child nodes with protrutions
                    if !layoutgraph::IsDescendantOf(adj.containerField().as_ref(), Some(node)) {
                        added.add(child.clone());
                        childNodes.push(child.clone());
                        break;
                    }
                }
            }
        } else {
            for (i, edgeAbduction) in abds.iter().enumerate() {
                scoringCancellationError(ctx, i as i64)?;
                if usedEdgeAbductions[i] {
                    continue;
                }
                let ea = edgeAbduction.b();
                if optIs(&ea.CurrentFrom, node) {
                    if let Some(of) = &ea.OriginallyFrom {
                        usedEdgeAbductions[i] = true;
                        if added.has(of) {
                            continue;
                        }
                        added.add(of.clone());
                        childNodes.push(of.clone());
                    }
                }
                if optIs(&ea.CurrentTo, node) {
                    if let Some(ot) = &ea.OriginallyTo {
                        usedEdgeAbductions[i] = true;
                        if added.has(ot) {
                            continue;
                        }
                        added.add(ot.clone());
                        childNodes.push(ot.clone());
                    }
                }
            }
        }

        for (i, nodeReplacement) in childNodes.iter().enumerate() {
            scoringCancellationError(ctx, i as i64)?;
            let symmetry = nodeSymmetry(ctx, nodeReplacement, edgeAbductions, checkNeighbors)?;
            score += symmetry;
            maxScore += 1;
        }
    }

    let isContainer = node.IsContainer();
    for (edgeIndex, e) in edges.iter().enumerate() {
        scoringCancellationError(ctx, edgeIndex as i64)?;
        let adjacentNode = node.Adjacent(e);
        let mut adjacentNodeReplacement = adjacentNode.clone();

        // We've already considered A's symmetry above, so we should only count B as an adjacent node if it's not connected to a child
        let mut isConnectedToChild = false;
        if isContainer {
            for (i, edgeAbduction) in abds.iter().enumerate() {
                scoringCancellationError(ctx, i as i64)?;
                let ea = edgeAbduction.b();
                if optIs(&ea.CurrentFrom, node) && optIs(&ea.CurrentTo, &adjacentNode) {
                    if ea.OriginallyFrom.is_some() {
                        isConnectedToChild = true;
                        break;
                    }
                }
                if optIs(&ea.CurrentFrom, &adjacentNode) && optIs(&ea.CurrentTo, node) {
                    usedEdgeAbductions[i] = true;
                    if ea.OriginallyTo.is_some() {
                        isConnectedToChild = true;
                        break;
                    }
                }
            }
        }
        if isConnectedToChild {
            continue;
        }

        // If we're considering symmetry for node B in the above diagram, we'll want to resolve to adjacent node to A, not container for A
        for (i, edgeAbduction) in abds.iter().enumerate() {
            scoringCancellationError(ctx, i as i64)?;
            if usedEdgeAbductions[i] {
                continue;
            }
            let ea = edgeAbduction.b();
            // For symmetry, use cluster vessels
            if let Some(cf) = &ea.CurrentFrom {
                if cf.IsClusterVessel() {
                    continue;
                }
            }
            if let Some(ct) = &ea.CurrentTo {
                if ct.IsClusterVessel() {
                    continue;
                }
            }
            if isSequenceVessel(&graph, &ea.CurrentFrom) || isSequenceVessel(&graph, &ea.CurrentTo) {
                continue;
            }
            if optIs(&ea.CurrentFrom, node) && optIs(&ea.CurrentTo, &adjacentNode) {
                usedEdgeAbductions[i] = true;
                if let Some(ot) = &ea.OriginallyTo {
                    adjacentNodeReplacement = ot.clone();
                }
                break;
            }
            if optIs(&ea.CurrentFrom, &adjacentNode) && optIs(&ea.CurrentTo, node) {
                usedEdgeAbductions[i] = true;
                if let Some(of) = &ea.OriginallyFrom {
                    adjacentNodeReplacement = of.clone();
                }
                break;
            }
        }

        // For clusters within containers, the Originally is pointed to the cluster node still
        let (cluster, sequence) = {
            let a = adjacentNodeReplacement.b();
            (a.Cluster.clone(), a.Sequence.clone())
        };
        if layoutgraph::clusterIsActive(cluster.as_ref()) {
            adjacentNodeReplacement = cluster.unwrap().b().Vessel.clone();
        } else if layoutgraph::sequenceIsActive(sequence.as_ref()) {
            adjacentNodeReplacement = sequence.unwrap().b().Vessel.clone();
        }

        if added.has(&adjacentNodeReplacement) {
            continue;
        }

        added.add(adjacentNodeReplacement.clone());
        dedupedAdjacentNodes.push(adjacentNodeReplacement);
    }

    // When we're resolving symmetry for children during node placement, all their edges were abducted to their containers, so here we find those
    for (i, edgeAbduction) in abds.iter().enumerate() {
        scoringCancellationError(ctx, i as i64)?;
        let ea = edgeAbduction.b();
        let mut connectedNode: Option<NodeRef> = None;
        if optIs(&ea.OriginallyFrom, node) {
            // When the adjacent node is also a container
            if ea.OriginallyTo.is_some()
                && ea.CurrentTo.is_some()
                && !ea.CurrentTo.as_ref().unwrap().IsClusterVessel()
                && !isSequenceVessel(&graph, &ea.CurrentTo)
            {
                connectedNode = ea.OriginallyTo.clone();
            } else {
                connectedNode = ea.CurrentTo.clone();
            }
        }
        if optIs(&ea.OriginallyTo, node) {
            if ea.OriginallyFrom.is_some()
                && ea.CurrentFrom.is_some()
                && !ea.CurrentFrom.as_ref().unwrap().IsClusterVessel()
                && !isSequenceVessel(&graph, &ea.CurrentFrom)
            {
                connectedNode = ea.OriginallyFrom.clone();
            } else {
                connectedNode = ea.CurrentFrom.clone();
            }
        }
        if let Some(connectedNode) = connectedNode {
            if added.has(&connectedNode) {
                continue;
            }

            added.add(connectedNode.clone());
            dedupedAdjacentNodes.push(connectedNode);
        }
    }

    let mut i: i64 = 0;
    while i < dedupedAdjacentNodes.len() as i64 {
        scoringCancellationError(ctx, i)?;
        let d = node.DistanceTo(&dedupedAdjacentNodes[i as usize], true);
        if d > 1200.0 {
            dedupedAdjacentNodes.remove(i as usize);
            i -= 1;
        }
        i += 1;
    }

    let mut matched = vec![false; dedupedAdjacentNodes.len()];
    let s = computeSymmetryScoreInto(ctx, node, &dedupedAdjacentNodes, &mut matched)?;
    score += s;
    maxScore += dedupedAdjacentNodes.len() as i64;

    // Node A may not have symmetry itself, but we still don't want it to move because it contributes to B's symmetry
    if checkNeighbors {
        for (i, adjacentNode) in dedupedAdjacentNodes.iter().enumerate() {
            scoringCancellationError(ctx, i as i64)?;
            if matched[i] {
                continue;
            }
            let adjNodeSymm = nodeSymmetry(ctx, adjacentNode, edgeAbductions, false)?;
            score += adjNodeSymm;
        }
    }

    // No adjacent nodes, not a container of a child with protruding edges
    if maxScore == 0 {
        return Ok(0.0);
    }
    ctxCheck(ctx)?;
    Ok(score / maxScore as f64)
}

#[inline]
fn isSequenceVessel(graph: &GraphRef, n: &Option<NodeRef>) -> bool {
    match n {
        Some(n) => graph.IsSequenceVessel(n),
        None => false,
    }
}

/// computeSymmetryScoreInto writes match state into caller-owned storage.
pub(crate) fn computeSymmetryScoreInto(ctx: &Ctx, node: &NodeRef, neigh: &[NodeRef], matchedSlice: &mut [bool]) -> GoResult<f64> {
    ctxCheck(ctx)?;
    let n = neigh.len();
    if n < 2 {
        return Ok(0.0);
    }
    let matchedSlice = &mut matchedSlice[..n];
    matchedSlice.iter_mut().for_each(|m| *m = false);

    let mut score = 0.0;

    let graph = node.g();
    let containers = graph.b().Containers.clone();
    let nodeContainer = node.containerField();
    let ownSiblings = containers.at(&nodeContainer);
    let mut otherSiblings: Vec<NodeRef>;
    let ntl = node.tlv();
    let (nw, nh) = (node.w(), node.h());

    for i in 0..n {
        scoringCancellationError(ctx, i as i64)?;
        if matchedSlice[i] {
            continue;
        }
        let n1 = &neigh[i];
        let n1Container = n1.containerField();
        if n1Container != nodeContainer {
            otherSiblings = containers.at(&n1Container);
        } else {
            otherSiblings = Vec::new();
        }

        let mut bestIdx: i64 = -1;
        let mut best = 0.0;

        for j in (i + 1)..n {
            scoringCancellationError(ctx, (j - i - 1) as i64)?;
            if matchedSlice[j] {
                continue;
            }
            let n2 = &neigh[j];
            if n1Container != n2.containerField() {
                continue;
            }
            // When nodes vary by a lot in size, it also doesn't come off as symmetrical
            let (area1, area2) = (n1.Area(), n2.Area());
            if area1 > 2.0 * area2 || area2 > 2.0 * area1 {
                continue;
            }

            let mut ms = f64::NEG_INFINITY;
            // The two nodes can be symmetrical across the x or y axis
            for isX in [true, false] {
                let mut axis = ntl.X + nw / 2.0;
                if !isX {
                    axis = ntl.Y + nh / 2.0;
                }
                if isMirrored(n1, n2, isX, axis) {
                    if node.OverlapsAlongDimension(n1, isX, true) && node.OverlapsAlongDimension(n2, isX, true) {
                        ms = 2.0;
                    } else {
                        ms = 0.5;
                    }
                    break;
                }
            }
            // skip obstruction search if the nodes aren't aligned
            if ms < 0.0 {
                continue;
            }

            // We only want to count mirrored objects when they have no obstructions from node to it
            let isObstructed = obstructed(ctx, node, n1, n2, &ownSiblings, &otherSiblings)?;
            if isObstructed {
                continue;
            }

            if ms > best {
                best = ms;
                bestIdx = j as i64;
                if ms == 2.0 {
                    break;
                }
            }
        }

        if bestIdx != -1 {
            matchedSlice[i] = true;
            matchedSlice[bestIdx as usize] = true;
            score += best;
        }
    }

    ctxCheck(ctx)?;
    Ok(score)
}

fn obstructed(ctx: &Ctx, center: &NodeRef, a: &NodeRef, b: &NodeRef, sib1: &[NodeRef], sib2: &[NodeRef]) -> GoResult<bool> {
    let bounds = scoringNodeBounds(Some(center)).including(scoringNodeBounds(Some(a))).including(scoringNodeBounds(Some(b)));
    let centerGraph = center.graph();
    let check = |sibs: &[NodeRef]| -> GoResult<bool> {
        for (i, s) in sibs.iter().enumerate() {
            scoringCancellationError(ctx, i as i64)?;
            if !s.has_tl() || s.graph() != centerGraph {
                continue;
            }
            if s == center || s == a || s == b {
                continue;
            }
            if bounds.excludes(Some(s)) {
                continue;
            }
            if center.IsDescendantOf(Some(s)) || a.IsDescendantOf(Some(s)) || b.IsDescendantOf(Some(s)) {
                continue;
            }
            if s.IsDescendantOf(Some(center)) || s.IsDescendantOf(Some(a)) || s.IsDescendantOf(Some(b)) {
                continue;
            }
            let cc = *center.Center().b();
            if passes(s, cc, *a.Center().b()) || passes(s, cc, *b.Center().b()) {
                return Ok(true);
            }
        }
        Ok(false)
    };
    let isObstructed = check(sib1)?;
    if isObstructed {
        return Ok(isObstructed);
    }
    check(sib2)
}

/// ColumnCrossingCost scores crossings between labeled table-column edges.
pub fn ColumnCrossingCost(ctx: &Ctx, node: &NodeRef, edgeAbductions: Option<&[EdgeAbductionRef]>) -> GoResult<f64> {
    ctxCheck(ctx)?;
    if !node.IsTable() {
        return Ok(0.0);
    }

    let abds: &[EdgeAbductionRef] = edgeAbductions.unwrap_or(&[]);
    let mut edgeToAbduction: Map<EdgeRef, EdgeAbductionRef> = Map::default();
    for (i, abduction) in abds.iter().enumerate() {
        scoringCancellationError(ctx, i as i64)?;
        let e = abduction.b().Edge.clone();
        edgeToAbduction.insert(e, abduction.clone());
    }

    let mut segments: Vec<layoutgraph::CrossingSegment> = Vec::new();
    let edges = node.edges();
    for (i, e) in edges.iter().enumerate() {
        scoringCancellationError(ctx, i as i64)?;
        let (fromPort, toPort, hasFromPort, hasToPort, _) = if let Some(abduction) = edgeToAbduction.get(e) {
            let ab = abduction.b();
            e.FacingTablePortValues(ab.OriginallyFrom.as_ref(), ab.OriginallyTo.as_ref())
        } else {
            e.FacingTablePortValues(None, None)
        };

        if !hasFromPort || !hasToPort {
            continue;
        }
        segments.push(layoutgraph::CrossingSegment { Start: fromPort, End: toPort });
    }

    let crossings = layoutgraph::CountSegmentCrossingsContext(ctx, &segments)?;
    Ok(layoutgraph::CrossingCostWeight * crossings as f64 * node.g().b().CellSize)
}

/// isMirrored checks if nodeA is mirrored to nodeB along the given axis line
/// isXAxis = true means they are mirrored along a vertical line
pub(crate) fn isMirrored(nodeA: &NodeRef, nodeB: &NodeRef, isXAxis: bool, axisVal: f64) -> bool {
    let (a, aw, ah) = (nodeA.tlv(), nodeA.w(), nodeA.h());
    let (b, bw, bh) = (nodeB.tlv(), nodeB.w(), nodeB.h());
    let nodeAtlX = a.X;
    let nodeAtlY = a.Y;
    let nodeAbrX = a.X + aw;
    let nodeAbrY = a.Y + ah;

    let nodeBtlX = b.X;
    let nodeBtlY = b.Y;
    let nodeBbrX = b.X + bw;
    let nodeBbrY = b.Y + bh;

    let symmetryTolerance = symmetryToleranceBand * nodeA.g().b().CellSize;
    if isXAxis {
        if a.X == b.X {
            return false;
        }

        // The left edge of node A should be on other side of axisVal from right edge of node B
        if a.X > b.X {
            // Axis val must be in between
            if !((axisVal > nodeBbrX) && (axisVal < nodeAtlX)) {
                return false;
            }
            // Their distances from the axis line should be roughly equal
            if ((nodeAtlX - axisVal) - (axisVal - nodeBbrX)).abs() > symmetryTolerance {
                return false;
            }
        } else if a.X < b.X {
            if !((axisVal > nodeAbrX) && (axisVal < nodeBtlX)) {
                return false;
            }
            if ((nodeBtlX - axisVal) - (axisVal - nodeAbrX)).abs() > symmetryTolerance {
                return false;
            }
        }

        // Centers match
        return (((nodeAtlY + nodeAbrY) / 2.0) - ((nodeBtlY + nodeBbrY) / 2.0)).abs() <= symmetryTolerance;
    }
    if a.Y == b.Y {
        return false;
    }
    if a.Y > b.Y {
        if !((axisVal > nodeBbrY) && (axisVal < nodeAtlY)) {
            return false;
        }
        if ((nodeAtlY - axisVal) - (axisVal - nodeBbrY)).abs() > symmetryTolerance {
            return false;
        }
    } else if a.Y < b.Y {
        if !((axisVal > nodeAbrY) && (axisVal < nodeBtlY)) {
            return false;
        }
        if ((nodeBtlY - axisVal) - (axisVal - nodeAbrY)).abs() > symmetryTolerance {
            return false;
        }
    }

    // Centers match
    (((nodeAtlX + nodeAbrX) / 2.0) - ((nodeBtlX + nodeBbrX) / 2.0)).abs() <= symmetryTolerance
}
