// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Copyright 2026 Niklas Krieger
//
// Parity test against goref/.../internal/routing/zz_ovgport_parity_test.go.

//! Go-parity for OVG construction and single-edge route search (routing half a).
//! Go side: goref/d2/d2layouts/d2talalayout/internal/routing/zz_ovgport_parity_test.go
//! (env-guarded); `scripts/gen-goref.sh` writes one dump per fixture from
//! `corpus/stages/<f>/20_Rescale.json` into `$ORTHOFLOW_GOREF/ovg-parity/<f>.json`:
//!   ORTHOFLOW_OVG_DUMP_IN=... ORTHOFLOW_OVG_DUMP_OUT=<dir>/<f>.json go test -run TestOVGPortParityDump ./d2layouts/d2talalayout/internal/routing/
//! Run: ORTHOFLOW_GOREF=<dir> cargo test -p orthoflow-routing ovg_parity -- --ignored --nocapture
//! Optional: ORTHOFLOW_OVG_PARITY_DIR=<dir> (default `$ORTHOFLOW_GOREF/ovg-parity`),
//! ORTHOFLOW_OVG_PARITY_ONLY=<substring>.
use super::*;
use crate::*;
use gocompat::context;
use layoutgraph::{GraphRef, NodeRef};
use serde_json::{json, Value};

fn pid(n: Option<&NodeRef>) -> String {
    match n {
        None => String::new(),
        Some(n) => format!("{}", n.b().ID),
    }
}

fn num(x: f64) -> Value {
    json!(x)
}

fn dumpOVG(dump: &mut serde_json::Map<String, Value>, ovgRef: &OVGRef) {
    let ovg = ovgRef.b();
    struct PN {
        x: f64,
        y: f64,
        center: bool,
        v: Value,
        nadj: usize,
    }
    let mut nodes: Vec<PN> = Vec::new();
    for n in &ovg.Nodes {
        let nb = n.b();
        let mut near: Vec<String> = nb.IsNearPort.as_ref().map(|s| s.iter().map(|o| pid(Some(o))).collect()).unwrap_or_default();
        near.sort();
        let mut owners: Vec<(String, String, String)> =
            nb.portOwners().iter().map(|(o, md)| (pid(Some(o)), format!("{}", md.directions.0), format!("{}", md.isCenterPort))).collect();
        gocompat::sort::SortSlice(&mut owners, |a, b| a.0 < b.0);
        let adj: Vec<Value> = nb
            .Edges
            .iter()
            .map(|e| {
                let a = n.adjacent(e);
                let p = a.P();
                json!([num(p.X), num(p.Y), num(e.Distance)])
            })
            .collect();
        let nadj = adj.len();
        let v = json!({
            "x": num(nb.Point.X), "y": num(nb.Point.Y), "center": nb.IsNodeCenter, "tunnel": nb.IsTunnel,
            "container": pid(nb.Container.as_ref()),
            "near": if near.is_empty() { Value::Null } else { json!(near) },
            "owners": if owners.is_empty() { Value::Null } else { json!(owners.iter().map(|o| json!([o.0, o.1, o.2])).collect::<Vec<_>>()) },
            "adj": if adj.is_empty() { Value::Null } else { Value::Array(adj) },
        });
        nodes.push(PN { x: nb.Point.X, y: nb.Point.Y, center: nb.IsNodeCenter, v, nadj });
    }
    gocompat::sort::SortSliceStable(&mut nodes, |a, b| {
        if a.x != b.x {
            return a.x < b.x;
        }
        if a.y != b.y {
            return a.y < b.y;
        }
        if a.center != b.center {
            return !a.center;
        }
        a.nadj < b.nadj
    });
    dump.insert("nodes".into(), if nodes.is_empty() { Value::Null } else { Value::Array(nodes.into_iter().map(|n| n.v).collect()) });
    let mut ports: Vec<(String, Value)> = ovg
        .Ports
        .iter()
        .map(|(n, ps)| {
            let v = if ps.is_empty() { Value::Null } else { json!(ps.iter().map(|p| json!([num(p.X()), num(p.Y())])).collect::<Vec<_>>()) };
            (pid(Some(n)), v)
        })
        .collect();
    gocompat::sort::SortSlice(&mut ports, |a, b| a.0 < b.0);
    dump.insert("ports".into(), if ports.is_empty() { Value::Null } else { json!(ports.into_iter().map(|(k, v)| json!([k, v])).collect::<Vec<_>>()) });
    let mut centers: Vec<(String, f64, f64)> = ovg.Centers.iter().map(|(n, c)| (pid(Some(n)), c.X(), c.Y())).collect();
    gocompat::sort::SortSlice(&mut centers, |a, b| a.0 < b.0);
    dump.insert("centers".into(), if centers.is_empty() { Value::Null } else { json!(centers.into_iter().map(|(k, x, y)| json!([k, num(x), num(y)])).collect::<Vec<_>>()) });
    dump.insert("edgeCount".into(), json!(ovg.Edges.len()));
    let guard = ovg.buildGuard.as_ref().unwrap().b();
    dump.insert("guardWork".into(), json!(guard.work));
    dump.insert("guardNodes".into(), json!(guard.nodes));
    dump.insert("guardEdges".into(), json!(guard.edges));
    dump.insert("guardCandidates".into(), json!(guard.candidates));
}

