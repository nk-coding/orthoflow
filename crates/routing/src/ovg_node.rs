// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/routing/ovg_node.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of routing/ovg_node.go.
#[allow(unused_imports)]
use crate::*;
use gocompat::{GoResult, Set};
use layoutgraph::NodeRef;

pub struct OVGNode {
    /// Go embeds `*geo.Point`. The pointee is never mutated through another
    /// alias after construction, so the port stores the value.
    pub Point: geo::Point,
    pub Edges: Vec<OVGEdgeRef>,
    /// Go `map[*layoutgraph.Node]struct{}` (nil until first insert).
    pub IsNearPort: Option<Set<NodeRef>>,
    /// portOwnersByNode keeps port metadata per graph node. More than one graph node can
    /// legitimately have a port at the same coordinate (for example, two shapes
    /// whose borders touch). The OVG canonicalizes nodes by coordinate, so a
    /// single owner and direction cannot describe that point correctly.
    /// (Go map; here a small insertion-ordered list, empty == nil.)
    pub(crate) portOwnersByNode: Vec<(NodeRef, ovgPortMetadata)>,
    pub Container: Option<NodeRef>,
    pub Index: usize,
    pub IsNodeCenter: bool,
    pub IsTunnel: bool,
}

gocompat::go_ptr!(OVGNodeRef, OVGNode);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ovgPortMetadata {
    pub(crate) directions: portDirectionSet,
    pub(crate) isCenterPort: bool,
}

/// A rounded OVG coordinate can represent more than one side of the same node.
/// A 1x1 square, for example, has top and left snap points at the same point.
/// Keep every valid role instead of letting the last side overwrite the first.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct portDirectionSet(pub u16);

pub(crate) fn newPortDirectionSet(direction: geo::Orientation) -> portDirectionSet {
    if direction.0 < geo::TopLeft.0 || direction.0 > geo::NONE.0 {
        return portDirectionSet(0);
    }
    portDirectionSet(1u16 << (direction.0 as u32))
}

impl portDirectionSet {
    #[inline]
    pub(crate) fn has(self, direction: geo::Orientation) -> bool {
        self.0 & newPortDirectionSet(direction).0 != 0
    }

    /// any reports whether predicate accepts at least one represented direction.
    /// Empty metadata is unrestricted and therefore behaves like NONE.
    #[inline]
    pub(crate) fn any(self, mut predicate: impl FnMut(geo::Orientation) -> bool) -> bool {
        if self.0 == 0 {
            return predicate(geo::NONE);
        }
        let mut d = geo::TopLeft.0;
        while d <= geo::NONE.0 {
            let direction = geo::Orientation(d);
            if self.has(direction) && predicate(direction) {
                return true;
            }
            d += 1;
        }
        false
    }

    pub(crate) fn transformed(self, transform: impl Fn(geo::Orientation) -> geo::Orientation) -> portDirectionSet {
        let mut transformed = portDirectionSet(0);
        self.any(|direction| {
            transformed.0 |= newPortDirectionSet(transform(direction)).0;
            false
        });
        transformed
    }
}

pub fn NewOVGNode(p: geo::Point) -> OVGNodeRef {
    // Edges grows lazily. A large share of candidate visibility nodes are
    // rejected before connection, so eagerly allocating four slots here creates
    // substantial garbage without helping surviving nodes.
    OVGNodeRef::new(OVGNode::new(p))
}

impl OVGNode {
    pub fn new(p: geo::Point) -> OVGNode {
        OVGNode {
            Point: p,
            Edges: Vec::new(),
            IsNearPort: None,
            portOwnersByNode: Vec::new(),
            Container: None,
            Index: 0,
            IsNodeCenter: false,
            IsTunnel: false,
        }
    }

    #[inline]
    pub fn X(&self) -> f64 {
        self.Point.X
    }
    #[inline]
    pub fn Y(&self) -> f64 {
        self.Point.Y
    }

    pub(crate) fn addPortMetadata(&mut self, owner: &NodeRef, added: ovgPortMetadata) {
        for (o, metadata) in self.portOwnersByNode.iter_mut() {
            if o == owner {
                metadata.directions.0 |= added.directions.0;
                metadata.isCenterPort = metadata.isCenterPort || added.isCenterPort;
                return;
            }
        }
        self.portOwnersByNode.push((owner.clone(), added));
    }

    pub(crate) fn addPortOwner(&mut self, owner: &NodeRef, direction: geo::Orientation, isCenterPort: bool) {
        self.addPortMetadata(owner, ovgPortMetadata { directions: newPortDirectionSet(direction), isCenterPort });
    }

    pub(crate) fn setCenterPort(&mut self, owner: &NodeRef) {
        let mut metadata = match self.portMetadataFor(owner) {
            Some(m) => m,
            None => return,
        };
        metadata.isCenterPort = true;
        self.addPortMetadata(owner, metadata);
    }

    #[inline]
    pub(crate) fn portMetadataFor(&self, owner: &NodeRef) -> Option<ovgPortMetadata> {
        for (o, m) in &self.portOwnersByNode {
            if o == owner {
                return Some(*m);
            }
        }
        None
    }

