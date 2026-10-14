// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Crate root of the Rust port of d2layouts/d2talalayout/internal/quality
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of `d2layouts/d2talalayout/internal/quality`.
#![allow(non_snake_case, non_camel_case_types, non_upper_case_globals, dead_code, clippy::all)]
#![allow(unused_imports, unused_variables, unused_mut)]

pub(crate) use gocompat::math;
pub(crate) use gocompat::{errorf, wrapf, Ctx, GoError, GoResult, Map, Set};
pub(crate) use layoutgraph::{EdgeRef, GraphRef, NodeRef};
pub(crate) use limits::WorkGuardRef;

mod crossings;
mod evaluation_guard;
mod inspection;
mod inspection_geometry;
mod inspection_labels;
mod labels;
mod score;
mod scoring;

pub use crossings::*;
pub use evaluation_guard::*;
pub use inspection::*;
pub use inspection_geometry::*;
pub use inspection_labels::*;
pub use labels::*;
pub use score::*;
pub use scoring::*;