fn dumpRoutes(ovg: &OVGRef, g: &GraphRef) -> Value {
    let ctx = context::Background();
    let mut out: Vec<Value> = Vec::new();
    let edges = g.b().Edges.clone();
    let mut router = match newOVGEdgeRouterWithWorkLimit(&ctx, Default, ovg, g, &[], &edges, maxRouteSearchWorkUnits) {
        Err(err) => return json!([{"edge": "", "err": err.Error(), "distance": 0, "points": null, "work": 0}]),
        Ok(r) => r,
    };
    for e in &edges {
        if e.b().From == e.b().To {
            continue;
        }
        let r = router.search(&ctx, e);
        let work = router.w().used();
        match r {
            Err(err) => {
                out.push(json!({"edge": e.DebugID(), "err": err.Error(), "distance": 0, "points": null, "work": work}));
                continue;
            }
            Ok((nodes, dist)) => {
                let pts: Vec<Value> = nodes.iter().map(|n| json!([num(n.X()), num(n.Y())])).collect();
                out.push(json!({"edge": e.DebugID(), "err": "", "distance": num(dist), "points": if pts.is_empty() { Value::Null } else { Value::Array(pts) }, "work": work}));
                if nodes.len() < 2 {
                    continue;
                }
                let route = RouteRef::new(Route { GEdge: e.clone(), FromPort: nodes[1].P(), ToPort: nodes[nodes.len() - 2].P(), OVGNodes: nodes });
                if let Err(err) = router.addRoute(&route) {
                    out.push(json!({"edge": e.DebugID(), "err": format!("addRoute: {}", err.Error()), "distance": 0, "points": null, "work": 0}));
                    break;
                }
            }
        }
    }
    if out.is_empty() {
        Value::Null
    } else {
        Value::Array(out)
    }
}

pub(crate) fn ovgParityDump(data: &[u8]) -> Value {
    let ctx = context::Background();
    let g = layoutgraph::NewGraph();
    graphjson::Unmarshal(&ctx, data, Some(&g)).unwrap();
    let mut dump = serde_json::Map::new();
    match buildOVGFromGraphWithLimits(&ctx, &g, &[], defaultOVGBuildLimits()) {
        Err(err) => {
            dump.insert("err".into(), json!(err.Error()));
        }
        Ok(ovg) => {
            dump.insert("err".into(), json!(""));
            dumpOVG(&mut dump, &ovg);
            dump.insert("routes".into(), dumpRoutes(&ovg, &g));
        }
    }
    Value::Object(dump)
}

fn firstDiff(a: &Value, b: &Value, path: &str) -> Option<String> {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            if x.as_f64() == y.as_f64() {
                None
            } else {
                Some(format!("{path}: go={x} rust={y}"))
            }
        }
        (Value::Array(x), Value::Array(y)) => {
            for i in 0..x.len().min(y.len()) {
                if let Some(d) = firstDiff(&x[i], &y[i], &format!("{path}[{i}]")) {
                    return Some(d);
                }
            }
            if x.len() != y.len() {
                return Some(format!("{path}: length go={} rust={}", x.len(), y.len()));
            }
            None
        }
        (Value::Object(x), Value::Object(y)) => {
            for (k, v) in x {
                if k == "guardWork" || k == "err" && path.contains("routes") && false {
                    continue;
                }
                let d = firstDiff(v, y.get(k).unwrap_or(&Value::Null), &format!("{path}.{k}"));
                if d.is_some() {
                    return d;
                }
            }
            None
        }
        (x, y) if x == y => None,
        (x, y) => {
            let xs = x.to_string();
            let ys = y.to_string();
            Some(format!("{path}: go={} rust={}", &xs[..xs.len().min(300)], &ys[..ys.len().min(300)]))
        }
    }
}

#[test]
#[ignore = "needs Go reference data (ORTHOFLOW_GOREF), see README"]
fn ovg_parity() {
    let root = testkit::goref();
    let dir = match std::env::var("ORTHOFLOW_OVG_PARITY_DIR") {
        Ok(d) => std::path::PathBuf::from(d),
        Err(_) => root.join("ovg-parity"),
    };
    let only = std::env::var("ORTHOFLOW_OVG_PARITY_ONLY").ok();
    let mut files: Vec<String> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).filter(|f| f.ends_with(".json")).collect();
    files.sort();
    let mut passed = 0;
    let mut failures: Vec<String> = Vec::new();
    for f in files {
        let name = &f[..f.len() - 5];
        if let Some(o) = &only {
            if !name.contains(o.as_str()) {
                continue;
            }
        }
        let input = root.join("corpus/stages").join(name).join("20_Rescale.json");
        if !input.exists() {
            continue;
        }
        let want: Value = serde_json::from_slice(&std::fs::read(std::path::Path::new(&dir).join(&f)).unwrap()).unwrap();
        let data = std::fs::read(&input).unwrap();
        let got = std::panic::catch_unwind(|| ovgParityDump(&data));
        match got {
            Err(_) => failures.push(format!("{name}: panicked")),
            Ok(got) => match firstDiff(&want, &got, name) {
                Some(d) => failures.push(d),
                None => passed += 1,
            },
        }
    }
    eprintln!("ovg parity: {passed} passed, {} failed", failures.len());
    for f in &failures {
        eprintln!("  {f}");
    }
    assert!(failures.is_empty());
}
