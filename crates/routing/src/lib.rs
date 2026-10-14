// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Crate root of the Rust port of d2layouts/d2talalayout/internal/routing
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of Go package internal/routing. One module per Go file.
#![allow(non_snake_case, non_camel_case_types, non_upper_case_globals, dead_code, unused_imports, unused_variables, unused_mut, clippy::all)]
mod balance_crossings; pub use balance_crossings::*;
mod balance_route_guard; pub use balance_route_guard::*;
mod cluster_route_guard; pub use cluster_route_guard::*;
mod coordinator; pub use coordinator::*;
mod cost; pub use cost::*;
mod doc; pub use doc::*;
mod edge_simplify; pub use edge_simplify::*;
mod filter; pub use filter::*;
mod geometry; pub use geometry::*;
mod graph_stage; pub use graph_stage::*;
mod loop_router; pub use loop_router::*;
mod nudge_channels; pub use nudge_channels::*;
mod ovg_edge; pub use ovg_edge::*;
mod ovg_edge_router; pub use ovg_edge_router::*;
mod ovg_edge_set; pub use ovg_edge_set::*;
mod ovg; pub use ovg::*;
mod ovg_hierarchy; pub use ovg_hierarchy::*;
mod ovg_node; pub use ovg_node::*;
mod ovg_resource; pub use ovg_resource::*;
mod port_interior_balance; pub use port_interior_balance::*;
mod postprocess; pub use postprocess::*;
mod priority_queue; pub use priority_queue::*;
mod route; pub use route::*;
mod route_search_guard; pub use route_search_guard::*;
mod route_stage_guard; pub use route_stage_guard::*;
mod shortcut_routes; pub use shortcut_routes::*;
mod slingshot; pub use slingshot::*;
mod snapshot; pub use snapshot::*;
mod standalone_route_guard; pub use standalone_route_guard::*;
mod standalone_router; pub use standalone_router::*;
mod swap_ports; pub use swap_ports::*;
mod trace; pub use trace::*;
mod tree_routes; pub use tree_routes::*;
mod tuning; pub use tuning::*;
mod tunnel; pub use tunnel::*;
mod turns; pub use turns::*;
mod work; pub use work::*;
// Temporary stubs for items owned by the other half of this crate; see file headers.
