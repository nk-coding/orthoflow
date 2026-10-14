// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Crate root of the Rust port of d2layouts/d2talalayout/internal/hierarchy
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of `d2layouts/d2talalayout/internal/hierarchy`.
#![allow(non_snake_case, non_camel_case_types, non_upper_case_globals, dead_code, clippy::all)]
#![allow(unused_imports, unused_variables, unused_mut)]

pub(crate) use gocompat::math;
pub(crate) use gocompat::{errorf, wrapf, Ctx, GoError, GoResult, Map, Set};
pub(crate) use layoutgraph::{EdgeRef, GraphRef, NodeRef, NodesMethods};
pub(crate) use limits::{OptimizationWorkGuardRef, WorkGuardRef};

mod brandes_kopf;
mod compound;
mod crossing;
mod discovery;
mod doc;
mod eligibility;
mod orientation;
mod placement;
mod rank;
mod rank_graph;
mod rank_queue;
mod rank_simplex;
mod sifting;

pub use brandes_kopf::*;
pub use compound::*;
pub use crossing::*;
pub use discovery::*;
pub use doc::*;
pub use eligibility::*;
pub use orientation::*;
pub use placement::*;
pub use rank::*;
pub use rank_graph::*;
pub use rank_queue::*;
pub use rank_simplex::*;
pub use sifting::*;

#[cfg(test)]
mod parity_test;
