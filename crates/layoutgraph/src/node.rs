// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/layoutgraph/node.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

pub(crate) const defaultIconSize: f64 = 32.0;
pub(crate) const threeDOffset: f64 = 15.0;
pub(crate) const multipleOffset: f64 = 10.0;

/// label.PADDING as a float64 (untyped Go constant in float context).
pub(crate) const LABEL_PADDING: f64 = label::PADDING as f64;

impl NodeRef {
    /// HasLeakyEdge checks if
    /// 1. node is a container
    /// 2. a descendant has an edge going outside the container
    pub fn HasLeakyEdge(&self) -> bool {
        if !self.b().isContainer {
            return false;
        }

        let mut hasLeakyEdge = false;
        let node = self.clone();
        self.dfsWalk(&mut |descendant: &NodeRef| -> bool {
            if *descendant == node {
                return false;
            }
            for e in descendant.edges() {
                let adj = descendant.adjacent(&e);
                if !isDescendantOf(Some(&adj), Some(&node)) {
                    hasLeakyEdge = true;
                    return true;
                }
            }
            false
        });
        hasLeakyEdge
    }

    pub(crate) fn fitNodeToGraph(&self, g: &GraphRef, padding: Spacing) {
        let nodes = g.b().Nodes.clone();
        let (tl, br) = nodes.fixedBounds();
        let (tl, br) = (tl.unwrap(), br.unwrap());
        self.expandForLabels(&tl, &br);
        self.fitToBoundingBox(&tl, &br, padding);
    }

    pub(crate) fn fitToBoundingBox(&self, tl: &geo::PointRef, br: &geo::PointRef, padding: Spacing) {
        let tlv = *tl.b();
        let brv = *br.b();
        let mut width = brv.X - tlv.X;
        let mut height = brv.Y - tlv.Y;
        let (desiredWidth, desiredHeight) = {
            let n = self.b();
            (n.DesiredWidth, n.DesiredHeight)
        };
        if let Some(lbl) = self.label() {
            let l = lbl.b();
            if !l.Position.IsOutside() {
                let minWidth = l.Width - padding.left - padding.right + LABEL_PADDING * 4.0;
                let minHeight = l.Height - padding.top - padding.bottom + LABEL_PADDING * 4.0;
                if desiredWidth.is_none() {
                    width = fmax(width, minWidth);
                }
                if desiredHeight.is_none() {
                    height = fmax(height, minHeight);
                }
            }
        }
        // TODO move type Spacing to d2 and update GetDimensionsToFit for all shapes
        let shape = self.shape();
        let (fitWidth, fitHeight) =
            shape.GetDimensionsToFit(width, height, padding.left + padding.right, padding.top + padding.bottom);
        if let Some(dw) = desiredWidth {
            self.set_w(fmax(fitWidth, dw));
        } else {
            self.set_w(fitWidth);
        }
        if let Some(dh) = desiredHeight {
            self.set_h(fmax(fitHeight, dh));
        } else {
            self.set_h(fitHeight);
        }
    }

    pub(crate) fn bounds(&self, allNodes: Option<&[NodeRef]>) -> (geo::PointRef, geo::PointRef) {
        self.boundsWithRounding(allNodes, true)
    }

    pub(crate) fn boundsWithRounding(
        &self,
        allNodes: Option<&[NodeRef]>,
        roundDimensions: bool,
    ) -> (geo::PointRef, geo::PointRef) {
        let (tl, br) = self.boundingBoxValues(allNodes, roundDimensions);
        (geo::PointRef::new(tl), geo::PointRef::new(br))
    }

    pub(crate) fn boundingBoxValues(&self, allNodes: Option<&[NodeRef]>, roundDimensions: bool) -> (geo::Point, geo::Point) {
        let mut tl = self.tlv();
        let (width, height) = (self.w(), self.h());
        let mut br = geo::Point { X: tl.X + width, Y: tl.Y + height };
        if roundDimensions {
            br.X = br.X.round();
            br.Y = br.Y.round();
        }
        let (dx, dy) = self.modifierElementAdjustments();
        if dx != 0.0 || dy != 0.0 {
            tl.Y -= dy;
            br.X += dx;
        }
        tl.X -= self.loopOffset(geo::Left);
        tl.Y -= self.loopOffset(geo::Top);
        br.X += self.loopOffset(geo::Right);
        br.Y += self.loopOffset(geo::Bottom);

        if let (Some(lbl), Some(allNodes)) = (self.label(), allNodes) {
            let (lpos, lw, lh) = {
                let l = lbl.b();
                (l.Position, l.Width, l.Height)
            };
            if lpos.IsOutside() {
                let labelTL = *self.LabelTopLeft(lpos, lw, lh).b();

                // include 2x padding so that outside labels will always be closest to their own node
                let boundaryOutsidePadding = LABEL_PADDING;
                // If the node is at the boundary, there won't be ambiguity
                let outsidePadding = 2.0 * LABEL_PADDING;

                if labelTL.X < tl.X {
                    if allNodes.Leftmost(self) {
                        tl.X = (labelTL.X - boundaryOutsidePadding).floor();
                    } else {
                        tl.X = (labelTL.X - outsidePadding).floor();
                    }
                }
                if labelTL.Y < tl.Y {
                    if allNodes.Topmost(self) {
                        tl.Y = (labelTL.Y - boundaryOutsidePadding).floor();
                    } else {
                        tl.Y = (labelTL.Y - outsidePadding).floor();
                    }
                }
                if labelTL.X > br.X {
                    if allNodes.Rightmost(self) {
                        br.X = (labelTL.X + lw + boundaryOutsidePadding).ceil();
                    } else {
                        br.X = (labelTL.X + lw + outsidePadding).ceil();
                    }
                }

                if labelTL.Y > br.Y {
                    if allNodes.Bottommost(self) {
                        br.Y = (labelTL.Y + lh + boundaryOutsidePadding).ceil();
                    } else {
                        br.Y = (labelTL.Y + lh + outsidePadding).ceil();
                    }
                }
            }
        }
        if let (Some(icon), Some(_)) = (self.icon(), allNodes) {
            let ipos = icon.b().Position;
            if self.b().shapeType != imageType && ipos.IsOutside() {
                let iconSize = MaxIconSize as f64;
                let iconTL = *self.LabelTopLeft(ipos, iconSize, iconSize).b();

                let outsidePadding = 2.0 * LABEL_PADDING;

                let left = (iconTL.X - outsidePadding).floor();
                if left < tl.X {
                    tl.X = left;
                }
                let top = (iconTL.Y - outsidePadding).floor();
                if top < tl.Y {
                    tl.Y = top;
                }
                let right = (iconTL.X + iconSize + outsidePadding).ceil();
                if right > br.X {
                    br.X = right;
                }
                let bottom = (iconTL.Y + iconSize + outsidePadding).ceil();
                if bottom > br.Y {
                    br.Y = bottom;
                }
            }
        }
        (tl, br)
    }

    // TODO: verify whether this correctly finds all overlapping ports.
    pub(crate) fn overlappingPorts(&self, n2: &NodeRef) -> Map<PointKey, bool> {
        let mut overlaps = Map::default();

        // Don't count container overlaps
        if isDescendantOf(Some(self), Some(n2)) || isDescendantOf(Some(n2), Some(self)) {
            return overlaps;
        }

        let n1Ports = self.ports();
        let n2Ports = n2.ports();
        for n1Port in &n1Ports {
            let found = n2Ports.iter().any(|p| p.X == n1Port.X && p.Y == n1Port.Y);
            if found {
                overlaps.insert(PointKey::from(*n1Port), true);
            }
        }

        overlaps
    }
}

/// A geo.Point used as a Go map key (`map[geo.Point]...`): compares and hashes
/// by the exact float64 values, matching Go's `==` on the struct (NaN keys never
/// match, -0 == +0).
#[derive(Clone, Copy, Debug)]
pub struct PointKey(pub geo::Point);

impl From<geo::Point> for PointKey {
    fn from(p: geo::Point) -> Self {
        PointKey(p)
    }
}

impl PartialEq for PointKey {
    fn eq(&self, other: &Self) -> bool {
        self.0.X == other.0.X && self.0.Y == other.0.Y
    }
}
impl Eq for PointKey {}
impl std::hash::Hash for PointKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // Normalize -0 to +0 so equal keys hash equally.
        let nx = if self.0.X == 0.0 { 0.0f64 } else { self.0.X };
        let ny = if self.0.Y == 0.0 { 0.0f64 } else { self.0.Y };
        state.write_u64(nx.to_bits());
        state.write_u64(ny.to_bits());
    }
}

/// Go `type Nodes []*Node`.
pub type Nodes = Vec<NodeRef>;

/// Methods of Go's `Nodes` slice type, implemented on `[NodeRef]` so both
/// `Vec<NodeRef>` and slices can call them (`Nodes(g.Nodes).fixedBounds()` →
/// `g.Nodes.fixedBounds()`).
pub trait NodesMethods {
    fn Leftmost(&self, node: &NodeRef) -> bool;
    fn Topmost(&self, node: &NodeRef) -> bool;
    fn Rightmost(&self, node: &NodeRef) -> bool;
    fn Bottommost(&self, node: &NodeRef) -> bool;
    fn DebugID(&self) -> String;
    fn setGraphReference(&self, g: &GraphRef);
    fn adjacentCount(&self) -> i64;
    fn clusters(&self, distanceThreshold: f64) -> Vec<Vec<NodeRef>>;
    fn clustersWithWorkGuard(&self, distanceThreshold: f64, guard: Option<&limits::WorkGuardRef>) -> GoResult<Vec<Vec<NodeRef>>>;
    fn center(&self) -> geo::PointRef;
    fn centerWithWorkGuard(&self, guard: &limits::WorkGuardRef) -> GoResult<geo::PointRef>;
    fn bounds(&self) -> (Option<geo::PointRef>, Option<geo::PointRef>);
    fn boundsWithWorkGuard(&self, guard: &limits::WorkGuardRef) -> GoResult<(Option<geo::PointRef>, Option<geo::PointRef>)>;
    fn unroundedBounds(&self) -> (Option<geo::PointRef>, Option<geo::PointRef>);
    fn boundingBox(&self, roundDimensions: bool) -> (Option<geo::PointRef>, Option<geo::PointRef>);
    fn fixedBounds(&self) -> (Option<geo::PointRef>, Option<geo::PointRef>);
    fn unroundedFixedBounds(&self) -> (Option<geo::PointRef>, Option<geo::PointRef>);
    fn intersectsNode(&self, edge: &EdgeRef) -> bool;
    fn subgraphContainer(&self) -> Option<NodeRef>;
    fn fixedNodes(&self) -> Vec<NodeRef>;
    fn hasFixedNode(&self) -> bool;
    fn fixedOrigin(&self) -> Option<geo::PointRef>;

    // Exported wrappers (placement_access.go, hierarchy_access.go,
    // structure_api.go, routing_access.go).
    fn FixedNodes(&self) -> Vec<NodeRef>;
    fn HasFixedNode(&self) -> bool;
    fn NumAdjacent(&self) -> i64;
    fn FixedBoundingBox(&self) -> (Option<geo::PointRef>, Option<geo::PointRef>);
    fn SetGraphReference(&self, g: &GraphRef);
    fn Center(&self) -> geo::PointRef;
    fn CenterWithWorkGuard(&self, guard: Option<&limits::WorkGuardRef>) -> GoResult<geo::PointRef>;
    fn BoundingBox(&self) -> (Option<geo::PointRef>, Option<geo::PointRef>);
    fn BoundingBoxWithWorkGuard(
        &self,
        guard: Option<&limits::WorkGuardRef>,
    ) -> GoResult<(Option<geo::PointRef>, Option<geo::PointRef>)>;
    fn DistanceClusters(&self, threshold: f64) -> Vec<Vec<NodeRef>>;
    fn DistanceClustersWithWorkGuard(&self, threshold: f64, guard: Option<&limits::WorkGuardRef>) -> GoResult<Vec<Vec<NodeRef>>>;
    fn IntersectsNode(&self, edge: &EdgeRef) -> bool;
}

impl NodesMethods for [NodeRef] {
    fn Leftmost(&self, node: &NodeRef) -> bool {
        let nx = node.x();
        for n in self {
            if n == node {
                continue;
            }
            let Some(tl) = n.tl() else { continue };
            if tl.b().X < nx {
                return false;
            }
        }
        true
    }

