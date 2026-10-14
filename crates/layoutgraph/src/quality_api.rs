// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/layoutgraph/quality_api.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

/// ValidatePositionedGraph checks the structural and positioned geometry
/// invariants required by algorithms that inspect labels and routed edges.
pub fn ValidatePositionedGraph(ctx: &Ctx, operation: &str, graph: &GraphRef) -> GoResult<()> {
    ValidatePositionedGraphSelection(ctx, operation, Some(graph), &[])
}

impl GraphRef {
    /// Area returns the area of the graph's unrounded bounding box, including
    /// routed edges and positioned labels.
    pub fn Area(&self) -> f64 {
        self.area()
    }
}

impl NodeRef {
    /// IsImage reports whether the node uses the image shape. Image nodes do not
    /// render their icon as a separately positioned object.
    pub fn IsImage(&self) -> bool {
        self.b().shapeType == imageType
    }

    /// EffectiveContainer returns the node's active layout container. Active
    /// cluster and sequence vessels replace the node's direct container while
    /// those grouping algorithms are running.
    pub fn EffectiveContainer(&self) -> Option<NodeRef> {
        self.container()
    }

    /// AncestryParent returns the parent relation used by layout ancestry checks.
    /// Unlike EffectiveContainer, active cluster and sequence ancestry follows the
    /// vessel itself.
    pub fn AncestryParent(&self) -> Option<NodeRef> {
        ancestryParent(Some(self))
    }
}
