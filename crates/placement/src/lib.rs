// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Crate root of the Rust port of d2layouts/d2talalayout/internal/placement
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of Go package internal/placement. One module per Go file.
#![allow(non_snake_case, non_camel_case_types, non_upper_case_globals, dead_code, unused_imports, unused_variables, unused_mut, clippy::all)]
mod alignment; pub use alignment::*;
mod axis; pub use axis::*;
mod candidate_movement; pub use candidate_movement::*;
mod cluster_connections; pub use cluster_connections::*;
mod cluster_optimization; pub use cluster_optimization::*;
mod compaction; pub use compaction::*;
mod container_orientation; pub use container_orientation::*;
mod dejitter; pub use dejitter::*;
mod direct; pub use direct::*;
mod edge_abduction_scratch; pub use edge_abduction_scratch::*;
mod equidistance; pub use equidistance::*;
mod gapreduction; pub use gapreduction::*;
mod initialize; pub use initialize::*;
mod metrics; pub use metrics::*;
mod moves; pub use moves::*;
mod node_placement; pub use node_placement::*;
mod optimization_guard; pub use optimization_guard::*;
mod optimizer_spatial_index; pub use optimizer_spatial_index::*;
mod sized_optimizer; pub use sized_optimizer::*;
mod sizeless_optimizer; pub use sizeless_optimizer::*;
mod stages; pub use stages::*;
mod stress_initialize; pub use stress_initialize::*;
mod swap; pub use swap::*;
mod symmetry; pub use symmetry::*;
mod transpose; pub use transpose::*;
mod tuning; pub use tuning::*;
mod types; pub use types::*;
// Temporary stubs for items owned by the other half of this crate; see file headers.
