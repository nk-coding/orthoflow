// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Copyright 2026 Niklas Krieger
//
// Stage invocations mirror d2layouts/d2talalayout/internal/engine/pipeline.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).

extern crate orthoflow_grouping as grouping;
#[test]
#[ignore = "needs Go reference data (ORTHOFLOW_GOREF), see README"]
fn replay_preprocess_sequences() {
    testkit::assert_replay("PreprocessSequences", |ctx, env| grouping::AddSequences(ctx, &env.graph, &env.random));
}

#[test]
#[ignore = "needs Go reference data (ORTHOFLOW_GOREF), see README"]
fn replay_preprocess_clusters() {
    testkit::assert_replay("PreprocessClusters", |ctx, env| grouping::AddClusters(ctx, &env.graph, env.seed, &env.random));
}

#[test]
#[ignore = "needs Go reference data (ORTHOFLOW_GOREF), see README"]
fn replay_cleanup_stuff() {
    testkit::assert_replay("CleanupStuff", |_ctx, env| {
        grouping::Cleanup(&env.graph);
        Ok(())
    });
}
