// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Copyright 2026 Niklas Krieger

#![allow(non_snake_case)]
//! Unit parity against the Go oracle `bin/hierlabref` (cmd/hierlabref):
//! quality.Inspect, labeling.PlaceNewEdges, loops.Route/ComputeOffsets and
//! hierarchy.PlaceCompound on corpus stage snapshots.
extern crate orthoflow_quality as quality;
use gocompat::context;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::Command;

fn stageFiles(pred: impl Fn(&str) -> bool) -> Vec<PathBuf> {
    let root = testkit::goref();
    let filter = testkit::filter();
    let mut out = vec![];
    let mut fixtures: Vec<_> = std::fs::read_dir(root.join("corpus/stages")).unwrap().map(|e| e.unwrap().path()).collect();
    fixtures.sort();
    for f in fixtures {
        if let Some(flt) = &filter {
            if !f.display().to_string().contains(flt.as_str()) {
                continue;
            }
        }
        let mut stages: Vec<_> = std::fs::read_dir(&f).unwrap().map(|e| e.unwrap().path()).collect();
        stages.sort();
        for s in stages {
            if pred(s.file_name().unwrap().to_str().unwrap()) {
                out.push(s);
            }
        }
    }
    out
}

fn errString<T>(r: &gocompat::GoResult<T>) -> &'static str {
    if r.is_err() {
        "error"
    } else {
        ""
    }
}

fn run(mode: &str, path: &PathBuf) -> Value {
    let ctx = context::Background();
    let g = testkit::load_graph(&ctx, path);
    let mut out = serde_json::Map::new();
    match mode {
        "inspect" => {
            let r = quality::Inspect(&ctx, Some(&g));
            out.insert("error".into(), json!(errString(&r)));
            let m = r.unwrap_or_default();
            out.insert(
                "metrics".into(),
                json!({"NodeOverlaps": m.NodeOverlaps, "RouteObstructions": m.RouteObstructions,
                       "TextOcclusions": m.TextOcclusions, "Crossings": m.Crossings,
                       "Detour": m.Detour, "RouteLength": m.RouteLength}),
            );
        }
        "compound" => {

            let r = hierarchy::PlaceCompound(&ctx, &g, Some(&gocompat::rand::New(gocompat::rand::NewSource(0))));
            out.insert("error".into(), json!(errString(&r)));
            out.insert("changed".into(), json!(*r.as_ref().unwrap_or(&false)));

        }
        "newedges" => {
            let edges: Vec<_> = g.b().Edges.iter().enumerate().filter(|(i, _)| i % 2 == 1).map(|(_, e)| e.clone()).collect();
            let r = labeling::PlaceNewEdges(&ctx, &g, &edges);
            out.insert("error".into(), json!(errString(&r)));
        }
        "loops" => {
            let nodes = g.b().Nodes.clone();
            let mut routes = vec![];
            for n in &nodes {
                for e in loops::Route(n) {
                    let pts: Vec<Value> = e.b().Points.iter().map(|p| json!([p.b().X, p.b().Y])).collect();
                    routes.push(if pts.is_empty() { Value::Null } else { Value::Array(pts) });
                }
            }
            out.insert("routes".into(), if routes.is_empty() { Value::Null } else { Value::Array(routes) });
            loops::ComputeOffsets(&g);
            let mut offsets = vec![];
            for n in &nodes {
                let mut o = vec![];
                for k in [geo::TopLeft, geo::Top, geo::TopRight, geo::Right, geo::BottomRight, geo::Bottom, geo::BottomLeft, geo::Left] {
                    o.push(json!(n.b().LoopOffsets.get(&k).copied().unwrap_or(-1.0)));
                }
                offsets.push(Value::Array(o));
            }
            out.insert("offsets".into(), if offsets.is_empty() { Value::Null } else { Value::Array(offsets) });
        }
        _ => unreachable!(),
    }
    let gd = graphjson::Marshal(&ctx, Some(&g)).unwrap();
    out.insert("graph".into(), serde_json::from_slice(&gd).unwrap());
    Value::Object(out)
}

fn check(mode: &str, files: Vec<PathBuf>) {
    let root = testkit::goref();
    let oracle = root.join("bin/hierlabref");
    if !oracle.exists() {
        eprintln!("bin/hierlabref missing; skipping");
        return;
    }
    let (mut pass, mut fail, mut panics) = (0, vec![], 0);
    std::panic::set_hook(Box::new(|_| {}));
    for path in &files {
        let o = Command::new(&oracle).arg(mode).arg(path).output().unwrap();
        let label = path.strip_prefix(&root).unwrap_or(path).display().to_string();
        if !o.status.success() {
            // Go panicked/failed: the port must panic too.
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(mode, path)));
            if r.is_err() {
                pass += 1;
                panics += 1;
            } else {
                fail.push(format!("{label}: oracle failed but port succeeded: {}", String::from_utf8_lossy(&o.stderr)));
            }
            continue;
        }
        let want: Value = serde_json::from_slice(&o.stdout).unwrap();
        let got = run(mode, path);
        let mut diffs = vec![];
        testkit::json_diff(&want, &got, "$", &mut diffs);
        if diffs.is_empty() {
            pass += 1;
        } else {
            fail.push(format!("{label}:\n    {}", diffs.join("\n    ")));
        }
    }
    eprintln!("hierlabref {mode}: {pass} passed ({panics} matched Go panics), {} failed", fail.len());
    for f in fail.iter().take(10) {
        eprintln!("  FAIL {f}");
    }
    assert!(fail.is_empty());
}

#[test]
#[ignore = "needs Go reference data (ORTHOFLOW_GOREF), see README"]
fn inspect_parity() {
    check("inspect", stageFiles(|n| n.starts_with("33_") || n.starts_with("34_") || n.starts_with("35_") || n.starts_with("36_")));
}

#[test]
#[ignore = "needs Go reference data (ORTHOFLOW_GOREF), see README"]
fn newedges_parity() {
    check("newedges", stageFiles(|n| n.starts_with("32_") || n.starts_with("33_")));
}

#[test]
#[ignore = "needs Go reference data (ORTHOFLOW_GOREF), see README"]
fn loops_parity() {
    check("loops", stageFiles(|n| n.starts_with("02_") || n.starts_with("20_") || n.starts_with("36_")));
}

#[test]
#[ignore = "needs Go reference data (ORTHOFLOW_GOREF), see README"]
fn compound_parity() {
    check("compound", stageFiles(|n| ["03_", "06_", "07_", "18_", "19_", "20_", "21_", "32_"].iter().any(|p| n.starts_with(p))));
}
