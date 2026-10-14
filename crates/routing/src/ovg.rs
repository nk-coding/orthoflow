// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/routing/ovg.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of routing/ovg.go.
#[allow(unused_imports)]
use crate::*;
use gocompat::context::Ctx;
use gocompat::{errorf, math, GoResult, Map, Set};
use layoutgraph::{GraphRef, HierarchyRef, NodeRef};
use std::rc::Rc;

/// Go map key for a float64 (`+0 == -0`; NaN keys are not supported).
#[inline]
pub(crate) fn f64key(x: f64) -> u64 {
    if x == 0.0 {
        0
    } else {
        x.to_bits()
    }
}

/// Go `map[float64]struct{}`. Go updates a float key on re-insertion
/// (NeedKeyUpdate), so the stored key is the most recently inserted variant.
#[derive(Default, Clone)]
pub(crate) struct goFloatSet {
    pub(crate) m: Map<u64, f64>,
}

impl goFloatSet {
    #[inline]
    pub(crate) fn insert(&mut self, x: f64) {
        self.m.insert(f64key(x), x);
    }
    #[inline]
    pub(crate) fn len(&self) -> usize {
        self.m.len()
    }
    pub(crate) fn values(&self) -> Vec<f64> {
        self.m.values().cloned().collect()
    }
}

/// Orthogonal Visibility Graph
pub struct OVG {
    pub NodesInsideBoundingBox: Vec<NodeRef>,
    pub Nodes: Vec<OVGNodeRef>,
    pub OccupiedPoints: Map<geo::Point, OVGNodeRef>,
    pub Edges: Vec<OVGEdgeRef>,
    /// Go `map[float64][]*OVGEdge`, keyed by `f64key`.
    pub VerticalEdges: Map<u64, Vec<OVGEdgeRef>>,
    pub HorizontalEdges: Map<u64, Vec<OVGEdgeRef>>,
    pub Ports: Map<NodeRef, Vec<OVGNodeRef>>,
    pub Centers: Map<NodeRef, OVGNodeRef>,

    pub(crate) fixedOverlapsCache: Vec<fixedOverlapsCacheEntry>,
    pub buildGuard: Option<ovgBuildGuardRef>,
}

gocompat::go_ptr!(OVGRef, OVG);

impl OVG {
    pub fn AddNodeUnchecked(&mut self, node: &OVGNodeRef) {
        self.Nodes.push(node.clone());
        self.OccupiedPoints.insert(node.P(), node.clone());
    }

    pub fn AddNode(&mut self, node: &OVGNodeRef) -> OVGNodeRef {
        if let Some(occupant) = self.OccupiedPoints.get(&node.P()) {
            occupant.clone()
        } else {
            self.AddNodeUnchecked(node);
            node.clone()
        }
    }

    pub fn Connect(&mut self, nodeA: &OVGNodeRef, nodeB: &OVGNodeRef) -> Option<OVGEdgeRef> {
        {
            let (a, b) = (nodeA.b(), nodeB.b());
            if a.IsNodeCenter && !b.isPort() || b.IsNodeCenter && !a.isPort() {
                // only ports can connect to center
                return None;
            }
        }
        let (pa, pb) = (nodeA.P(), nodeB.P());
        // this distance doesn't change as the OVG nodes don't move around
        // so it is safe to precompute it here and just use it later on during edge routing
        let distance = geo::EuclideanDistance(pa.X, pa.Y, pb.X, pb.Y);
        let edge = newOVGEdgeWithDistance(nodeA, nodeB, distance);
        self.Edges.push(edge.clone());
        nodeA.addEdge(edge.clone());
        nodeB.addEdge(edge.clone());
        if edge.isVertical() {
            self.VerticalEdges.entry(f64key(pa.X)).or_default().push(edge.clone());
        } else if edge.isHorizontal() {
            self.HorizontalEdges.entry(f64key(pa.Y)).or_default().push(edge.clone());
        }
        Some(edge)
    }

    pub(crate) fn removeIsolatedNodes(&mut self, guard: &mut ovgBuildGuard) -> GoResult<()> {
        guard.check()?;
        let mut newNodes: Vec<OVGNodeRef> = Vec::new();

        for n in &self.Nodes {
            guard.step()?;
            if !n.b().Edges.is_empty() {
                newNodes.push(n.clone());
            }
        }

        self.Nodes = newNodes;
        guard.check()
    }

    pub(crate) fn reindexOccupiedPoints(&mut self) {
        let mut m: Map<geo::Point, OVGNodeRef> = gocompat::NewMapCap(self.Nodes.len());
        for node in &self.Nodes {
            m.insert(node.P(), node.clone());
        }
        self.OccupiedPoints = m;
    }
}

pub fn NewOVG(nodesInsideBoundingBox: Vec<NodeRef>) -> OVG {
    OVG {
        NodesInsideBoundingBox: nodesInsideBoundingBox,
        Nodes: Vec::new(),
        OccupiedPoints: Map::default(),
        Edges: Vec::new(),
        VerticalEdges: Map::default(),
        HorizontalEdges: Map::default(),
        Ports: Map::default(),
        Centers: Map::default(),
        fixedOverlapsCache: Vec::new(),
        buildGuard: None,
    }
}

pub(crate) fn newBuildOVG(nodesInsideBoundingBox: Vec<NodeRef>, guard: Option<&ovgBuildGuardRef>) -> OVG {
    let mut ovg = NewOVG(nodesInsideBoundingBox);
    ovg.buildGuard = guard.cloned();
    ovg
}

pub(crate) fn buildOVGFromGraphWithLimits(ctx: &Ctx, g: &GraphRef, nearbyNodes: &[NodeRef], limits: ovgBuildLimits) -> GoResult<OVGRef> {
    let guard = newOVGBuildGuard(Some(ctx), limits)?;
    buildOVGFromGraphWithGuard(g, nearbyNodes, &guard)
}

pub(crate) fn buildOVGFromGraphWithGuard(g: &GraphRef, nearbyNodes: &[NodeRef], guardRef: &ovgBuildGuardRef) -> GoResult<OVGRef> {
    let ovg = {
        let mut guard = guardRef.bm();
        buildOVGFromGraphInner(g, nearbyNodes, guardRef, &mut guard)?
    };
    Ok(OVGRef::new(ovg))
}

