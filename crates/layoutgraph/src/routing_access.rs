// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/layoutgraph/routing_access.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

/// RoutingCostState is the cached routing-cost state that an atomic routing
/// stage must restore on failure.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RoutingCostState {
    pub Crossing: f64,
    pub Turn: f64,
    pub NonCenterPort: f64,
}

impl GraphRef {
    pub fn RoutingCosts(&self) -> RoutingCostState {
        let g = self.b();
        RoutingCostState { Crossing: g.crossingCost, Turn: g.turnCost, NonCenterPort: g.nonCenterPortCost }
    }

    pub fn RestoreRoutingCosts(&self, state: RoutingCostState) {
        let mut g = self.bm();
        g.crossingCost = state.Crossing;
        g.turnCost = state.Turn;
        g.nonCenterPortCost = state.NonCenterPort;
    }

    pub fn CrossingCost(&self) -> f64 {
        self.crossingCostValue()
    }
    pub fn NonCenterPortCost(&self) -> f64 {
        self.nonCenterPortCostValue()
    }
}

impl EdgeSegmentRef {
    pub fn Owner(&self) -> EdgeRef {
        self.b().edge.clone()
    }
}

impl EdgeSegment {
    pub fn Owner(&self) -> EdgeRef {
        self.edge.clone()
    }
}

impl EdgeRef {
    pub fn IDValue(&self) -> EntityID {
        self.entityID()
    }
    pub fn IsDuplicateOf(&self, other: &EdgeRef) -> bool {
        self.isDuplicateOf(other)
    }
    pub fn HasDuplicateIn(&self, edges: &[EdgeRef]) -> bool {
        self.hasDuplicateIn(edges)
    }
    pub fn MatchingArrowheads(&self, other: &EdgeRef) -> bool {
        self.hasMatchingArrowheads(other)
    }
    pub fn OwnArrowheadsMatch(&self) -> bool {
        self.ownArrowheadsMatch()
    }
    pub fn IsStraight(&self) -> bool {
        self.isStraight()
    }
    pub fn HasOverlappingEnd(&self) -> bool {
        self.hasOverlappingEnd()
    }
    pub fn EuclideanDistance(&self) -> f64 {
        self.euclideanDistance()
    }
    pub fn IsTreeEdge(&self) -> bool {
        self.isTreeEdge()
    }
}

impl NodeRef {
    pub fn ContainsPoint(&self, point: &geo::Point, padding: f64) -> bool {
        self.containsPoint(point, padding)
    }

    pub fn PortsByOrientation(&self, orientation: geo::Orientation) -> Vec<geo::Point> {
        self.portsByOrientation(orientation)
    }

    pub fn CenterPorts(&self) -> Vec<geo::Point> {
        self.centerPorts()
    }
    pub fn ContainsPointOnBox(&self, point: &geo::Point) -> bool {
        self.isPointOnNode(point)
    }
    pub fn IsPointNear(&self, point: &geo::Point) -> bool {
        self.isPointNear(point)
    }
    pub fn IsWithinBounds(&self, topLeft: &geo::Point, bottomRight: &geo::Point) -> bool {
        self.isWithinBounds(topLeft, bottomRight)
    }
    pub fn PortOrientation(&self, point: &geo::Point) -> geo::Orientation {
        self.pointToPortOrientation(point)
    }
    pub fn MirroredPorts(&self) -> Map<PointKey, geo::Point> {
        self.mirroredPorts()
    }
    pub fn OverlappingPorts(&self, other: &NodeRef) -> Map<PointKey, bool> {
        self.overlappingPorts(other)
    }
    pub fn DistanceTo(&self, other: &NodeRef, includeSizes: bool) -> f64 {
        self.distanceTo(other, includeSizes)
    }
}
