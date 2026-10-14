// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Crate root of the Rust port of lib/shape
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of `d2/lib/shape` (plus the geometry part of `d2/lib/svg/path.go` as module `svg`).
#![allow(non_snake_case, non_camel_case_types, non_upper_case_globals, dead_code, unused_imports, clippy::all)]

pub mod svg;

mod shape;
mod shape_c4_person;
mod shape_callout;
mod shape_circle;
mod shape_class;
mod shape_cloud;
mod shape_code;
mod shape_cylinder;
mod shape_diamond;
mod shape_document;
mod shape_hexagon;
mod shape_image;
mod shape_oval;
mod shape_package;
mod shape_page;
mod shape_parallelogram;
mod shape_person;
mod shape_queue;
mod shape_real_square;
mod shape_square;
mod shape_step;
mod shape_stored_data;
mod shape_table;
mod shape_text;

pub use shape::*;
pub use shape_c4_person::*;
pub use shape_callout::*;
pub use shape_circle::*;
pub use shape_class::*;
pub use shape_cloud::*;
pub use shape_code::*;
pub use shape_cylinder::*;
pub use shape_diamond::*;
pub use shape_document::*;
pub use shape_hexagon::*;
pub use shape_image::*;
pub use shape_oval::*;
pub use shape_package::*;
pub use shape_page::*;
pub use shape_parallelogram::*;
pub use shape_person::*;
pub use shape_queue::*;
pub use shape_real_square::*;
pub use shape_square::*;
pub use shape_step::*;
pub use shape_stored_data::*;
pub use shape_table::*;
pub use shape_text::*;