fn buildOVGFromGraphInner(g: &GraphRef, nearbyNodes: &[NodeRef], guardRef: &ovgBuildGuardRef, guard: &mut ovgBuildGuard) -> GoResult<OVG> {
    guard.check()?;
    let gNodes = g.b().Nodes.clone();
    let orthogonalNodeCapacity = checkedOVGSliceCapacity(&[gNodes.len(), nearbyNodes.len()])?;
    let mut orthogonalNodes: Vec<NodeRef> = Vec::with_capacity(orthogonalNodeCapacity);
    orthogonalNodes.extend(nearbyNodes.iter().cloned());

    let mut hierarchyOVGs: Vec<OVG> = Vec::new();
    let mut seenHierarchies: Set<HierarchyRef> = Set::default();
    for n in &gNodes {
        guard.step()?;
        let h = n.b().Hierarchy.clone();
        match h {
            None => orthogonalNodes.push(n.clone()),
            Some(h) => {
                if !seenHierarchies.contains(&h) {
                    seenHierarchies.insert(h.clone());
                    let hierarchyOVG = newOVGForHierarchy(g, &h, guardRef, guard)?;
                    hierarchyOVGs.push(hierarchyOVG);
                }
            }
        }
    }

    let mut ovg = newBuildOVG(orthogonalNodes, Some(guardRef));
    ovg.addPorts(g, guard)?;
    for hOVG in &hierarchyOVGs {
        guard.step()?;
        ovg.mergePorts(hOVG, guard)?;
    }
    ovg.addNodesIntersections(g, guard)?;
    for hOVG in &hierarchyOVGs {
        guard.step()?;
        ovg.mergeNonPorts(hOVG, guard)?;
    }

    ovg.addEdgesNodes(g, guard)?;
    ovg.addTreeNodes(g, guard)?;
    let (tl, br) = guard.tightBoundingBox(&ovg.NodesInsideBoundingBox)?;
    let tl = tl.expect("nil pointer dereference: OVG bounding box");
    let br = br.expect("nil pointer dereference: OVG bounding box");
    ovg.addNewBoundaryLayers(g, tl, br, guard)?;
    ovg.addPortConnectionNodesAtBoundaries(g, tl, br, guard)?;
    ovg.addCornerNodes(g, tl, br, guard)?;
    ovg.addTunnels(g, guard)?;

    // TODO if any OVG nodes are too close to container edge, split them
    ovg.connectNodes(g, guard)?;
    ovg.connectPortsToCenter(guard)?;
    ovg.removeIsolatedNodes(guard)?;
    ovg.mapNodesToContainer(g, guard)?;
    ovg.flagNodesNearPorts(guard)?;
    for (i, node) in ovg.Nodes.iter().enumerate() {
        guard.step()?;
        node.bm().Index = i;
    }

    Ok(ovg)
}

fn orientationOfSide(i: usize) -> geo::Orientation {
    match i {
        0 => geo::Top,
        1 => geo::Left,
        2 => geo::Bottom,
        3 => geo::Right,
        _ => geo::Orientation::default(),
    }
}

impl OVG {
    /// For each tala.Node, get the ports and add them as OVG Nodes
    pub(crate) fn addPorts(&mut self, g: &GraphRef, guard: &mut ovgBuildGuard) -> GoResult<()> {
        guard.check()?;
        // If all of a container's children have no connections, there's no need to consider them for pathing
        let mut skipPathing: Set<NodeRef> = Set::default();
        let containers = g.b().Containers.values();
        for children in &containers {
            guard.step()?;
            let mut has = false;
            for child in children {
                guard.step()?;
                if !child.b().Edges.is_empty() {
                    has = true;
                    break;
                }
            }
            if !has {
                for child in children {
                    guard.step()?;
                    skipPathing.insert(child.clone());
                }
            }
        }

        // Add ports
        let nodes = self.NodesInsideBoundingBox.clone();
        for node in &nodes {
            guard.step()?;
            if skipPathing.contains(node) {
                continue;
            }
            // most shapes have 12 ports (3 on each side)
            self.Ports.insert(node.clone(), Vec::with_capacity(12));
            let shape = node.shape();
            let tl = node.tlv();
            let (w, h) = (node.w(), node.h());
            for (i, relativePoints) in shape.SnapPointPercentages().iter().enumerate() {
                guard.step()?;
                for point in relativePoints {
                    guard.step()?;
                    let rp = *point.b();
                    let port = guard.addPoint(self, geo::Point { X: tl.X + math::Round(w * rp.XPercentage), Y: tl.Y + math::Round(h * rp.YPercentage) })?;
                    self.Ports.get_mut(node).unwrap().push(port.clone());
                    let direction = orientationOfSide(i);
                    port.addPortOwner(node, direction, false);
                }
            }

            let ports = self.Ports.get(node).unwrap().clone();
            for index in shape.CenterPortIndices() {
                guard.step()?;
                ports[index as usize].setCenterPort(node);
            }
        }
        guard.check()
    }

    /// For all OVG Nodes added from ports, take their x/y cartesian product to create a grid of OVG Nodes
    pub(crate) fn addNodesIntersections(&mut self, g: &GraphRef, guard: &mut ovgBuildGuard) -> GoResult<()> {
        guard.check()?;
        let mut xCandidates = goFloatSet::default();
        let mut yCandidates = goFloatSet::default();

        for node in &self.Nodes {
            guard.step()?;
            let p = node.P();
            xCandidates.insert(p.X);
            yCandidates.insert(p.Y);
        }

        self.addIntersections(g, &xCandidates, &yCandidates, guard)
    }

    pub(crate) fn addIntersections(&mut self, g: &GraphRef, xs: &goFloatSet, ys: &goFloatSet, guard: &mut ovgBuildGuard) -> GoResult<()> {
        guard.check()?;
        let candidateCount = checkedOVGIntersectionCount(xs.len(), ys.len())?;
        // Reserve the complete Cartesian product before allocating its orderings or
        // entering the loop. This makes a limit failure deterministic and cheap.
        guard.reserveCandidates(candidateCount)?;
        guard.reserveWork(candidateCount)?;

        let mut xOrder: Vec<f64> = Vec::with_capacity(xs.len());
        for x in xs.values() {
            guard.check()?;
            xOrder.push(x);
        }
        guard.reserveSortWork(xOrder.len())?;
        gocompat::sort::SlicesSort(&mut xOrder);

        let mut yOrder: Vec<f64> = Vec::with_capacity(ys.len());
        for y in ys.values() {
            guard.check()?;
            yOrder.push(y);
        }
        guard.reserveSortWork(yOrder.len())?;
        gocompat::sort::SlicesSort(&mut yOrder);

        let graphNodes = g.b().Nodes.clone();
        let fixedOverlaps = self.fixedOverlapsForBuild(g, &graphNodes, guard)?;
        let portIndex = newOVGPortIndex(&self.Ports, Some(g), &fixedOverlaps, guard)?;
        let proximityIndex = newOVGPointProximityIndex(g, &xOrder, guard)?;

        // TODO don't have cross section of port nodes from container to descendants
        // Cross section of all the port nodes
        for &px in &xOrder {
            guard.check()?;
            for &py in &yOrder {
                guard.check()?;
                // Can't be too close to any port or node
                let tooClose = portIndex.tooClose(px, py, layoutgraph::MinPortClearance, guard)?;
                if tooClose {
                    continue;
                }
                // Charge candidate inspection before doing any geometry, but keep the
                // candidate on the stack until it survives both rejection filters.
                guard.step()?;
                let nearNode = proximityIndex.pointNear(px, py, guard)?;
                if nearNode {
                    continue;
                }
                let unobstructed = hasUnobstructedLineToPorts(geo::Point { X: px, Y: py }, self, &portIndex, 2, guard)?;
                if unobstructed {
                    let n = NewOVGNode(geo::Point { X: px, Y: py });
                    guard.addNode(self, &n)?;
                }
            }
        }
        guard.check()
    }

