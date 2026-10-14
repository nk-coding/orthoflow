// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/layoutgraph/placement_access.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

impl GraphRef {
    pub fn FixedNodes(&self) -> Vec<NodeRef> {
        self.fixedNodes()
    }

    pub fn TreeEdgeMap(&self) -> Map<EdgeRef, bool> {
        self.buildIsTreeEdgeMap()
    }
    pub fn AddIsolatedTreeEdges(&self, edges: &mut Map<EdgeRef, bool>) {
        self.addIsolatedTreeEdgesToMap(edges);
    }

    pub fn ApplyEdgeAbductions(&self, abductions: &[EdgeAbductionRef]) {
        self.applyEdgeAbductions(abductions);
    }

    pub fn SyncNestedGeometry(&self) {
        self.syncNested();
    }
    pub fn TurnCost(&self) -> f64 {
        self.turnCostValue()
    }
    pub fn HalveTurnCost(&self) {
        self.bm().turnCost /= 2.0;
    }
    pub fn ResetTurnCost(&self) {
        self.resetTurnCost();
    }

    pub fn ContainerFixedOrigin(&self, container: Option<&NodeRef>) -> Option<geo::PointRef> {
        self.containerFixedOrigin(container)
    }
}

impl EdgeRef {
    pub fn FacingTablePortValues(&self, from: Option<&NodeRef>, to: Option<&NodeRef>) -> (geo::Point, geo::Point, bool, bool, geo::Orientation) {
        let ports = self.facingTablePorts(from, to);
        (ports.from, ports.to, ports.hasFrom, ports.hasTo, ports.orientation)
    }
    pub fn HasTableColumn(&self) -> bool {
        self.hasTableColumn()
    }
    pub fn IsAxisAligned(&self) -> bool {
        self.isAxisAligned()
    }
    pub fn NumSegments(&self) -> i64 {
        self.segmentCount()
    }
    pub fn BoundingBox(&self) -> (geo::PointRef, geo::PointRef) {
        self.bounds()
    }
    pub fn BoundingBoxValues(&self) -> (geo::Point, geo::Point) {
        self.boundingBoxValues()
    }
}

impl NodeRef {
    pub fn IsClass(&self) -> bool {
        self.b().shapeType == classType
    }
    pub fn Ports(&self) -> Vec<geo::Point> {
        self.ports()
    }

    pub fn OrthogonalDistanceTo(&self, other: &NodeRef) -> (f64, f64) {
        self.orthogonalDistanceTo(other)
    }
    pub fn ConnectedNodes(&self, excluded: &[NodeRef], graph: &GraphRef) -> Vec<NodeRef> {
        self.connectedNodes(excluded, graph)
    }
    pub fn PointPastFixedOrigin(&self, x: f64, y: f64, includeSizes: bool) -> bool {
        self.isPointPastFixedOrigin(x, y, includeSizes)
    }
    pub fn DoesOverlapAt(&self, other: &NodeRef, point: &geo::PointRef) -> bool {
        self.doesOverlapAt(other, point)
    }
    pub fn DoesOverlap(&self, other: &NodeRef) -> bool {
        self.doesOverlap(other)
    }
    pub fn OverlapsAlongDimension(&self, other: &NodeRef, horizontal: bool, includeSizes: bool) -> bool {
        self.overlapsAlongDimension(other, horizontal, includeSizes)
    }
    pub fn VisibilityGraphCandidate(&self, horizontal: bool, checkSide: bool, includeSizes: bool, other: &NodeRef, padding: f64) -> bool {
        self.isVisibilityGraphCandidate(horizontal, checkSide, includeSizes, other, padding)
    }
    pub fn MoveAbsWithChildren(&self, x: f64, y: f64) {
        self.moveNodeAbsWithChildren(x, y);
    }
    pub fn MoveWithChildren(&self, dx: f64, dy: f64) {
        self.moveNodeWithChildren(dx, dy);
    }
    pub fn PositionContainerChildren(&self, withPadding: bool) {
        self.positionContainerChildren(withPadding);
    }
    pub fn NearestSharedAncestor(&self, other: &NodeRef) -> Option<NodeRef> {
        self.nearestSharedAncestor(other)
    }
    pub fn ContainerLevel(&self) -> i64 {
        self.containerLevel()
    }
    /// Go `node.IsDescendantOf(ancestor)`; `ancestor` may be nil (the root).
    pub fn IsDescendantOf(&self, ancestor: Option<&NodeRef>) -> bool {
        isDescendantOf(Some(self), ancestor)
    }
    pub fn IDValue(&self) -> EntityID {
        self.entityID()
    }
    pub fn IsAdjacentTo(&self, other: &NodeRef, includeSizes: bool) -> bool {
        self.isAdjacentTo(other, includeSizes)
    }
    pub fn PassesThrough(&self, first: Option<&geo::PointRef>, second: Option<&geo::PointRef>) -> bool {
        self.passesThrough(first, second)
    }
    pub fn SpillsOutOf(&self, container: &NodeRef) -> bool {
        self.spillsOutOf(container)
    }
    pub fn DeltaTo(&self, other: &NodeRef, at: Option<&geo::PointRef>) -> i64 {
        self.deltaTo(other, at)
    }
    pub fn IsMajorityTarget(&self) -> bool {
        self.isMajorityTarget()
    }
    pub fn FitToGraph(&self, graph: &GraphRef, padding: Spacing) {
        self.fitNodeToGraph(graph, padding);
    }
}

/// Go `node.IsDescendantOf(ancestor)` for a possibly-nil node.
pub fn IsDescendantOf(node: Option<&NodeRef>, ancestor: Option<&NodeRef>) -> bool {
    isDescendantOf(node, ancestor)
}

impl ClusterRef {
    pub fn IsActive(&self) -> bool {
        self.isActive()
    }
}

impl SequenceRef {
    pub fn AbductedNodeByEdge(&self, edge: &EdgeRef) -> Option<NodeRef> {
        self.findAbductedNodeByEdge(edge)
    }
}

impl TreeRef {
    pub fn SentinelNode(&self) -> NodeRef {
        self.sentinelNode()
    }
}

impl TransactionRef {
    pub fn OriginalDimensions(&self, node: &NodeRef) -> (f64, f64) {
        let prior = self.b().PriorGraphState.clone().expect("nil PriorGraphState");
        let p = prior.b();
        match p.nodeGeometry.get(node) {
            Some(g) => (g.width, g.height),
            None => (0.0, 0.0),
        }
    }
}

/// ValidateSubgraphCombination validates node-owned topology across a set of
/// aliased graph views after each distinct graph-owned map identity has passed
/// Validate.
pub fn ValidateSubgraphCombination(ctx: &Ctx, graphs: &[GraphRef]) -> GoResult<()> {
    let graphs: Vec<Option<GraphRef>> = graphs.iter().cloned().map(Some).collect();
    validateCombineNodeTopology(ctx, &graphs)
}