    fn Topmost(&self, node: &NodeRef) -> bool {
        let ny = node.y();
        for n in self {
            if n == node {
                continue;
            }
            let Some(tl) = n.tl() else { continue };
            if tl.b().Y < ny {
                return false;
            }
        }
        true
    }

    fn Rightmost(&self, node: &NodeRef) -> bool {
        let nr = node.x() + node.w();
        for n in self {
            if n == node {
                continue;
            }
            let Some(tl) = n.tl() else { continue };
            let x = tl.b().X;
            if x + n.w() > nr {
                return false;
            }
        }
        true
    }

    fn Bottommost(&self, node: &NodeRef) -> bool {
        let nb = node.y() + node.h();
        for n in self {
            if n == node {
                continue;
            }
            let Some(tl) = n.tl() else { continue };
            let y = tl.b().Y;
            if y + n.h() > nb {
                return false;
            }
        }
        true
    }

    fn DebugID(&self) -> String {
        let nodeIDs: Vec<String> = self.iter().map(|n| n.DebugID()).collect();
        format!("[{}]", nodeIDs.join(", "))
    }

    fn setGraphReference(&self, g: &GraphRef) {
        for n in self {
            n.bm().Graph = Some(g.clone());
            for child in g.allDescendantNodes(Some(n), true) {
                child.bm().Graph = Some(g.clone());
            }
        }
    }

    fn adjacentCount(&self) -> i64 {
        let mut sum = 0i64;
        for n in self {
            sum += n.b().Edges.len() as i64;
        }
        sum
    }

    // 1. Algorithm is to seed clusters with nodes which connected/near and further than a threshold distance
    // 2. Do recursive scans in the threshold distance perimeter
    // ---- if the scan finds a node which is not part of a cluster, it becomes part of the scanning node's cluster
    // ---- if the scan finds a node which is part of a cluster, that cluster is merged into the scanning node's cluster
    fn clusters(&self, distanceThreshold: f64) -> Vec<Vec<NodeRef>> {
        match self.clustersWithWorkGuard(distanceThreshold, None) {
            Ok(c) => c,
            Err(err) => panic!("{}", err),
        }
    }

    fn clustersWithWorkGuard(&self, distanceThreshold: f64, guard: Option<&limits::WorkGuardRef>) -> GoResult<Vec<Vec<NodeRef>>> {
        let nodes = self;
        let charge = |units: i64| -> GoResult<()> {
            match guard {
                None => Ok(()),
                Some(g) => g.Add(units),
            }
        };
        let finish = || -> GoResult<()> {
            match guard {
                None => Ok(()),
                Some(g) => g.Finish(),
            }
        };
        let chargeSort = |length: i64| -> GoResult<()> {
            let mut width: i64 = 1;
            while width < length {
                charge(length)?;
                if width > length / 2 {
                    break;
                }
                width *= 2;
            }
            Ok(())
        };

        let mut nodeToCluster: Map<NodeRef, i64> = Map::default();

        let mut nextGeneratedClusterID: i64 = 0;

        let mut seedNodes: Vec<NodeRef> = Vec::new();
        let mut maybeAddSeedNodes = |seedNodes: &mut Vec<NodeRef>, n: &NodeRef, adj: &NodeRef| {
            let d = n.distance(adj, true);
            if geo::PrecisionCompare(d, distanceThreshold, geo::PRECISION) > 0 {
                seedNodes.push(n.clone());
                seedNodes.push(adj.clone());
            }
        };
        // Step 1
        for n in nodes {
            let (nedges, nnears) = {
                let nb = n.b();
                (nb.Edges.len() as i64, nb.Nears.len() as i64)
            };
            charge(1 + nedges + nnears)?;
            for e in n.edges() {
                let adj = n.adjacent(&e);
                maybeAddSeedNodes(&mut seedNodes, n, &adj);
            }
            chargeSort(nnears)?;
            for near0 in n.orderedNears() {
                let mut near = near0;
                let ncluster = near.clusterField();
                let nseq = near.sequenceField();
                if let Some(c) = ncluster {
                    let v = c.b().Vessel.clone();
                    near = v;
                } else if let Some(s) = nseq {
                    let v = s.b().Vessel.clone();
                    near = v;
                } else {
                    let ntt = n.g().b().NodeToTree.clone();
                    if let Some(tree0) = ntt.get(&near) {
                        let mut tree = tree0;
                        if guard.is_none() {
                            near = tree.root().sentinelNode();
                        } else {
                            loop {
                                let parent = tree.b().Parent.clone();
                                let Some(parent) = parent else { break };
                                guard.unwrap().Step()?;
                                tree = parent;
                            }
                            near = tree.sentinelNode();
                        }
                    }
                }
                if !nodeEq(near.containerField().as_ref(), n.containerField().as_ref()) {
                    continue;
                }
                maybeAddSeedNodes(&mut seedNodes, n, &near);
            }
        }

        if seedNodes.len() < 2 {
            finish()?;
            return Ok(Vec::new());
        }

        // Step 2
        fn mergeNodesInPerimeter(
            n: &NodeRef,
            nodes: &[NodeRef],
            nodeToCluster: &mut Map<NodeRef, i64>,
            distanceThreshold: f64,
            charge: &dyn Fn(i64) -> GoResult<()>,
        ) -> GoResult<()> {
            charge(nodes.len() as i64)?;
            let clusterID = mapAt(nodeToCluster, n);
            for otherN in nodes {
                if otherN == n {
                    continue;
                }
                if otherN.b().FixedTopLeft.is_some() {
                    continue;
                }
                // TODO: consider caching this distance & comparison as it's the same computed a few lines above
                let d = n.distance(otherN, true);
                if geo::PrecisionCompare(d, distanceThreshold, geo::PRECISION) > 0 {
                    continue;
                }
                let (otherClusterID, isIn) = match nodeToCluster.get(otherN) {
                    Some(&id) => (id, true),
                    None => (0, false),
                };
                if isIn && otherClusterID == clusterID {
                    continue;
                }

                let v = mapAt(nodeToCluster, n);
                nodeToCluster.insert(otherN.clone(), v);
                if !isIn {
                    mergeNodesInPerimeter(otherN, nodes, nodeToCluster, distanceThreshold, charge)?;
                }
            }
            Ok(())
        }

        charge(seedNodes.len() as i64)?;
        for node in &seedNodes {
            if node.b().FixedTopLeft.is_some() {
                continue;
            }
            // createCluster
            if !nodeToCluster.contains_key(node) {
                let clusterID = nextGeneratedClusterID;
                nextGeneratedClusterID += 1;
                nodeToCluster.insert(node.clone(), clusterID);
            }
            mergeNodesInPerimeter(node, nodes, &mut nodeToCluster, distanceThreshold, &charge)?;
        }

        charge(nodes.len() as i64)?;
        if !nodes.hasFixedNode() {
            if nodeToCluster.len() != nodes.len() {
                panic!("Clustered nodes: {}. Total: {}", nodeToCluster.len(), nodes.len());
            }
        }

        let mut clusterMapping: Map<i64, Vec<NodeRef>> = Map::default();
        charge(nodeToCluster.len() as i64)?;
        for (node, clusterID) in nodeToCluster.iter() {
            clusterMapping.entry(*clusterID).or_default().push(node.clone());
        }

        for cluster in clusterMapping.values_mut() {
            chargeSort(cluster.len() as i64)?;
            sortNodesByID(cluster);
        }

        let mut clusterIDs: Vec<i64> = Vec::with_capacity(clusterMapping.len());
        charge(clusterMapping.len() as i64)?;
        for clusterID in clusterMapping.keys() {
            clusterIDs.push(*clusterID);
        }
        chargeSort(clusterIDs.len() as i64)?;
        gocompat::sort::Ints(&mut clusterIDs);

        let mut clusters: Vec<Vec<NodeRef>> = Vec::new();
        charge(clusterIDs.len() as i64)?;
        for clusterID in &clusterIDs {
            clusters.push(clusterMapping.get(clusterID).cloned().unwrap_or_default());
        }

        finish()?;
        Ok(clusters)
    }

    fn center(&self) -> geo::PointRef {
        let (tl, br) = self.bounds();
        let tl = *tl.unwrap().b();
        let br = *br.unwrap().b();
        newPoint(tl.X + (br.X - tl.X) / 2.0, tl.Y + (br.Y - tl.Y) / 2.0)
    }

    fn centerWithWorkGuard(&self, guard: &limits::WorkGuardRef) -> GoResult<geo::PointRef> {
        let (tl, br) = self.boundsWithWorkGuard(guard)?;
        // Go dereferences the (possibly nil) points here.
        let tl = *tl.expect("nil pointer dereference").b();
        let br = *br.expect("nil pointer dereference").b();
        Ok(newPoint(tl.X + (br.X - tl.X) / 2.0, tl.Y + (br.Y - tl.Y) / 2.0))
    }

    fn bounds(&self) -> (Option<geo::PointRef>, Option<geo::PointRef>) {
        self.boundingBox(true)
    }

    fn boundsWithWorkGuard(&self, guard: &limits::WorkGuardRef) -> GoResult<(Option<geo::PointRef>, Option<geo::PointRef>)> {
        let nodes = self;
        if nodes.is_empty() {
            guard.Finish()?;
            return Ok((Some(newPoint(f64::NEG_INFINITY, f64::NEG_INFINITY)), Some(newPoint(f64::INFINITY, f64::INFINITY))));
        }
        let mut minX = f64::INFINITY;
        let mut minY = f64::INFINITY;
        let mut maxX = f64::NEG_INFINITY;
        let mut maxY = f64::NEG_INFINITY;

        for node in nodes {
            guard.Step()?;
            if !node.has_tl() {
                return Ok((None, None));
            }
            if let Some(lbl) = node.label() {
                if lbl.b().Position.IsOutside() {
                    // An outside label can scan the complete peer set along each axis.
                    guard.Add(2 * nodes.len() as i64)?;
                }
            }
            let (tl, br) = node.boundingBoxValues(Some(nodes), true);
            minX = fmin(minX, tl.X);
            minY = fmin(minY, tl.Y);
            maxX = fmax(maxX, br.X);
            maxY = fmax(maxY, br.Y);
        }

        guard.Finish()?;
        Ok((Some(newPoint(minX, minY)), Some(newPoint(maxX, maxY))))
    }

    fn unroundedBounds(&self) -> (Option<geo::PointRef>, Option<geo::PointRef>) {
        self.boundingBox(false)
    }

    fn boundingBox(&self, roundDimensions: bool) -> (Option<geo::PointRef>, Option<geo::PointRef>) {
        let nodes = self;
        if nodes.is_empty() {
            return (Some(newPoint(f64::NEG_INFINITY, f64::NEG_INFINITY)), Some(newPoint(f64::INFINITY, f64::INFINITY)));
        }
        let mut minX = f64::INFINITY;
        let mut minY = f64::INFINITY;
        let mut maxX = f64::NEG_INFINITY;
        let mut maxY = f64::NEG_INFINITY;

        for node in nodes {
            if !node.has_tl() {
                return (None, None);
            }
            let (tl, br) = node.boundingBoxValues(Some(nodes), roundDimensions);
            minX = fmin(minX, tl.X);
            minY = fmin(minY, tl.Y);
            maxX = fmax(maxX, br.X);
            maxY = fmax(maxY, br.Y);
        }

        (Some(newPoint(minX, minY)), Some(newPoint(maxX, maxY)))
    }

    // for nodes in the same container, return the bounding box accounting for any fixed node offset
    fn fixedBounds(&self) -> (Option<geo::PointRef>, Option<geo::PointRef>) {
        let (mut tl, br) = self.bounds();
        if let Some(fixedOrigin) = self.fixedOrigin() {
            tl = Some(fixedOrigin);
        }
        (tl, br)
    }

    fn unroundedFixedBounds(&self) -> (Option<geo::PointRef>, Option<geo::PointRef>) {
        let (mut tl, br) = self.unroundedBounds();
        if let Some(fixedOrigin) = self.fixedOrigin() {
            tl = Some(fixedOrigin);
        }
        (tl, br)
    }