    /// For each edge in tala.Edges, add nodes on the edge perimeter (bounding box) and half-way through the bounding box
    pub(crate) fn addEdgesNodes(&mut self, g: &GraphRef, guard: &mut ovgBuildGuard) -> GoResult<()> {
        guard.check()?;
        let (graphNodes, gEdges) = {
            let gb = g.b();
            (gb.Nodes.clone(), gb.Edges.clone())
        };
        let fixedOverlaps = self.fixedOverlapsForBuild(g, &graphNodes, guard)?;
        let portIndex = newOVGPortIndex(&self.Ports, Some(g), &fixedOverlaps, guard)?;
        for edge in &gEdges {
            guard.step()?;
            if edge.IsLoop() {
                continue;
            }
            let (from, to) = {
                let e = edge.b();
                (e.From.clone(), e.To.clone())
            };
            let (fh, th) = (from.b().Hierarchy.clone(), to.b().Hierarchy.clone());
            if fh.is_some() && th.is_some() && th == fh {
                // if the edge is between two nodes in the same hierarchy, it was already handled
                // by the hierarchical OVG routine
                continue;
            }
            let mut fillPoints = self.perimeterPoints(&from, &to, guard)?;
            let halfwayPoints = self.halfwayPoints(&from, &to, guard)?;
            fillPoints.extend(halfwayPoints);
            for fillPoint in &fillPoints {
                guard.step()?;
                guard.step()?;
                let nearNode = guard.pointNearGraphNode(g, fillPoint)?;
                if nearNode {
                    continue;
                }
                let unobstructed = hasUnobstructedLineToPorts(*fillPoint, self, &portIndex, 1, guard)?;
                if unobstructed {
                    let fillPointNode = NewOVGNode(*fillPoint);
                    guard.addNode(self, &fillPointNode)?;
                }
            }
        }
        guard.check()
    }

    pub(crate) fn perimeterPoints(&mut self, node: &NodeRef, otherNode: &NodeRef, guard: &mut ovgBuildGuard) -> GoResult<Vec<geo::Point>> {
        let mut fillPoints: Vec<geo::Point> = Vec::new();
        let (ntl, nw, nh) = (node.tlv(), node.w(), node.h());
        let (otl, ow, oh) = (otherNode.tlv(), otherNode.w(), otherNode.h());
        // Perimeter points
        let top = math::Min(ntl.Y, otl.Y) - overshootAmount;
        let bottom = math::Max(ntl.Y + nh, otl.Y + oh) + overshootAmount;
        let left = math::Min(ntl.X, otl.X) - overshootAmount;
        let right = math::Max(ntl.X + nw, otl.X + ow) + overshootAmount;

        for graphNode in [node, otherNode] {
            guard.step()?;
            let topPorts = guard.portsByOrientation(self, graphNode, geo::Top)?;
            for portNode in &topPorts {
                guard.step()?;
                fillPoints.push(geo::Point { X: portNode.X(), Y: top });
            }
            let bottomPorts = guard.portsByOrientation(self, graphNode, geo::Bottom)?;
            for portNode in &bottomPorts {
                guard.step()?;
                fillPoints.push(geo::Point { X: portNode.X(), Y: bottom });
            }
            let leftPorts = guard.portsByOrientation(self, graphNode, geo::Left)?;
            for portNode in &leftPorts {
                guard.step()?;
                fillPoints.push(geo::Point { X: left, Y: portNode.Y() });
            }
            let rightPorts = guard.portsByOrientation(self, graphNode, geo::Right)?;
            for portNode in &rightPorts {
                guard.step()?;
                fillPoints.push(geo::Point { X: right, Y: portNode.Y() });
            }
        }

        fillPoints.push(geo::Point { X: left, Y: top });
        fillPoints.push(geo::Point { X: right, Y: top });
        fillPoints.push(geo::Point { X: left, Y: bottom });
        fillPoints.push(geo::Point { X: right, Y: bottom });
        guard.check()?;
        Ok(fillPoints)
    }

