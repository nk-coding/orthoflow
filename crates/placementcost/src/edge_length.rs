// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/placementcost/edge_length.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

/// EdgeLengthOptions controls which geometric penalties contribute to edge length.
///
/// `EdgeAbductions: None` is Go's nil slice (NodeEdgeLength then gathers the
/// graph's cluster abductions); `Some(vec![])` is an empty non-nil slice.
#[derive(Clone, Default)]
pub struct EdgeLengthOptions {
    /// EdgeAbductions supplies temporary endpoint ownership during placement.
    pub EdgeAbductions: Option<Vec<EdgeAbductionRef>>,
    /// IncludeNodeSizes measures distances between node boundaries instead of origins.
    pub IncludeNodeSizes: bool,
    /// EnforceMinimumGap penalizes connected nodes that are too close together.
    pub EnforceMinimumGap: bool,
    /// PenalizeDirection adds cost when an edge departs from its preferred direction.
    pub PenalizeDirection: bool,
}

/// NodesEdgeLength sums the edge-length cost rooted at each node in order.
pub fn NodesEdgeLength(ctx: &Ctx, nodes: &[NodeRef], options: &EdgeLengthOptions) -> GoResult<f64> {
    ctxCheck(ctx)?;
    let mut result = 0.0;
    for node in nodes {
        let length = NodeEdgeLength(ctx, node, options)?;
        result += length;
    }
    ctxCheck(ctx)?;
    Ok(result)
}

#[derive(Default)]
pub(crate) struct edgeScratch {
    pub(crate) nRepl: Vec<NodeRef>,
    pub(crate) aRepl: Vec<NodeRef>,
    pub(crate) labeledEdgeCount: Map<labeledEdgePair, i64>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct labeledEdgePair {
    pub(crate) from: layoutgraph::EntityID,
    pub(crate) to: layoutgraph::EntityID,
}

/// The prepare phase's cancellation checks. `count` mirrors the Go
/// edgeLengthPreparationContext Err-call counter used by the scorer.
pub(crate) struct prepChecker<'a> {
    pub(crate) ctx: &'a Ctx,
    pub(crate) count: i64,
}

impl<'a> prepChecker<'a> {
    #[inline]
    fn scoringCancellationError(&mut self, iteration: i64) -> GoResult<()> {
        if iteration % scoringCancellationCheckInterval != 0 {
            return Ok(());
        }
        self.count += 1;
        checkScoringCancellation(self.ctx)
    }

    fn checkScoringSlice(&mut self, length: usize) -> GoResult<()> {
        let mut start = 0i64;
        while start < length as i64 {
            self.scoringCancellationError(start)?;
            start += scoringCancellationCheckInterval;
        }
        Ok(())
    }
}

#[inline]
pub(crate) fn optIs(a: &Option<NodeRef>, b: &NodeRef) -> bool {
    match a {
        Some(a) => a == b,
        None => false,
    }
}

/// Helper function to calculate midpoint and check semi-diagonal for horizontal directions
fn calculateHorizontalMidpoint(nodeReplacement: &NodeRef, adjacentNodeReplacement: &NodeRef) -> (f64, bool) {
    let (n, nw) = (nodeReplacement.tlv(), nodeReplacement.w());
    let (a, aw) = (adjacentNodeReplacement.tlv(), adjacentNodeReplacement.w());
    let ceil = math::Min(n.X + nw, a.X + aw);
    let floor = math::Max(n.X, a.X);
    let mid = (floor + ceil) / 2.0;
    let isSemiDiagonal = (n.X - a.X).abs() > SideEdgeSpacing || (n.X + nw - (a.X + aw)).abs() > SideEdgeSpacing;
    (mid, isSemiDiagonal)
}

/// Helper function to calculate midpoint and check semi-diagonal for vertical directions
fn calculateVerticalMidpoint(nodeReplacement: &NodeRef, adjacentNodeReplacement: &NodeRef) -> (f64, bool) {
    let (n, nh) = (nodeReplacement.tlv(), nodeReplacement.h());
    let (a, ah) = (adjacentNodeReplacement.tlv(), adjacentNodeReplacement.h());
    let ceil = math::Min(n.Y + nh, a.Y + ah);
    let floor = math::Max(n.Y, a.Y);
    let mid = (floor + ceil) / 2.0;
    let isSemiDiagonal = (n.Y - a.Y).abs() > SideEdgeSpacing || (n.Y + nh - (a.Y + ah)).abs() > SideEdgeSpacing;
    (mid, isSemiDiagonal)
}

/// NodeEdgeLength evaluates the incident-edge cost rooted at node.
pub fn NodeEdgeLength(ctx: &Ctx, node: &NodeRef, options: &EdgeLengthOptions) -> GoResult<f64> {
    ctxCheck(ctx)?;
    let mut s = edgeScratch::default();
    let mut checks = prepChecker { ctx, count: 0 };
    let setup = prepareNodeEdgeLength(&mut checks, node, options, &mut s)?;
    evaluateNodeEdgeLength(ctx, node, options, &s, setup)
}

#[derive(Clone, Copy, Default)]
pub(crate) struct edgeLengthStatic {
    pub(crate) direction: geo::Orientation,
    pub(crate) directionFactor: f64,
}

