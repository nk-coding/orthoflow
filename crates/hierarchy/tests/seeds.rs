// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Copyright 2026 Niklas Krieger

#![allow(non_snake_case)]
//! Stage 4 (PreprocessHierarchies) on Go's stage snapshots with several
//! hierarchy-placement seeds, compared against `bin/talaref replay`.
extern crate orthoflow_hierarchy as hierarchy;
use gocompat::context;
use serde_json::Value;
use std::process::Command;

#[test]
#[ignore = "needs Go reference data (ORTHOFLOW_GOREF), see README"]
fn preprocess_hierarchies_seeds() {
    let root = testkit::goref();
    let talaref = root.join("bin/talaref");
    if !talaref.exists() {
        eprintln!("bin/talaref missing; skipping");
        return;
    }
    let filter = testkit::filter();
    let mut fixtures: Vec<_> = std::fs::read_dir(root.join("corpus/stages")).unwrap().map(|e| e.unwrap().path()).collect();
    fixtures.sort();
    let (mut pass, mut fail, mut goErr) = (0, vec![], 0);
    for f in fixtures {
        if let Some(flt) = &filter {
            if !f.display().to_string().contains(flt.as_str()) {
                continue;
            }
        }
        let snap03 = f.join("03_PreprocessTrees.json");
        let snap04 = f.join("04_PreprocessHierarchies.json");
        // Only fixtures where stage 4 does something are interesting.
        if std::fs::read(&snap03).unwrap() == std::fs::read(&snap04).unwrap() {
            continue;
        }
        for seed in 2..=9i64 {
            let o = Command::new(&talaref).args(["replay", "-seed", &seed.to_string(), "-stage", "4"]).arg(&snap03).output().unwrap();
            let label = format!("{}/seed{}", f.file_name().unwrap().to_str().unwrap(), seed);
            let ctx0 = context::Background();
            let g = testkit::load_graph(&ctx0, &snap03);
            let (ctx, _guard) = layoutgraph::EnsureTransactionWorkGuard(&ctx0, "AutolayoutTransactions").unwrap();
            let r = gocompat::rand::New(gocompat::rand::NewSource(seed));
            let res = (|| -> gocompat::GoResult<()> {
                hierarchy::Assign(&ctx, &g, None, &hierarchy::Candidates(&g))?;
                hierarchy::Place(&ctx, &g, None, Some(&r))?;
                hierarchy::RemoveIsolatedMemberships(&g);
                Ok(())
            })();
            if !o.status.success() {
                goErr += 1;
                if res.is_err() {
                    pass += 1;
                } else {
                    fail.push(format!("{label}: Go failed, port succeeded: {}", String::from_utf8_lossy(&o.stderr)));
                }
                continue;
            }
            if let Err(e) = res {
                fail.push(format!("{label}: port failed: {e}"));
                continue;
            }
            let want: Value = serde_json::from_slice(&o.stdout).unwrap();
            let got: Value = serde_json::from_slice(&graphjson::Marshal(&ctx, Some(&g)).unwrap()).unwrap();
            let mut diffs = vec![];
            testkit::json_diff(&want, &got, "$", &mut diffs);
            if diffs.is_empty() {
                pass += 1;
            } else {
                fail.push(format!("{label}:\n    {}", diffs.join("\n    ")));
            }
        }
    }
    eprintln!("PreprocessHierarchies seeds 2..9: {pass} passed ({goErr} Go errors), {} failed", fail.len());
    for f in fail.iter().take(10) {
        eprintln!("  FAIL {f}");
    }
    assert!(fail.is_empty());
}