    /// add points halfway between node and otherNode
    pub(crate) fn halfwayPoints(&mut self, node: &NodeRef, otherNode: &NodeRef, guard: &mut ovgBuildGuard) -> GoResult<Vec<geo::Point>> {
        let mut fillPoints: Vec<geo::Point> = Vec::new();
        let orientation = node.Orientation(otherNode);
        if orientation == geo::NONE {
            guard.check()?;
            return Ok(fillPoints);
        }
        // Halfway points
        // Fill from node to otherNode, which means if node is to the left of otherNode, we are filling rightwards, from node
        let mut fillTop = false;
        let mut fillRight = false;
        let mut fillBottom = false;
        let mut fillLeft = false;
        if orientation == geo::Top {
            fillBottom = true;
        } else if orientation == geo::TopLeft {
            fillBottom = true;
            fillRight = true;
        } else if orientation == geo::TopRight {
            fillBottom = true;
            fillLeft = true;
        } else if orientation == geo::Bottom {
            fillTop = true;
        } else if orientation == geo::BottomLeft {
            fillTop = true;
            fillRight = true;
        } else if orientation == geo::BottomRight {
            fillTop = true;
            fillLeft = true;
        } else if orientation == geo::Left {
            fillRight = true;
        } else if orientation == geo::Right {
            fillLeft = true;
        }

        let mut nodeTL = node.tlv();
        let mut nodeBR = geo::Point { X: math::Round(nodeTL.X + node.w()), Y: math::Round(nodeTL.Y + node.h()) };

        let mut otherNodeTL = otherNode.tlv();
        let mut otherNodeBR = geo::Point { X: math::Round(otherNodeTL.X + otherNode.w()), Y: math::Round(otherNodeTL.Y + otherNode.h()) };

        let unwrapBox = |r: (Option<geo::Point>, Option<geo::Point>)| -> (geo::Point, geo::Point) {
            (r.0.expect("nil pointer dereference: bounding box"), r.1.expect("nil pointer dereference: bounding box"))
        };

        // all cluster nodes should use the same halfway points based on the whole cluster
        if let Some(c) = node.clusterField() {
            let nodes = c.b().Nodes.clone();
            (nodeTL, nodeBR) = unwrapBox(guard.tightBoundingBox(&nodes)?);
        }
        if let Some(c) = otherNode.clusterField() {
            let nodes = c.b().Nodes.clone();
            (otherNodeTL, otherNodeBR) = unwrapBox(guard.tightBoundingBox(&nodes)?);
        }

        let mut nodePorts: Vec<OVGNodeRef> = self.Ports.get(node).cloned().unwrap_or_default();
        let mut otherNodePorts: Vec<OVGNodeRef> = self.Ports.get(otherNode).cloned().unwrap_or_default();
        if let Some(s) = node.sequenceField() {
            let nodes = s.b().Nodes.clone();
            (nodeTL, nodeBR) = unwrapBox(guard.tightBoundingBox(&nodes)?);
            for n in &nodes {
                guard.step()?;
                if n != node {
                    if let Some(p) = self.Ports.get(n) {
                        nodePorts.extend(p.iter().cloned());
                    }
                }
            }
        }
        if let Some(s) = otherNode.sequenceField() {
            let nodes = s.b().Nodes.clone();
            (otherNodeTL, otherNodeBR) = unwrapBox(guard.tightBoundingBox(&nodes)?);
            for n in &nodes {
                guard.step()?;
                if n != otherNode {
                    if let Some(p) = self.Ports.get(n) {
                        otherNodePorts.extend(p.iter().cloned());
                    }
                }
            }
        }

        if fillTop {
            let y = (nodeTL.Y + otherNodeBR.Y) / 2.0;
            for portNode in nodePorts.iter().chain(otherNodePorts.iter()) {
                guard.step()?;
                fillPoints.push(geo::Point { X: portNode.X(), Y: y });
            }
        }
        if fillBottom {
            let y = (otherNodeTL.Y + nodeBR.Y) / 2.0;
            for portNode in nodePorts.iter().chain(otherNodePorts.iter()) {
                guard.step()?;
                fillPoints.push(geo::Point { X: portNode.X(), Y: y });
            }
        }
        if fillLeft {
            let x = (nodeTL.X + otherNodeBR.X) / 2.0;
            for portNode in nodePorts.iter().chain(otherNodePorts.iter()) {
                guard.step()?;
                fillPoints.push(geo::Point { X: x, Y: portNode.Y() });
            }
        }
        if fillRight {
            let x = (otherNodeTL.X + nodeBR.X) / 2.0;
            for portNode in nodePorts.iter().chain(otherNodePorts.iter()) {
                guard.step()?;
                fillPoints.push(geo::Point { X: x, Y: portNode.Y() });
            }
        }

        // These are the corner points for the halfway fill points
        if fillRight && fillBottom {
            let x = (otherNodeTL.X + nodeBR.X) / 2.0;
            let y = (otherNodeTL.Y + nodeBR.Y) / 2.0;
            fillPoints.push(geo::Point { X: x, Y: y });
        }

        if fillRight && fillTop {
            let x = (otherNodeTL.X + nodeBR.X) / 2.0;
            let y = (nodeTL.Y + otherNodeBR.Y) / 2.0;
            fillPoints.push(geo::Point { X: x, Y: y });
        }

        if fillLeft && fillTop {
            let x = (nodeTL.X + otherNodeBR.X) / 2.0;
            let y = (nodeTL.Y + otherNodeBR.Y) / 2.0;
            fillPoints.push(geo::Point { X: x, Y: y });
        }

        if fillLeft && fillBottom {
            let x = (nodeTL.X + otherNodeBR.X) / 2.0;
            let y = (otherNodeTL.Y + nodeBR.Y) / 2.0;
            fillPoints.push(geo::Point { X: x, Y: y });
        }
        guard.check()?;
        Ok(fillPoints)
    }

    pub(crate) fn addTreeNodes(&mut self, g: &GraphRef, guard: &mut ovgBuildGuard) -> GoResult<()> {
        guard.check()?;
        let mut portArrowheads: Map<OVGNodeRef, Set<layoutgraph::Arrowhead>> = Map::default();
        // If there are trees in this subgraph, we need to add ovg nodes for the specific tree edge routes
        let (gNodes, nodeToTree) = {
            let gb = g.b();
            (gb.Nodes.clone(), gb.NodeToTree.clone())
        };
        for node in &gNodes {
            guard.step()?;
            let tree = nodeToTree.get(node);
            if let Some(tree) = tree {
                let treePath = treeEdgePathForBuild(&tree, &self.Ports, &portArrowheads, Some(&mut *guard))?;

                guard.addPoint(self, treePath.SourceMidpoint)?;
                guard.addPoint(self, treePath.TargetMidpoint)?;
                let sentinelEdge = tree.b().SentinelEdge.clone().expect("tree without sentinel edge");
                // if the shape centers are misaligned, treePath may use a new parent-aligned port so we add it here
                if isSentinelEdgeSource(Some(&tree)) {
                    let port = guard.addNode(self, &treePath.SourcePortNode)?;
                    if !port.isPortOf(node) {
                        self.Ports.entry(node.clone()).or_default().push(port.clone());
                        port.addPortOwner(node, treePath.SourceOrientationToTarget.GetOpposite(), false);
                    }

                    let ah = sentinelEdge.b().TargetArrowhead.clone();
                    portArrowheads.entry(treePath.TargetPortNode.clone()).or_default().insert(ah);
                } else {
                    let port = guard.addNode(self, &treePath.TargetPortNode)?;
                    if !port.isPortOf(node) {
                        self.Ports.entry(node.clone()).or_default().push(port.clone());
                        port.addPortOwner(node, treePath.SourceOrientationToTarget, false);
                    }

                    let ah = sentinelEdge.b().SourceArrowhead.clone();
                    portArrowheads.entry(treePath.SourcePortNode.clone()).or_default().insert(ah);
                }
            }
        }
        guard.check()
    }

    /// addNewBoundaryLayers adds new nodes at the graph boundaries
    pub(crate) fn addNewBoundaryLayers(&mut self, g: &GraphRef, tl: geo::Point, br: geo::Point, guard: &mut ovgBuildGuard) -> GoResult<()> {
        guard.check()?;
        // Don't count containers for children obstruction
        // Pad boundaries with points to allow edges to reach around
        let mut i = 0.0f64;
        while i < extraInterestingPointLayers {
            guard.step()?;
            // Go ranges over the slice header captured at loop start.
            let n = self.Nodes.len();
            for k in 0..n {
                guard.step()?;
                let p = self.Nodes[k].P();
                let mut point: Option<geo::Point> = None;
                if p.Y == tl.Y {
                    point = Some(geo::Point { X: p.X, Y: p.Y - (ovgPadding * (i + 1.0)) });
                }
                if p.Y == br.Y {
                    point = Some(geo::Point { X: p.X, Y: p.Y + (ovgPadding * (i + 1.0)) });
                }
                if p.X == tl.X {
                    point = Some(geo::Point { X: p.X - (ovgPadding * (i + 1.0)), Y: p.Y });
                }
                if p.X == br.X {
                    point = Some(geo::Point { X: p.X + (ovgPadding * (i + 1.0)), Y: p.Y });
                }

                if let Some(point) = point {
                    let nearNode = guard.pointNearGraphNode(g, &point)?;
                    if !nearNode {
                        guard.addPoint(self, point)?;
                    }
                }
            }
            i += 1.0;
        }
        Ok(())
    }

