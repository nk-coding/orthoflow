// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Crate root of the Rust port of d2layouts/d2talalayout/internal/nodeshape
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of `internal/nodeshape`.
#![allow(non_snake_case, non_camel_case_types, non_upper_case_globals, dead_code, unused_imports, clippy::all)]

pub use ::shape::Shape as D2Shape;
pub(crate) use gocompat::{Map, Set};

mod shape;
pub use self::shape::*;
mod snap_manual;
mod shape_c4person;
pub use shape_c4person::*;
mod shape_callout;
pub use shape_callout::*;
mod shape_circle;
pub use shape_circle::*;
mod shape_cloud;
pub use shape_cloud::*;
mod shape_cylinder;
pub use shape_cylinder::*;
mod shape_diamond;
pub use shape_diamond::*;
mod shape_document;
pub use shape_document::*;
mod shape_hexagon;
pub use shape_hexagon::*;
mod shape_image;
pub use shape_image::*;
mod shape_oval;
pub use shape_oval::*;
mod shape_package;
pub use shape_package::*;
mod shape_page;
pub use shape_page::*;
mod shape_parallelogram;
pub use shape_parallelogram::*;
mod shape_person;
pub use shape_person::*;
mod shape_queue;
pub use shape_queue::*;
mod shape_square;
pub use shape_square::*;
mod shape_square_gen;
pub use shape_square_gen::*;
mod shape_step;
pub use shape_step::*;
mod shape_stored_data;
pub use shape_stored_data::*;
mod shape_table;
pub use shape_table::*;

#[cfg(test)]
mod shape_test;