    #[inline]
    pub(crate) fn isPort(&self) -> bool {
        !self.portOwnersByNode.is_empty()
    }

    #[inline]
    pub(crate) fn isPortOf(&self, owner: &NodeRef) -> bool {
        self.portMetadataFor(owner).is_some()
    }

    pub(crate) fn portDirectionsFor(&self, owner: &NodeRef) -> (portDirectionSet, bool) {
        match self.portMetadataFor(owner) {
            Some(m) => (m.directions, true),
            None => (portDirectionSet(0), false),
        }
    }

    pub(crate) fn hasPortDirection(&self, owner: &NodeRef, direction: geo::Orientation) -> bool {
        let (directions, ok) = self.portDirectionsFor(owner);
        ok && directions.has(direction)
    }

    pub(crate) fn portDirectionsForObstacle(&self, owner: &NodeRef) -> portDirectionSet {
        self.portDirectionsFor(owner).0
    }

    pub(crate) fn setPortDirections(&mut self, owner: &NodeRef, directions: portDirectionSet) {
        for (o, m) in self.portOwnersByNode.iter_mut() {
            if o == owner {
                m.directions = directions;
                return;
            }
        }
    }

    pub(crate) fn isCenterPortOf(&self, owner: &NodeRef) -> bool {
        match self.portMetadataFor(owner) {
            Some(m) => m.isCenterPort,
            None => false,
        }
    }

    /// Go `n.portOwners()` (the map; read-only view).
    #[inline]
    pub(crate) fn portOwners(&self) -> &[(NodeRef, ovgPortMetadata)] {
        &self.portOwnersByNode
    }

    pub(crate) fn addEdge(&mut self, e: OVGEdgeRef) {
        self.Edges.push(e);
    }

    pub(crate) fn distanceToBoundary(&self, nodeB: &NodeRef) -> f64 {
        let x1 = self.Point.X;
        let y1 = self.Point.Y;

        let tl = nodeB.tlv();
        let (w, h) = (nodeB.w(), nodeB.h());
        let x2 = tl.X;
        let y2 = tl.Y;
        let x2b = tl.X + w;
        let y2b = tl.Y + h;

        let left = x2b < x1;
        let right = x1 < x2;
        let top = y2b < y1;
        let bottom = y1 < y2;

        if top && left {
            geo::EuclideanDistance(x1, y1, x2b, y2b)
        } else if left && bottom {
            geo::EuclideanDistance(x1, y1, x2b, y2)
        } else if bottom && right {
            geo::EuclideanDistance(x1, y1, x2, y2)
        } else if right && top {
            geo::EuclideanDistance(x1, y1, x2, y2b)
        } else if left {
            x1 - x2b
        } else if right {
            x2 - x1
        } else if bottom {
            y2 - y1
        } else if top {
            y1 - y2b
        } else {
            0.0
        }
    }
}

impl OVGNodeRef {
    /// Go `n.X` (promoted from the embedded point).
    #[inline]
    pub fn X(&self) -> f64 {
        self.b().Point.X
    }
    /// Go `n.Y`
    #[inline]
    pub fn Y(&self) -> f64 {
        self.b().Point.Y
    }
    /// Go `*n.Point`
    #[inline]
    pub fn P(&self) -> geo::Point {
        self.b().Point
    }
    /// A fresh `*geo.Point` holding the node's coordinates (for APIs taking pointers).
    #[inline]
    pub fn PRef(&self) -> geo::PointRef {
        geo::PointRef::new(self.b().Point)
    }
    #[inline]
    pub fn Index(&self) -> usize {
        self.b().Index
    }

    pub(crate) fn addPortMetadata(&self, owner: &NodeRef, added: ovgPortMetadata) {
        self.bm().addPortMetadata(owner, added)
    }
    pub(crate) fn addPortOwner(&self, owner: &NodeRef, direction: geo::Orientation, isCenterPort: bool) {
        self.bm().addPortOwner(owner, direction, isCenterPort)
    }
    pub(crate) fn setCenterPort(&self, owner: &NodeRef) {
        self.bm().setCenterPort(owner)
    }
    pub(crate) fn portMetadataFor(&self, owner: &NodeRef) -> Option<ovgPortMetadata> {
        self.b().portMetadataFor(owner)
    }
    pub(crate) fn isPort(&self) -> bool {
        self.b().isPort()
    }
    pub(crate) fn isPortOf(&self, owner: &NodeRef) -> bool {
        self.b().isPortOf(owner)
    }
    pub(crate) fn portDirectionsFor(&self, owner: &NodeRef) -> (portDirectionSet, bool) {
        self.b().portDirectionsFor(owner)
    }
    pub(crate) fn hasPortDirection(&self, owner: &NodeRef, direction: geo::Orientation) -> bool {
        self.b().hasPortDirection(owner, direction)
    }
    pub(crate) fn portDirectionsForObstacle(&self, owner: &NodeRef) -> portDirectionSet {
        self.b().portDirectionsForObstacle(owner)
    }
    pub(crate) fn setPortDirections(&self, owner: &NodeRef, directions: portDirectionSet) {
        self.bm().setPortDirections(owner, directions)
    }
    pub(crate) fn isCenterPortOf(&self, owner: &NodeRef) -> bool {
        self.b().isCenterPortOf(owner)
    }
    /// Go `n.portOwners()`: a snapshot of the owner → metadata entries.
    pub(crate) fn portOwners(&self) -> Vec<(NodeRef, ovgPortMetadata)> {
        self.b().portOwnersByNode.clone()
    }