    /// For each given OVG node, try to add a new node at the graph boundaries that make a direct connection to this node
    pub(crate) fn addPortConnectionNodesAtBoundaries(&mut self, g: &GraphRef, tl: geo::Point, br: geo::Point, guard: &mut ovgBuildGuard) -> GoResult<()> {
        guard.check()?;
        let mut i = 0.0f64;
        while i < extraInterestingPointLayers {
            guard.step()?;
            let portsEntries: Vec<(NodeRef, Vec<OVGNodeRef>)> = self.Ports.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
            for (node, ports) in &portsEntries {
                guard.step()?;
                let mut added = false;
                let mut seenPorts: Set<OVGNodeRef> = gocompat::NewSetCap(ports.len());
                for portNode in ports {
                    guard.step()?;
                    if seenPorts.contains(portNode) {
                        continue;
                    }
                    seenPorts.insert(portNode.clone());
                    let (portDirections, _) = portNode.portDirectionsFor(node);
                    let pp = portNode.P();
                    let mut directionErr: Option<gocompat::GoError> = None;
                    let nodesInside = self.NodesInsideBoundingBox.clone();
                    portDirections.any(|portDirection| {
                        let r: GoResult<()> = (|| {
                            guard.step()?;
                            let topPoint = geo::Point { X: pp.X, Y: tl.Y - (ovgPadding * (i + 1.0)) };
                            let bottomPoint = geo::Point { X: pp.X, Y: br.Y + (ovgPadding * (i + 1.0)) };
                            let leftPoint = geo::Point { X: tl.X - (ovgPadding * (i + 1.0)), Y: pp.Y };
                            let rightPoint = geo::Point { X: br.X + (ovgPadding * (i + 1.0)), Y: pp.Y };

                            for boundaryPoint in [topPoint, bottomPoint, leftPoint, rightPoint] {
                                guard.step()?;
                                let mut passesThroughARectangle = false;
                                for rectangle in &nodesInside {
                                    guard.step()?;
                                    let empty = Vec::new();
                                    let rports = self.Ports.get(rectangle).unwrap_or(&empty);
                                    let passesThrough = guard.passesThroughAllowingPorts(rectangle, &pp, &boundaryPoint, portDirection, rports)?;
                                    if passesThrough {
                                        // We make exception for container
                                        let mut isRectangleNodeContainer = false;
                                        if rectangle.IsContainer() {
                                            let descendant = guard.isDescendantOf(Some(node), Some(rectangle))?;
                                            if descendant {
                                                isRectangleNodeContainer = true;
                                            }
                                        }
                                        if !isRectangleNodeContainer {
                                            passesThroughARectangle = true;
                                            break;
                                        }
                                    }
                                }
                                if passesThroughARectangle {
                                    continue;
                                }
                                let nearNode = guard.pointNearGraphNode(g, &boundaryPoint)?;
                                if !nearNode {
                                    guard.addPoint(self, boundaryPoint)?;
                                    added = true;
                                }
                            }
                            Ok(())
                        })();
                        match r {
                            Ok(()) => false,
                            Err(e) => {
                                directionErr = Some(e);
                                true
                            }
                        }
                    });
                    if let Some(e) = directionErr {
                        return Err(e);
                    }
                }
                if !added {
                    self.createPortsConnectionsToBoundaries(node, ports, tl, br, guard)?;
                }
            }
            i += 1.0;
        }
        Ok(())
    }

    pub(crate) fn addCornerNodes(&mut self, g: &GraphRef, tl: geo::Point, br: geo::Point, guard: &mut ovgBuildGuard) -> GoResult<()> {
        guard.check()?;
        // Fill in the corners of boundary points
        let mut i = 0.0f64;
        while i < extraInterestingPointLayers {
            guard.step()?;
            let points = [
                geo::Point { X: tl.X - (ovgPadding * (i + 1.0)), Y: tl.Y - (ovgPadding * (i + 1.0)) },
                geo::Point { X: br.X + (ovgPadding * (i + 1.0)), Y: tl.Y - (ovgPadding * (i + 1.0)) },
                geo::Point { X: br.X + (ovgPadding * (i + 1.0)), Y: br.Y + (ovgPadding * (i + 1.0)) },
                geo::Point { X: tl.X - (ovgPadding * (i + 1.0)), Y: br.Y + (ovgPadding * (i + 1.0)) },
            ];

            for point in points {
                let nearNode = guard.pointNearGraphNode(g, &point)?;
                if !nearNode {
                    guard.addPoint(self, point)?;
                }
            }
            i += 1.0;
        }
        guard.check()
    }

    pub(crate) fn addTunnels(&mut self, g: &GraphRef, guard: &mut ovgBuildGuard) -> GoResult<()> {
        guard.check()?;
        let mut tunnels = buildTunnels(g, guard)?;
        for tunnel in tunnels.iter_mut() {
            guard.step()?;
            for entry in [&mut tunnel.EntryA, &mut tunnel.EntryB] {
                guard.step()?;
                let candidate = entry.OVGNode.clone();
                let canonical = guard.addNode(self, &candidate)?;
                if canonical != candidate {
                    for (owner, metadata) in candidate.portOwners() {
                        guard.step()?;
                        canonical.addPortMetadata(&owner, metadata);
                    }
                    entry.OVGNode = canonical;
                }
                entry.OVGNode.bm().IsTunnel = true;
                self.Ports.entry(entry.Node.clone()).or_default().push(entry.OVGNode.clone());
            }

            let (a, b) = (tunnel.EntryA.OVGNode.clone(), tunnel.EntryB.OVGNode.clone());
            guard.connect(self, &a, &b)?;
        }
        guard.check()
    }

    pub(crate) fn connectPortsToCenter(&mut self, guard: &mut ovgBuildGuard) -> GoResult<()> {
        guard.check()?;
        let nodes = self.NodesInsideBoundingBox.clone();
        for node in &nodes {
            guard.step()?;
            let centerNode = guard.newCandidateNode(*node.Center().b())?;
            self.Centers.insert(node.clone(), centerNode.clone());
            // We don't care that it's the center of an invisible node, since that's not special
            if !node.b().IsInvisible {
                centerNode.bm().IsNodeCenter = true;
            }
            guard.addNodeUnchecked(self, &centerNode)?;
            let ports = self.Ports.get(node).cloned().unwrap_or_default();
            for portNode in &ports {
                guard.connect(self, &centerNode, portNode)?;
            }
        }
        guard.check()
    }

