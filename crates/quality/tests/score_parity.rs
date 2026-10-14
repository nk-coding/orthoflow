// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Copyright 2026 Niklas Krieger

#![allow(non_snake_case)]
//! quality.EvaluateWithArea parity against the Go oracle `bin/talaref score`.
//! Graphs: corpus/expected/*.json (`.graph`) and corpus/stages/*/3[0-6]_*.json.
extern crate orthoflow_quality as quality;
use gocompat::context;
use serde_json::Value;
use std::path::PathBuf;
use std::process::Command;

fn oracle(talaref: &PathBuf, path: &PathBuf) -> Result<(f64, f64), String> {
    let out = Command::new(talaref).arg("score").arg(path).output().unwrap();
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    Ok((v["penalty"].as_f64().unwrap(), v["area"].as_f64().unwrap()))
}

#[test]
#[ignore = "needs Go reference data (ORTHOFLOW_GOREF), see README"]
fn evaluate_with_area_parity() {
    let root = testkit::goref();
    let talaref = root.join("bin/talaref");
    if !talaref.exists() {
        eprintln!("bin/talaref missing; skipping");
        return;
    }
    let filter = testkit::filter();
    std::panic::set_hook(Box::new(|_| {}));
    let tmp = std::env::temp_dir().join(format!("tala-quality-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();

    let mut files: Vec<PathBuf> = vec![];
    let mut expected: Vec<_> = std::fs::read_dir(root.join("corpus/expected")).unwrap().map(|e| e.unwrap().path())
        .filter(|p| p.extension().map_or(false, |x| x == "json")).collect();
    expected.sort();
    for p in expected {
        let v: Value = serde_json::from_slice(&std::fs::read(&p).unwrap()).unwrap();
        let out = tmp.join(format!("expected__{}", p.file_name().unwrap().to_str().unwrap()));
        std::fs::write(&out, serde_json::to_vec(&v["graph"]).unwrap()).unwrap();
        files.push(out);
    }
    let mut fixtures: Vec<_> = std::fs::read_dir(root.join("corpus/stages")).unwrap().map(|e| e.unwrap().path()).collect();
    fixtures.sort();
    for f in fixtures {
        let mut stages: Vec<_> = std::fs::read_dir(&f).unwrap().map(|e| e.unwrap().path()).collect();
        stages.sort();
        for s in stages {
            let name = s.file_name().unwrap().to_str().unwrap().to_string();
            if name.starts_with('3') && name.as_bytes()[1] >= b'0' && name.as_bytes()[1] <= b'6' {
                files.push(s);
            }
        }
    }

    let (mut pass, mut fail, mut errors) = (0, vec![], 0);
    for path in &files {
        let label = path.strip_prefix(&root).unwrap_or(path).display().to_string();
        if let Some(f) = &filter {
            if !path.display().to_string().contains(f.as_str()) {
                continue;
            }
        }
        let ctx = context::Background();
        let g = testkit::load_graph(&ctx, path);
        // Go panics (nil TopLeft) on unplaced edge labels before stage 33; the
        // oracle then fails, and so must the port.
        let got = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            quality::EvaluateWithArea(&ctx, Some(&g)).map_err(|e| e.to_string())
        }))
        .unwrap_or_else(|_| Err("panic".to_string()));
        let want = oracle(&talaref, path);
        if want.is_err() {
            errors += 1;
        }
        let same = match (&got, &want) {
            (Ok((p1, a1)), Ok((p2, a2))) => p1.to_bits() == p2.to_bits() && a1.to_bits() == a2.to_bits(),
            (Err(_), Err(_)) => true,
            _ => false,
        };
        if same {
            pass += 1;
        } else {
            fail.push(format!("{label}: got {got:?} want {want:?}"));
        }
    }
    std::fs::remove_dir_all(&tmp).ok();
    eprintln!("EvaluateWithArea parity: {pass} passed ({errors} of them Go errors/panics, matched), {} failed", fail.len());
    for f in fail.iter().take(20) {
        eprintln!("  FAIL {f}");
    }
    assert!(fail.is_empty());
}
