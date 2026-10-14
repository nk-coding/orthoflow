// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/layoutgraph/packing_access.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

/// ContextWithTransactionWorkGuard lets a layout domain share one aggregate
/// request budget with every speculative graph transaction it creates.
pub fn ContextWithTransactionWorkGuard(ctx: &Ctx, guard: Option<&limits::WorkGuardRef>) -> Ctx {
    contextWithTransactionWorkGuard(ctx, guard)
}

pub fn Covers(outer: &geo::Box, inner: &geo::Box) -> bool {
    covers(outer, inner)
}

impl GraphRef {
    pub fn IsBadStateWithWorkGuard(
        &self,
        node: Option<&NodeRef>,
        graphState: Option<&GraphStateRef>,
        ignoreContainerEscape: bool,
        guard: &limits::WorkGuardRef,
    ) -> GoResult<bool> {
        self.isBadStateContext(node, graphState, ignoreContainerEscape, guard)
    }

    pub fn AllDescendantNodesWithWorkGuard(
        &self,
        node: Option<&NodeRef>,
        includeClusterNodes: bool,
        guard: &limits::WorkGuardRef,
    ) -> GoResult<Vec<NodeRef>> {
        self.allDescendantNodesGuarded(node, includeClusterNodes, guard)
    }
}

pub fn NewEdgeSegment(start: &geo::PointRef, end: &geo::PointRef, edge: &EdgeRef) -> EdgeSegmentRef {
    EdgeSegmentRef::new(EdgeSegment { Segment: geo::Segment::new(start.clone(), end.clone()), edge: edge.clone() })
}

impl NodeRef {
    pub fn AllReachableNodesContext(
        &self,
        includeContainers: bool,
        includeNears: bool,
        traverseTrees: bool,
        ignore: Option<&Set<NodeRef>>,
        guard: &limits::WorkGuardRef,
    ) -> GoResult<Vec<NodeRef>> {
        self.allReachableNodesGuarded(includeContainers, includeNears, traverseTrees, ignore, guard)
    }

    pub fn ReachableNodesContext(
        &self,
        shouldVisit: &dyn Fn(&NodeRef) -> bool,
        includeContainers: bool,
        includeNears: bool,
        traverseTrees: bool,
        ignore: Option<&Set<NodeRef>>,
        guard: &limits::WorkGuardRef,
    ) -> GoResult<Vec<NodeRef>> {
        self.reachableNodesGuarded(shouldVisit, includeContainers, includeNears, traverseTrees, ignore, guard)
    }

    /// Go `node.BoundingBox(allNodes)`; pass `None` for Go's nil `Nodes`.
    pub fn BoundingBox(&self, allNodes: Option<&[NodeRef]>) -> (geo::PointRef, geo::PointRef) {
        self.bounds(allNodes)
    }

    pub fn Orientation(&self, other: &NodeRef) -> geo::Orientation {
        self.orientation(other)
    }

    pub fn OverlapsLine(&self, start: &geo::Point, end: &geo::Point, delta: f64) -> bool {
        self.overlapsLine(start, end, delta)
    }

    pub fn ModifierElementAdjustments(&self) -> (f64, f64) {
        self.modifierElementAdjustments()
    }
}