    pub(crate) fn mapNodesToContainer(&mut self, g: &GraphRef, guard: &mut ovgBuildGuard) -> GoResult<()> {
        guard.check()?;
        let dfsContainerOrder = guard.containerRDFSOrder(g, None)?;
        for ovgNode in &self.Nodes {
            guard.step()?;
            let p = ovgNode.P();
            for container in &dfsContainerOrder {
                guard.step()?;
                if container.ContainsPointOnBox(&p) {
                    ovgNode.bm().Container = Some(container.clone());
                    break;
                }
            }
        }
        Ok(())
    }

    /// connectNodes connects all OVG nodes to the nearest node on the left, top, right and bottom if the edge
    /// doesn't go through any graph node.
    pub(crate) fn connectNodes(&mut self, g: &GraphRef, guard: &mut ovgBuildGuard) -> GoResult<()> {
        guard.check()?;
        if self.Edges.len() as u64 > guard.edges {
            let need = self.Edges.len() as u64 - guard.edges;
            guard.reserveEdges(need)?;
        }
        // Create edges
        let mut horizontal: Map<u64, (f64, Vec<OVGNodeRef>)> = Map::default();
        let mut vertical: Map<u64, (f64, Vec<OVGNodeRef>)> = Map::default();
        let mut eligibleNodeCount = 0u64;
        let nodes = self.Nodes.clone();
        for node in &nodes {
            guard.step()?;
            // Tunnels can only connect to each other (and to their node centers)
            if node.b().IsTunnel {
                continue;
            }

            // sequence steps should only connect from certain sides
            let restricted = guard.isRestrictedSequencePort(node)?;
            if restricted {
                continue;
            }

            eligibleNodeCount += 1;
            let p = node.P();
            {
                let e = horizontal.entry(f64key(p.Y)).or_insert((p.Y, Vec::new()));
                e.0 = p.Y;
                e.1.push(node.clone());
            }
            {
                let e = vertical.entry(f64key(p.X)).or_insert((p.X, Vec::new()));
                e.0 = p.X;
                e.1.push(node.clone());
            }
        }

        fn sortLines(lines: &Map<u64, (f64, Vec<OVGNodeRef>)>, guard: &mut ovgBuildGuard) -> GoResult<Vec<(u64, f64)>> {
            let mut coords: Vec<(u64, f64)> = Vec::new();
            for (k, v) in lines.iter() {
                guard.step()?;
                coords.push((*k, v.0));
            }
            guard.reserveSortWork(coords.len())?;
            gocompat::sort::SlicesSortFunc(&mut coords, |a, b| gocompat::sort::cmpCompare(a.1, b.1));
            Ok(coords)
        }

        let h = horizontal.len() as u64;
        let v = vertical.len() as u64;
        let edgeCapacity = checkedOVGEdgeCapacity(self.Edges.len() as u64, eligibleNodeCount, h, v, guard.limits.edges, maxIntAsUint64())?;
        // we need to copy existing edges (e.g., we already connect the nodes when adding tunnels)
        let mut edges: Vec<OVGEdgeRef> = Vec::with_capacity(edgeCapacity.max(self.Edges.len()));
        edges.extend(self.Edges.iter().cloned());
        self.Edges = edges;
        let boundingNodes = self.NodesInsideBoundingBox.clone();
        let fixedOverlaps = self.fixedOverlapsForBuild(g, &boundingNodes, guard)?;
        let horizontalLines = sortLines(&horizontal, guard)?;
        for (k, y) in &horizontalLines {
            guard.step()?;
            let mut line = std::mem::take(&mut horizontal.get_mut(k).unwrap().1);
            self.connectNodesOnSameLine(&mut line, *y, true, &fixedOverlaps, guard)?;
        }

        let verticalLines = sortLines(&vertical, guard)?;
        for (k, x) in &verticalLines {
            guard.step()?;
            let mut line = std::mem::take(&mut vertical.get_mut(k).unwrap().1);
            self.connectNodesOnSameLine(&mut line, *x, false, &fixedOverlaps, guard)?;
        }
        Ok(())
    }

