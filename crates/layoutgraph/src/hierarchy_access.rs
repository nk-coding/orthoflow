// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/layoutgraph/hierarchy_access.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

pub fn EnsureTransactionWorkGuard(ctx: &Ctx, location: &str) -> GoResult<(Ctx, limits::WorkGuardRef)> {
    ensureTransactionWorkGuard(ctx, location)
}

/// ExistingTransactionWorkGuard checks and returns the request's aggregate
/// transaction guard without allocating a derived context or standalone budget.
pub fn ExistingTransactionWorkGuard(ctx: &Ctx, location: &str) -> GoResult<(Option<limits::WorkGuardRef>, bool)> {
    existingTransactionWorkGuard(ctx, location)
}

/// GraphStateSnapshotOptions selects the topology and route identity captured
/// by a rollback snapshot.
#[derive(Clone, Copy, Debug, Default)]
pub struct GraphStateSnapshotOptions {
    pub CaptureTopology: bool,
    pub CaptureEdgeRoutes: bool,
}

/// NewGraphStateSnapshot creates an empty rollback snapshot with the requested
/// capture scope.
pub fn NewGraphStateSnapshot(options: GraphStateSnapshotOptions) -> GraphStateRef {
    GraphStateRef::new(GraphState {
        captureTopology: options.CaptureTopology,
        captureEdgeRoutes: options.CaptureEdgeRoutes,
        ..GraphState::default()
    })
}

impl GraphStateRef {
    pub fn UpdateWithWorkGuard(&self, g: &GraphRef, guard: &limits::WorkGuardRef) -> GoResult<()> {
        self.updateContext(g, guard)
    }

    /// OwnedNodes returns every node whose state is owned by g, including graph
    /// nodes temporarily removed by tree or sequence preprocessing. It charges the
    /// same work units in the same traversal order as the former hierarchy code.
    pub fn OwnedNodes(&self, g: &GraphRef, guard: &limits::WorkGuardRef) -> GoResult<Set<NodeRef>> {
        let mut owned: Set<NodeRef> = Set::default();
        for node in g.nodes() {
            guard.Step()?;
            owned.insert(node);
        }
        let nodes: Vec<NodeRef> = self.b().nodes.keys().cloned().collect();
        for node in nodes {
            guard.Step()?;
            if node.graph().as_ref() == Some(g) {
                owned.insert(node);
            }
        }
        Ok(owned)
    }
}

pub fn Validate(ctx: &Ctx, operation: &str, g: &GraphRef) -> GoResult<()> {
    validateEngineGraph(ctx, operation, Some(g))
}

impl GraphRef {
    pub fn HasFixedNode(&self) -> bool {
        self.hasFixedNode()
    }

    pub fn AllDescendantNodes(&self, node: Option<&NodeRef>, includeClusterNodes: bool) -> Vec<NodeRef> {
        self.allDescendantNodes(node, includeClusterNodes)
    }

    pub fn AbductEdges(&self, container: Option<&NodeRef>, toGraph: &GraphRef) -> Vec<EdgeAbductionRef> {
        self.abductEdges(container, toGraph)
    }

    pub fn RestoreEdgeAbductions(&self, edgeAbductions: &[EdgeAbductionRef]) -> Vec<EdgeAbductionRef> {
        self.restoreEdgeAbductions(edgeAbductions)
    }

    /// ReplaceEdgesUnchecked replaces graph edge ownership and rebuilds both
    /// endpoint incidence lists. The caller is responsible for supplying valid
    /// graph-owned endpoints.
    pub fn ReplaceEdgesUnchecked(&self, edges: Vec<EdgeRef>) {
        for edge in self.edges() {
            edge.from().bm().Edges = Vec::new();
            edge.to().bm().Edges = Vec::new();
        }
        self.bm().Edges = edges.clone();
        for edge in &edges {
            edge.from().addEdge(edge);
            edge.to().addEdge(edge);
        }
    }

    pub fn ContainerPadding(&self, container: Option<&NodeRef>, considerChildren: bool) -> Spacing {
        self.containerPadding(container, considerChildren)
    }
}

pub fn UniformSpacing(value: f64) -> Spacing {
    Spacing { top: value, bottom: value, left: value, right: value }
}

impl Spacing {
    pub fn Left(&self) -> f64 {
        self.left
    }
    pub fn Right(&self) -> f64 {
        self.right
    }
    pub fn Top(&self) -> f64 {
        self.top
    }
    pub fn Bottom(&self) -> f64 {
        self.bottom
    }
}

impl NodeRef {
    pub fn IsContainer(&self) -> bool {
        self.b().isContainer
    }

    pub fn IsTable(&self) -> bool {
        self.b().shapeType == tableType
    }

    pub fn SetClusterVessel(&self, value: bool) {
        self.bm().isClusterVessel = value;
    }

    pub fn Adjacent(&self, edge: &EdgeRef) -> NodeRef {
        self.adjacent(edge)
    }

    pub fn Center(&self) -> geo::PointRef {
        self.center()
    }

    pub fn Translate(&self, dx: f64, dy: f64) {
        self.translate(dx, dy);
    }

    pub fn Transpose(&self) {
        self.transpose();
    }

    pub fn Mirror(&self, x: bool, y: bool) {
        self.mirror(x, y);
    }

    pub fn FitToBoundingBox(&self, topLeft: &geo::PointRef, bottomRight: &geo::PointRef, padding: Spacing) {
        self.fitToBoundingBox(topLeft, bottomRight, padding);
    }

    pub fn WrapChildren(&self) {
        self.wrapChildren();
    }
}

impl EdgeRef {
    pub fn IsLoop(&self) -> bool {
        self.isLoop()
    }

    pub fn IsDirected(&self) -> bool {
        self.isDirected()
    }

    pub fn DirectedEndpoints(&self) -> (Option<NodeRef>, Option<NodeRef>, bool) {
        self.directedEndpoints()
    }

    pub fn IsTargetedTo(&self, node: &NodeRef) -> bool {
        self.isTargetedTo(node)
    }

    pub fn IsBetweenTableColumns(&self) -> bool {
        self.isBetweenTableColumns()
    }

    /// HierarchyRankWeight returns the authored-edge multiplicity represented by a
    /// temporary hierarchy DAG edge. Ordinary graph edges have implicit weight 1.
    pub fn HierarchyRankWeight(&self) -> i64 {
        let e = self.b();
        if !e.hierarchyRankWeightSet {
            return 1;
        }
        e.hierarchyRankWeight
    }

    /// SetHierarchyRankWeight records temporary hierarchy DAG metadata. It is kept
    /// out of graph serialization and reset by ordinary graph cloning.
    pub fn SetHierarchyRankWeight(&self, weight: i64) {
        let mut e = self.bm();
        e.hierarchyRankWeight = weight;
        e.hierarchyRankWeightSet = true;
    }
}