    pub(crate) fn sharesPortOwner(&self, other: &OVGNodeRef) -> bool {
        let n = self.b();
        if n.portOwnersByNode.is_empty() {
            return false;
        }
        let o = other.b();
        for (owner, _) in &n.portOwnersByNode {
            if o.isPortOf(owner) {
                return true;
            }
        }
        false
    }

    pub(crate) fn addEdge(&self, e: OVGEdgeRef) {
        self.bm().addEdge(e)
    }

    #[inline]
    pub(crate) fn adjacent(&self, e: &OVGEdge) -> OVGNodeRef {
        if *self == e.From {
            return e.To.clone();
        }
        e.From.clone()
    }

    pub fn Adjacent(&self, edge: &OVGEdge) -> OVGNodeRef {
        self.adjacent(edge)
    }

    pub(crate) fn distanceToBoundary(&self, nodeB: &NodeRef) -> f64 {
        self.b().distanceToBoundary(nodeB)
    }

    /// hasUnobstructedLineToPorts checks if an OVG node has unobstructed lines to non-container ports
    /// numNodes indicates how many ports belonging to different nodes it must have lines to
    pub(crate) fn hasUnobstructedLineToPorts(&self, ovg: &OVG, portIndex: &ovgPortIndex, numNodes: i64, guard: &mut ovgBuildGuard) -> GoResult<bool> {
        hasUnobstructedLineToPorts(self.P(), ovg, portIndex, numNodes, guard)
    }
}

/// Go `(*OVGNode).hasUnobstructedLineToPorts` for a candidate node, which only
/// uses the node's point.
pub(crate) fn hasUnobstructedLineToPorts(ovgPoint: geo::Point, ovg: &OVG, portIndex: &ovgPortIndex, numNodes: i64, guard: &mut ovgBuildGuard) -> GoResult<bool> {
    guard.check()?;
    // Only care about ovg nodes which are port aligned
    let mut distinctPortNodeCount: i64 = 0;
    let mut ovgPointRef: Option<geo::PointRef> = None;

    let mut aligned = portIndex.alignedOwners(ovgPoint.X, ovgPoint.Y);
    let hasAlignedPort = !aligned.byX.is_empty() || !aligned.byY.is_empty();
    loop {
        let (ownerIndex, ok) = aligned.next(guard)?;
        if !ok {
            break;
        }
        let gNode = &portIndex.owners[ownerIndex];
        let portNodes = match ovg.Ports.get(gNode) {
            Some(p) => p,
            None => continue,
        };
        let gtl = gNode.tlv();
        let (gw, gh) = (gNode.w(), gNode.h());
        let (gInvisible, gSequence) = {
            let gb = gNode.b();
            (gb.IsInvisible, gb.Sequence.clone())
        };
        for portNode in portNodes {
            guard.step()?;
            let pp = portNode.P();
            if pp.X != ovgPoint.X && pp.Y != ovgPoint.Y {
                continue;
            }
            // Port nodes which can make a straight line between it, its shape's edge, and the point, are NOT candidates for unobstructed path
            if pp.X == ovgPoint.X && ((pp.X == gtl.X) || (pp.X == gtl.X + gw)) {
                continue;
            }
            if pp.Y == ovgPoint.Y && ((pp.Y == gtl.Y) || (pp.Y == gtl.Y + gh)) {
                continue;
            }

            // Remove nodes which are obstructed from a port by another node
            let mut hasDirectLineToThisPort = true;
            let blockers = if pp.X == ovgPoint.X {
                portIndex.verticalBlockers.get(&f64key(pp.X))
            } else {
                portIndex.horizontalBlockers.get(&f64key(pp.Y))
            };
            if let Some(blockers) = blockers {
                let portRef = geo::PointRef::new(pp);
                let ovgRef = ovgPointRef.get_or_insert_with(|| geo::PointRef::new(ovgPoint)).clone();
                for otherGNode in blockers {
                    guard.step()?;
                    if gNode == otherGNode {
                        continue;
                    }
                    if gInvisible {
                        continue;
                    }
                    // don't count nodes of the same sequence as blocking each other
                    if let Some(gs) = &gSequence {
                        if let Some(os) = &otherGNode.b().Sequence {
                            if gs == os {
                                continue;
                            }
                        }
                    }
                    if otherGNode.PassesThrough(Some(&portRef), Some(&ovgRef)) {
                        hasDirectLineToThisPort = false;
                        break;
                    }
                }
            }

            if hasDirectLineToThisPort {
                distinctPortNodeCount += 1;
                if distinctPortNodeCount == numNodes {
                    return Ok(true);
                }
                break;
            }
        }
    }

    guard.check()?;
    Ok(!hasAlignedPort)
}