    /// connectNodesOnSameLine connects nodes on the same horizontal/vertical line.
    pub(crate) fn connectNodesOnSameLine(&mut self, nodes: &mut Vec<OVGNodeRef>, linePosition: f64, isHorizontal: bool, fixedOverlaps: &Set<NodeRef>, guard: &mut ovgBuildGuard) -> GoResult<()> {
        guard.check()?;

        let mut intersectCandidates: Vec<NodeRef> = Vec::new();
        for node in &self.NodesInsideBoundingBox {
            guard.step()?;
            if fixedOverlaps.contains(node) {
                continue;
            }
            let tl = node.tlv();
            let (w, h) = (node.w(), node.h());
            if isHorizontal && tl.Y <= linePosition && tl.Y + h >= linePosition {
                intersectCandidates.push(node.clone());
            } else if !isHorizontal && tl.X <= linePosition && tl.X + w >= linePosition {
                intersectCandidates.push(node.clone());
            }
        }
        guard.reserveSortWork(intersectCandidates.len())?;

        // Sort by top-left (pre-read keys; the comparator only reads them).
        {
            let mut keyed: Vec<(f64, NodeRef)> = intersectCandidates
                .iter()
                .map(|n| {
                    let tl = n.tlv();
                    (if isHorizontal { tl.X } else { tl.Y }, n.clone())
                })
                .collect();
            gocompat::sort::SortSlice(&mut keyed, |a, b| a.0 < b.0);
            intersectCandidates = keyed.into_iter().map(|(_, n)| n).collect();
        }

        guard.reserveSortWork(nodes.len())?;
        {
            let mut keyed: Vec<(f64, OVGNodeRef)> = nodes
                .iter()
                .map(|n| {
                    let p = n.P();
                    (if isHorizontal { p.X } else { p.Y }, n.clone())
                })
                .collect();
            gocompat::sort::SortSlice(&mut keyed, |a, b| a.0 < b.0);
            *nodes = keyed.into_iter().map(|(_, n)| n).collect();
        }

        // Cache the candidates' geometry and ports (immutable during the sweep).
        let candGeo: Vec<(geo::Point, f64, f64, bool, bool)> = intersectCandidates
            .iter()
            .map(|r| (r.tlv(), r.w(), r.h(), r.b().Sequence.is_some(), r.IsContainer()))
            .collect();

        let mut startIntersectionSearchAt = 0usize;
        let mut i = 0usize;
        while i + 1 < nodes.len() {
            guard.step()?;
            let node = nodes[i].clone();
            let closest = nodes[i + 1].clone();
            let misdirected = self.isMisdirectedPortPair(&node, &closest, isHorizontal, guard)?;
            if misdirected {
                i += 1;
                continue;
            }
            let np = node.P();
            let cp = closest.P();

            let mut shouldConnect = true;
            let mut j = startIntersectionSearchAt;
            while j < intersectCandidates.len() {
                guard.step()?;
                let rectangle = &intersectCandidates[j];
                let (rtl, _rw, rh, rIsSeq, rIsContainer) = candGeo[j];
                if (isHorizontal && rtl.X > cp.X) || (!isHorizontal && rtl.Y > cp.Y) {
                    // here, rectangle is after closest to the right or to the bottom
                    // since intersectCandidates is sorted, no other nodes will intersect, so we can skip the test
                    break;
                }

                let empty = Vec::new();
                let rports = self.Ports.get(rectangle).unwrap_or(&empty);
                let mut passesThrough = |from: &OVGNodeRef, fromP: &geo::Point, toP: &geo::Point, guard: &mut ovgBuildGuard| -> GoResult<bool> {
                    let mut passes = true;
                    let mut directionErr: Option<gocompat::GoError> = None;
                    from.portDirectionsForObstacle(rectangle).any(|direction| {
                        match guard.passesThroughAllowingPorts(rectangle, fromP, toP, direction, rports) {
                            Err(e) => {
                                directionErr = Some(e);
                                true
                            }
                            Ok(pt) => {
                                if !pt {
                                    passes = false;
                                    return true;
                                }
                                false
                            }
                        }
                    });
                    match directionErr {
                        Some(e) => Err(e),
                        None => Ok(passes),
                    }
                };
                let c1 = passesThrough(&node, &np, &cp, guard)?;
                let c2 = passesThrough(&closest, &cp, &np, guard)?;
                if c1 && c2 {
                    if rIsSeq {
                        // sequences will have top and bottom ports overlapped by previous sequence nodes, but this overlap shouldn't prevent a connection with that port
                        if !isHorizontal {
                            if rtl.Y == cp.Y || rtl.Y + rh == np.Y {
                                j += 1;
                                continue;
                            }
                        }
                    }
                    // Passing through a container is okay ...
                    let mut isContainerException = false;
                    if rIsContainer {
                        // ... if at least one of the nodes is within the container
                        if rectangle.ContainsPointOnBox(&np) || rectangle.ContainsPointOnBox(&cp) {
                            isContainerException = true;
                        }
                    }

                    if !isContainerException {
                        shouldConnect = false;
                        break;
                    }
                } else {
                    // this node didn't intersect with the current OVG, so we don't need to
                    // check any other nodes prior to it for the next OVG nodes
                    // (the loop is on top of sorted nodes)
                    startIntersectionSearchAt = j;
                }
                j += 1;
            }

            if shouldConnect {
                guard.connect(self, &node, &closest)?;
            }
            i += 1;
        }
        Ok(())
    }

    /// Note: assumes first node is left(IsHorizontal=true) or above(IsHorizontal=false) second
    pub(crate) fn isMisdirectedPortPair(&self, first: &OVGNodeRef, second: &OVGNodeRef, isHorizontal: bool, guard: &mut ovgBuildGuard) -> GoResult<bool> {
        guard.check()?;
        let firstOwners = first.portOwners();
        let secondOwners = second.portOwners();
        if firstOwners.is_empty() || secondOwners.is_empty() {
            guard.check()?;
            return Ok(false);
        }

        // A shared point can represent ports of several touching nodes. Reject the
        // connection only if every possible owner pair is misdirected; one valid
        // interpretation is enough to make the OVG segment usable.
        for (firstOwner, firstMetadata) in &firstOwners {
            guard.step()?;
            for (secondOwner, secondMetadata) in &secondOwners {
                guard.step()?;
                if firstOwner == secondOwner {
                    return Ok(false);
                }

                // Since first is left/above second, at least one pair of roles must
                // face each other (not the other way around).
                let mut correctDirections = firstMetadata.directions.has(geo::Bottom) && secondMetadata.directions.has(geo::Top);
                if isHorizontal {
                    correctDirections = firstMetadata.directions.has(geo::Right) && secondMetadata.directions.has(geo::Left);
                }
                // ports must face each other unless it is a descendant and an ancestor
                let firstBelowSecond = guard.isDescendantOf(Some(firstOwner), Some(secondOwner))?;
                let secondBelowFirst = guard.isDescendantOf(Some(secondOwner), Some(firstOwner))?;
                if correctDirections || firstBelowSecond || secondBelowFirst {
                    return Ok(false);
                }
            }
        }
        guard.check()?;
        Ok(true)
    }

    /// createPortsConnectionsToBoundaries will create paths for isolated graph nodes.
    pub(crate) fn createPortsConnectionsToBoundaries(&mut self, owner: &NodeRef, ports: &[OVGNodeRef], tl: geo::Point, br: geo::Point, guard: &mut ovgBuildGuard) -> GoResult<()> {
        guard.check()?;
        let mut seenPorts: Set<OVGNodeRef> = gocompat::NewSetCap(ports.len());
        for port in ports {
            guard.step()?;
            if seenPorts.contains(port) {
                continue;
            }
            seenPorts.insert(port.clone());
            let (portDirections, _) = port.portDirectionsFor(owner);
            let pp = port.P();
            let mut directionErr: Option<gocompat::GoError> = None;
            portDirections.any(|portDirection| {
                let r: GoResult<()> = (|| {
                    guard.step()?;
                    let newPoint = if portDirection == geo::Top {
                        geo::Point { X: pp.X, Y: pp.Y - layoutgraph::MinPortClearance }
                    } else if portDirection == geo::Bottom {
                        geo::Point { X: pp.X, Y: pp.Y + layoutgraph::MinPortClearance }
                    } else if portDirection == geo::Left {
                        geo::Point { X: pp.X - layoutgraph::MinPortClearance, Y: pp.Y }
                    } else if portDirection == geo::Right {
                        geo::Point { X: pp.X + layoutgraph::MinPortClearance, Y: pp.Y }
                    } else {
                        return Ok(());
                    };
                    let newNode = guard.addPoint(self, newPoint)?;
                    let np = newNode.P();

                    // add nodes at the OVG boundary to connect with this new node
                    let mut i = 1i64;
                    while (i as f64) < extraInterestingPointLayers + 1.0 {
                        guard.step()?;
                        if portDirection == geo::Top || portDirection == geo::Bottom {
                            // left boundary
                            guard.addPoint(self, geo::Point { X: tl.X - (ovgPadding * i as f64), Y: np.Y })?;
                            // right boundary
                            guard.addPoint(self, geo::Point { X: br.X + (ovgPadding * i as f64), Y: np.Y })?;
                        } else if portDirection == geo::Left || portDirection == geo::Right {
                            // top boundary
                            guard.addPoint(self, geo::Point { X: np.X, Y: tl.Y - (ovgPadding * i as f64) })?;
                            // bottom boundary
                            guard.addPoint(self, geo::Point { X: np.X, Y: br.Y + (ovgPadding * i as f64) })?;
                        }
                        i += 1;
                    }
                    Ok(())
                })();
                match r {
                    Ok(()) => false,
                    Err(e) => {
                        directionErr = Some(e);
                        true
                    }
                }
            });
            if let Some(e) = directionErr {
                return Err(e);
            }
        }
        guard.check()
    }

