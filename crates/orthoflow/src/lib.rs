// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from the following files of terrastruct/d2, Copyright 2022 Terrastruct Inc.:
//   - d2layouts/d2talalayout/layout.go
//   - d2layouts/d2talalayout/result.go
// (seed selection; the public API itself is new).
// Rust port: Copyright 2026 Niklas Krieger.

//! orthoflow: an orthogonal graph layout engine. Unofficial, independent,
//! bit-exact Rust port of the TALA layout engine from Terrastruct's D2
//! (<https://github.com/terrastruct/d2>); not affiliated with or endorsed by
//! Terrastruct.
//!
//! graphjson in → multi-seed layout → graphjson out:
//!
//! ```no_run
//! let input = std::fs::read_to_string("graph.json").unwrap();
//! let report = orthoflow::layout(&input, &[1, 2, 3]).unwrap();
//! println!("best seed {:?}", report.best_seed);
//! let laid_out: &str = report.graph.as_deref().unwrap(); // graphjson with positions
//! # let _ = laid_out;
//! ```
//!
//! Like the Go reference oracle `talaref` (`layoutBest`), every seed runs one
//! full `engine::Layout`, each result is scored with
//! `quality::EvaluateWithArea`, and the best seed is the lowest exact penalty,
//! then the lowest area; exact ties favour the later seed.
#![allow(non_snake_case)]

use gocompat::Ctx;
use layoutgraph::GraphRef;

/// Seeds used when the caller does not provide any.
pub const DEFAULT_SEEDS: [i64; 3] = [1, 2, 3];

/// Per-seed outcome (Go `seedReport`).
#[derive(Clone, Debug, PartialEq)]
pub struct SeedReport {
    pub seed: i64,
    pub penalty: Option<f64>,
    pub area: Option<f64>,
    /// Empty when the seed succeeded.
    pub error: String,
}

/// Go `layoutReport`: per-seed scores, the selected seed and its graph.
#[derive(Clone, Debug, PartialEq)]
pub struct LayoutReport {
    pub seeds: Vec<SeedReport>,
    pub best_seed: Option<i64>,
    /// graphjson of the selected layout (`None` = JSON null).
    pub graph: Option<String>,
}

impl LayoutReport {
    /// Encodes the report exactly like Go's `json.Marshal(layoutReport)`.
    pub fn to_json(&self) -> String {
        let mut s = String::from("{\"seeds\":[");
        for (i, r) in self.seeds.iter().enumerate() {
            if i > 0 {
                s.push(',');
            }
            s.push_str("{\"seed\":");
            s.push_str(&r.seed.to_string());
            if let Some(p) = r.penalty {
                s.push_str(",\"penalty\":");
                s.push_str(&GoJSONFloat(p));
            }
            if let Some(a) = r.area {
                s.push_str(",\"area\":");
                s.push_str(&GoJSONFloat(a));
            }
            if !r.error.is_empty() {
                s.push_str(",\"error\":");
                s.push_str(&GoJSONString(&r.error));
            }
            s.push('}');
        }
        s.push_str("],\"bestSeed\":");
        match self.best_seed {
            Some(b) => s.push_str(&b.to_string()),
            None => s.push_str("null"),
        }
        s.push_str(",\"graph\":");
        match &self.graph {
            Some(g) => s.push_str(g),
            None => s.push_str("null"),
        }
        s.push('}');
        s
    }
}

/// Go encoding/json float64 formatting.
pub fn GoJSONFloat(f: f64) -> String {
    let abs = f.abs();
    let mut fmt = b'f';
    if abs != 0.0 && (abs < 1e-6 || abs >= 1e21) {
        fmt = b'e';
    }
    let mut b = gocompat::strconv::FormatFloatShortest(f, fmt).into_bytes();
    if fmt == b'e' {
        let n = b.len();
        if n >= 4 && b[n - 4] == b'e' && b[n - 3] == b'-' && b[n - 2] == b'0' {
            b[n - 2] = b[n - 1];
            b.truncate(n - 1);
        }
    }
    String::from_utf8(b).unwrap()
}

/// Go encoding/json string encoding (HTML-safe escaping, like json.Marshal).
pub fn GoJSONString(s: &str) -> String {
    let enc = serde_json::to_string(s).unwrap();
    enc.replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029")
}

/// Decodes graphjson into a fresh zero graph (Go `var g layoutgraph.Graph`).
pub fn ReadGraph(ctx: &Ctx, data: &[u8]) -> Result<GraphRef, String> {
    let g = layoutgraph::NewZeroGraph();
    if let Err(err) = graphjson::Unmarshal(ctx, data, Some(&g)) {
        layoutgraph::Teardown(&g);
        return Err(err.to_string());
    }
    Ok(g)
}

