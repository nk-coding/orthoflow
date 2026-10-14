// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Copyright 2026 Niklas Krieger
//
// Stage invocations mirror d2layouts/d2talalayout/internal/engine/pipeline.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).

//! Stage replays for routing half b (post-processing stages + EdgeRouting).
//! Each mirrors the stage wrapper in engine/pipeline.go.

extern crate orthoflow_routing as routing;
#[test]
#[ignore = "needs Go reference data (ORTHOFLOW_GOREF), see README"]
fn replay_crosshatch() {
    testkit::assert_replay("Crosshatch", |ctx, env| routing::Crosshatch(ctx, &env.graph));
}

#[test]
#[ignore = "needs Go reference data (ORTHOFLOW_GOREF), see README"]
fn replay_simplify_edge_routes() {
    testkit::assert_replay("SimplifyEdgeRoutes", |ctx, env| routing::SimplifyEdgeRoutes(ctx, &env.graph));
}

#[test]
#[ignore = "needs Go reference data (ORTHOFLOW_GOREF), see README"]
fn replay_swap_edge_ports() {
    testkit::assert_replay("SwapEdgePorts", |ctx, env| routing::SwapAllEdgePorts(ctx, &env.graph));
}

#[test]
#[ignore = "needs Go reference data (ORTHOFLOW_GOREF), see README"]
fn replay_straight_edges_fallback() {
    testkit::assert_replay("StraightEdgesFallback", |ctx, env| routing::StraightEdgesFallback(ctx, &env.graph));
}

#[test]
#[ignore = "needs Go reference data (ORTHOFLOW_GOREF), see README"]
fn replay_balance_edge_segments() {
    testkit::assert_replay("BalanceEdgeSegments", |ctx, env| routing::BalanceEdgeSegments(ctx, &env.graph));
}

#[test]
#[ignore = "needs Go reference data (ORTHOFLOW_GOREF), see README"]
fn replay_fix_cluster_edge_branching() {
    testkit::assert_replay("FixClusterEdgeBranching", |ctx, env| routing::FixClusterEdgeBranching(ctx, &env.graph));
}

#[test]
#[ignore = "needs Go reference data (ORTHOFLOW_GOREF), see README"]
fn replay_trace_edges_to_shape_border() {
    testkit::assert_replay("TraceEdgesToShapeBorder", |ctx, env| routing::TraceEdgesToShapeBorder(ctx, &env.graph));
}

#[test]
#[ignore = "needs Go reference data (ORTHOFLOW_GOREF), see README"]
fn replay_reorder_duplicates() {
    testkit::assert_replay("ReorderDuplicates", |ctx, env| routing::ReorderDuplicates(ctx, &env.graph));
}

#[test]
#[ignore = "needs Go reference data (ORTHOFLOW_GOREF), see README"]
fn replay_nudge_edge_channels() {
    testkit::assert_replay("NudgeEdgeChannels", |ctx, env| routing::NudgeEdgeChannels(ctx, &env.graph));
}

#[test]
#[ignore = "needs Go reference data (ORTHOFLOW_GOREF), see README"]
fn replay_shortcut_edge_routes() {
    testkit::assert_replay("ShortcutEdgeRoutes", |ctx, env| routing::ShortcutEdgeRoutes(ctx, &env.graph));
}

/// engine/pipeline.go runGraphRouting with a fresh pipeline (stages 21 and 24).
#[test]
#[ignore = "needs Go reference data (ORTHOFLOW_GOREF), see README"]
fn replay_edge_routing() {
    testkit::assert_replay("EdgeRouting", |ctx, env| {
        let options = routing::GraphRouteOptions {
            ForceReroute: env.forceReroute,
            RoutesPreviouslyCompleted: env.edgeRoutingComplete,
            ..Default::default()
        };
        let complete = routing::RouteGraph(ctx, &env.graph, options)?;
        env.edgeRoutingComplete = complete;
        Ok(())
    });
}