pub(crate) fn prepareNodeEdgeLength(
    ctx: &mut prepChecker,
    node: &NodeRef,
    options: &EdgeLengthOptions,
    s: &mut edgeScratch,
) -> GoResult<edgeLengthStatic> {
    let includeSizes = options.IncludeNodeSizes;
    let directionPenalty = options.PenalizeDirection;
    let graph = node.g();

    let gathered: Vec<EdgeAbductionRef>;
    let edgeAbductions: &[EdgeAbductionRef] = match &options.EdgeAbductions {
        Some(v) => v,
        None => {
            let clusters = graph.b().Clusters.clone();
            let mut clusterOrder: Vec<NodeRef> = clusters.keys();
            layoutgraph::SortNodesByID(&mut clusterOrder);
            let mut all: Vec<EdgeAbductionRef> = Vec::new();
            for (i, vessel) in clusterOrder.iter().enumerate() {
                ctx.scoringCancellationError(i as i64)?;
                let cluster = clusters.get(vessel).expect("nil pointer dereference: cluster");
                all.extend(cluster.b().EdgeAbductions.iter().cloned());
            }
            gathered = all;
            &gathered
        }
    };

    let mut usedEdgeAbductions = vec![false; edgeAbductions.len()];
    ctx.checkScoringSlice(usedEdgeAbductions.len())?;

    let edges = node.edges();
    s.nRepl.clear();
    s.aRepl.clear();
    ctx.checkScoringSlice(edges.len())?;
    ctx.checkScoringSlice(edges.len())?;

    for (i, e) in edges.iter().enumerate() {
        ctx.scoringCancellationError(i as i64)?;
        let adjacentNode = node.Adjacent(e);
        // These are sometimes replaced by container children
        let mut nodeReplacement = node.clone();
        let mut adjacentNodeReplacement = adjacentNode.clone();

        if includeSizes {
            for (j, edgeAbduction) in edgeAbductions.iter().enumerate() {
                ctx.scoringCancellationError(j as i64)?;
                if usedEdgeAbductions[j] {
                    continue;
                }
                let ea = edgeAbduction.b();
                if optIs(&ea.CurrentFrom, node) && optIs(&ea.CurrentTo, &adjacentNode) {
                    usedEdgeAbductions[j] = true;
                    if let Some(ot) = &ea.OriginallyTo {
                        adjacentNodeReplacement = ot.clone();
                    }
                    if let Some(of) = &ea.OriginallyFrom {
                        nodeReplacement = of.clone();
                    }
                    break;
                }
                if optIs(&ea.CurrentFrom, &adjacentNode) && optIs(&ea.CurrentTo, node) {
                    usedEdgeAbductions[j] = true;
                    if let Some(of) = &ea.OriginallyFrom {
                        adjacentNodeReplacement = of.clone();
                    }
                    if let Some(ot) = &ea.OriginallyTo {
                        nodeReplacement = ot.clone();
                    }
                    break;
                }
            }
        }

        s.nRepl.push(nodeReplacement);
        s.aRepl.push(adjacentNodeReplacement);
    }

    let mut direction = node.ContainerDirection();
    let mut directionFactor = 0.0;
    if directionPenalty {
        if direction != geo::NONE {
            // heavily weight specified direction with sizes
            if includeSizes {
                directionFactor = 6.0;
            } else {
                directionFactor = 1.5;
            }
        } else {
            if node.IsTable() {
                direction = geo::Right;
                directionFactor = 0.5;
            } else {
                // Check for nodes with more than 2 labeled edges to same node
                // Track both total edges and how many are "from" this node
                let mut edgeCounts: Map<NodeRef, i64> = Map::default();
                let mut fromCounts: Map<NodeRef, i64> = Map::default();
                let mut hasMultipleLabels = false;
                let mut multiLabelNode: Option<NodeRef> = None;

                if includeSizes {
                    for (i, e) in edges.iter().enumerate() {
                        ctx.scoringCancellationError(i as i64)?;
                        let nodeReplacement = &s.nRepl[i];
                        // If this node is a container and the replacement is a child, the edge isn't actually connected
                        if nodeReplacement != node {
                            continue;
                        }
                        let adjacentNodeReplacement = &s.aRepl[i];
                        if *adjacentNodeReplacement != node.Adjacent(e) {
                            continue;
                        }
                        if e.b().Label.is_some() {
                            if adjacentNodeReplacement == nodeReplacement {
                                continue;
                            }
                            *edgeCounts.entry(adjacentNodeReplacement.clone()).or_insert(0) += 1;
                            let mut from = Some(e.from());
                            let (semanticFrom, _, directed) = e.DirectedEndpoints();
                            if directed {
                                from = semanticFrom;
                            }
                            if optIs(&from, node) {
                                *fromCounts.entry(adjacentNodeReplacement.clone()).or_insert(0) += 1;
                            }
                            if edgeCounts[adjacentNodeReplacement] > 2 {
                                hasMultipleLabels = true;
                                multiLabelNode = Some(adjacentNodeReplacement.clone());
                            }
                        }
                    }
                }

                if hasMultipleLabels {
                    // If more edges are from this node to the target, use Left orientation
                    // Otherwise use Right orientation
                    let m = multiLabelNode.unwrap();
                    let fc = fromCounts.get(&m).copied().unwrap_or(0);
                    let ec = edgeCounts.get(&m).copied().unwrap_or(0);
                    if fc > ec / 2 {
                        direction = geo::Left;
                    } else {
                        direction = geo::Right;
                    }
                    directionFactor = 10.0;
                } else {
                    // Default to bottom right if no multiple labeled edges
                    direction = geo::BottomRight;
                    directionFactor = 0.3;
                }
            }

            if node.IsClusterVessel() {
                directionFactor = 0.2;
                let cluster = graph.b().Clusters.get(node).expect("nil pointer dereference: cluster");
                let arrangement = cluster.b().Arrangement.clone();
                if arrangement == layoutgraph::Row {
                    direction = geo::Bottom;
                } else if arrangement == layoutgraph::Column {
                    direction = geo::Right;
                }
            }
        }
    }

    // get the edges between each nodeReplacement and adjacentNodeReplacement
    s.labeledEdgeCount.clear();
    for (i, edge) in edges.iter().enumerate() {
        ctx.scoringCancellationError(i as i64)?;
        if !includeSizes || edge.b().Label.is_none() {
            continue;
        }
        let pair = labeledEdgePair { from: s.nRepl[i].id(), to: s.aRepl[i].id() };
        *s.labeledEdgeCount.entry(pair).or_insert(0) += 1;
    }

    Ok(edgeLengthStatic { direction, directionFactor })
}

