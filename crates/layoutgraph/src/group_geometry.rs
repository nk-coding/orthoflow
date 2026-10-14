// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/layoutgraph/group_geometry.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

/// ClusterGeometryWork is the mutation contract used while synchronizing
/// cluster geometry. Layoutgraph owns the geometry; callers that need guarded
/// descendant movement provide the operations on top of the shared work budget.
pub trait ClusterGeometryWork: SharedWorkStepper {
    fn MoveNodeWithChildren(&self, node: &NodeRef, dx: f64, dy: f64) -> GoResult<()>;
    fn PositionContainerChildren(&self, node: &NodeRef) -> GoResult<()>;
}

pub(crate) struct unmeteredGroupGeometryWork;

impl SharedWorkStepper for unmeteredGroupGeometryWork {
    fn Step(&self) -> GoResult<()> {
        Ok(())
    }
    fn Finish(&self) -> GoResult<()> {
        Ok(())
    }
}

impl ClusterGeometryWork for unmeteredGroupGeometryWork {
    fn MoveNodeWithChildren(&self, node: &NodeRef, dx: f64, dy: f64) -> GoResult<()> {
        node.moveNodeWithChildren(dx, dy);
        Ok(())
    }
    fn PositionContainerChildren(&self, node: &NodeRef) -> GoResult<()> {
        node.positionContainerChildren(false);
        Ok(())
    }
}

pub(crate) static unmeteredGroupGeometry: unmeteredGroupGeometryWork = unmeteredGroupGeometryWork;
