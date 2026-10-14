// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/layoutgraph/structure_api.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

pub const MaxTopologyReferences: i64 = maxEngineTopologyReferences;
pub const MaxRoutePoints: i64 = maxEngineRoutePoints;

impl NodeRef {
    /// Area returns a node's rectangular area.
    pub fn Area(&self) -> f64 {
        self.area()
    }

    /// OwningContainer resolves the active cluster or sequence vessel before
    /// falling back to the node's direct container.
    pub fn OwningContainer(&self) -> Option<NodeRef> {
        self.container()
    }

    pub fn IsSequenceStep(&self) -> bool {
        self.b().shapeType == stepType
    }

    pub fn SameShape(&self, other: &NodeRef) -> bool {
        self.b().shapeType == other.b().shapeType
    }

    pub fn UnmarkClusterVessel(&self) {
        self.bm().isClusterVessel = false;
    }

    pub fn OrderedNears(&self) -> Vec<NodeRef> {
        self.orderedNears()
    }

    pub fn ConnectionTo(&self, other: &NodeRef) -> Option<EdgeRef> {
        self.connectionTo(other)
    }

    pub fn WalkRDFS(&self, apply: &mut dyn FnMut(&NodeRef)) {
        self.rdfsWalk(apply);
    }

    pub fn SetContainer(&self, value: bool) {
        self.bm().isContainer = value;
    }
}

impl GraphRef {
    pub fn BoundingBox(&self) -> (Option<geo::PointRef>, Option<geo::PointRef>) {
        self.bounds()
    }

    pub fn NewRequestTransactionWithWorkGuard(
        &self,
        ctx: &Ctx,
        guard: Option<&limits::WorkGuardRef>,
        options: TransactionOptions,
    ) -> GoResult<TransactionRef> {
        self.newRequestTransactionWithGuard(ctx, guard, options)
    }

    pub fn IsTreeSentinel(&self, node: Option<&NodeRef>) -> bool {
        self.isTreeSentinel(node)
    }

    pub fn IsSequenceVessel(&self, node: &NodeRef) -> bool {
        self.isSequence(node)
    }

    /// ContainerRDFSOrder returns nested containers from innermost to outermost
    /// while charging the supplied request work budget.
    pub fn ContainerRDFSOrder(&self, root: Option<&NodeRef>, guard: &limits::WorkGuardRef) -> GoResult<Vec<NodeRef>> {
        self.containerRDFSOrderContext(root, guard)
    }

    pub fn ContainerRDFSOrderUnbounded(&self, root: Option<&NodeRef>) -> Vec<NodeRef> {
        self.containerRDFSOrder(root)
    }
}

impl HerdAssignmentRef {
    pub fn SameSidePairCount(&self) -> i64 {
        self.b().sameSidePaired.len() as i64
    }

    pub fn OppositeSidePairCount(&self) -> i64 {
        self.b().oppositeSidePaired.len() as i64
    }

    pub fn PairSameSide(&self, node: &NodeRef) {
        self.bm().sameSidePaired.insert(node.clone());
    }

    pub fn PairOppositeSide(&self, node: &NodeRef) {
        self.bm().oppositeSidePaired.insert(node.clone());
    }
}

impl EdgeRef {
    pub fn Reconnect(&self, node: &NodeRef, to: bool) {
        self.reconnect(node, to);
    }

    pub fn IsBidirectional(&self) -> bool {
        self.isBidirectional()
    }

    pub fn IsUndirected(&self) -> bool {
        self.isUndirected()
    }
}

impl SequenceRef {
    pub fn IsActive(&self) -> bool {
        self.isActive()
    }
}

pub fn RestoreGraphState(graph: &GraphRef, state: &GraphStateRef) {
    rollbackGraphState(graph, state);
}

/// SortNodesByID gives sibling layout domains the engine's stable node order.
pub fn SortNodesByID(nodes: &mut [NodeRef]) {
    sortNodesByID(nodes);
}
