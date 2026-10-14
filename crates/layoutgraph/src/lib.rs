// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Crate root of the Rust port of d2layouts/d2talalayout/internal/layoutgraph
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of `d2layouts/d2talalayout/internal/layoutgraph`.
#![allow(non_snake_case, non_camel_case_types, non_upper_case_globals, dead_code, clippy::all)]
#![allow(unused_imports, unused_variables, unused_mut)]

pub(crate) use gocompat::{errorf, wrapf, Ctx, CtxKey, GoError, GoResult, Map, Sentinel, Set};
pub(crate) use geo::{PointsExt, RouteExt};

mod cluster;
mod clone;
mod connected_set;
mod doc;
mod edge;
mod edge_style;
mod errors;
mod geometry;
mod geometry_policy;
mod graph;
mod graphjson_access;
mod group_geometry;
mod herding;
mod hierarchy;
mod hierarchy_access;
mod hub_spoke;
mod icon;
mod id;
#[path = "label.rs"]
mod label_;
mod labeling_access;
mod layout;
mod neighbor_requirements;
mod node;
mod node_graph_ownership;
mod overlap_scratch;
mod packing_access;
mod placement_access;
mod placement_cost;
mod positioned_validation;
mod quality_api;
mod routing_access;
mod rust_helpers;
mod segment_crossing;
mod sequence;
#[path = "shape.rs"]
mod shape_;
mod structure_api;
mod subgraph_access;
mod topology_api;
mod topology_preflight;
mod transaction;
mod transaction_snapshot;
mod tree;
mod teardown;
mod tree_access;
mod work;

pub use label_::*;
pub use cluster::*;
pub use clone::*;
pub use connected_set::*;
pub use doc::*;
pub use edge::*;
pub use edge_style::*;
pub use errors::*;
pub use geometry::*;
pub use geometry_policy::*;
pub use graph::*;
pub use graphjson_access::*;
pub use group_geometry::*;
pub use herding::*;
pub use hierarchy::*;
pub use hierarchy_access::*;
pub use hub_spoke::*;
pub use icon::*;
pub use id::*;
pub use labeling_access::*;
pub use layout::*;
pub use neighbor_requirements::*;
pub use node::*;
pub use node_graph_ownership::*;
pub use overlap_scratch::*;
pub use packing_access::*;
pub use placement_access::*;
pub use placement_cost::*;
pub use positioned_validation::*;
pub use quality_api::*;
pub use routing_access::*;
pub use rust_helpers::*;
pub use segment_crossing::*;
pub use sequence::*;
pub use shape_::*;
pub use structure_api::*;
pub use subgraph_access::*;
pub use topology_api::*;
pub use topology_preflight::*;
pub use transaction::*;
pub use transaction_snapshot::*;
pub use tree::*;
pub use teardown::*;
pub use tree_access::*;
pub use work::*;

#[cfg(test)]
mod tests;