    fn intersectsNode(&self, edge: &EdgeRef) -> bool {
        let (from, to, points) = {
            let e = edge.b();
            (e.From.clone(), e.To.clone(), e.Points.clone())
        };
        // Check for intersections on other nodes
        for otherNode in self {
            // Can intersect with a container if one endpoint is inside it
            if isDescendantOf(Some(&from), Some(otherNode)) {
                continue;
            }
            if isDescendantOf(Some(&to), Some(otherNode)) {
                continue;
            }

            let mut i = 0usize;
            while i + 1 < points.len() {
                if otherNode.passesThrough(Some(&points[i]), Some(&points[i + 1])) {
                    return true;
                }
                i += 1;
            }
        }
        false
    }

    // if this graph is a subgraph, return the container of the least nested node
    // normally this is the nil container
    fn subgraphContainer(&self) -> Option<NodeRef> {
        let mut root: Option<NodeRef> = None;
        let mut minLevel = i64::MAX;
        for n in self {
            let c = n.container();
            let Some(c) = c else { return None };
            let level = c.containerLevel();
            if level < minLevel {
                minLevel = level;
                root = Some(c);
            }
        }
        root
    }

    fn fixedNodes(&self) -> Vec<NodeRef> {
        let mut fixed = Vec::new();
        for n in self {
            if n.b().FixedTopLeft.is_some() {
                fixed.push(n.clone());
            }
        }
        fixed
    }

    fn hasFixedNode(&self) -> bool {
        for n in self {
            if n.b().FixedTopLeft.is_some() {
                return true;
            }
        }
        false
    }

    // If container A has child B at (80,30) with a FixedTopLeft of (70,20) (ignoring padding)
    // then the FixedOrigin point of the nodes [B,C] should be at (10,10), despite their bounding box TL being at (80,30).
    // This way when A positions itself around its children, it has the space for B's FixedTopLeft to be correct.
    fn fixedOrigin(&self) -> Option<geo::PointRef> {
        let container = self.subgraphContainer();
        for child in self {
            if !nodeEq(child.container().as_ref(), container.as_ref()) {
                continue;
            }
            // the origin is the same for all fixed nodes in the container (see note above)
            if let Some(p) = child.fixedOrigin() {
                return Some(p);
            }
        }
        None
    }

    fn FixedNodes(&self) -> Vec<NodeRef> {
        self.fixedNodes()
    }
    fn HasFixedNode(&self) -> bool {
        self.hasFixedNode()
    }
    fn NumAdjacent(&self) -> i64 {
        self.adjacentCount()
    }
    fn FixedBoundingBox(&self) -> (Option<geo::PointRef>, Option<geo::PointRef>) {
        self.fixedBounds()
    }
    fn SetGraphReference(&self, g: &GraphRef) {
        self.setGraphReference(g)
    }
    fn Center(&self) -> geo::PointRef {
        self.center()
    }
    fn CenterWithWorkGuard(&self, guard: Option<&limits::WorkGuardRef>) -> GoResult<geo::PointRef> {
        let Some(guard) = guard else {
            return Err(errorf!("TALA Center requires a work guard"));
        };
        self.centerWithWorkGuard(guard)
    }
    fn BoundingBox(&self) -> (Option<geo::PointRef>, Option<geo::PointRef>) {
        self.bounds()
    }
    fn BoundingBoxWithWorkGuard(
        &self,
        guard: Option<&limits::WorkGuardRef>,
    ) -> GoResult<(Option<geo::PointRef>, Option<geo::PointRef>)> {
        let Some(guard) = guard else {
            return Err(errorf!("TALA BoundingBox requires a work guard"));
        };
        self.boundsWithWorkGuard(guard)
    }
    fn DistanceClusters(&self, threshold: f64) -> Vec<Vec<NodeRef>> {
        self.clusters(threshold)
    }
    fn DistanceClustersWithWorkGuard(&self, threshold: f64, guard: Option<&limits::WorkGuardRef>) -> GoResult<Vec<Vec<NodeRef>>> {
        let Some(guard) = guard else {
            return Err(errorf!("TALA DistanceClusters requires a work guard"));
        };
        self.clustersWithWorkGuard(threshold, Some(guard))
    }
    fn IntersectsNode(&self, edge: &EdgeRef) -> bool {
        self.intersectsNode(edge)
    }
}

/// Go `Spacing` (unexported fields; use the accessor methods).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Spacing {
    pub(crate) top: f64,
    pub(crate) bottom: f64,
    pub(crate) left: f64,
    pub(crate) right: f64,
}

/// Node positions and dimensions for shapes should always be whole numbers
/// When used in OVGs for edge routing, they can take decimal values
pub struct Node {
    pub D2ID: Option<String>,
    pub ID: EntityID,
    /// Embedded `geo.Box`. Shared with `Shape` (Go `nodeshape.New(t, &n.Box)`).
    pub Box: geo::BoxRef,
    pub Edges: Vec<EdgeRef>,
    /// Nears is a list of nodes which this node wants to position itself close to one of
    /// They're different from edges that have additional heuristics like alignment, symmetry, tree/cluster-formation, etc
    /// And we don't reward being closer to all of them
    pub Nears: Set<NodeRef>,
    pub Graph: Option<GraphRef>,
    pub FontSize: Option<i64>,

    /// This defines the snap points (embedded `nodeshape.Shape`; nil for
    /// struct-literal nodes that never called SetShape).
    pub Shape: Option<nodeshape::ShapeRc>,
    pub(crate) shapeType: nodeshape::Kind,

    pub ForceHierarchy: bool,

    pub Container: Option<NodeRef>,
    pub Cluster: Option<ClusterRef>,
    pub Sequence: Option<SequenceRef>,
    pub HerdAssignment: Option<HerdAssignmentRef>,
    pub Hierarchy: Option<HierarchyRef>,

    pub Label: Option<LabelRef>,
    pub Icon: Option<IconRef>,

    pub FixedTopLeft: Option<geo::PointRef>,

    /// The node should be be these dimensions, but containers can still grow to fit their children
    pub DesiredWidth: Option<f64>,
    pub DesiredHeight: Option<f64>,

    /// LoopOffsets contains the distance (offset) from the node bounding box
    /// to loop borders.
    pub LoopOffsets: Map<geo::Orientation, f64>,

    pub(crate) margin: Spacing,
    pub(crate) padding: Spacing,

    pub(crate) isClusterVessel: bool,
    pub(crate) isContainer: bool,

    /// to adjust where connections are for 3d/multiple styles
    pub Is3D: bool,
    pub IsMultiple: bool,

    /// Connections can go through invisible nodes.
    /// For example, opacity 0 grid cells used for spacing
    pub IsInvisible: bool,

    /// LongDistanceNeighborRequirements records optimizer-derived parallel-edge
    /// spacing by adjacent node.
    pub LongDistanceNeighborRequirements: Map<NodeRef, LongDistanceNeighborRequirements>,
}

impl Default for Node {
    /// Go zero value `layoutgraph.Node{}` (with a fresh zero Box).
    fn default() -> Self {
        Node {
            D2ID: None,
            ID: 0,
            Box: geo::BoxRef::new(geo::Box::default()),
            Edges: Vec::new(),
            Nears: Set::default(),
            Graph: None,
            FontSize: None,
            Shape: None,
            shapeType: nodeshape::Kind::default(),
            ForceHierarchy: false,
            Container: None,
            Cluster: None,
            Sequence: None,
            HerdAssignment: None,
            Hierarchy: None,
            Label: None,
            Icon: None,
            FixedTopLeft: None,
            DesiredWidth: None,
            DesiredHeight: None,
            LoopOffsets: Map::default(),
            margin: Spacing::default(),
            padding: Spacing::default(),
            isClusterVessel: false,
            isContainer: false,
            Is3D: false,
            IsMultiple: false,
            IsInvisible: false,
            LongDistanceNeighborRequirements: Map::default(),
        }
    }
}

impl Node {
    /// Shallow Go struct copy (`*node` by value): pointer fields stay shared,
    /// maps/slices are copied by header (the port copies their contents).
    /// NOTE: `Box` is copied as the same handle; callers that need Go's
    /// value-copy of the embedded box must copy `*Box.b()` themselves.
    pub(crate) fn shallowCopy(&self) -> Node {
        Node {
            D2ID: self.D2ID.clone(),
            ID: self.ID,
            Box: self.Box.clone(),
            Edges: self.Edges.clone(),
            Nears: self.Nears.clone(),
            Graph: self.Graph.clone(),
            FontSize: self.FontSize,
            Shape: self.Shape.clone(),
            shapeType: self.shapeType,
            ForceHierarchy: self.ForceHierarchy,
            Container: self.Container.clone(),
            Cluster: self.Cluster.clone(),
            Sequence: self.Sequence.clone(),
            HerdAssignment: self.HerdAssignment.clone(),
            Hierarchy: self.Hierarchy.clone(),
            Label: self.Label.clone(),
            Icon: self.Icon.clone(),
            FixedTopLeft: self.FixedTopLeft.clone(),
            DesiredWidth: self.DesiredWidth,
            DesiredHeight: self.DesiredHeight,
            LoopOffsets: self.LoopOffsets.clone(),
            margin: self.margin,
            padding: self.padding,
            isClusterVessel: self.isClusterVessel,
            isContainer: self.isContainer,
            Is3D: self.Is3D,
            IsMultiple: self.IsMultiple,
            IsInvisible: self.IsInvisible,
            LongDistanceNeighborRequirements: self.LongDistanceNeighborRequirements.clone(),
        }
    }
}

gocompat::go_ptr!(NodeRef, Node);

/// Go `n.DebugID()` on a possibly-nil node.
pub fn DebugIDOf(n: Option<&NodeRef>) -> String {
    match n {
        None => "nil".to_string(),
        Some(n) => n.DebugID(),
    }
}

/// Go `n.Level()` on a possibly-nil node.
pub fn LevelOf(n: Option<&NodeRef>) -> i64 {
    match n {
        None => 0,
        Some(n) => n.Level(),
    }
}

/// Go `node.entityID()` on a possibly-nil node (nil → 0).
#[inline]
pub fn entityIDOf(n: Option<&NodeRef>) -> EntityID {
    match n {
        None => 0,
        Some(n) => n.b().ID,
    }
}

impl NodeRef {
    /// The embedded shape (panics like a Go nil-interface call when unset).
    #[inline]
    #[track_caller]
    pub fn shape(&self) -> nodeshape::ShapeRc {
        self.b().Shape.clone().expect("nil pointer dereference: node.Shape")
    }

    pub fn DebugID(&self) -> String {
        let (d2id, isVessel, graph, id) = {
            let n = self.b();
            (n.D2ID.clone(), n.isClusterVessel, n.Graph.clone(), n.ID)
        };
        if let Some(d) = d2id {
            return d;
        }
        if isVessel {
            let g = graph.expect("nil pointer dereference: node.Graph");
            let clusters = g.b().Clusters.clone();
            let c = clusters.get(self).expect("nil pointer dereference: cluster");
            return format!("Cluster vessel of: {}", c.DebugID());
        }
        if let Some(g) = graph {
            let sequences = g.b().Sequences.clone();
            if sequences.len() > 0 {
                if let Some(s) = sequences.get(self) {
                    return format!("Sequence vessel of: {}", s.DebugID());
                }
            }
        }
        id.to_string()
    }

    pub fn Level(&self) -> i64 {
        let (cluster, sequence, container) = {
            let n = self.b();
            (n.Cluster.clone(), n.Sequence.clone(), n.Container.clone())
        };
        if clusterIsActive(cluster.as_ref()) {
            let v = cluster.unwrap().b().Vessel.clone();
            return v.Level();
        }
        if sequenceIsActive(sequence.as_ref()) {
            let v = sequence.unwrap().b().Vessel.clone();
            return v.Level();
        }
        1 + LevelOf(container.as_ref())
    }

    pub(crate) fn adjacent(&self, e: &EdgeRef) -> NodeRef {
        let (from, to) = {
            let eb = e.b();
            (eb.From.clone(), eb.To.clone())
        };
        if *self == from {
            return to;
        }
        from
    }
}

pub fn NewNode(id: EntityID, width: f64, height: f64) -> NodeRef {
    let node = NodeRef::new(Node {
        ID: id,
        Box: geo::BoxRef::new(geo::Box { TopLeft: None, Width: width, Height: height }),
        ..Node::default()
    });
    node.SetShape("");
    node
}