/// Go `node.PassesThrough(p1, p2)` on point values: a verbatim port of
/// layoutgraph's segmentIntersectsBox(p1, p2, &node.Box) that avoids
/// allocating point handles on this hot path.
#[inline]
pub(crate) fn passes(node: &NodeRef, p1: geo::Point, p2: geo::Point) -> bool {
    let (btl, bw, bh) = {
        let n = node.b();
        let bx = n.Box.b();
        let Some(tl) = bx.TopLeft.as_ref() else {
            return false;
        };
        let p = *tl.b();
        (p, bx.Width, bx.Height)
    };
    let (mut left, mut right) = (btl.X, btl.X + bw);
    if left > right {
        std::mem::swap(&mut left, &mut right);
    }
    let (mut top, mut bottom) = (btl.Y, btl.Y + bh);
    if top > bottom {
        std::mem::swap(&mut top, &mut bottom);
    }
    // A NaN box boundary cannot contain a point or survive the clipping below.
    if left.is_nan() || right.is_nan() || top.is_nan() || bottom.is_nan() {
        return false;
    }
    // Reject a segment only when both endpoints lie outside the same box side.
    if (p1.X < left && p2.X < left) || (p1.X > right && p2.X > right) || (p1.Y < top && p2.Y < top) || (p1.Y > bottom && p2.Y > bottom) {
        return false;
    }
    let contains = |p: &geo::Point| -> bool { left <= p.X && p.X <= right && top <= p.Y && p.Y <= bottom };
    if contains(&p1) || contains(&p2) {
        return true;
    }
    // Clip the segment against the closed box.
    let mut tEnter = 0.0f64;
    let mut tExit = 1.0f64;
    let mut clipAxis = |start: f64, delta: f64, minCoord: f64, maxCoord: f64| -> bool {
        if delta == 0.0 {
            return minCoord <= start && start <= maxCoord;
        }
        let mut t1 = (minCoord - start) / delta;
        let mut t2 = (maxCoord - start) / delta;
        if t1 > t2 {
            std::mem::swap(&mut t1, &mut t2);
        }
        tEnter = math::Max(tEnter, t1);
        tExit = math::Min(tExit, t2);
        tEnter <= tExit
    };
    if !clipAxis(p1.X, p2.X - p1.X, left, right) || !clipAxis(p1.Y, p2.Y - p1.Y, top, bottom) {
        return false;
    }
    tEnter < tExit
}

#[inline]
fn pt(x: f64, y: f64) -> geo::Point {
    geo::Point { X: x, Y: y }
}

/// Per-edge state shared by the Go closures in evaluateNodeEdgeLength.
struct edgeState {
    nodeReplacement: NodeRef,
    adjacentNodeReplacement: NodeRef,
    nodeCenter: Option<geo::Point>,
    adjacentNodeCenter: Option<geo::Point>,
    edgeDirection: geo::Orientation,
    edgeDirectionCalculated: bool,
    includeSizes: bool,
}

impl edgeState {
    #[inline]
    fn getCenters(&mut self) {
        if self.nodeCenter.is_none() {
            self.nodeCenter = Some(*self.nodeReplacement.Center().b());
            self.adjacentNodeCenter = Some(*self.adjacentNodeReplacement.Center().b());
        }
    }

    #[inline]
    fn getEdgeDirection(&mut self) -> geo::Orientation {
        if !self.edgeDirectionCalculated {
            if self.includeSizes {
                self.edgeDirection = self.nodeReplacement.Orientation(&self.adjacentNodeReplacement);
            } else {
                self.edgeDirection = sizelessOrientation(&self.nodeReplacement, &self.adjacentNodeReplacement);
            }
            self.edgeDirectionCalculated = true;
        }
        self.edgeDirection
    }

    #[inline]
    fn swap(&mut self) {
        std::mem::swap(&mut self.nodeReplacement, &mut self.adjacentNodeReplacement);
        std::mem::swap(&mut self.nodeCenter, &mut self.adjacentNodeCenter);
    }
}

