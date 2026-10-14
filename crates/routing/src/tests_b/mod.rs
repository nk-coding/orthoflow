// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from the following files of terrastruct/d2, Copyright 2022 Terrastruct Inc.:
//   - d2layouts/d2talalayout/internal/routing/test_helpers_test.go
//   - d2layouts/d2talalayout/internal/routing/kernel_test_helpers_test.go
// Rust port: Copyright 2026 Niklas Krieger.

//! Ported Go unit tests (routing half b). Shared helpers live here.
#![allow(dead_code)]
use crate::*;
use gocompat::context::{self, Ctx};
use gocompat::GoError;
use layoutgraph::{EdgeRef, GraphRef, NodeRef};

mod balance_order_test;
mod edge_postprocess_test;
mod edge_simplify_safety_test;
mod graph_test;
mod nudge_channels_test;
mod port_interior_balance_test;
mod reorder_duplicates_test;
mod route_flavor_test;
mod routing_cancellation_test;
mod shortcut_routes_test;
mod slingshot_test;
mod stage_atomicity_test;
mod stage_resource_test;
mod standalone_router_test;
mod standalone_table_ports_test;
mod swap_ports_test;
mod tree_loop_test;

pub(crate) fn bg() -> Ctx {
    context::Background()
}

pub(crate) fn canceledContext() -> Ctx {
    let (ctx, cancel) = context::WithCancel(&context::Background());
    cancel();
    ctx
}

pub(crate) fn requireCanceled(err: &GoError) {
    assert!(err.Is(&context::Canceled), "errors.Is({err}, context.Canceled) = false");
}

pub(crate) fn requireCanceledAt(err: &GoError, location: &str) {
    requireCanceled(err);
    assert!(err.Error().contains(location), "cancellation error = {err}, want operation {location:?}");
}

pub(crate) fn pt(x: f64, y: f64) -> geo::PointRef {
    geo::NewPoint(x, y)
}

pub(crate) fn pts(points: &[(f64, f64)]) -> Vec<geo::PointRef> {
    points.iter().map(|&(x, y)| geo::NewPoint(x, y)).collect()
}

pub(crate) fn setPoints(e: &EdgeRef, points: &[(f64, f64)]) {
    e.bm().Points = pts(points);
}

/// `*e.Points[i]`
pub(crate) fn p(e: &EdgeRef, i: usize) -> geo::Point {
    e.b().Points[i].get()
}

pub(crate) fn values(e: &EdgeRef) -> Vec<(f64, f64)> {
    e.b().Points.iter().map(|p| (p.X(), p.Y())).collect()
}

/// `layoutgraph.NewNode(id, w, h)` with `TopLeft = geo.NewPoint(x, y)`.
pub(crate) fn nodeAt(id: i64, w: f64, h: f64, x: f64, y: f64) -> NodeRef {
    let n = layoutgraph::NewNode(id, w, h);
    n.set_tl(Some(geo::NewPoint(x, y)));
    n
}

/// Snapshot of every route's point values (and point identities) in a graph.
pub(crate) fn routeState(g: &GraphRef) -> Vec<Vec<(usize, f64, f64)>> {
    g.edges().iter().map(|e| e.b().Points.iter().map(|p| (p.addr(), p.X(), p.Y())).collect()).collect()
}

// evenlyDistribute returns a list of positions of length num, where the values are evenly distributed between floor and ceiling
pub(crate) fn evenlyDistribute(floor: f64, ceil: f64, num: i64) -> Vec<f64> {
    let mut out = Vec::new();
    if num <= 0 || floor >= ceil {
        return out;
    }
    let increment = ((ceil - floor) / (num + 1) as f64).floor();
    if increment <= 0.0 {
        return out;
    }
    for i in 1..=num {
        out.push(floor + i as f64 * increment);
    }
    out
}

/// post_route_atomicity_test.go's exactRouteTestSnapshot: point identities
/// and values of one route.
pub(crate) struct exactRouteTestSnapshot {
    pub(crate) edge: EdgeRef,
    pub(crate) route: Vec<geo::PointRef>,
    pub(crate) values: Vec<geo::Point>,
}

pub(crate) fn captureExactRouteTest(edge: &EdgeRef) -> exactRouteTestSnapshot {
    let route = edge.points();
    let values = route.iter().map(|p| p.get()).collect();
    exactRouteTestSnapshot { edge: edge.clone(), route, values }
}

impl exactRouteTestSnapshot {
    pub(crate) fn changed(&self) -> bool {
        let points = self.edge.points();
        if points.len() != self.route.len() {
            return true;
        }
        for (i, p) in points.iter().enumerate() {
            if *p != self.route[i] {
                return true;
            }
        }
        for (i, p) in self.route.iter().enumerate() {
            if p.get() != self.values[i] {
                return true;
            }
        }
        false
    }

    #[track_caller]
    pub(crate) fn assertRestored(&self) {
        assert!(!self.changed(), "route was not restored exactly");
    }
}