impl NodeRef {
    pub fn InitIcon(&self) {
        self.bm().Icon = Some(IconRef::new(Icon::default()));
    }

    pub fn SetShape(&self, shapeType: &str) {
        // The shape retains this exact box pointer so its geometry follows node
        // moves and resizes.
        let bx = self.b().Box.clone();
        let (s, kind, ok) = nodeshape::New(shapeType, bx);
        if ok {
            let mut n = self.bm();
            n.Shape = s;
            n.shapeType = kind;
        }
    }

    /// ShapeType reports the node's canonical shape type.
    pub fn ShapeType(&self) -> String {
        self.shape().GetType()
    }

    /// NumColumns reports the number of columns in a table-shaped node.
    pub fn NumColumns(&self) -> i64 {
        nodeshape::NumColumns(&*self.shape())
    }

    pub fn SetNumColumns(&self, numColumns: i64) {
        nodeshape::SetNumColumns(&*self.shape(), numColumns);
    }

    pub fn AddNear(&self, otherN: Option<&NodeRef>) {
        let Some(otherN) = otherN else { return };
        if self == otherN {
            return;
        }
        self.bm().Nears.insert(otherN.clone());
        otherN.bm().Nears.insert(self.clone());
    }

    pub(crate) fn entityID(&self) -> EntityID {
        self.b().ID
    }

    pub(crate) fn deltaTo(&self, n2: &NodeRef, atPoint: Option<&geo::PointRef>) -> i64 {
        match self.deltaToGuarded(Some(n2), atPoint, None) {
            Ok(d) => d,
            Err(err) => panic!("{}", err),
        }
    }

    pub(crate) fn deltaToGuarded(
        &self,
        n2: Option<&NodeRef>,
        atPoint: Option<&geo::PointRef>,
        guard: Option<&dyn workStepper>,
    ) -> GoResult<i64> {
        let (Some(n2), Some(atPoint)) = (n2, atPoint) else {
            return Err(invariant::New("spacing check received incomplete nodes"));
        };
        let n1 = self;
        // n1 may have multiple edges to n2. We want to get the largest distance that we need to maintain
        let mut maxEdgeWidth = i64::MIN;
        let mut maxEdgeHeight = i64::MIN;
        let mut isConnected = false;
        let edges = n1.edges();
        for e in &edges {
            if let Some(guard) = guard {
                guard.Step()?;
            }
            // TODO (Sun Apr 10 16:07:25 2022) this doesn't take edge abductions into consideration, so it may not find a minWidth to a child
            if n1.adjacent(e) == *n2 {
                isConnected = true;
                let (mw, mh) = {
                    let eb = e.b();
                    (eb.MinWidth, eb.MinHeight)
                };
                if mw > maxEdgeWidth {
                    maxEdgeWidth = mw;
                }
                if mh > maxEdgeHeight {
                    maxEdgeHeight = mh;
                }
            }
        }

        let (mut horizontalDelta, mut verticalDelta);
        if isConnected {
            horizontalDelta = ConnectedNodeGap;
            verticalDelta = ConnectedNodeGap;
        } else {
            horizontalDelta = NodeGap;
            verticalDelta = NodeGap;
        }

        let (n1Shape, n1Loops, n1Margin) = {
            let b = n1.b();
            (b.shapeType, b.LoopOffsets.clone(), b.margin)
        };
        let (n2Shape, n2Loops, n2Margin) = {
            let b = n2.b();
            (b.shapeType, b.LoopOffsets.clone(), b.margin)
        };

        if n1Shape == tableType || n2Shape == tableType {
            horizontalDelta = TableNodeGap;
        }

        if maxEdgeHeight > verticalDelta {
            verticalDelta = maxEdgeHeight;
        }
        if maxEdgeWidth > horizontalDelta {
            horizontalDelta = maxEdgeWidth;
        }

        // Ordinary unlabeled boxes often require the same gap in every direction.
        // In that case the orientation and directional-margin switches cannot
        // affect the result; retain the edge scan above for its validation and work.
        if horizontalDelta == verticalDelta
            && n1Loops.is_empty()
            && n2Loops.is_empty()
            && n1Margin == Spacing::default()
            && n2Margin == Spacing::default()
        {
            return Ok(horizontalDelta);
        }
        let o = n1.orientationAtPoint(n2, Some(atPoint));

        if !n1Loops.is_empty() || !n2Loops.is_empty() {
            let mut loopDeltas = NodeGap;
            if !n1Loops.is_empty() {
                loopDeltas += mapAt(&n1Loops, &o.GetOpposite()) as i64;
            }
            if !n2Loops.is_empty() {
                loopDeltas += mapAt(&n2Loops, &o) as i64;
            }
            if loopDeltas > horizontalDelta {
                horizontalDelta = loopDeltas;
            }
            if loopDeltas > verticalDelta {
                verticalDelta = loopDeltas;
            }
        }

        let (mut n1LabelWidth, mut n1LabelHeight, mut n2LabelWidth, mut n2LabelHeight) = (0i64, 0i64, 0i64, 0i64);
        let opp = o.GetOpposite();
        if opp == geo::Bottom {
            n1LabelHeight = n1Margin.bottom as i64;
        } else if opp == geo::Top {
            n1LabelHeight = n1Margin.top as i64;
        } else if opp == geo::Right {
            n1LabelWidth = n1Margin.right as i64;
        } else if opp == geo::Left {
            n1LabelWidth = n1Margin.left as i64;
        } else if opp == geo::BottomLeft {
            n1LabelWidth = n1Margin.left as i64;
            n1LabelHeight = n1Margin.bottom as i64;
        } else if opp == geo::BottomRight {
            n1LabelWidth = n1Margin.right as i64;
            n1LabelHeight = n1Margin.bottom as i64;
        } else if opp == geo::TopLeft {
            n1LabelWidth = n1Margin.left as i64;
            n1LabelHeight = n1Margin.top as i64;
        } else if opp == geo::TopRight {
            n1LabelWidth = n1Margin.right as i64;
            n1LabelHeight = n1Margin.top as i64;
        }
        if o == geo::Bottom {
            n2LabelHeight = n2Margin.bottom as i64;
        } else if o == geo::Top {
            n2LabelHeight = n2Margin.top as i64;
        } else if o == geo::Right {
            n2LabelWidth = n2Margin.right as i64;
        } else if o == geo::Left {
            n2LabelWidth = n2Margin.left as i64;
        } else if o == geo::BottomLeft {
            n2LabelWidth = n2Margin.left as i64;
            n2LabelHeight = n2Margin.bottom as i64;
        } else if o == geo::BottomRight {
            n2LabelWidth = n2Margin.right as i64;
            n2LabelHeight = n2Margin.bottom as i64;
        } else if o == geo::TopLeft {
            n2LabelWidth = n2Margin.left as i64;
            n2LabelHeight = n2Margin.top as i64;
        } else if o == geo::TopRight {
            n2LabelWidth = n2Margin.right as i64;
            n2LabelHeight = n2Margin.top as i64;
        }
        if horizontalDelta < n1LabelWidth + n2LabelWidth {
            horizontalDelta = n1LabelWidth + n2LabelWidth;
        }
        if verticalDelta < n1LabelHeight + n2LabelHeight {
            verticalDelta = n1LabelHeight + n2LabelHeight;
        }

        if o == geo::Top || o == geo::Bottom {
            Ok(verticalDelta)
        } else if o == geo::Left || o == geo::Right {
            Ok(horizontalDelta)
        } else {
            if horizontalDelta < verticalDelta {
                return Ok(horizontalDelta);
            }
            Ok(verticalDelta)
        }
    }

    pub fn UpdateSpacing(&self) {
        let lbl = self.label();
        if let Some(lbl) = lbl {
            let (fixed, pos, lw, lh) = {
                let l = lbl.b();
                (l.positionFixed, l.Position, l.Width, l.Height)
            };
            if fixed {
                let width = lw + 2.0 * LABEL_PADDING;
                let height = lh + 2.0 * LABEL_PADDING;
                let mut n = self.bm();
                use label::*;
                if pos == OutsideTopLeft || pos == OutsideTopCenter || pos == OutsideTopRight {
                    n.margin.top = height;
                } else if pos == OutsideBottomLeft || pos == OutsideBottomCenter || pos == OutsideBottomRight {
                    n.margin.bottom = height;
                } else if pos == OutsideLeftTop || pos == OutsideLeftMiddle || pos == OutsideLeftBottom {
                    n.margin.left = width;
                } else if pos == OutsideRightTop || pos == OutsideRightMiddle || pos == OutsideRightBottom {
                    n.margin.right = width;
                } else if pos == InsideTopLeft || pos == InsideTopCenter || pos == InsideTopRight {
                    n.padding.top = height;
                } else if pos == InsideBottomLeft || pos == InsideBottomCenter || pos == InsideBottomRight {
                    n.padding.bottom = height;
                } else if pos == InsideMiddleLeft {
                    n.padding.left = width;
                } else if pos == InsideMiddleRight {
                    n.padding.right = width;
                }
            }
        }

        if let Some(icon) = self.icon() {
            let (fixed, pos) = {
                let i = icon.b();
                (i.positionFixed, i.Position)
            };
            if fixed {
                let iconSize = (MaxIconSize as f64) + 2.0 * LABEL_PADDING;
                let mut n = self.bm();
                use label::*;
                if pos == OutsideTopLeft || pos == OutsideTopCenter || pos == OutsideTopRight {
                    n.margin.top = fmax(n.margin.top, iconSize);
                } else if pos == OutsideBottomLeft || pos == OutsideBottomCenter || pos == OutsideBottomRight {
                    n.margin.bottom = fmax(n.margin.bottom, iconSize);
                } else if pos == OutsideLeftTop || pos == OutsideLeftMiddle || pos == OutsideLeftBottom {
                    n.margin.left = fmax(n.margin.left, iconSize);
                } else if pos == OutsideRightTop || pos == OutsideRightMiddle || pos == OutsideRightBottom {
                    n.margin.right = fmax(n.margin.right, iconSize);
                } else if pos == InsideTopLeft || pos == InsideTopCenter || pos == InsideTopRight {
                    n.padding.top = fmax(n.padding.top, iconSize);
                } else if pos == InsideBottomLeft || pos == InsideBottomCenter || pos == InsideBottomRight {
                    n.padding.bottom = fmax(n.padding.bottom, iconSize);
                } else if pos == InsideMiddleLeft {
                    n.padding.left = fmax(n.padding.left, iconSize);
                } else if pos == InsideMiddleRight {
                    n.padding.right = fmax(n.padding.right, iconSize);
                }
            }
        }

        let (dx, dy) = self.modifierElementAdjustments();
        if dx != 0.0 || dy != 0.0 {
            let mut n = self.bm();
            n.margin.top += dy;
            n.margin.right += dx;
        }
    }

    /// A covers B but not C, B does not cover A
    pub fn Covers(&self, n2: &NodeRef) -> bool {
        let b1 = self.boxValue();
        let b2 = n2.boxValue();
        covers(&b1, &b2)
    }
}

// TODO move to d2 geo.Box
pub(crate) fn covers(b1: &geo::Box, b2: &geo::Box) -> bool {
    let t1 = *b1.TopLeft.as_ref().expect("nil pointer dereference").b();
    let t2 = *b2.TopLeft.as_ref().expect("nil pointer dereference").b();
    (t2.X >= t1.X) && (t2.Y >= t1.Y) && (t2.X + b2.Width <= t1.X + b1.Width) && (t2.Y + b2.Height <= t1.Y + b1.Height)
}

/// boxesOverlapWithPadding reports whether the interiors of two axis-aligned
/// boxes overlap, treating a gap smaller than padding as overlap. Boundary
/// contact, including a gap exactly equal to padding, is not an overlap.
pub(crate) fn boxesOverlapWithPadding(b1: &geo::Box, b2: &geo::Box, padding: f64) -> bool {
    let t1 = *b1.TopLeft.as_ref().expect("nil pointer dereference").b();
    let t2 = *b2.TopLeft.as_ref().expect("nil pointer dereference").b();
    let b1Right = t1.X + b1.Width;
    let b2Right = t2.X + b2.Width;
    if t1.X >= b2Right + padding || t2.X >= b1Right + padding {
        return false;
    }

    let b1Bottom = t1.Y + b1.Height;
    let b2Bottom = t2.Y + b2.Height;
    t1.Y < b2Bottom + padding && t2.Y < b1Bottom + padding
}

