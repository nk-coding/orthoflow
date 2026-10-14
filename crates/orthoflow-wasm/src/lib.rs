// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Copyright 2026 Niklas Krieger

//! wasm-bindgen exports of orthoflow (used by the `orthoflow` npm package).
//!
//! ```js
//! const report = JSON.parse(layout(graphJson, new BigInt64Array([1n, 2n, 3n])));
//! const single = JSON.parse(layoutWithSeed(graphJson, 7n));
//! ```
//! The report JSON has the same format as `orthoflow layout` (and Go's
//! `talaref layout`). Invalid graphjson throws a JS Error. Seeds are `i64`
//! (`BigInt64Array` / `bigint` in JS); the npm package wraps this for numbers.
use wasm_bindgen::prelude::*;

fn report(r: Result<orthoflow::LayoutReport, String>) -> Result<String, JsValue> {
    match r {
        Ok(rep) => Ok(rep.to_json()),
        Err(e) => Err(JsError::new(&e).into()),
    }
}

/// layout(graphJson: string, seeds: BigInt64Array): string
/// An empty seed array means [1, 2, 3].
#[wasm_bindgen(js_name = layout)]
pub fn layout(graph_json: &str, seeds: Vec<i64>) -> Result<String, JsValue> {
    report(orthoflow::layout(graph_json, &seeds))
}

/// layoutWithSeed(graphJson: string, seed: bigint): string
#[wasm_bindgen(js_name = layoutWithSeed)]
pub fn layout_with_seed(graph_json: &str, seed: i64) -> Result<String, JsValue> {
    report(orthoflow::layout_with_seed(graph_json, seed))
}

/// debugStages(graphJson, seed): one line per stage, `NN_Stage<TAB><graph json>`
/// (port harness; raw text so int64 IDs stay exact).
#[wasm_bindgen(js_name = debugStages)]
pub fn debug_stages(graph_json: &str, seed: i64) -> Result<String, JsValue> {
    match orthoflow::debug_stages(graph_json, seed) {
        Ok(stages) => {
            let mut s = String::new();
            for (name, g) in &stages {
                s.push_str(name);
                s.push('\t');
                s.push_str(g);
                s.push('\n');
            }
            Ok(s)
        }
        Err(e) => Err(JsError::new(&e).into()),
    }
}

/// debugReplay(graphJson, seed, stageIndex): graph json after running only that stage.
#[wasm_bindgen(js_name = debugReplay)]
pub fn debug_replay(graph_json: &str, seed: i64, stage: i32) -> Result<String, JsValue> {
    orthoflow::debug_replay(graph_json, seed, stage as i64).map_err(|e| JsError::new(&e).into())
}

/// memoryBytes(): current size of the WASM linear memory (diagnostics, e.g.
/// checking that repeated layouts do not leak).
#[wasm_bindgen(js_name = memoryBytes)]
pub fn memory_bytes() -> f64 {
    #[cfg(target_arch = "wasm32")]
    {
        (core::arch::wasm32::memory_size(0) * 65536) as f64
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        0.0
    }
}
