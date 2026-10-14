// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Crate root of the Rust port of d2layouts/d2talalayout/internal/placementcost
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of `d2layouts/d2talalayout/internal/placementcost`.
#![allow(non_snake_case, non_camel_case_types, non_upper_case_globals, dead_code, clippy::all)]
#![allow(unused_imports, unused_variables, unused_mut)]

pub(crate) use gocompat::{Ctx, GoError, GoResult, Map, Set};
pub(crate) use gocompat::math;
pub(crate) use layoutgraph::{EdgeAbductionRef, EdgeRef, GraphRef, NodeRef, NodesMethods};

mod axis;
mod cluster;
mod doc;
mod edge_length;
mod edge_length_scorer;
mod flow_continuity;
mod geometry;
mod graph;
mod obstruction_bounds;
mod symmetry;

pub use axis::*;
pub use cluster::*;
pub use doc::*;
pub use edge_length::*;
pub use edge_length_scorer::*;
pub use flow_continuity::*;
pub use geometry::*;
pub use graph::*;
pub use obstruction_bounds::*;
pub use symmetry::*;