impl NodeRef {
    pub(crate) fn doesOverlapCalc(&self, n2: &NodeRef, delta: f64) -> bool {
        boxesOverlapWithPadding(&self.boxValue(), &n2.boxValue(), delta)
    }

    /// DoesOverlapExact reports whether node boxes overlap without clearance padding.
    pub fn DoesOverlapExact(&self, n2: &NodeRef) -> bool {
        self.doesOverlapCalc(n2, 0.0)
    }

    /// doesOverlapAt returns whether placing n1 at p overlaps with n2
    pub(crate) fn doesOverlapAt(&self, n2: &NodeRef, p: &geo::PointRef) -> bool {
        let delta = self.deltaTo(n2, Some(p)) as f64;
        let b1 = geo::Box { TopLeft: Some(p.clone()), Width: self.w(), Height: self.h() };
        boxesOverlapWithPadding(&b1, &n2.boxValue(), delta)
    }

    pub(crate) fn doesOverlap(&self, n2: &NodeRef) -> bool {
        let tl = self.tl();
        let delta = self.deltaTo(n2, tl.as_ref()) as f64;
        self.doesOverlapCalc(n2, delta)
    }

    // for 2 overlapping nodes, return the area of the overlap
    pub(crate) fn overlapArea(&self, otherNode: &NodeRef) -> f64 {
        // area of overlap is area of rect from middle 2 xs and middle 2 ys
        let mut xs: Vec<f64> = Vec::with_capacity(4);
        let mut ys: Vec<f64> = Vec::with_capacity(4);
        let a = self.tlv();
        let (aw, ah) = (self.w(), self.h());
        let o = otherNode.tlv();
        let (ow, oh) = (otherNode.w(), otherNode.h());

        xs.push(a.X);
        xs.push(a.X + aw);
        ys.push(a.Y);
        ys.push(a.Y + ah);

        xs.push(o.X);
        xs.push(o.X + ow);
        ys.push(o.Y);
        ys.push(o.Y + oh);

        gocompat::slices::Sort(&mut xs);
        gocompat::slices::Sort(&mut ys);

        let width = xs[2] - xs[1];
        let height = ys[2] - ys[1];

        width * height
    }
}

pub(crate) fn sortNodesByID(nodes: &mut [NodeRef]) {
    if nodes.len() < 2 {
        return;
    }
    let ids: Vec<EntityID> = nodes.iter().map(|n| n.b().ID).collect();
    // sort.Slice permutes the slice; sort (id, node) pairs together so the
    // comparator sees the same keys as Go.
    let mut pairs: Vec<(EntityID, NodeRef)> = ids.into_iter().zip(nodes.iter().cloned()).collect();
    gocompat::sort::Slice(&mut pairs, |a, b| a.0 < b.0);
    for (i, (_, n)) in pairs.into_iter().enumerate() {
        nodes[i] = n;
    }
}

/// sortNodesByID for slices that may contain nil (e.g. `TreeOrder` keys).
pub(crate) fn sortOptNodesByID(nodes: &mut [Option<NodeRef>]) {
    if nodes.len() < 2 {
        return;
    }
    let mut pairs: Vec<(EntityID, Option<NodeRef>)> = nodes.iter().map(|n| (entityIDOf(n.as_ref()), n.clone())).collect();
    gocompat::sort::Slice(&mut pairs, |a, b| a.0 < b.0);
    for (i, (_, n)) in pairs.into_iter().enumerate() {
        nodes[i] = n;
    }
}

impl NodeRef {
    pub(crate) fn addEdge(&self, e: &EdgeRef) {
        self.bm().Edges.push(e.clone());
    }

    pub(crate) fn removeEdge(&self, e: &EdgeRef) {
        let mut n = self.bm();
        if let Some(i) = n.Edges.iter().position(|x| x == e) {
            n.Edges.remove(i);
        }
    }

    pub(crate) fn isPointNear(&self, point: &geo::Point) -> bool {
        let t = self.tlv();
        let (w, h) = (self.w(), self.h());
        (((t.X - MinRouteNodeClearance) <= point.X) && ((t.X + w + MinRouteNodeClearance) >= point.X))
            && (((t.Y - MinRouteNodeClearance) <= point.Y) && ((t.Y + h + MinRouteNodeClearance) >= point.Y))
    }

    pub(crate) fn isPointOnNode(&self, point: &geo::Point) -> bool {
        let t = self.tlv();
        let (w, h) = (self.w(), self.h());
        ((t.X <= point.X) && ((t.X + w) >= point.X)) && ((t.Y <= point.Y) && ((t.Y + h) >= point.Y))
    }

    pub(crate) fn distanceTo(&self, nodeB: &NodeRef, includeSizes: bool) -> f64 {
        distanceBetweenBoxes(&nodeDistanceBox(self, includeSizes), &nodeDistanceBox(nodeB, includeSizes))
    }
}

pub(crate) fn nodeDistanceBox(node: &NodeRef, includeSize: bool) -> geo::Box {
    let mut bx = geo::Box { TopLeft: node.tl(), Width: 0.0, Height: 0.0 };
    if includeSize {
        bx.Width = node.w();
        bx.Height = node.h();
    }
    bx
}

/// distanceBetweenBoxes returns the shortest Euclidean distance between two
/// closed, axis-aligned boxes. Overlapping and touching boxes have distance 0.
pub(crate) fn distanceBetweenBoxes(b1: &geo::Box, b2: &geo::Box) -> f64 {
    let t1 = *b1.TopLeft.as_ref().expect("nil pointer dereference").b();
    let t2 = *b2.TopLeft.as_ref().expect("nil pointer dereference").b();
    let dx = intervalGap(t1.X, t1.X + b1.Width, t2.X, t2.X + b2.Width);
    let dy = intervalGap(t1.Y, t1.Y + b1.Height, t2.Y, t2.Y + b2.Height);
    geo::EuclideanDistance(0.0, 0.0, dx, dy)
}

pub(crate) fn intervalGap(aStart: f64, aEnd: f64, bStart: f64, bEnd: f64) -> f64 {
    if aEnd < bStart {
        return bStart - aEnd;
    }
    if bEnd < aStart {
        return aStart - bEnd;
    }
    0.0
}

impl NodeRef {
    pub(crate) fn isAdjacentTo(&self, nodeB: &NodeRef, includeSizes: bool) -> bool {
        let d = self.distanceTo(nodeB, includeSizes);
        if includeSizes {
            if d <= self.g().b().CellSize {
                return true;
            }
        } else if d <= 1.0 {
            return true;
        }
        false
    }

    pub(crate) fn isWithinBounds(&self, tl: &geo::Point, br: &geo::Point) -> bool {
        let t = self.tlv();
        let (w, h) = (self.w(), self.h());
        if t.X > br.X {
            return false;
        }
        if t.Y > br.Y {
            return false;
        }
        if t.X + w < tl.X {
            return false;
        }
        if t.Y + h < tl.Y {
            return false;
        }

        true
    }

    pub(crate) fn distanceBetweenCenters(&self, nodeB: &NodeRef, includeSizes: bool) -> f64 {
        let c = 1.0 / 20.0;
        let a = self.tlv();
        let b = nodeB.tlv();

        let mut xCenter = (a.X - b.X).abs();
        let mut yCenter = (a.Y - b.Y).abs();

        if includeSizes {
            let nodeAWidth = self.w();
            let nodeAHeight = self.h();
            let nodeBWidth = nodeB.w();
            let nodeBHeight = nodeB.h();

            xCenter = ((a.X + nodeAWidth / 2.0) - (b.X + nodeBWidth / 2.0)).abs() / (nodeAWidth + nodeBWidth);
            yCenter = ((a.Y + nodeAHeight / 2.0) - (b.Y + nodeBHeight / 2.0)).abs() / (nodeAHeight + nodeBHeight);
        }

        c * fmin(xCenter, yCenter)
    }

    pub fn IsBlocked(&self, nodeA: &NodeRef, nodeB: &NodeRef, includeSizes: bool, isHorizontal: bool) -> bool {
        let n = self.tlv();
        let (nw, nh) = (self.w(), self.h());
        let a = nodeA.tlv();
        let (aw, ah) = (nodeA.w(), nodeA.h());
        let b = nodeB.tlv();
        let (bw, bh) = (nodeB.w(), nodeB.h());
        // Check that node is in front of nodeA and behind nodeB (in between)
        if isHorizontal {
            if includeSizes {
                if !((n.X >= (a.X + aw)) && ((n.X + nw) <= b.X)) {
                    return false;
                }
            } else if !((n.X >= a.X) && (n.X <= b.X)) {
                return false;
            }
        } else if includeSizes {
            if !((n.Y >= (a.Y + ah)) && ((n.Y + nh) <= b.Y)) {
                return false;
            }
        } else if !((n.Y >= a.Y) && (n.Y <= b.Y)) {
            return false;
        }

        // Check that node blocks nodeA from nodeB
        if isHorizontal {
            if includeSizes {
                if (n.Y <= fmax(a.Y, b.Y)) && ((n.Y + nh) >= fmin(a.Y + ah, b.Y + bh)) {
                    return true;
                }
            } else if (n.Y <= fmax(a.Y, b.Y)) && (n.Y >= fmin(a.Y, b.Y)) {
                return true;
            }
        } else {
            // Same as horizontal, just axis flipped
            if includeSizes {
                if (n.X <= fmax(a.X, b.X)) && ((n.X + nw) >= fmin(a.X + aw, b.X + bw)) {
                    return true;
                }
            } else if (n.X <= fmax(a.X, b.X)) && (n.X >= fmin(a.X, b.X)) {
                return true;
            }
        }

        false
    }

    pub(crate) fn distance(&self, nodeB: &NodeRef, includeSizes: bool) -> f64 {
        self.distanceTo(nodeB, includeSizes) + self.distanceBetweenCenters(nodeB, includeSizes)
    }

    // HAS TO BE DIAGONAL FROM EACH OTHER
    pub(crate) fn orthogonalDistanceTo(&self, nodeB: &NodeRef) -> (f64, f64) {
        let a = self.tlv();
        let b = nodeB.tlv();
        let rightA = a.X + self.w();
        let bottomA = a.Y + self.h();
        let rightB = b.X + nodeB.w();
        let bottomB = b.Y + nodeB.h();

        let x = if a.X < b.X { fmax(0.0, b.X - rightA) } else { fmax(0.0, a.X - rightB) };

        let y = if a.Y < b.Y { fmax(0.0, b.Y - bottomA) } else { fmax(0.0, a.Y - bottomB) };

        (x, y)
    }

    pub(crate) fn center(&self) -> geo::PointRef {
        let t = self.tlv();
        newPoint(t.X + self.w() / 2.0, t.Y + self.h() / 2.0)
    }

    pub(crate) fn overlapsAlongDimension(&self, n2: &NodeRef, isHorizontal: bool, includeSizes: bool) -> bool {
        let a = self.tlv();
        let b = n2.tlv();
        if isHorizontal {
            if includeSizes {
                if a.Y > (b.Y + n2.h()) {
                    return false;
                }
                if (a.Y + self.h()) < b.Y {
                    return false;
                }
            } else {
                if a.Y > b.Y {
                    return false;
                }
                if a.Y < b.Y {
                    return false;
                }
            }
        } else if includeSizes {
            if a.X > (b.X + n2.w()) {
                return false;
            }
            if (a.X + self.w()) < b.X {
                return false;
            }
        } else {
            if a.X > b.X {
                return false;
            }
            if a.X < b.X {
                return false;
            }
        }
        true
    }

