// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Copyright 2026 Niklas Krieger
//
// Stage invocations mirror d2layouts/d2talalayout/internal/engine/pipeline.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).

//! Stage-replay parity for the refinement half of placement (half b).
//! Each wrapper mirrors engine/pipeline.go exactly.

extern crate orthoflow_placement as placement;
#[test]
#[ignore = "needs Go reference data (ORTHOFLOW_GOREF), see README"]
fn replay_prescale() {
    testkit::assert_replay("Prescale", |_ctx, env| {
        placement::Prescale(&env.graph);
        Ok(())
    });
}

#[test]
#[ignore = "needs Go reference data (ORTHOFLOW_GOREF), see README"]
fn replay_preprocess() {
    testkit::assert_replay("Preprocess", |_ctx, env| {
        placement::Prepare(&env.graph);
        Ok(())
    });
}

#[test]
#[ignore = "needs Go reference data (ORTHOFLOW_GOREF), see README"]
fn replay_swap_stuff() {
    testkit::assert_replay("SwapStuff", |ctx, env| placement::Swap(ctx, &env.graph));
}

#[test]
#[ignore = "needs Go reference data (ORTHOFLOW_GOREF), see README"]
fn replay_transpose() {
    testkit::assert_replay("Transpose", |ctx, env| placement::TransposeAll(ctx, &env.graph));
}

#[test]
#[ignore = "needs Go reference data (ORTHOFLOW_GOREF), see README"]
fn replay_align_axes() {
    testkit::assert_replay("AlignAxes", |ctx, env| {
        if !env.alignAxesNeeded {
            return Ok(());
        }
        env.alignAxesNeeded = false;
        placement::Align(ctx, &env.graph)
    });
}

#[test]
#[ignore = "needs Go reference data (ORTHOFLOW_GOREF), see README"]
fn replay_gap_normalization() {
    testkit::assert_replay("GapNormalization", |ctx, env| {
        let changed = placement::NormalizeGaps(ctx, &env.graph);
        match changed {
            Ok(c) => {
                env.alignAxesNeeded = c;
                Ok(())
            }
            Err(e) => Err(e),
        }
    });
}

#[test]
#[ignore = "needs Go reference data (ORTHOFLOW_GOREF), see README"]
fn replay_optimize_clusters() {
    testkit::assert_replay("OptimizeClusters", |ctx, env| {
        env.graph.ComputeCellSize();
        env.alignAxesNeeded = placement::OptimizeClusters(ctx, &env.graph)?;
        Ok(())
    });
}

#[test]
#[ignore = "needs Go reference data (ORTHOFLOW_GOREF), see README"]
fn replay_balance_symmetry() {
    testkit::assert_replay("BalanceSymmetry", |ctx, env| placement::BalanceSymmetry(ctx, &env.graph));
}

#[test]
#[ignore = "needs Go reference data (ORTHOFLOW_GOREF), see README"]
fn replay_equidistance() {
    testkit::assert_replay("Equidistance", |ctx, env| {
        env.graph.ComputeCellSize();
        env.alignAxesNeeded = placement::Equidistance(ctx, &env.graph)?;
        Ok(())
    });
}

#[test]
#[ignore = "needs Go reference data (ORTHOFLOW_GOREF), see README"]
fn replay_rescale() {
    testkit::assert_replay("Rescale", |_ctx, env| {
        env.graph.ComputeCellSize();
        placement::Pad(&env.graph);
        Ok(())
    });
}

#[test]
#[ignore = "needs Go reference data (ORTHOFLOW_GOREF), see README"]
fn replay_dejitter() {
    testkit::assert_replay("Dejitter", |ctx, env| {
        env.forceReroute = placement::Dejitter(ctx, &env.graph)?;
        Ok(())
    });
}

#[test]
#[ignore = "needs Go reference data (ORTHOFLOW_GOREF), see README"]
fn replay_normalize() {
    testkit::assert_replay("Normalize", |_ctx, env| {
        placement::Normalize(&env.graph);
        Ok(())
    });
}
