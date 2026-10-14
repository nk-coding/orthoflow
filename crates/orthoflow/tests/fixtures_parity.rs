// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Copyright 2026 Niklas Krieger

//! End-to-end parity with Go: runs the full 3-seed layout (`orthoflow::layout`
//! with seeds 1, 2, 3) on every fixture in `tests/fixtures/{corpus,random}/inputs`
//! and compares the report (per-seed penalty and area, selected seed, laid-out
//! graph) exactly with the Go output in `.../expected`. Numbers must be
//! bit-identical and entity IDs (63-bit integers) must match exactly; only
//! integral floats may be printed differently (`354` vs `354.0`).
//!
//! `ORTHOFLOW_FILTER=<substring>` limits the fixtures, `ORTHOFLOW_JOBS=<n>`
//! sets the number of worker threads (default: available parallelism).
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

fn check_dir(set: &str) {
    let dir = testkit::fixtures_dir().join(set);
    let filter = testkit::filter();
    let mut inputs: Vec<PathBuf> = std::fs::read_dir(dir.join("inputs"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .filter(|p| filter.as_ref().is_none_or(|f| p.file_name().unwrap().to_string_lossy().contains(f.as_str())))
        .collect();
    inputs.sort();
    assert!(filter.is_some() || !inputs.is_empty(), "no fixtures in {}", dir.display());

    let jobs = std::env::var("ORTHOFLOW_JOBS")
        .ok()
        .and_then(|j| j.parse().ok())
        .unwrap_or_else(|| std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1))
        .max(1);
    let next = AtomicUsize::new(0);
    let failures = Mutex::new(Vec::<String>::new());
    std::thread::scope(|s| {
        for _ in 0..jobs {
            s.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                let Some(input) = inputs.get(i) else { break };
                if let Err(e) = check_one(input, &dir.join("expected").join(input.file_name().unwrap())) {
                    failures.lock().unwrap().push(format!("{}: {e}", input.file_name().unwrap().to_string_lossy()));
                }
            });
        }
    });
    let mut failures = failures.into_inner().unwrap();
    failures.sort();
    eprintln!("{set}: {} passed, {} failed", inputs.len() - failures.len(), failures.len());
    for f in failures.iter().take(20) {
        eprintln!("  FAIL {f}");
    }
    assert!(failures.is_empty(), "{} {set} layouts differ from Go", failures.len());
}

fn check_one(input: &Path, expected: &Path) -> Result<(), String> {
    let graph = std::fs::read_to_string(input).map_err(|e| format!("read input: {e}"))?;
    let want: Value = serde_json::from_slice(&std::fs::read(expected).map_err(|e| format!("read expected: {e}"))?)
        .map_err(|e| format!("parse expected: {e}"))?;
    let report = orthoflow::layout(&graph, &[1, 2, 3])?;
    let got: Value = serde_json::from_str(&report.to_json()).map_err(|e| format!("parse report: {e}"))?;
    let mut diffs = Vec::new();
    testkit::json_diff(&want, &got, "$", &mut diffs);
    if diffs.is_empty() {
        Ok(())
    } else {
        Err(diffs.join("; "))
    }
}

#[test]
fn corpus_layouts_match_go() {
    check_dir("corpus");
}

#[test]
fn random_layouts_match_go() {
    check_dir("random");
}