    // isVisibilityGraphCandidate checks for nodes which are in between given parameters node and otherNode
    pub(crate) fn isVisibilityGraphCandidate(
        &self,
        isHorizontal: bool,
        checkSide: bool,
        includeSizes: bool,
        otherNode: &NodeRef,
        padding: f64,
    ) -> bool {
        let n = self.tlv();
        let o = otherNode.tlv();
        if isHorizontal {
            if checkSide && (n.X >= o.X) {
                return false;
            }
            if includeSizes {
                if n.Y > (o.Y + otherNode.h() + padding) {
                    return false;
                }
                if (n.Y + self.h() + padding) < o.Y {
                    return false;
                }
            } else {
                if n.Y > o.Y {
                    return false;
                }
                if n.Y < o.Y {
                    return false;
                }
            }
        } else {
            if checkSide && (n.Y >= o.Y) {
                return false;
            }
            if includeSizes {
                if n.X > (o.X + otherNode.w() + padding) {
                    return false;
                }
                if (n.X + self.w() + padding) < o.X {
                    return false;
                }
            } else {
                if n.X > o.X {
                    return false;
                }
                if n.X < o.X {
                    return false;
                }
            }
        }

        true
    }

    pub(crate) fn passesThrough(&self, p1: Option<&geo::PointRef>, p2: Option<&geo::PointRef>) -> bool {
        segmentIntersectsBox(p1, p2, Some(&self.boxValue()))
    }
}

// TODO: extract to `geo` package
pub(crate) fn segmentIntersectsBox(p1: Option<&geo::PointRef>, p2: Option<&geo::PointRef>, bx: Option<&geo::Box>) -> bool {
    let (Some(p1), Some(p2), Some(bx)) = (p1, p2, bx) else {
        return false;
    };
    let Some(btl) = bx.TopLeft.as_ref() else {
        return false;
    };
    let p1 = *p1.b();
    let p2 = *p2.b();
    let btl = *btl.b();

    let (mut left, mut right) = (btl.X, btl.X + bx.Width);
    if left > right {
        std::mem::swap(&mut left, &mut right);
    }
    let (mut top, mut bottom) = (btl.Y, btl.Y + bx.Height);
    if top > bottom {
        std::mem::swap(&mut top, &mut bottom);
    }
    // A NaN box boundary cannot contain a point or survive the clipping below.
    #[allow(clippy::eq_op)]
    if left != left || right != right || top != top || bottom != bottom {
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
        tEnter = fmax(tEnter, t1);
        tExit = fmin(tExit, t2);
        tEnter <= tExit
    };

    if !clipAxis(p1.X, p2.X - p1.X, left, right) || !clipAxis(p1.Y, p2.Y - p1.Y, top, bottom) {
        return false;
    }
    tEnter < tExit
}

/// orientation returns a positive value for a clockwise turn, a negative value
/// for a counter-clockwise turn, and zero for collinear points.
pub(crate) fn orientation(p: &geo::Point, q: &geo::Point, r: &geo::Point) -> f64 {
    let pqX = q.X - p.X;
    let pqY = q.Y - p.Y;
    let prX = r.X - p.X;
    let prY = r.Y - p.Y;
    pqY * prX - pqX * prY
}

pub(crate) fn intersects(p1: &geo::Point, q1: &geo::Point, p2: &geo::Point, q2: &geo::Point) -> bool {
    let p2Side = orientation(p1, q1, p2);
    let q2Side = orientation(p1, q1, q2);
    let p1Side = orientation(p2, q2, p1);
    let q1Side = orientation(p2, q2, q1);

    if p2Side == 0.0 && q2Side == 0.0 && p1Side == 0.0 && q1Side == 0.0 {
        return closedIntervalsOverlap(p1.X, q1.X, p2.X, q2.X) && closedIntervalsOverlap(p1.Y, q1.Y, p2.Y, q2.Y);
    }

    straddlesLine(p2Side, q2Side) && straddlesLine(p1Side, q1Side)
}

pub(crate) fn straddlesLine(side1: f64, side2: f64) -> bool {
    side1 == 0.0 || side2 == 0.0 || (side1 < 0.0) != (side2 < 0.0)
}

pub(crate) fn closedIntervalsOverlap(a1: f64, a2: f64, b1: f64, b2: f64) -> bool {
    let (aMin, aMax) = (fmin(a1, a2), fmax(a1, a2));
    let (bMin, bMax) = (fmin(b1, b2), fmax(b1, b2));
    aMin <= bMax && bMin <= aMax
}

impl NodeRef {
    // return true if the point is within the node's bounding box with delta padding
    pub(crate) fn containsPoint(&self, p: &geo::Point, delta: f64) -> bool {
        let t = self.tlv();
        let (w, h) = (self.w(), self.h());
        t.X - delta <= p.X && t.X + w + delta >= p.X && t.Y - delta <= p.Y && t.Y + h + delta >= p.Y
    }

    // return true if the line overlaps the node bounding box
    pub(crate) fn overlapsLine(&self, p1: &geo::Point, p2: &geo::Point, delta: f64) -> bool {
        if self.containsPoint(p1, delta) || self.containsPoint(p2, delta) {
            return true;
        }

        let t = self.tlv();
        let (w, h) = (self.w(), self.h());
        let l = t.X - delta;
        let r = t.X + w + delta;
        let tt = t.Y - delta;
        let b = t.Y + h + delta;

        let tl = geo::Point { X: l, Y: tt };
        let br = geo::Point { X: r, Y: b };
        let tr = geo::Point { X: r, Y: tt };
        let bl = geo::Point { X: l, Y: b };

        intersects(&tl, &tr, p1, p2) || intersects(&tr, &br, p1, p2) || intersects(&br, &bl, p1, p2) || intersects(&bl, &tl, p1, p2)
    }

    pub fn Surrounds(&self, other: &NodeRef, withPadding: f64) -> bool {
        let n = self.tlv();
        let (nw, nh) = (self.w(), self.h());
        let o = other.tlv();
        let (ow, oh) = (other.w(), other.h());
        n.X + withPadding < o.X && n.X + nw - withPadding > o.X + ow && n.Y + withPadding < o.Y && n.Y + nh - withPadding > o.Y + oh
    }

    // orientation returns the orientation of node relative to otherNode.
    // E.g. node ---> otherNode, here, node is to the left of otherNode, so Left would be returned
    pub(crate) fn orientation(&self, otherNode: &NodeRef) -> geo::Orientation {
        let (Some(ntl), Some(otl)) = (self.tl(), otherNode.tl()) else {
            return geo::NONE;
        };
        let n = *ntl.b();
        let o = *otl.b();
        let (nw, nh) = (self.w(), self.h());
        let (ow, oh) = (otherNode.w(), otherNode.h());
        if (n.Y + nh) < o.Y {
            if (n.X + nw) < o.X {
                return geo::TopLeft;
            }
            if (o.X + ow) < n.X {
                return geo::TopRight;
            }
            return geo::Top;
        }

        if (o.Y + oh) < n.Y {
            if (n.X + nw) < o.X {
                return geo::BottomLeft;
            }
            if (o.X + ow) < n.X {
                return geo::BottomRight;
            }
            return geo::Bottom;
        }

        if (o.X + ow) < n.X {
            return geo::Right;
        }

        if (n.X + nw) < o.X {
            return geo::Left;
        }

        geo::NONE
    }

    pub(crate) fn orientationAtPoint(&self, otherNode: &NodeRef, point: Option<&geo::PointRef>) -> geo::Orientation {
        // This is an optimized version that doesn't temporarily modify node's position
        let Some(point) = point else { return geo::NONE };
        let Some(otl) = otherNode.tl() else { return geo::NONE };
        let p = *point.b();
        let o = *otl.b();
        let (nw, nh) = (self.w(), self.h());
        let (ow, oh) = (otherNode.w(), otherNode.h());

        // Simulate node being at point instead of its actual position
        if (p.Y + nh) < o.Y {
            if (p.X + nw) < o.X {
                return geo::TopLeft;
            }
            if (o.X + ow) < p.X {
                return geo::TopRight;
            }
            return geo::Top;
        }

        if (o.Y + oh) < p.Y {
            if (p.X + nw) < o.X {
                return geo::BottomLeft;
            }
            if (o.X + ow) < p.X {
                return geo::BottomRight;
            }
            return geo::Bottom;
        }

        if (o.X + ow) < p.X {
            return geo::Right;
        }

        if (p.X + nw) < o.X {
            return geo::Left;
        }

        geo::NONE
    }

    // allReachableNodesGuarded returns all connected nodes in BFS order
    // This includes itself at index 0
    pub(crate) fn allReachableNodesGuarded(
        &self,
        includeContainers: bool,
        includeNears: bool,
        traverseTrees: bool,
        ignore: Option<&Set<NodeRef>>,
        guard: &dyn workStepper,
    ) -> GoResult<Vec<NodeRef>> {
        self.reachableNodesGuarded(&|_| true, includeContainers, includeNears, traverseTrees, ignore, guard)
    }
}

/// reachabilityIsDescendantOf preserves isDescendantOf's ancestry precedence
/// while charging every parent hop.
pub(crate) fn reachabilityIsDescendantOf(
    maybeDescendant: Option<&NodeRef>,
    maybeAncestor: Option<&NodeRef>,
    guard: &dyn workStepper,
) -> GoResult<bool> {
    let mut current = maybeDescendant.cloned();
    loop {
        guard.Step()?;
        if nodeEq(maybeAncestor, current.as_ref()) {
            return Ok(true);
        }
        let Some(c) = current else {
            return Ok(false);
        };
        current = ancestryParent(Some(&c));
    }
}

impl NodeRef {
    pub(crate) fn reachableNodesGuarded(
        &self,
        shouldVisit: &dyn Fn(&NodeRef) -> bool,
        includeContainers: bool,
        includeNears: bool,
        traverseTrees: bool,
        ignore: Option<&Set<NodeRef>>,
        guard: &dyn workStepper,
    ) -> GoResult<Vec<NodeRef>> {
        let node = self;
        let mut reachableNodes: Vec<NodeRef> = Vec::new();
        let mut visitQueue: std::collections::VecDeque<NodeRef> = std::collections::VecDeque::new();
        visitQueue.push_back(node.clone());

        let mut reachedOrVisited: Set<NodeRef> = Set::default();
        reachedOrVisited.insert(node.clone());

        let queue = |n: &NodeRef, reachedOrVisited: &mut Set<NodeRef>, visitQueue: &mut std::collections::VecDeque<NodeRef>| {
            if let Some(ignore) = ignore {
                if ignore.contains(n) {
                    return;
                }
            }
            if reachedOrVisited.contains(n) {
                return;
            }
            if !shouldVisit(n) {
                return;
            }
            reachedOrVisited.insert(n.clone());
            visitQueue.push_back(n.clone());
        };

        let graph = node.g();
        let (nodeToTree, trees, clusters, nodeContainer) = {
            let g = graph.b();
            (g.NodeToTree.clone(), g.Trees.clone(), g.Clusters.clone(), node.containerField())
        };

        while let Some(curr) = visitQueue.pop_front() {
            guard.Step()?;
            let mut includeNode = true;
            if traverseTrees && nodeToTree.has(&curr) {
                includeNode = false;
            }
            if includeNode {
                reachableNodes.push(curr.clone());
            }
            reachedOrVisited.insert(curr.clone());

            for e in curr.edges() {
                guard.Step()?;
                let adjacentNode = curr.adjacent(&e);
                queue(&adjacentNode, &mut reachedOrVisited, &mut visitQueue);
            }

            if traverseTrees {
                if let Some(ts) = trees.get(&Some(curr.clone())) {
                    for tree in ts {
                        guard.Step()?;
                        let tn = tree.b().Node.clone();
                        queue(&tn, &mut reachedOrVisited, &mut visitQueue);
                    }
                } else if let Some(tree) = nodeToTree.get(&curr) {
                    let parent = tree.b().Parent.clone();
                    if let Some(parent) = parent {
                        let pn = parent.b().Node.clone();
                        queue(&pn, &mut reachedOrVisited, &mut visitQueue);
                    } else {
                        let sn = tree.sentinelNode();
                        queue(&sn, &mut reachedOrVisited, &mut visitQueue);
                    }
                    let children = tree.b().Children.clone();
                    for c in children {
                        guard.Step()?;
                        let cn = c.b().Node.clone();
                        queue(&cn, &mut reachedOrVisited, &mut visitQueue);
                    }
                }
            }
            if includeNears {
                let mut nears = curr.orderedNears();
                if curr.b().isClusterVessel {
                    let cluster = clusters.get(&curr).expect("nil pointer dereference: cluster");
                    let cnodes = cluster.b().Nodes.clone();
                    for cn in cnodes {
                        nears.extend(cn.orderedNears());
                    }
                }
                for near0 in nears {
                    guard.Step()?;
                    let mut near = near0;
                    let ncl = near.clusterField();
                    let nsq = near.sequenceField();
                    if clusterIsActive(ncl.as_ref()) {
                        let v = ncl.unwrap().b().Vessel.clone();
                        near = v;
                    } else if sequenceIsActive(nsq.as_ref()) {
                        let v = nsq.unwrap().b().Vessel.clone();
                        near = v;
                    }
                    if !nodeEq(near.containerField().as_ref(), nodeContainer.as_ref()) {
                        continue;
                    }
                    queue(&near, &mut reachedOrVisited, &mut visitQueue);
                }
            }

            if includeContainers {
                if let Some(seq) = curr.sequenceField() {
                    let steps = seq.b().Nodes.clone();
                    for step in steps {
                        guard.Step()?;
                        queue(&step, &mut reachedOrVisited, &mut visitQueue);
                    }
                }
                if curr.b().isClusterVessel {
                    let cluster = clusters.get(&curr).expect("nil pointer dereference: cluster");
                    let cnodes = cluster.b().Nodes.clone();
                    for n in cnodes {
                        guard.Step()?;
                        queue(&n, &mut reachedOrVisited, &mut visitQueue);
                    }
                }

                let gnodes = graph.b().Nodes.clone();
                for otherNode in gnodes {
                    guard.Step()?;
                    if reachedOrVisited.contains(&otherNode) {
                        continue;
                    }
                    let mut isDescendant = reachabilityIsDescendantOf(Some(&curr), Some(&otherNode), guard)?;
                    if !isDescendant {
                        isDescendant = reachabilityIsDescendantOf(Some(&otherNode), Some(&curr), guard)?;
                    }
                    if isDescendant {
                        queue(&otherNode, &mut reachedOrVisited, &mut visitQueue);
                    }
                }
            }
        }
        Ok(reachableNodes)
    }

