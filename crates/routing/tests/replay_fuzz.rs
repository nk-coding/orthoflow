// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Copyright 2026 Niklas Krieger

//! Randomized differential parity for routing half b: replays the inputs
//! produced by the Go oracle `bin/routebref` (route perturbations of stage
//! snapshots) and compares with Go's stage outputs or errors.
//! Data: one directory per case written by `routebref <seed> <stage-snapshot.json> <dir>/<case>`
//! or `routebref route <seed> <stage-snapshot.json> <dir>/<case>` (goref, see README).
//! Run: ORTHOFLOW_FUZZ_DIR=<dir> cargo test --release -p orthoflow-routing --test replay_fuzz -- --ignored --nocapture
extern crate orthoflow_routing as routing;
use gocompat::context;
use std::path::PathBuf;

fn run(stage: &str, ctx: &gocompat::Ctx, g: &layoutgraph::GraphRef) -> gocompat::GoResult<()> {
    match stage {
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
        "EdgeRouting" => routing::RouteGraph(ctx, g, routing::GraphRouteOptions::default()).map(|_| ()),
        _ => unreachable!("{stage}"),
    }
}

#[test]
#[ignore = "needs Go reference data (ORTHOFLOW_FUZZ_DIR), see module docs"]
fn replay_fuzz_cases() {
    let Ok(dir) = std::env::var("ORTHOFLOW_FUZZ_DIR") else {
        eprintln!("ORTHOFLOW_FUZZ_DIR not set");
        return;
    };
    let dir = PathBuf::from(dir);
    let filter = testkit::filter();
    let mut cases: Vec<PathBuf> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).filter(|p| p.is_dir()).collect();
    cases.sort();
    let mut passed = std::collections::BTreeMap::<String, (usize, usize)>::new();
    let mut failed: Vec<String> = Vec::new();
    for case in cases {
        let mut inputs: Vec<String> = std::fs::read_dir(&case)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .filter(|n| n.ends_with(".in.json"))
            .collect();
        inputs.sort();
        for input in inputs {
            let stage = input.trim_end_matches(".in.json").to_string();
            let label = format!("{}/{stage}", case.file_name().unwrap().to_string_lossy());
            if let Some(f) = &filter {
                if !label.contains(f.as_str()) {
                    continue;
                }
            }
            let inBytes = std::fs::read(case.join(&input)).unwrap();
            let ctx0 = context::Background();
            let g = layoutgraph::NewGraph();
            graphjson::Unmarshal(&ctx0, &inBytes, Some(&g)).unwrap();
            let (ctx, _guard) = layoutgraph::EnsureTransactionWorkGuard(&ctx0, "AutolayoutTransactions").unwrap();
            let result = run(&stage, &ctx, &g);
            let errPath = case.join(format!("{stage}.err"));
            if errPath.exists() {
                let want = std::fs::read_to_string(&errPath).unwrap();
                match result {
                    Err(_) => passed.entry(stage.clone()).or_default().1 += 1,
                    Ok(()) => failed.push(format!("{label}: Go failed ({want}) but the port succeeded")),
                }
                continue;
            }
            if let Err(e) = result {
                failed.push(format!("{label}: port error {e}"));
                continue;
            }
            let actual: serde_json::Value = serde_json::from_slice(&graphjson::Marshal(&ctx, Some(&g)).unwrap()).unwrap();
            let expected: serde_json::Value =
                serde_json::from_slice(&std::fs::read(case.join(format!("{stage}.out.json"))).unwrap()).unwrap();
            let before: serde_json::Value = serde_json::from_slice(&inBytes).unwrap();
            let mut diffs = vec![];
            testkit::json_diff(&expected, &actual, "$", &mut diffs);
            if diffs.is_empty() {
                let entry = passed.entry(stage.clone()).or_default();
                entry.0 += 1;
                let mut changed = vec![];
                testkit::json_diff(&before, &expected, "$", &mut changed);
                if !changed.is_empty() {
                    *passed.entry(format!("{stage} (changed)")).or_default() = (passed.get(&format!("{stage} (changed)")).map_or(0, |v| v.0) + 1, 0);
                }
            } else {
                failed.push(format!("{label}:\n    {}", diffs.join("\n    ")));
            }
        }
    }
    for (stage, (ok, errs)) in &passed {
        eprintln!("fuzz replay {stage}: {ok} passed, {errs} matching errors");
    }
    for f in failed.iter().take(30) {
        eprintln!("  FAIL {f}");
    }
    eprintln!("{} failures", failed.len());
    assert!(failed.is_empty(), "{} fuzz replays differ", failed.len());
}