/// Go closure checkAlternateRouteBlocked (only used for semi-diagonals).
fn checkAlternateRouteBlocked(st: &mut edgeState, otherNode: &NodeRef, isL1Blocked: &mut bool, isL2Blocked: &mut bool) -> bool {
    if otherNode.IsDescendantOf(Some(&st.nodeReplacement)) || otherNode.IsDescendantOf(Some(&st.adjacentNodeReplacement)) {
        return false;
    }
    st.getCenters();
    let edgeDir = st.getEdgeDirection();
    match edgeDir {
        geo::Top | geo::Bottom => {
            let swapped = edgeDir == geo::Bottom;
            if swapped {
                st.swap();
            }
            let nc = st.nodeCenter.unwrap();
            let ac = st.adjacentNodeCenter.unwrap();
            let (n, nw) = (st.nodeReplacement.tlv(), st.nodeReplacement.w());
            let (a, aw) = (st.adjacentNodeReplacement.tlv(), st.adjacentNodeReplacement.w());
            let floor = math::Max(n.X, a.X);
            let ceil = math::Min(n.X + nw, a.X + aw);
            if !*isL2Blocked {
                let minX = math::Min(n.X, a.X);
                let lowerMidX = minX + (floor - minX).abs() / 2.0;
                if n.X < a.X {
                    if passes(otherNode, pt(lowerMidX, nc.Y), pt(lowerMidX, ac.Y)) || passes(otherNode, pt(lowerMidX, ac.Y), ac) {
                        *isL2Blocked = true;
                    }
                }
                if n.X > a.X {
                    if passes(otherNode, nc, pt(lowerMidX, nc.Y)) || passes(otherNode, pt(lowerMidX, ac.Y), pt(lowerMidX, nc.Y)) {
                        *isL2Blocked = true;
                    }
                }
            }
            if !*isL1Blocked {
                let maxX = math::Max(n.X + nw, a.X + aw);
                let upperMidX = maxX - (ceil - maxX).abs() / 2.0;
                if n.X + nw > a.X + aw {
                    if passes(otherNode, pt(upperMidX, nc.Y), pt(upperMidX, ac.Y)) || passes(otherNode, ac, pt(upperMidX, ac.Y)) {
                        *isL1Blocked = true;
                    }
                }
                if n.X + nw < a.X + aw {
                    if passes(otherNode, nc, pt(upperMidX, nc.Y)) || passes(otherNode, pt(upperMidX, ac.Y), pt(upperMidX, nc.Y)) {
                        *isL1Blocked = true;
                    }
                }
            }
            if swapped {
                st.swap();
            }
        }
        geo::Left | geo::Right => {
            // Right uses the same code as Left but swapped, then swaps back.
            let swapped = edgeDir == geo::Right;
            if swapped {
                st.swap();
            }
            let nc = st.nodeCenter.unwrap();
            let ac = st.adjacentNodeCenter.unwrap();
            let (n, nh) = (st.nodeReplacement.tlv(), st.nodeReplacement.h());
            let (a, ah) = (st.adjacentNodeReplacement.tlv(), st.adjacentNodeReplacement.h());
            let floor = math::Max(n.Y, a.Y);
            let ceil = math::Min(n.Y + nh, a.Y + ah);
            if !*isL1Blocked {
                let minY = math::Min(n.Y, a.Y);
                let lowerMidY = minY + (floor - minY).abs() / 2.0;
                if n.Y < a.Y {
                    if passes(otherNode, pt(nc.X, lowerMidY), pt(ac.X, lowerMidY)) || passes(otherNode, ac, pt(ac.X, lowerMidY)) {
                        *isL1Blocked = true;
                    }
                }
                if n.Y > a.Y {
                    if passes(otherNode, nc, pt(nc.X, lowerMidY)) || passes(otherNode, pt(ac.X, lowerMidY), pt(nc.X, lowerMidY)) {
                        *isL1Blocked = true;
                    }
                }
            }
            if !*isL2Blocked {
                let maxY = math::Max(n.Y + nh, a.Y + ah);
                let upperMidY = maxY - (ceil - maxY).abs() / 2.0;
                if n.Y + nh > a.Y + ah {
                    if passes(otherNode, pt(nc.X, upperMidY), pt(ac.X, upperMidY)) || passes(otherNode, ac, pt(ac.X, upperMidY)) {
                        *isL2Blocked = true;
                    }
                }
                if n.Y + nh < a.Y + ah {
                    if passes(otherNode, nc, pt(nc.X, upperMidY)) || passes(otherNode, pt(ac.X, upperMidY), pt(nc.X, upperMidY)) {
                        *isL2Blocked = true;
                    }
                }
            }
            if swapped {
                st.swap();
            }
        }
        _ => panic!("did not expect diagonal"),
    }

    *isL1Blocked && *isL2Blocked
}

fn clusterSidePenalty(arrangement: &layoutgraph::ClusterArrangement, otherClusterDirection: geo::Orientation, thisClusterDirection: geo::Orientation) -> bool {
    let top = |o: geo::Orientation| matches!(o, geo::TopLeft | geo::Top | geo::TopRight);
    let bottom = |o: geo::Orientation| matches!(o, geo::BottomLeft | geo::Bottom | geo::BottomRight);
    let left = |o: geo::Orientation| matches!(o, geo::TopLeft | geo::Left | geo::BottomLeft);
    let right = |o: geo::Orientation| matches!(o, geo::TopRight | geo::Right | geo::BottomRight);
    if *arrangement == layoutgraph::Row {
        if top(otherClusterDirection) {
            return top(thisClusterDirection);
        }
        if bottom(otherClusterDirection) {
            return bottom(thisClusterDirection);
        }
    } else if *arrangement == layoutgraph::Column {
        if left(otherClusterDirection) {
            return left(thisClusterDirection);
        }
        if right(otherClusterDirection) {
            return right(thisClusterDirection);
        }
    }
    false
}

/// The cluster penalties applied when `clusterSide` (a node replacement with a
/// cluster) is connected to `other`.
fn clusterPenalty(st: &mut edgeState, clusterSide: &NodeRef, other: &NodeRef, turnCost: f64, distance: &mut f64) {
    let badClusterArrangementPenalty = turnCost;
    let cluster = clusterSide.clusterField().unwrap();
    let (vessel, arrangement) = {
        let c = cluster.b();
        (c.Vessel.clone(), c.Arrangement.clone())
    };
    if arrangement == layoutgraph::Row {
        let edgeDir = st.getEdgeDirection();
        if edgeDir == geo::Left || edgeDir == geo::Right {
            *distance += badClusterArrangementPenalty * 2.0;
        }
    } else if arrangement == layoutgraph::Column {
        let edgeDir = st.getEdgeDirection();
        if edgeDir == geo::Top || edgeDir == geo::Bottom {
            *distance += badClusterArrangementPenalty * 2.0;
        }
    }

    // This is a unique penalty because otherwise, two nodes could sit above the cluster (if it's a row)
    // and avoid all other penalties (including symmetry)
    let thisClusterDirection = vessel.Orientation(other);
    let (firstExternalNode, secondExternalNode, exactlyTwo) = clusterExactlyTwoExternalConnectedNodes(&cluster);
    if exactlyTwo {
        let otherClusterNode = if firstExternalNode.as_ref() == Some(other) { secondExternalNode } else { firstExternalNode };
        let otherClusterDirection = vessel.Orientation(otherClusterNode.as_ref().expect("nil pointer dereference"));
        // Penalize being on the same side as other external
        if clusterSidePenalty(&arrangement, otherClusterDirection, thisClusterDirection) {
            *distance += badClusterArrangementPenalty;
        }
    }
    // Reward being aligned with center when on the right side
    let mut cost = turnCost;
    if arrangement == layoutgraph::Row {
        if thisClusterDirection == geo::Top || thisClusterDirection == geo::Bottom {
            cost = (vessel.Center().b().X - other.Center().b().X).abs();
        }
    } else if arrangement == layoutgraph::Column {
        if thisClusterDirection == geo::Right || thisClusterDirection == geo::Left {
            cost = (vessel.Center().b().Y - other.Center().b().Y).abs();
        }
    }
    *distance += cost;
}