    pub(crate) fn connectionTo(&self, otherNode: &NodeRef) -> Option<EdgeRef> {
        for e in self.edges() {
            if self.adjacent(&e) == *otherNode {
                return Some(e);
            }
        }
        None
    }

    // spillsOutOfNode detects whether the node has both
    // a corner inside the container and a corner outside the container node
    pub(crate) fn spillsOutOf(&self, node2: &NodeRef) -> bool {
        let t = self.tlv();
        let (w, h) = (self.w(), self.h());
        let corners = [
            t,
            geo::Point { X: t.X + w, Y: t.Y },
            geo::Point { X: t.X + w, Y: t.Y + h },
            geo::Point { X: t.X, Y: t.Y + h },
        ];

        let mut hasInside = false;
        let mut hasOutside = false;

        for corner in &corners {
            if node2.isPointOnNode(corner) {
                hasInside = true;
            } else {
                hasOutside = true;
            }
        }

        hasInside && hasOutside
    }

    // connectedNodes returns all nodes (including this one) that should move with it.
    // This includes nodes with a path to it, nodes in the same container
    // TODO for some reason e.From.Graph isn't populated with containers, so we pass mainGraph instead
    //
    // NOTE: Go iterates `mainGraph.Clusters` and `mainGraph.Containers` (maps)
    // while enqueueing; the enqueue order of a node that is both a container
    // child and a container depends on Go's random map order. The port uses
    // the Rust map order.
    pub(crate) fn connectedNodes(&self, excludedNodes: &[NodeRef], mainGraph: &GraphRef) -> Vec<NodeRef> {
        let node = self;
        let mut nodes: Vec<NodeRef> = Vec::new();
        let mut queue: std::collections::VecDeque<NodeRef> = std::collections::VecDeque::new();
        queue.push_back(node.clone());
        let mut inQueue: Set<NodeRef> = Set::default();
        inQueue.insert(node.clone());

        let mut excludedSet: Set<NodeRef> = Set::default();
        for n in excludedNodes {
            excludedSet.insert(n.clone());
        }

        let addQueue = |n: &NodeRef, queue: &mut std::collections::VecDeque<NodeRef>, inQueue: &mut Set<NodeRef>| {
            if !inQueue.contains(n) {
                queue.push_back(n.clone());
                inQueue.insert(n.clone());
            }
        };

        let (clusters, containers) = {
            let g = mainGraph.b();
            (g.Clusters.clone(), g.Containers.clone())
        };

        while let Some(curr) = queue.pop_front() {
            let mut isAncestorOrDescendantOfExcluded = false;
            for excluded in excludedNodes {
                if isDescendantOf(Some(&curr), Some(excluded)) {
                    isAncestorOrDescendantOfExcluded = true;
                    break;
                }
                if isDescendantOf(Some(excluded), Some(&curr)) && !isDescendantOf(Some(node), Some(&curr)) {
                    isAncestorOrDescendantOfExcluded = true;
                    break;
                }
            }
            if isAncestorOrDescendantOfExcluded {
                continue;
            }
            nodes.push(curr.clone());
            for e in curr.edges() {
                let connected = curr.adjacent(&e);
                if excludedSet.contains(&connected) {
                    continue;
                }
                addQueue(&connected, &mut queue, &mut inQueue);
            }

            // If the current is a cluster vessel, a cluster node may be a container, which we should include the children of
            if curr.b().isClusterVessel {
                let cluster = clusters.get(&curr).expect("nil pointer dereference: cluster");
                let cnodes = cluster.b().Nodes.clone();
                for n in cnodes {
                    if n.b().isContainer {
                        for child in containers.at(&Some(n.clone())) {
                            addQueue(&child, &mut queue, &mut inQueue);
                        }
                    }
                }
            }

            // If the current is a child of a cluster node container, we should include that cluster vessel
            for (vessel, cluster) in clusters.entries() {
                let cnodes = cluster.b().Nodes.clone();
                for n in cnodes {
                    if n.b().isContainer {
                        let mut nodeIsChild = false;
                        let mut excludedNodeIsChild = false;
                        for child in containers.at(&Some(n.clone())) {
                            if child == curr {
                                nodeIsChild = true;
                            }
                            if excludedSet.contains(&child) {
                                excludedNodeIsChild = true;
                                break;
                            }
                        }
                        if excludedNodeIsChild {
                            continue;
                        }
                        if nodeIsChild {
                            addQueue(&vessel, &mut queue, &mut inQueue);
                        }
                    }
                }
            }

            for (container, children) in containers.entries() {
                let Some(container) = container else { continue };
                if excludedSet.contains(&container) {
                    continue;
                }

                // If the node is part a container, include the container (which will include all children downstream) ...
                let mut nodeIsChild = false;
                let mut excludedNodeIsChild = false;

                for child in &children {
                    if *child == curr {
                        nodeIsChild = true;
                    }
                    if excludedSet.contains(child) {
                        excludedNodeIsChild = true;
                        break;
                    }
                }
                // ... unless an excluded node is also within that container
                if excludedNodeIsChild {
                    continue;
                }
                if nodeIsChild {
                    addQueue(&container, &mut queue, &mut inQueue);
                }

                // If the node is a container, everything within it is included
                if curr == container {
                    for child in containers.at(&Some(curr.clone())) {
                        addQueue(&child, &mut queue, &mut inQueue);
                    }
                }
            }
        }

        nodes
    }

    pub(crate) fn area(&self) -> f64 {
        self.w() * self.h()
    }

    /// LabelTopLeft returns the top-left point for a label of the given size and
    /// position relative to node.
    pub fn LabelTopLeft(&self, labelPosition: label::Position, width: f64, height: f64) -> geo::PointRef {
        let bx: geo::Box = if labelPosition.IsOutside() { self.shape().GetBox().b().clone() } else { self.InnerBox() };

        labelPosition.GetPointOnBox(&bx, LABEL_PADDING, width, height)
    }

    /// NodeOverlapCount reports how many nodes overlap node within delta.
    pub fn NodeOverlapCount(&self, nodes: &[NodeRef], delta: f64) -> i64 {
        let mut count = 0;
        for otherNode in nodes {
            if self.doesOverlapCalc(otherNode, delta) {
                count += 1;
            }
        }
        count
    }

    pub(crate) fn ports(&self) -> Vec<geo::Point> {
        let mut ports = Vec::with_capacity(12);
        let t = self.tlv();
        let (w, h) = (self.w(), self.h());
        for relativePoints in self.shape().SnapPointPercentages() {
            for point in relativePoints.iter() {
                let (xp, yp) = relPct(point);
                ports.push(geo::Point { X: t.X + (w * xp).round(), Y: t.Y + (h * yp).round() });
            }
        }
        ports
    }

    pub(crate) fn nthPortValue(&self, index: i64) -> (geo::Point, bool) {
        let mut i = 0i64;
        let t = self.tlv();
        let (w, h) = (self.w(), self.h());
        for relativePoints in self.shape().SnapPointPercentages() {
            for point in relativePoints.iter() {
                if i == index {
                    let (xp, yp) = relPct(point);
                    return (geo::Point { X: t.X + (w * xp).round(), Y: t.Y + (h * yp).round() }, true);
                }
                i += 1;
            }
        }
        (geo::Point::default(), false)
    }

    pub(crate) fn nthPort(&self, index: i64) -> Option<geo::Point> {
        let (port, ok) = self.nthPortValue(index);
        if !ok {
            return None;
        }
        Some(port)
    }

    pub(crate) fn pointToPortOrientation(&self, p: &geo::Point) -> geo::Orientation {
        for o in [geo::Top, geo::Right, geo::Left, geo::Bottom] {
            let indices = self.shape().PortIndices(o);
            for i in indices {
                let port = self.nthPort(i as i64);
                // p.Equals(port): false for a nil port.
                if let Some(port) = port {
                    if p.X == port.X && p.Y == port.Y {
                        return o;
                    }
                }
            }
        }
        geo::NONE
    }

    pub(crate) fn nthPortOnSideValue(&self, orientation: geo::Orientation, n: i64) -> (geo::Point, bool) {
        let indices = self.shape().PortIndices(orientation);
        self.nthPortValue(indices[n as usize] as i64)
    }

    pub(crate) fn tableColumnPortValue(&self, orientation: geo::Orientation, columnIndex: i64) -> (geo::Point, bool) {
        let (port, ok) = nodeshape::TableColumnPortValue(&*self.shape(), orientation, columnIndex);
        if ok {
            return (port, true);
        }
        self.nthPortOnSideValue(orientation, columnIndex)
    }

    pub(crate) fn centerPorts(&self) -> Vec<geo::Point> {
        let ports = self.ports();
        let indices = self.shape().CenterPortIndices();

        let mut centerPorts = Vec::with_capacity(indices.len());
        for index in indices {
            centerPorts.push(ports[index as usize]);
        }
        centerPorts
    }

    pub(crate) fn portsByOrientation(&self, orientation: geo::Orientation) -> Vec<geo::Point> {
        let ports = self.ports();
        let indices = self.shape().PortIndices(orientation);

        let mut portsForOrientation = Vec::with_capacity(indices.len());
        for index in indices {
            portsForOrientation.push(ports[index as usize]);
        }
        portsForOrientation
    }

    pub(crate) fn mirroredPorts(&self) -> Map<PointKey, geo::Point> {
        let ports = self.ports();

        let mut portToMirrored = Map::default();
        for (index, mirrorIndex) in self.shape().MirroredPortIndices().iter() {
            portToMirrored.insert(PointKey(ports[*index as usize]), ports[*mirrorIndex as usize]);
        }
        portToMirrored
    }

    pub(crate) fn mirror(&self, x: bool, y: bool) {
        // When mirroring, we need to consider the loop offsets
        // otherwise, the node bounding box can change can end up in the wrong position.
        let (mut topOffset, mut bottomOffset, mut leftOffset, mut rightOffset) = (0.0, 0.0, 0.0, 0.0);
        if !self.b().LoopOffsets.is_empty() {
            topOffset = self.loopOffset(geo::Top);
            leftOffset = self.loopOffset(geo::Left);
            rightOffset = self.loopOffset(geo::Right);
            bottomOffset = self.loopOffset(geo::Bottom);
        }
        let tl = self.tl().expect("nil pointer dereference: node.TopLeft");
        let (w, h) = (self.w(), self.h());
        if x {
            let v = tl.b().X;
            tl.bm().X = -v - w + leftOffset - rightOffset;
        }
        if y {
            let v = tl.b().Y;
            tl.bm().Y = -v - h + topOffset - bottomOffset;
        }
    }

