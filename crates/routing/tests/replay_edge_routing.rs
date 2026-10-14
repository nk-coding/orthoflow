// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Copyright 2026 Niklas Krieger
//
// Stage invocations mirror d2layouts/d2talalayout/internal/engine/pipeline.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).

//! Stage replay for EdgeRouting (stages 21 and 24), mirroring
//! engine/pipeline.go edgeRoutingStage/runGraphRouting.
#![allow(non_snake_case)]

extern crate orthoflow_routing as routing;
#[test]
#[ignore = "needs Go reference data (ORTHOFLOW_GOREF), see README"]
fn replay_edge_routing() {
    testkit::assert_replay("EdgeRouting", |ctx, env| {
        let options = routing::GraphRouteOptions {
            ForceReroute: env.forceReroute,
            RoutesPreviouslyCompleted: env.edgeRoutingComplete,
            ..Default::default()
        };
        // Go: on error the pipeline restores edgeRoutingComplete; on success it
        // stores RouteGraph's result (the observers only toggle that flag and
        // the snapshot cache, neither of which reaches the graph).
        let routingComplete = routing::RouteGraph(ctx, &env.graph, options)?;
        env.edgeRoutingComplete = routingComplete;
        Ok(())
    });
}
