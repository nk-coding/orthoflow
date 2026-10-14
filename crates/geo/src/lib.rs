// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Crate root of the Rust port of lib/geo
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of `d2/lib/geo`.
#![allow(non_snake_case, non_camel_case_types, non_upper_case_globals, dead_code, unused_imports, clippy::all)]

mod bezier;
#[path = "box.rs"]
mod box_;
mod ellipse;
#[path = "math.rs"]
mod math_;
mod orientation;
mod point;
mod route;
mod segment;
mod spacing;
mod vector;

pub use bezier::*;
pub use box_::*;
pub use ellipse::*;
pub use math_::*;
pub use orientation::*;
pub use point::*;
pub use route::*;
pub use segment::*;
pub use spacing::*;
pub use vector::*;

#[cfg(test)]
mod tests;
