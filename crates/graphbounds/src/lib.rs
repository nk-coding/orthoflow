// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Crate root of the Rust port of d2layouts/d2talalayout/internal/graphbounds
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of `d2layouts/d2talalayout/internal/graphbounds`.
#![allow(non_snake_case, non_camel_case_types, non_upper_case_globals, dead_code, clippy::all)]
#![allow(unused_imports)]

pub(crate) use gocompat::{GoError, GoResult, Set};
pub(crate) use layoutgraph::{NodeRef, NodesMethods};

mod bounds;
mod doc;

pub use bounds::*;
pub use doc::*;

#[cfg(test)]
mod tests;
