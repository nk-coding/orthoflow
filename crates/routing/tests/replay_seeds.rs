// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Copyright 2026 Niklas Krieger

//! Extra parity data for routing half b: replays the routing stages on Go
//! stage snapshots of other seeds (`talaref stages -seed S`), comparing each
//! stage's output with Go's next snapshot. Post-processing stages are pure
//! functions of the graph; EdgeRouting is compared when Go rerouted.
//! Data: `<dir>/seed<S>/<fixture>/NN_Stage.json`, written by
//! `talaref stages -seed S <input> <dir>/seed<S>/<fixture>` (goref, see README).
//! Run: ORTHOFLOW_SEEDS_DIR=<dir> cargo test --release -p orthoflow-routing --test replay_seeds -- --ignored --nocapture
extern crate orthoflow_routing as routing;
use gocompat::context;
use std::path::PathBuf;

fn run(stage: &str, ctx: &gocompat::Ctx, g: &layoutgraph::GraphRef) -> gocompat::GoResult<()> {
    match stage {
        "EdgeRouting" => routing::RouteGraph(ctx, g, routing::GraphRouteOptions::default()).map(|_| ()),
        "Crosshatch" => routing::Crosshatch(ctx, g),
        "SimplifyEdgeRoutes" => routing::SimplifyEdgeRoutes(ctx, g),
        "SwapEdgePorts" => routing::SwapAllEdgePorts(ctx, g),
        "StraightEdgesFallback" => routing::StraightEdgesFallback(ctx, g),
        "BalanceEdgeSegments" => routing::BalanceEdgeSegments(ctx, g),
        "FixClusterEdgeBranching" => routing::FixClusterEdgeBranching(ctx, g),
        "TraceEdgesToShapeBorder" => routing::TraceEdgesToShapeBorder(ctx, g),
        "ReorderDuplicates" => routing::ReorderDuplicates(ctx, g),
        "NudgeEdgeChannels" => routing::NudgeEdgeChannels(ctx, g),
        "ShortcutEdgeRoutes" => routing::ShortcutEdgeRoutes(ctx, g),
        _ => unreachable!(),
    }
}

#[test]
#[ignore = "needs Go reference data (ORTHOFLOW_SEEDS_DIR), see module docs"]
fn replay_other_seeds() {
    let Ok(dir) = std::env::var("ORTHOFLOW_SEEDS_DIR") else {
        eprintln!("ORTHOFLOW_SEEDS_DIR not set");
        return;
    };
    let dir = PathBuf::from(dir);
    let filter = testkit::filter();
    let stagesOfInterest = [
        "EdgeRouting",
        "Crosshatch",
        "SimplifyEdgeRoutes",
        "SwapEdgePorts",
        "StraightEdgesFallback",
        "BalanceEdgeSegments",
        "FixClusterEdgeBranching",
        "TraceEdgesToShapeBorder",
        "ReorderDuplicates",
        "NudgeEdgeChannels",
        "ShortcutEdgeRoutes",
    ];
    let mut passed = std::collections::BTreeMap::<String, usize>::new();
    let mut failed: Vec<String> = Vec::new();
    let mut seedDirs: Vec<PathBuf> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).filter(|p| p.is_dir()).collect();
    seedDirs.sort();
    for seedDir in seedDirs {
        let mut fixtures: Vec<PathBuf> = std::fs::read_dir(&seedDir).unwrap().map(|e| e.unwrap().path()).filter(|p| p.is_dir()).collect();
        fixtures.sort();
        for fixture in fixtures {
            let name = format!("{}/{}", seedDir.file_name().unwrap().to_string_lossy(), fixture.file_name().unwrap().to_string_lossy());
            if let Some(f) = &filter {
                if !name.contains(f.as_str()) {
                    continue;
                }
            }
            let mut stages: Vec<String> =
                std::fs::read_dir(&fixture).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
            stages.sort();
            for i in 1..stages.len() {
                let stageName = stages[i].trim_end_matches(".json").splitn(2, '_').nth(1).unwrap().to_string();
                if !stagesOfInterest.contains(&stageName.as_str()) {
                    continue;
                }
                let prevBytes = std::fs::read(fixture.join(&stages[i - 1])).unwrap();
                let wantBytes = std::fs::read(fixture.join(&stages[i])).unwrap();
                if stageName == "EdgeRouting" && prevBytes == wantBytes {
                    // Previously completed routes kept by the pipeline.
                    continue;
                }
                let ctx0 = context::Background();
                let g = layoutgraph::NewGraph();
                graphjson::Unmarshal(&ctx0, &prevBytes, Some(&g)).unwrap();
                let (ctx, _guard) = layoutgraph::EnsureTransactionWorkGuard(&ctx0, "AutolayoutTransactions").unwrap();
                let label = format!("{name}/{}", stages[i]);
                if let Err(e) = run(&stageName, &ctx, &g) {
                    failed.push(format!("{label}: error {e}"));
                    continue;
                }
                let actual: serde_json::Value = serde_json::from_slice(&graphjson::Marshal(&ctx, Some(&g)).unwrap()).unwrap();
                let expected: serde_json::Value = serde_json::from_slice(&wantBytes).unwrap();
                let mut diffs = vec![];
                testkit::json_diff(&expected, &actual, "$", &mut diffs);
                if diffs.is_empty() {
                    *passed.entry(stageName).or_default() += 1;
                } else {
                    failed.push(format!("{label}:\n    {}", diffs.join("\n    ")));
                }
            }
        }
    }
    for (stage, n) in &passed {
        eprintln!("seeds replay {stage}: {n} passed");
    }
    for f in failed.iter().take(30) {
        eprintln!("  FAIL {f}");
    }
    assert!(failed.is_empty(), "{} replays differ", failed.len());
}
