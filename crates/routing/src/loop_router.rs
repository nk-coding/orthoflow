// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/routing/loop_router.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of routing/loop_router.go.
#[allow(unused_imports)]
use crate::*;
use layoutgraph::EdgeRef;

// makeLoopRoute makes a `Route` based on the edge points
pub(crate) fn makeLoopRoute(e: &EdgeRef, ovg: &OVG) -> RouteRef {
    let (from, to, points) = {
        let eb = e.b();
        (eb.From.clone(), eb.To.clone(), eb.Points.clone())
    };
    let mut nodes: Vec<OVGNodeRef> = Vec::with_capacity(points.len() + 2);
    nodes.push(ovg.Centers.get(&from).expect("loop route source has no OVG center").clone());
    for p in &points {
        nodes.push(NewOVGNode(p.get()));
    }
    nodes.push(ovg.Centers.get(&to).expect("loop route target has no OVG center").clone());
    RouteRef::new(Route {
        GEdge: e.clone(),
        FromPort: points[0].get(),
        ToPort: points[points.len() - 1].get(),
        OVGNodes: nodes,
    })
}
