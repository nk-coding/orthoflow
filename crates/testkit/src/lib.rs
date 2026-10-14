// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Copyright 2026 Niklas Krieger

//! Test helpers for orthoflow's parity tests.
//!
//! * [`json_diff`]: exact, numeric-aware JSON comparison.
//! * [`fixtures_dir`]: the Go-generated fixtures shipped in `tests/fixtures`.
//! * [`goref`] / [`replay_stage`]: stage-replay parity against Go reference
//!   data generated with `scripts/gen-goref.sh` (not shipped; the directory is
//!   passed in the `ORTHOFLOW_GOREF` environment variable).
//!
//! `$ORTHOFLOW_GOREF/corpus/replay/<fixture>/NN_Stage.json` is Go's result of
//! running only stage NN on Go's snapshot of stage NN-1
//! (`corpus/stages/<fixture>/`, or the input for stage 0). `replay_stage` does
//! the same with a port's stage function and compares exactly. Typical use in
//! `crates/<pkg>/tests/replay.rs`:
//!
//! ```text
//! #[test]
//! #[ignore = "needs Go reference data (ORTHOFLOW_GOREF)"]
//! fn replay_binpack() {
//!     testkit::assert_replay("BinPack", |ctx, env| packing::Pack(ctx, &env.graph, None));
//! }
//! ```
//!
//! Env: `ORTHOFLOW_FILTER=substring` limits fixtures (substring match),
//! `ORTHOFLOW_DUMP=dir` writes mismatching outputs as
//! `dir/<fixture>__<stage>.json` for inspection.
#![allow(non_snake_case)]

use gocompat::context::{self, Ctx};
use gocompat::rand::{self, RandRef};
use gocompat::GoResult;
use layoutgraph::GraphRef;
use serde_json::Value;
use std::path::{Path, PathBuf};

/// What engine/pipeline.go's `pipeline` struct gives a stage. Fresh per replay,
/// exactly like Go's DebugReplayStage (newPipeline(graph, seed, false)).
pub struct ReplayEnv {
    pub graph: GraphRef,
    pub seed: i64,
    /// pipeline.random
    pub random: RandRef,
    /// pipeline.hierarchyPlacementRandom
    pub hierarchyPlacementRandom: RandRef,
    /// pipeline.alignAxesNeeded initial value
    pub alignAxesNeeded: bool,
    pub forceReroute: bool,
    pub edgeRoutingComplete: bool,
}

/// Message used by tests that need the Go reference data.
pub const GOREF_HELP: &str = "needs Go reference data: set ORTHOFLOW_GOREF to the output of scripts/gen-goref.sh";

/// Workspace root.
pub fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap()
}

/// `tests/fixtures` (Go-generated inputs and expected outputs shipped with the repository).
pub fn fixtures_dir() -> PathBuf {
    workspace_root().join("tests/fixtures")
}

/// Go reference data directory (`ORTHOFLOW_GOREF`, produced by
/// `scripts/gen-goref.sh`): `corpus/{inputs,stages,replay,expected}`, `bin/`
/// (Go oracle binaries), `base-parity/`, `graph-parity/`, `ovg-parity/`.
/// Relative paths are resolved against the workspace root. Panics when unset.
pub fn goref() -> PathBuf {
    match std::env::var_os("ORTHOFLOW_GOREF") {
        Some(d) => {
            let p = PathBuf::from(d);
            let p = if p.is_absolute() { p } else { workspace_root().join(p) };
            p.canonicalize().unwrap_or_else(|e| panic!("ORTHOFLOW_GOREF {}: {e}", p.display()))
        }
        None => panic!("{GOREF_HELP}"),
    }
}

/// `ORTHOFLOW_FILTER` (substring filter on fixture names), if set.
pub fn filter() -> Option<String> {
    std::env::var("ORTHOFLOW_FILTER").ok()
}