/// layoutBest mirrors d2talalayout's seed selection on a decoded graph. The
/// input graph is left intact (the caller owns it).
pub fn LayoutBest(ctx: &Ctx, input: &GraphRef, seeds: &[i64]) -> Result<LayoutReport, String> {
    let mut rep = LayoutReport { seeds: Vec::new(), best_seed: None, graph: None };
    let mut best: Option<GraphRef> = None;
    let (mut bestP, mut bestA) = (0.0f64, 0.0f64);
    for &seed in seeds {
        let mut r = SeedReport { seed, penalty: None, area: None, error: String::new() };
        match engine::Layout(ctx, Some(input), engine::LayoutOptions { Seed: seed }) {
            Ok(g) => match quality::EvaluateWithArea(ctx, Some(&g)) {
                Ok((p, a)) => {
                    r.penalty = Some(p);
                    r.area = Some(a);
                    if best.is_none() || p < bestP || (p == bestP && (a <= bestA || bestA.is_nan())) {
                        if let Some(old) = best.take() {
                            layoutgraph::Teardown(&old);
                        }
                        best = Some(g);
                        bestP = p;
                        bestA = a;
                        rep.best_seed = Some(seed);
                    } else {
                        layoutgraph::Teardown(&g);
                    }
                }
                Err(err) => {
                    layoutgraph::Teardown(&g);
                    r.error = err.to_string();
                }
            },
            Err(err) => r.error = err.to_string(),
        }
        rep.seeds.push(r);
    }
    if let Some(best) = best {
        let data = graphjson::Marshal(ctx, Some(&best));
        layoutgraph::Teardown(&best);
        let data = data.map_err(|e| e.to_string())?;
        rep.graph = Some(String::from_utf8(data).map_err(|e| e.to_string())?);
    }
    Ok(rep)
}

/// Lays out a graphjson document with every seed and returns the report.
/// An empty `seeds` slice means [`DEFAULT_SEEDS`].
pub fn layout(graph_json: &str, seeds: &[i64]) -> Result<LayoutReport, String> {
    let seeds: &[i64] = if seeds.is_empty() { &DEFAULT_SEEDS } else { seeds };
    let ctx = gocompat::context::Background();
    let g = ReadGraph(&ctx, graph_json.as_bytes())?;
    let rep = LayoutBest(&ctx, &g, seeds);
    layoutgraph::Teardown(&g);
    rep
}

/// Single-seed layout (same report format, one seed).
pub fn layout_with_seed(graph_json: &str, seed: i64) -> Result<LayoutReport, String> {
    layout(graph_json, &[seed])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn json_floats() {
        assert_eq!(GoJSONFloat(1e-7), "1e-7");
        assert_eq!(GoJSONFloat(1.5e21), "1.5e+21");
        assert_eq!(GoJSONFloat(100.0), "100");
        assert_eq!(GoJSONFloat(0.25), "0.25");
        assert_eq!(GoJSONFloat(123456789.125), "123456789.125");
        assert_eq!(GoJSONString("a<b>&\"c\n"), "\"a\\u003cb\\u003e\\u0026\\\"c\\n\"");
    }
}

/// Port-harness hook: graphjson after every pipeline stage (seed `seed`), as
/// (`NN_StageName`, graph json) pairs, like `talaref stages`.
pub fn debug_stages(graph_json: &str, seed: i64) -> Result<Vec<(String, String)>, String> {
    let ctx = gocompat::context::Background();
    let g = ReadGraph(&ctx, graph_json.as_bytes())?;
    let mut out: Vec<(String, String)> = Vec::new();
    let res = engine::DebugRunStages(&ctx, &g, seed, |i, name, g| {
        let data = graphjson::Marshal(&ctx, Some(g))
            .map_err(|e| gocompat::GoError::new(format!("marshal after {}: {}", name, e)))?;
        out.push((format!("{:02}_{}", i, name), String::from_utf8(data).unwrap()));
        Ok(())
    });
    layoutgraph::Teardown(&g);
    match res {
        Ok(ws) => {
            layoutgraph::Teardown(&ws);
            Ok(out)
        }
        Err(e) => Err(e.to_string()),
    }
}

/// Port-harness hook: runs only stage `stage` on a snapshot, like `talaref replay`.
pub fn debug_replay(graph_json: &str, seed: i64, stage: i64) -> Result<String, String> {
    let ctx = gocompat::context::Background();
    let g = ReadGraph(&ctx, graph_json.as_bytes())?;
    let res = engine::DebugReplayStage(&ctx, &g, seed, stage)
        .map_err(|e| e.to_string())
        .and_then(|_| graphjson::Marshal(&ctx, Some(&g)).map_err(|e| e.to_string()));
    layoutgraph::Teardown(&g);
    Ok(String::from_utf8(res?).unwrap())
}
