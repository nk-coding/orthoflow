// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Copyright 2026 Niklas Krieger
//
// Stage invocations mirror d2layouts/d2talalayout/internal/engine/pipeline.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).

//! Stage-replay parity for 07 NodePlacement (placement.Place).
extern crate orthoflow_placement as placement;
#[test]
#[ignore = "needs Go reference data (ORTHOFLOW_GOREF), see README"]
fn replay_node_placement() {
    testkit::assert_replay("NodePlacement", |ctx, env| placement::Place(ctx, &env.graph, env.seed));
}

/// Extra seeds: `ORTHOFLOW_SEED_DIR=<dir>/<seed>` holding `<fixture>/06_PreprocessHubs.json`
/// and Go's `talaref replay -seed S -stage 7` output as `<fixture>/replay07.json`.
#[test]
#[ignore]
fn replay_node_placement_seed_dir() {
    let dir = std::path::PathBuf::from(std::env::var("ORTHOFLOW_SEED_DIR").expect("ORTHOFLOW_SEED_DIR"));
    let seed: i64 = dir.file_name().unwrap().to_str().unwrap().parse().unwrap();
    let mut fixtures: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).collect();
    fixtures.sort();
    let (mut passed, mut failed) = (0, vec![]);
    for f in fixtures {
        let ctx0 = gocompat::context::Background();
        let g = testkit::load_graph(&ctx0, &f.join("06_PreprocessHubs.json"));
        let (ctx, _guard) = layoutgraph::EnsureTransactionWorkGuard(&ctx0, "AutolayoutTransactions").unwrap();
        let expected: serde_json::Value = match std::fs::read(f.join("replay07.json")) {
            Ok(b) if !b.is_empty() => serde_json::from_slice(&b).unwrap(),
            _ => continue,
        };
        if let Err(e) = placement::Place(&ctx, &g, seed) {
            failed.push(format!("{}: error {e}", f.display()));
            continue;
        }
        let actual: serde_json::Value = serde_json::from_slice(&graphjson::Marshal(&ctx, Some(&g)).unwrap()).unwrap();
        let mut diffs = vec![];
        testkit::json_diff(&expected, &actual, "$", &mut diffs);
        if diffs.is_empty() {
            passed += 1;
        } else {
            failed.push(format!("{}: {}", f.display(), diffs.join("; ")));
        }
    }
    eprintln!("seed {seed}: {passed} passed, {} failed", failed.len());
    for f in &failed {
        eprintln!("  FAIL {f}");
    }
    assert!(failed.is_empty());
}