pub fn load_graph(ctx: &Ctx, path: &Path) -> GraphRef {
    let data = std::fs::read(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let g = layoutgraph::NewGraph();
    graphjson::Unmarshal(ctx, &data, Some(&g)).unwrap_or_else(|e| panic!("unmarshal {}: {e}", path.display()));
    g
}

/// Numeric-aware exact JSON equality (Go prints 100 where serde prints 100.0;
/// integers above 2^53 are compared exactly). Returns the first few differing paths.
pub fn json_diff(expected: &Value, actual: &Value, path: &str, out: &mut Vec<String>) {
    if out.len() >= 5 {
        return;
    }
    match (expected, actual) {
        (Value::Number(a), Value::Number(b)) => {
            let same = match (a.as_i64(), b.as_i64(), a.as_u64(), b.as_u64()) {
                (Some(x), Some(y), _, _) => x == y,
                (_, _, Some(x), Some(y)) => x == y,
                _ => a.as_f64().map(f64::to_bits) == b.as_f64().map(f64::to_bits)
                    || a.as_f64() == b.as_f64(),
            };
            if !same {
                out.push(format!("{path}: expected {a} got {b}"));
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            if a.len() != b.len() {
                out.push(format!("{path}: length expected {} got {}", a.len(), b.len()));
            }
            for (i, (x, y)) in a.iter().zip(b).enumerate() {
                json_diff(x, y, &format!("{path}[{i}]"), out);
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            for (k, x) in a {
                match b.get(k) {
                    Some(y) => json_diff(x, y, &format!("{path}.{k}"), out),
                    None => out.push(format!("{path}.{k}: missing key")),
                }
            }
            for k in b.keys() {
                if !a.contains_key(k) {
                    out.push(format!("{path}.{k}: unexpected key"));
                }
            }
        }
        (a, b) if a == b => {}
        (a, b) => out.push(format!("{path}: expected {a} got {b}")),
    }
}

pub struct ReplayReport {
    pub passed: usize,
    pub failed: Vec<String>,
}

/// Replays every stage whose file name contains `_<stage_name>.json` on every
/// fixture. `run` mirrors the stage wrapper in engine/pipeline.go.
pub fn replay_stage(stage_name: &str, run: impl Fn(&Ctx, &mut ReplayEnv) -> GoResult<()>) -> ReplayReport {
    let root = goref();
    let filter = filter();
    let dump = std::env::var("ORTHOFLOW_DUMP").ok().map(PathBuf::from);
    let mut fixtures: Vec<_> = std::fs::read_dir(root.join("corpus/stages")).unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
    fixtures.sort();
    let suffix = format!("_{stage_name}.json");
    let mut report = ReplayReport { passed: 0, failed: vec![] };
    for fixture in fixtures {
        if let Some(f) = &filter {
            if !fixture.contains(f.as_str()) {
                continue;
            }
        }
        let mut stages: Vec<_> = std::fs::read_dir(root.join("corpus/stages").join(&fixture)).unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
        stages.sort();
        for (i, stage) in stages.iter().enumerate() {
            if !stage.ends_with(&suffix) {
                continue;
            }
            let prev = if i == 0 {
                root.join("corpus/inputs").join(format!("{fixture}.json"))
            } else {
                root.join("corpus/stages").join(&fixture).join(&stages[i - 1])
            };
            let ctx0 = context::Background();
            let graph = load_graph(&ctx0, &prev);
            let (ctx, _guard) = layoutgraph::EnsureTransactionWorkGuard(&ctx0, "AutolayoutTransactions").unwrap();
            let seed = 1i64;
            let mut env = ReplayEnv {
                graph: graph.clone(),
                seed,
                random: rand::New(rand::NewSource(seed)),
                hierarchyPlacementRandom: rand::New(rand::NewSource(seed)),
                alignAxesNeeded: true,
                forceReroute: false,
                edgeRoutingComplete: false,
            };
            let label = format!("{fixture}/{stage}");
            if let Err(e) = run(&ctx, &mut env) {
                report.failed.push(format!("{label}: stage returned error: {e}"));
                continue;
            }
            let actual_bytes = graphjson::Marshal(&ctx, Some(&graph)).unwrap();
            let actual: Value = serde_json::from_slice(&actual_bytes).unwrap();
            let expected: Value = serde_json::from_slice(
                &std::fs::read(root.join("corpus/replay").join(&fixture).join(stage)).unwrap()).unwrap();
            let mut diffs = vec![];
            json_diff(&expected, &actual, "$", &mut diffs);
            if diffs.is_empty() {
                report.passed += 1;
            } else {
                if let Some(d) = &dump {
                    std::fs::create_dir_all(d).ok();
                    std::fs::write(d.join(format!("{fixture}__{stage}")), &actual_bytes).ok();
                }
                report.failed.push(format!("{label}:\n    {}", diffs.join("\n    ")));
            }
        }
    }
    report
}

/// `replay_stage` + panic with a summary if any fixture differs.
pub fn assert_replay(stage_name: &str, run: impl Fn(&Ctx, &mut ReplayEnv) -> GoResult<()>) {
    let r = replay_stage(stage_name, run);
    eprintln!("replay {stage_name}: {} passed, {} failed", r.passed, r.failed.len());
    for f in r.failed.iter().take(20) {
        eprintln!("  FAIL {f}");
    }
    assert!(r.failed.is_empty(), "{} {stage_name} replays differ from Go", r.failed.len());
}
