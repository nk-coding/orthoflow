// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Crate root of the Rust port of d2layouts/d2talalayout/internal/grouping
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of `d2layouts/d2talalayout/internal/grouping`.
#![allow(non_snake_case, non_camel_case_types, non_upper_case_globals, dead_code, clippy::all)]
#![allow(unused_imports, unused_variables, unused_mut)]

pub(crate) use gocompat::rand::RandRef;
pub(crate) use gocompat::{errorf, math, Ctx, GoError, GoResult, Map, Set};
pub(crate) use layoutgraph::{
    ClusterRef, EdgeAbduction, EdgeAbductionRef, EdgeRef, EntityID, GraphRef, NodeRef, NodesMethods, SequenceRef,
    TreeRef,
};
pub(crate) use limits::WorkGuardRef;

mod cluster_discovery;
mod clusters;
mod join;
mod lifecycle;
mod sequences;

pub use cluster_discovery::*;
pub use clusters::*;
pub use join::*;
pub use lifecycle::*;
pub use sequences::*;