    /// flagNodesNearPorts finds OVG Nodes that are lined up and too close to ports as they should be penalized during edge routing
    pub(crate) fn flagNodesNearPorts(&mut self, guard: &mut ovgBuildGuard) -> GoResult<()> {
        guard.check()?;
        for (owner, ports) in self.Ports.iter() {
            guard.step()?;
            for port in ports {
                guard.step()?;
                let pp = port.P();
                // 5 is just a rough estimate for the port + 4 adjacent nodes
                let mut seen: Set<OVGNodeRef> = gocompat::NewSetCap(5);
                seen.insert(port.clone());
                let mut queue: std::collections::VecDeque<OVGNodeRef> = std::collections::VecDeque::with_capacity(5);
                let pedges = port.b().Edges.clone();
                for e in &pedges {
                    guard.step()?;
                    let adj = port.adjacent(e);
                    let ap = adj.P();
                    if ap.X != pp.X && ap.Y != pp.Y {
                        continue;
                    }
                    queue.push_back(adj);
                }
                while let Some(curr) = queue.pop_front() {
                    guard.step()?;
                    if seen.contains(&curr) {
                        continue;
                    }
                    seen.insert(curr.clone());

                    // only check non-ports, or ports of other nodes
                    // but, if adj is a port of the current graph node, also follow its adjacents
                    if !curr.isPortOf(owner) {
                        let cp = curr.P();
                        if math::Abs(cp.Y - pp.Y) >= nodeProximityThreshold {
                            // the current node is not too close to the port, so no need to investigate the adjacents
                            continue;
                        }
                        if math::Abs(cp.X - pp.X) >= nodeProximityThreshold {
                            continue;
                        }

                        // `curr` is near the port
                        curr.bm().IsNearPort.get_or_insert_with(Set::default).insert(owner.clone());
                    }

                    // follow other nodes, as they be near too
                    let cedges = curr.b().Edges.clone();
                    for e in &cedges {
                        guard.step()?;
                        let adj = curr.adjacent(e);
                        let ap = adj.P();
                        if ap.X != pp.X && ap.Y != pp.Y {
                            continue;
                        }
                        queue.push_back(adj);
                    }
                }
            }
        }
        guard.check()
    }

    pub(crate) fn mergePorts(&mut self, other: &OVG, guard: &mut ovgBuildGuard) -> GoResult<()> {
        guard.check()?;
        for (n, ports) in other.Ports.iter() {
            guard.reserveWork(ports.len() as u64 + 1)?;
            let mut canonicalPorts: Vec<OVGNodeRef> = Vec::with_capacity(ports.len());
            for port in ports {
                let canonical = guard.addNode(self, port)?;
                if canonical != *port {
                    for (owner, metadata) in port.portOwners() {
                        guard.step()?;
                        canonical.addPortMetadata(&owner, metadata);
                    }
                }
                canonicalPorts.push(canonical);
            }
            self.Ports.insert(n.clone(), canonicalPorts);
        }
        guard.reserveWork(other.NodesInsideBoundingBox.len() as u64)?;
        self.NodesInsideBoundingBox.extend(other.NodesInsideBoundingBox.iter().cloned());
        guard.check()
    }

    pub(crate) fn mergeNonPorts(&mut self, other: &OVG, guard: &mut ovgBuildGuard) -> GoResult<()> {
        guard.check()?;
        for n in &other.Nodes {
            guard.step()?;
            if !n.isPort() {
                guard.addNode(self, n)?;
            }
        }
        guard.check()
    }
}

/// Go `(*OVG).MarshalJSON` node/edge serialization order helper.
pub(crate) fn ovgNodeOrder(n1: &geo::Point, n2: &geo::Point) -> bool {
    if n1.X == n2.X {
        return n1.Y < n2.Y;
    }
    n1.X < n2.X
}

impl OVGRef {
    /// Go `(*OVG).MarshalJSON`.
    pub fn MarshalJSON(&self) -> GoResult<Vec<u8>> {
        let g = self.b();
        let mut sortedNodes: Vec<geo::Point> = g.Nodes.iter().map(|n| n.P()).collect();
        gocompat::sort::SortSlice(&mut sortedNodes, |a, b| ovgNodeOrder(a, b));
        let mut sortedEdges: Vec<(OVGNodeRef, geo::Point, geo::Point)> = g.Edges.iter().map(|e| (e.From.clone(), e.From.P(), e.To.P())).collect();
        gocompat::sort::SortSlice(&mut sortedEdges, |a, b| if a.0 == b.0 { ovgNodeOrder(&a.2, &b.2) } else { ovgNodeOrder(&a.1, &b.1) });
        let f = |x: f64| gocompat::strconv::FormatFloatV(x);
        let mut s = String::from("{\"nodes\":");
        if sortedNodes.is_empty() {
            s.push_str("null");
        } else {
            s.push('[');
            for (i, p) in sortedNodes.iter().enumerate() {
                if i > 0 {
                    s.push(',');
                }
                s.push_str(&format!("{{\"x\":{},\"y\":{}}}", f(p.X), f(p.Y)));
            }
            s.push(']');
        }
        s.push_str(",\"edges\":");
        if sortedEdges.is_empty() {
            s.push_str("null");
        } else {
            s.push('[');
            for (i, e) in sortedEdges.iter().enumerate() {
                if i > 0 {
                    s.push(',');
                }
                s.push_str(&format!("{{\"from\":{{\"x\":{},\"y\":{}}},\"to\":{{\"x\":{},\"y\":{}}}}}", f(e.1.X), f(e.1.Y), f(e.2.X), f(e.2.Y)));
            }
            s.push(']');
        }
        s.push('}');
        Ok(s.into_bytes())
    }
}


#[cfg(test)]
#[path = "ovg_parity_test.rs"]
mod ovg_parity_test;

#[cfg(test)]
#[path = "ovg_half_a_unit_test.rs"]
mod ovg_half_a_unit_test;
