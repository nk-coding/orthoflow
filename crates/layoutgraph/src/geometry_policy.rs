// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/layoutgraph/geometry_policy.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

#[allow(unused_imports)]
use crate::*;

// Shared geometry invariants used by multiple layout stages.

/// AxisAlignmentTolerance is the maximum coordinate difference still treated
/// as axis-aligned.
pub const AxisAlignmentTolerance: f64 = 1.0;
/// NodeGap is the minimum gap between unconnected nodes.
pub const NodeGap: i64 = 20;
/// ConnectedNodeGap is the minimum gap between connected nodes.
pub const ConnectedNodeGap: i64 = 60;
/// TableNodeGap leaves enough room for table-edge routing.
pub const TableNodeGap: i64 = 2 * ConnectedNodeGap;
/// ContainerPadding is the ordinary padding on each side of a container.
pub const ContainerPadding: i64 = 60;

/// MaxIconSize caps positioned icon geometry.
pub const MaxIconSize: i64 = 64;

/// MinArrowheadClearance is the minimum route length needed to fit an arrowhead.
pub const MinArrowheadClearance: f64 = 20.0;
/// MinPortClearance keeps the first route bend away from its port.
pub const MinPortClearance: f64 = 20.0;
/// MinRouteNodeClearance keeps visibility-graph points away from node bounds.
pub const MinRouteNodeClearance: f64 = 20.0;
/// TreeParentSpacing is the fixed gap used to place and route tree levels.
pub const TreeParentSpacing: f64 = 100.0;

/// CrossingCostWeight is the shared penalty for one edge crossing.
/// Go evaluates the untyped constant 0.48*0.48*0.48 exactly and then rounds
/// once to float64.
pub const CrossingCostWeight: f64 = 0.110592;
