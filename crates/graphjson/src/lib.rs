// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Crate root of the Rust port of d2layouts/d2talalayout/internal/graphjson
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of `d2layouts/d2talalayout/internal/graphjson`.
//! Package graphjson serializes and reconstructs TALA layout graphs.
#![allow(non_snake_case, non_camel_case_types, non_upper_case_globals, dead_code, clippy::all)]
#![allow(unused_imports)]

pub(crate) use gocompat::{errorf, Ctx, GoError, GoResult, Map, Set};
pub(crate) use layoutgraph::{EdgeRef, GraphRef, NodeRef, NodesMethods};

mod codec;
mod direction;
mod preflight;
mod representation;

pub use codec::*;
pub use direction::*;
pub use preflight::*;
pub use representation::*;

#[cfg(test)]
mod tests;