pub(crate) fn evaluateNodeEdgeLength(
    ctx: &Ctx,
    node: &NodeRef,
    options: &EdgeLengthOptions,
    s: &edgeScratch,
    setup: edgeLengthStatic,
) -> GoResult<f64> {
    let includeSizes = options.IncludeNodeSizes;
    let directionPenalty = options.PenalizeDirection;
    let graph = node.g();
    let (cellSize, containers) = {
        let g = graph.b();
        (g.CellSize, g.Containers.clone())
    };
    let mut minGapSize = 0.0;
    if options.IncludeNodeSizes && options.EnforceMinimumGap {
        minGapSize = cellSize;
    }
    let direction = setup.direction;
    let directionFactor = setup.directionFactor;
    let compass = directionCompass(direction);
    let mut totalDistance = 0.0;
    let nodeContainer = node.EffectiveContainer();
    let edges = node.edges();
    let nodeFrom = |e: &EdgeRef| -> bool { e.from() == *node };
    // Go appends node.Graph.Containers[curr] for each added key; the port keeps
    // the keys and reads the (unchanged) map entries while scanning.
    let mut obstructionAdded: Vec<Option<NodeRef>> = Vec::new();

    for (i, e) in edges.iter().enumerate() {
        scoringCancellationError(ctx, i as i64)?;
        let adjacentNode = node.Adjacent(e);
        // Used during initialization and when adjacent is outer container
        if !adjacentNode.has_tl() {
            continue;
        }
        let mut st = edgeState {
            nodeReplacement: s.nRepl[i].clone(),
            adjacentNodeReplacement: s.aRepl[i].clone(),
            nodeCenter: None,
            adjacentNodeCenter: None,
            edgeDirection: geo::NONE,
            edgeDirectionCalculated: false,
            includeSizes,
        };
        let nodeReplacement = st.nodeReplacement.clone();
        let adjacentNodeReplacement = st.adjacentNodeReplacement.clone();
        let nrCluster = nodeReplacement.clusterField();
        let arCluster = adjacentNodeReplacement.clusterField();

        let mut nodeVal = geo::Point::default();
        let mut adjacentNodeVal = geo::Point::default();

        let mut isDiagonal = false;
        let mut isSemiDiagonal = false;
        if includeSizes {
            let edgeDir = st.getEdgeDirection();
            if edgeDir == geo::NONE {
                // This is okay
                continue;
            }
            if edgeDir.IsDiagonal() {
                isDiagonal = true;
            }
            match edgeDir {
                geo::Top => {
                    let (mid, semiDiag) = calculateHorizontalMidpoint(&nodeReplacement, &adjacentNodeReplacement);
                    nodeVal = pt(mid, nodeReplacement.y() + nodeReplacement.h());
                    adjacentNodeVal = pt(mid, adjacentNodeReplacement.y());
                    isSemiDiagonal = semiDiag;
                }
                geo::Bottom => {
                    let (mid, semiDiag) = calculateHorizontalMidpoint(&nodeReplacement, &adjacentNodeReplacement);
                    nodeVal = pt(mid, nodeReplacement.y());
                    adjacentNodeVal = pt(mid, adjacentNodeReplacement.y() + adjacentNodeReplacement.h());
                    isSemiDiagonal = semiDiag;
                }
                geo::Left => {
                    let (mid, semiDiag) = calculateVerticalMidpoint(&nodeReplacement, &adjacentNodeReplacement);
                    nodeVal = pt(nodeReplacement.x() + nodeReplacement.w(), mid);
                    adjacentNodeVal = pt(adjacentNodeReplacement.x(), mid);
                    isSemiDiagonal = semiDiag;
                }
                geo::Right => {
                    let (mid, semiDiag) = calculateVerticalMidpoint(&nodeReplacement, &adjacentNodeReplacement);
                    nodeVal = pt(nodeReplacement.x(), mid);
                    adjacentNodeVal = pt(adjacentNodeReplacement.x() + adjacentNodeReplacement.w(), mid);
                    isSemiDiagonal = semiDiag;
                }
                geo::TopLeft | geo::TopRight | geo::BottomLeft | geo::BottomRight => {
                    st.getCenters();
                    nodeVal = st.nodeCenter.unwrap();
                    adjacentNodeVal = st.adjacentNodeCenter.unwrap();
                }
                _ => {}
            }
            if nrCluster.is_some() || arCluster.is_some() {
                isSemiDiagonal = false;
                isDiagonal = false;
                st.getCenters();
                let nc = st.nodeCenter.unwrap();
                let ac = st.adjacentNodeCenter.unwrap();
                // if it's diagonal, you still have to use the boundaries, or else a line passes through innocent siblings
                match edgeDir {
                    geo::Top | geo::TopLeft | geo::TopRight => {
                        nodeVal = pt(nc.X, nodeReplacement.y() + nodeReplacement.h());
                        adjacentNodeVal = pt(ac.X, adjacentNodeReplacement.y());
                    }
                    geo::Bottom | geo::BottomLeft | geo::BottomRight => {
                        nodeVal = pt(nc.X, nodeReplacement.y());
                        adjacentNodeVal = pt(ac.X, adjacentNodeReplacement.y() + adjacentNodeReplacement.h());
                    }
                    geo::Left => {
                        nodeVal = pt(nodeReplacement.x() + nodeReplacement.w(), nc.Y);
                        adjacentNodeVal = pt(adjacentNodeReplacement.x(), ac.Y);
                    }
                    geo::Right => {
                        nodeVal = pt(nodeReplacement.x(), nc.Y);
                        adjacentNodeVal = pt(adjacentNodeReplacement.x() + adjacentNodeReplacement.w(), ac.Y);
                    }
                    _ => {}
                }
            }
        } else {
            if let Some(p) = nodeReplacement.tl() {
                nodeVal = *p.b();
            }
            if let Some(p) = adjacentNodeReplacement.tl() {
                adjacentNodeVal = *p.b();
            }
        }

        // ... and the distance is a straight line
        let mut distance;
        if includeSizes && e.HasTableColumn() {
            let mut from = &nodeReplacement;
            let mut to = &adjacentNodeReplacement;
            if !nodeFrom(e) {
                std::mem::swap(&mut from, &mut to);
            }
            distance = distanceBetweenTableColumns(&graph, e, from, to);
        } else if nrCluster.is_some() || arCluster.is_some() {
            //  I want to use the vessel of the cluster, otherwise during node placement, a connected node will jump back and forth between optimizing each one.
            if let Some(c) = &nrCluster {
                let vessel = c.b().Vessel.clone();
                distance = placementDistance(&vessel, &adjacentNodeReplacement, includeSizes);
            } else {
                let vessel = arCluster.as_ref().unwrap().b().Vessel.clone();
                distance = placementDistance(&vessel, &nodeReplacement, includeSizes);
            }
        } else if isDiagonal {
            // the distance is the sum of that L shape
            let corner = pt(nodeVal.X, adjacentNodeVal.Y);
            distance = distanceToPoint(&nodeReplacement, &corner, includeSizes) + distanceToPoint(&adjacentNodeReplacement, &corner, includeSizes);
            if includeSizes {
                distance += graph.TurnCost();
            }
        } else {
            distance = placementDistance(&nodeReplacement, &adjacentNodeReplacement, includeSizes);
        }

        if includeSizes && node.IsContainer() && adjacentNode.IsContainer() && nodeContainer == adjacentNode.EffectiveContainer() {
            let mut containerAlignmentPenalty = 0.0;
            let (n, nw, nh) = (node.tlv(), node.w(), node.h());
            let (a, aw, ah) = (adjacentNode.tlv(), adjacentNode.w(), adjacentNode.h());
            // If the widths are roughly equal in size, we want alignment on either one of the sides or the center
            if math::Min(nw, aw) > math::Max(nw, aw) * 0.75 {
                if (n.X != a.X) && (n.X + nw != a.X + aw) && ((n.X + nw) / 2.0 != (a.X + aw) / 2.0) {
                    containerAlignmentPenalty = graph.TurnCost();
                }
            }

            if math::Min(nh, ah) > math::Max(nh, ah) * 0.75 {
                if (n.Y != a.Y) && (n.Y + nh != a.Y + ah) && ((n.Y + nh) / 2.0 != (a.Y + ah) / 2.0) {
                    containerAlignmentPenalty = graph.TurnCost();
                }
            }

            distance += containerAlignmentPenalty;
        }

        if includeSizes && !e.IsBetweenTableColumns() {
            let bounds = scoringNodeBounds(Some(&nodeReplacement)).including(scoringNodeBounds(Some(&adjacentNodeReplacement)));
            // for diagonals and semi-diagonals we want to see if both corners are covered by any obstructions
            let mut passesThroughCornerA = false;
            let mut passesThroughCornerB = false;

            // The L shape that goes clockwise
            let mut isL1Blocked = false;
            // The L shape that goes counterclockwise
            let mut isL2Blocked = false;

            if isSemiDiagonal {
                let edgeDir = st.getEdgeDirection();
                let (n, nw, nh) = (nodeReplacement.tlv(), nodeReplacement.w(), nodeReplacement.h());
                let (a, aw, ah) = (adjacentNodeReplacement.tlv(), adjacentNodeReplacement.w(), adjacentNodeReplacement.h());
                match edgeDir {
                    geo::Left | geo::Right => {
                        if (n.Y - a.Y).abs() <= SideEdgeSpacing {
                            isL1Blocked = true;
                        } else if (n.Y + nh - (a.Y + ah)).abs() <= SideEdgeSpacing {
                            isL2Blocked = true;
                        }
                    }
                    geo::Top | geo::Bottom => {
                        if (n.X - a.X).abs() <= SideEdgeSpacing {
                            isL2Blocked = true;
                        } else if (n.X + nw - (a.X + aw)).abs() <= SideEdgeSpacing {
                            isL1Blocked = true;
                        }
                    }
                    _ => {}
                }
            }

            let mut bestRouteBlocked = false;
            let mut alternateRouteBlocked = !isSemiDiagonal;
            obstructionAdded.clear();
            let ancestor = nodeReplacement.NearestSharedAncestor(&adjacentNodeReplacement);

            for start in [&nodeReplacement, &adjacentNodeReplacement] {
                let mut curr = start.EffectiveContainer();
                loop {
                    if obstructionAdded.contains(&curr) {
                        break;
                    }
                    obstructionAdded.push(curr.clone());
                    let Some(c) = curr else {
                        break;
                    };
                    curr = c.EffectiveContainer();
                    if curr == ancestor {
                        break;
                    }
                }
            }

            let containerMap = containers.b();
            'sets: for (obstructionSetIndex, key) in obstructionAdded.iter().enumerate() {
                let obstructions: &[NodeRef] = containerMap.get(key).map(|v| v.as_slice()).unwrap_or(&[]);
                scoringCancellationError(ctx, obstructionSetIndex as i64)?;
                for (i, obstruction) in obstructions.iter().enumerate() {
                    scoringCancellationError(ctx, i as i64)?;
                    if bounds.excludes(Some(obstruction)) {
                        continue;
                    }
                    if nodeReplacement.IsDescendantOf(Some(obstruction)) || adjacentNodeReplacement.IsDescendantOf(Some(obstruction)) {
                        continue;
                    }
                    // Used during initialization, or sibling coming from subgraph that hasn't gone yet
                    if !obstruction.has_tl() {
                        continue;
                    }
                    if obstruction.graph() != nodeReplacement.graph() {
                        // not in the same subgraph, can happen during placement
                        continue;
                    }
                    if *obstruction == nodeReplacement || *obstruction == adjacentNodeReplacement {
                        continue;
                    }
                    if obstruction.IsClusterVessel() {
                        let isOwnVessel = |c: &Option<layoutgraph::ClusterRef>| match c {
                            Some(c) => c.b().Vessel == *obstruction,
                            None => false,
                        };
                        if isOwnVessel(&nrCluster) || isOwnVessel(&arCluster) {
                            // Penalties are calculated elsewhere below for cluster nodes, so don't double count
                            continue;
                        }
                    }

                    if !bestRouteBlocked {
                        // Go closure checkBestRouteBlocked.
                        let blocked = if obstruction.IsDescendantOf(Some(&nodeReplacement)) || obstruction.IsDescendantOf(Some(&adjacentNodeReplacement)) {
                            false
                        } else if isDiagonal {
                            let cornerA = pt(nodeVal.X, adjacentNodeVal.Y);
                            if !passesThroughCornerA && (passes(obstruction, nodeVal, cornerA) || passes(obstruction, adjacentNodeVal, cornerA)) {
                                passesThroughCornerA = true;
                            }
                            let cornerB = pt(adjacentNodeVal.X, nodeVal.Y);
                            if !passesThroughCornerB && (passes(obstruction, nodeVal, cornerB) || passes(obstruction, adjacentNodeVal, cornerB)) {
                                passesThroughCornerB = true;
                            }
                            passesThroughCornerA && passesThroughCornerB
                        } else {
                            passes(obstruction, nodeVal, adjacentNodeVal)
                        };
                        if blocked {
                            bestRouteBlocked = true;
                        }
                    }
                    if bestRouteBlocked && !alternateRouteBlocked && checkAlternateRouteBlocked(&mut st, obstruction, &mut isL1Blocked, &mut isL2Blocked) {
                        alternateRouteBlocked = true;
                    }
                    if bestRouteBlocked && alternateRouteBlocked {
                        break;
                    }
                }
                if bestRouteBlocked && alternateRouteBlocked {
                    break 'sets;
                }
            }
            drop(containerMap);
            if bestRouteBlocked {
                let passThroughCost;
                // Penalize deeply for obstructing cluster
                if nrCluster.is_some() || arCluster.is_some() {
                    passThroughCost = graph.TurnCost() * 2.0;
                } else if isDiagonal {
                    // When it's diagonal, the shortest route would've been an L shaped, so to turn it in an S-shape is only one extra turn
                    passThroughCost = graph.TurnCost();
                } else if isSemiDiagonal {
                    if alternateRouteBlocked {
                        passThroughCost = graph.TurnCost() * 2.0;
                    } else {
                        // Just an L-shape
                        passThroughCost = graph.TurnCost();
                    }
                } else {
                    // When it's straight, takes 2 extra turns to get around the obstruction
                    passThroughCost = graph.TurnCost() * 2.0;
                }
                distance += passThroughCost;
            }

            let turnCost = graph.TurnCost();
            if nrCluster.is_some() {
                clusterPenalty(&mut st, &nodeReplacement, &adjacentNodeReplacement, turnCost, &mut distance);
            }
            if arCluster.is_some() {
                clusterPenalty(&mut st, &adjacentNodeReplacement, &nodeReplacement, turnCost, &mut distance);
            }
        }

        if includeSizes && e.IsBetweenTableColumns() {
            let columnIndex: i64 = {
                let eb = e.b();
                if eb.From == nodeReplacement {
                    eb.ToTableColumnIndex.expect("nil pointer dereference")
                } else {
                    eb.FromTableColumnIndex.expect("nil pointer dereference")
                }
            };
            let adjEdges = adjacentNodeReplacement.edges();
            for (i, e2) in adjEdges.iter().enumerate() {
                scoringCancellationError(ctx, i as i64)?;
                if !e2.IsBetweenTableColumns() {
                    continue;
                }
                let otherTable = adjacentNodeReplacement.Adjacent(e2);
                let otherColumnIndex: i64 = {
                    let eb = e2.b();
                    if eb.From == otherTable {
                        eb.ToTableColumnIndex.expect("nil pointer dereference")
                    } else {
                        eb.FromTableColumnIndex.expect("nil pointer dereference")
                    }
                };

                let directionToOther = nodeReplacement.Orientation(&otherTable);

                let mut sameSide = false;
                // The other table has to be on the same side as us for this crossing calculation to matter
                let otherEdgeDirection = otherTable.Orientation(&adjacentNodeReplacement);
                let edgeDir = st.getEdgeDirection();
                let rightish = |o: geo::Orientation| matches!(o, geo::BottomRight | geo::Right | geo::TopRight);
                let leftish = |o: geo::Orientation| matches!(o, geo::BottomLeft | geo::Left | geo::TopLeft);
                if rightish(edgeDir) {
                    if rightish(otherEdgeDirection) {
                        sameSide = true;
                    }
                } else if leftish(edgeDir) {
                    if leftish(otherEdgeDirection) {
                        sameSide = true;
                    }
                }

                if sameSide {
                    match directionToOther {
                        // The other table is below us
                        geo::TopRight | geo::Top | geo::TopLeft => {
                            // The other table connects to column higher than ours
                            if otherColumnIndex < columnIndex {
                                distance += graph.CrossingCost();
                            }
                        }
                        // The other table is above us
                        geo::BottomRight | geo::Bottom | geo::BottomLeft => {
                            // The other table connects to column lower than ours
                            if otherColumnIndex > columnIndex {
                                distance += graph.CrossingCost();
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        // If the distance is less than the minGapSize (and it's enforced), we don't want to reward that
        distance = math::Max(distance, minGapSize);

        // Add a penalty according to edge direction
        let mut outerNode = &nodeReplacement;
        if nodeReplacement.containerField() != adjacentNodeReplacement.containerField() {
            if depth(Some(&nodeReplacement)) > depth(Some(&adjacentNodeReplacement)) {
                outerNode = &adjacentNodeReplacement;
            }
        }
        if directionPenalty && (e.IsDirected() || outerNode.ContainerDirection() != geo::NONE) {
            let mut edgeDir = st.getEdgeDirection();
            let mut from = Some(e.from());
            let (semanticFrom, _, directed) = e.DirectedEndpoints();
            if directed {
                from = semanticFrom;
            }
            if optIs(&from, node) {
                edgeDir = edgeDir.GetOpposite();
            }

            let mut directionUsed = edgeDir;

            // For clusters, we care more about alignment in center than compass direction
            // So coalesce diagonal directions to the same direction
            if nrCluster.is_some() || arCluster.is_some() {
                match direction {
                    geo::Left => {
                        if directionUsed == geo::TopLeft || directionUsed == geo::BottomLeft {
                            directionUsed = geo::Left;
                        }
                    }
                    geo::Right => {
                        if directionUsed == geo::TopRight || directionUsed == geo::BottomRight {
                            directionUsed = geo::Right;
                        }
                    }
                    geo::Top => {
                        if directionUsed == geo::TopLeft || directionUsed == geo::TopRight {
                            directionUsed = geo::Top;
                        }
                    }
                    geo::Bottom => {
                        if directionUsed == geo::BottomLeft || directionUsed == geo::BottomRight {
                            directionUsed = geo::Bottom;
                        }
                    }
                    _ => {}
                }
            }

            let mut dirDelta = (compassDelta(compass, directionCompass(directionUsed)) as f64).abs();
            if !e.IsDirected() {
                // if there is a container direction set, also apply to bidirectional edges
                dirDelta = 0.1 * dirDelta + 0.9 * (compassAxisDelta(compass, directionCompass(directionUsed)) as f64).abs();
            }
            let mut baseCost = cellSize;
            if !includeSizes {
                baseCost = 1.0;
            }
            distance += directionFactor * dirDelta * baseCost * sizelessDirectionDeltaFactor;
        }

        if includeSizes {
            if e.HasLargeArrowheadLabel() {
                let edgeDir = st.getEdgeDirection();
                if !edgeDir.IsHorizontal() {
                    distance += 10.0 * graph.TurnCost();
                }
            } else if e.b().Label.is_some() {
                if !directionPenalty || !direction.IsVertical() {
                    let pair = labeledEdgePair { from: nodeReplacement.id(), to: adjacentNodeReplacement.id() };
                    let count = s.labeledEdgeCount.get(&pair).copied().unwrap_or(0);
                    // with 2 vertical edges a label can go on each side without obstructing each other
                    if count > 2 {
                        let edgeDir = st.getEdgeDirection();
                        if !edgeDir.IsHorizontal() {
                            // penalize labeled edges with many duplicates if they are not horizontal
                            distance += count as f64 * graph.TurnCost();
                        }
                    }
                }
            }
        }

        totalDistance += distance;
    }

    // We don't care about getting as close as possible to all Nears, we just want to get close to one of them
    let nears: Vec<NodeRef> = {
        let n = node.b();
        if n.Nears.is_empty() {
            Vec::new()
        } else {
            n.Nears.iter().cloned().collect()
        }
    };
    if !nears.is_empty() {
        let mut minDistanceToNear = f64::INFINITY;
        for (nearIndex, near) in nears.iter().enumerate() {
            scoringCancellationError(ctx, nearIndex as i64)?;
            // Not initialized
            if !near.has_tl() {
                minDistanceToNear = 0.0;
                continue;
            }
            let distance = node.DistanceTo(near, includeSizes);
            minDistanceToNear = math::Min(minDistanceToNear, distance);
        }
        totalDistance += minDistanceToNear;
    }

    let herd = node.b().HerdAssignment.clone();
    if let (Some(herd), true) = (herd, includeSizes) {
        let (orientation, val) = {
            let h = herd.b();
            (h.Orientation, h.Val)
        };
        let mut d = 0.0;
        match orientation {
            geo::Bottom => d = val - (node.y() + node.h()),
            geo::Top => d = node.y() - val,
            geo::Left => d = node.x() - val,
            geo::Right => d = val - (node.x() + node.w()),
            _ => {}
        }

        if d >= 0.0 {
            totalDistance += d;
        } else {
            // extra penalty since it makes all other sheep further from herd fence
            totalDistance -= d - cellSize;
        }
    }

    // add a penalty for common uncle siblings not being aligned
    let siblings = graph.b().CommonUncleSiblings.get(node);
    if let Some(siblings) = siblings {
        let axisScore = AxisScore(&siblings);
        let mut cost = cellSize;
        if !includeSizes {
            cost = 1.0;
        }
        totalDistance += cost * (1.0 - axisScore) * (siblings.len() as f64 - 1.0);
    }

    ctxCheck(ctx)?;
    if includeSizes {
        totalDistance += flowContinuityCost(node, s);
    }
    Ok(totalDistance)
}