    pub(crate) fn moveNodeAbsWithChildren(&self, x: f64, y: f64) {
        if let Some(tl) = self.tl() {
            let t = *tl.b();
            if t.X == x && t.Y == y {
                return;
            }
        }
        let t = self.tlv();
        self.moveNodeWithChildren(x - t.X, y - t.Y);
    }

    pub(crate) fn moveNodeWithChildren(&self, dx: f64, dy: f64) {
        if dx == 0.0 && dy == 0.0 {
            return;
        }
        self.translate(dx, dy);
        for child in self.g().allDescendantNodes(Some(self), true) {
            child.translate(dx, dy);
        }
    }

    pub(crate) fn translate(&self, dx: f64, dy: f64) {
        let tl = self.tl().expect("nil pointer dereference: node.TopLeft");
        let mut p = tl.bm();
        p.X += dx;
        p.Y += dy;
    }

    pub(crate) fn positionContainerChildren(&self, withPadding: bool) {
        if !self.b().isContainer {
            return;
        }
        let graph = self.g();
        let children: Vec<NodeRef> = graph.b().Containers.at(&Some(self.clone()));
        let mut padding = Spacing::default();
        if withPadding {
            padding = graph.containerPadding(Some(self), false);
        }
        let (tl, br) = children.fixedBounds();
        let (tl, br) = (tl.expect("nil pointer dereference"), br.expect("nil pointer dereference"));
        self.expandForLabels(&tl, &br);
        let (tlv, brv) = (*tl.b(), *br.b());
        let innerTL = self.InsidePlacement(brv.X - tlv.X, brv.Y - tlv.Y, padding);
        let dx = innerTL.X - tlv.X;
        let dy = innerTL.Y - tlv.Y;
        for childN in &children {
            childN.moveNodeWithChildren(dx, dy);
        }
    }

    pub fn CanContain(&self) -> bool {
        let st = self.b().shapeType;
        st != imageType && st != codeType && st != tableType && st != classType && st != textType
    }

    // Make enough room for label to be center positioned
    // Even though labels are placed at the last step, the most common is for long labels to be outside centered top or bottom.
    // So we pre-emptively make enough room for it.
    pub(crate) fn expandForLabels(&self, tl: &geo::PointRef, br: &geo::PointRef) {
        let children: Vec<NodeRef> = self.g().b().Containers.at(&Some(self.clone()));

        for child in &children {
            let Some(lbl) = child.label() else { continue };
            let ct = child.tlv();
            let cw = child.w();
            let (tlx, brx) = (tl.b().X, br.b().X);
            if ct.X == tlx || ct.X + cw == brx {
                let lw = lbl.b().Width;
                if lw > cw {
                    let nx = fmin(tlx, (ct.X + (cw / 2.0 - lw / 2.0)).floor());
                    tl.bm().X = nx;
                    let nbx = fmax(brx, (ct.X + cw - (cw / 2.0 - lw / 2.0)).ceil());
                    br.bm().X = nbx;
                }
            }
        }
    }

    pub(crate) fn wrapChildren(&self) {
        if !self.b().isContainer {
            return;
        }
        let graph = self.g();
        let children: Vec<NodeRef> = graph.b().Containers.at(&Some(self.clone()));
        let padding = graph.containerPadding(Some(self), false);
        let (tl, br) = children.fixedBounds();
        let (tl, br) = (tl.expect("nil pointer dereference"), br.expect("nil pointer dereference"));
        self.expandForLabels(&tl, &br);
        self.fitToBoundingBox(&tl, &br, padding);
        let (tlv, brv) = (*tl.b(), *br.b());
        let innerTL = self.InsidePlacement(brv.X - tlv.X, brv.Y - tlv.Y, padding);
        self.translate(tlv.X - innerTL.X, tlv.Y - innerTL.Y);
    }

    /// InsidePlacement returns the top-left point at which content of the given
    /// size and padding fits inside n.
    pub fn InsidePlacement(&self, width: f64, height: f64, padding: Spacing) -> geo::Point {
        let padX = padding.left + padding.right;
        let padY = padding.top + padding.bottom;
        // TODO update d2's GetInsidePlacement to take Spacing
        let shape = self.shape();
        let mut p = shape.GetInsidePlacement(width, height, padX, padY);

        if shape.GetType() == shape::CIRCLE_TYPE {
            let totalWidth = width + padX;
            let totalHeight = height + padY;

            let innerBox = shape.GetInnerBox();
            let (ibw, ibh) = { (innerBox.b().Width, innerBox.b().Height) };
            if ibw > totalWidth {
                p.X += (ibw - totalWidth) / 2.0;
            }
            if ibh > totalHeight {
                p.Y += (ibh - totalHeight) / 2.0;
            }
        }

        // always round the results from shape
        p.X = p.X.round();
        p.Y = p.Y.round();
        p.X -= (padX / 2.0).round() - padding.left;
        p.Y -= (padY / 2.0).round() - padding.top;
        p
    }

    /// InnerBox returns n's rounded content box.
    pub fn InnerBox(&self) -> geo::Box {
        // always round the results from shape
        let bx = self.shape().GetInnerBox();
        let b = bx.b();
        let t = *b.TopLeft.as_ref().expect("nil pointer dereference").b();
        geo::Box { TopLeft: Some(newPoint(t.X.round(), t.Y.round())), Width: b.Width.round(), Height: b.Height.round() }
    }

    pub(crate) fn isMajorityTarget(&self) -> bool {
        let mut counter = 0i64;
        for e in self.edges() {
            if e.isDirected() {
                if e.isTargetedTo(self) {
                    counter += 1;
                } else {
                    counter -= 1;
                }
            }
        }
        counter > 0
    }

    /// IconSize returns the icon size appropriate for iconPosition on n.
    pub fn IconSize(&self, iconPosition: label::Position) -> f64 {
        let minDimension = fmin(self.w(), self.h());
        let mut size = if iconPosition == label::InsideMiddleCenter {
            0.5 * minDimension
        } else {
            fmin(minDimension, fmax(defaultIconSize, 0.5 * minDimension))
        };
        size = fmin(size, MaxIconSize as f64);

        if !iconPosition.IsOutside() {
            let bx = self.InnerBox();
            size = fmin(fmax(0.0, bx.Width - 2.0 * LABEL_PADDING), size);
            size = fmin(fmax(0.0, bx.Height - 2.0 * LABEL_PADDING), size);
        }
        size
    }

    pub(crate) fn orderedNears(&self) -> Vec<NodeRef> {
        let mut nears: Vec<NodeRef> = self.b().Nears.iter().cloned().collect();
        sortNodesByID(&mut nears);
        nears
    }

    pub(crate) fn dfsWalk(&self, applyFunc: &mut dyn FnMut(&NodeRef) -> bool) -> bool {
        if applyFunc(self) {
            // stop early if apply returns true
            return true;
        }

        let graph = self.g();
        let (containers, clusters, sequences) = {
            let g = graph.b();
            (g.Containers.clone(), g.Clusters.clone(), g.Sequences.clone())
        };
        if self.b().isContainer {
            for childNode in containers.at(&Some(self.clone())) {
                if childNode.dfsWalk(applyFunc) {
                    return true;
                }
            }
        }

        if self.b().isClusterVessel {
            let c = clusters.get(self).expect("nil pointer dereference: cluster");
            let cnodes = c.b().Nodes.clone();
            for cn in cnodes {
                if cn.dfsWalk(applyFunc) {
                    return true;
                }
            }
        } else if let Some(s) = sequences.get(self) {
            let steps = s.b().Nodes.clone();
            for step in steps {
                if step.dfsWalk(applyFunc) {
                    return true;
                }
            }
        }
        false
    }

    pub(crate) fn rdfsWalk(&self, applyFunc: &mut dyn FnMut(&NodeRef)) {
        let graph = self.g();
        let (containers, clusters, sequences) = {
            let g = graph.b();
            (g.Containers.clone(), g.Clusters.clone(), g.Sequences.clone())
        };
        if self.b().isContainer {
            for childNode in containers.at(&Some(self.clone())) {
                childNode.rdfsWalk(applyFunc);
            }
        }

        if self.b().isClusterVessel {
            let c = clusters.get(self).expect("nil pointer dereference: cluster");
            let cnodes = c.b().Nodes.clone();
            for cn in cnodes {
                cn.rdfsWalk(applyFunc);
            }
        } else if let Some(s) = sequences.get(self) {
            let steps = s.b().Nodes.clone();
            for step in steps {
                step.rdfsWalk(applyFunc);
            }
        }

        applyFunc(self);
    }

    pub(crate) fn container(&self) -> Option<NodeRef> {
        let (cluster, sequence, container) = {
            let n = self.b();
            (n.Cluster.clone(), n.Sequence.clone(), n.Container.clone())
        };
        if clusterIsActive(cluster.as_ref()) {
            let v = cluster.unwrap().b().Vessel.clone();
            let c = v.b().Container.clone();
            c
        } else if sequenceIsActive(sequence.as_ref()) {
            let v = sequence.unwrap().b().Vessel.clone();
            let c = v.b().Container.clone();
            c
        } else {
            container
        }
    }

    pub(crate) fn containerLevel(&self) -> i64 {
        let mut level = 0;
        let mut curr = Some(self.clone());
        while let Some(c) = curr {
            level += 1;
            curr = c.container();
        }
        level
    }

    // returns the most nested container that is an ancestor of both n and otherNode
    pub(crate) fn nearestSharedAncestor(&self, otherNode: &NodeRef) -> Option<NodeRef> {
        let mut container = self.container();
        let mut otherContainer = otherNode.container();
        if container.is_none() || otherContainer.is_none() {
            return None;
        }
        let mut nLevel = self.containerLevel();
        let mut otherLevel = otherNode.containerLevel();

        while !nodeEq(container.as_ref(), otherContainer.as_ref()) {
            if nLevel == otherLevel {
                container = container.unwrap().container();
                nLevel -= 1;
                otherContainer = otherContainer.unwrap().container();
                otherLevel -= 1;
            } else if nLevel > otherLevel {
                container = container.unwrap().container();
                nLevel -= 1;
            } else {
                otherContainer = otherContainer.unwrap().container();
                otherLevel -= 1;
            }
            if container.is_none() || otherContainer.is_none() {
                return None;
            }
        }
        container
    }

    pub(crate) fn transpose(&self) {
        let (w, h) = (self.w(), self.h());
        self.set_w(h);
        self.set_h(w);
        self.tl().expect("nil pointer dereference: node.TopLeft").Transpose();
        let mut n = self.bm();
        let p = n.padding;
        n.padding.left = p.top;
        n.padding.top = p.left;
        n.padding.right = p.bottom;
        n.padding.bottom = p.right;
        let m = n.margin;
        n.margin.left = m.top;
        n.margin.top = m.left;
        n.margin.right = m.bottom;
        n.margin.bottom = m.right;
    }

    /// ContainerDirection reports the layout direction of n's owning container.
    pub fn ContainerDirection(&self) -> geo::Orientation {
        self.g().Direction(self.container().as_ref())
    }

    pub(crate) fn pad(&self, padding: i64) {
        let pad = padding as f64;
        self.translate(-pad, -pad);
        let (w, h) = (self.w(), self.h());
        self.set_w(w + 2.0 * pad);
        self.set_h(h + 2.0 * pad);
    }

    pub(crate) fn modifierElementAdjustments(&self) -> (f64, f64) {
        let n = self.b();
        let (mut dx, mut dy) = (0.0, 0.0);
        if n.Is3D {
            if n.shapeType == hexagonType {
                dy = threeDOffset / 2.0;
            } else {
                dy = threeDOffset;
            }
            dx = threeDOffset;
        } else if n.IsMultiple {
            dy = multipleOffset;
            dx = multipleOffset;
        }
        (dx, dy)
    }
}

/// (XPercentage, YPercentage) of a relative point.
#[inline]
fn relPct(p: &geo::RelativePointRef) -> (f64, f64) {
    let p = p.b();
    (p.XPercentage, p.YPercentage)
}
