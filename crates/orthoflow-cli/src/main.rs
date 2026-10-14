// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Copyright 2026 Niklas Krieger

//! `orthoflow` command-line tool. Its modes and output formats match the Go
//! reference oracle `talaref` (goref/d2/d2layouts/d2talalayout/cmd/talaref), so
//! outputs of the two can be compared directly.
//!
//! Modes are dispatched from `main`; add a new mode by writing a
//! `fn mode_<name>(args: &[String]) -> Result<(), String>` and registering it
//! in `MODES`.
use std::io::Write;

type ModeFn = fn(&[String]) -> Result<(), String>;

/// (mode name, handler, usage)
const MODES: &[(&str, ModeFn, &str)] = &[
    ("roundtrip", mode_roundtrip, "roundtrip in.json"),
    ("stages", mode_stages, "stages -seed S in.json outdir"),
    ("replay", mode_replay, "replay -seed S -stage N snapshot.json"),
    ("layout", mode_layout, "layout -seeds 1,2,3 in.json"),
    ("score", mode_score, "score graph.json"),
    ("bench", mode_bench, "bench -seeds 1,2,3 -iters N files..."),
];

fn usage() -> String {
    let mut s = String::from("usage:\n");
    for (_, _, u) in MODES {
        s.push_str("  orthoflow ");
        s.push_str(u);
        s.push('\n');
    }
    s
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprint!("{}", usage());
        std::process::exit(2);
    }
    let mode = args[1].as_str();
    let Some((_, handler, _)) = MODES.iter().find(|(name, _, _)| *name == mode) else {
        eprintln!("unknown mode {:?}", mode);
        eprint!("{}", usage());
        std::process::exit(2);
    };
    if let Err(err) = handler(&args[2..]) {
        eprintln!("{}", err);
        std::process::exit(1);
    }
}

/// Reads a graphjson file and decodes it into a fresh graph.
pub fn read_graph(ctx: &gocompat::Ctx, path: &str) -> Result<layoutgraph::GraphRef, String> {
    let data = std::fs::read(path).map_err(|e| format!("read {}: {}", path, e))?;
    let g = layoutgraph::NewGraph();
    graphjson::Unmarshal(ctx, &data, Some(&g)).map_err(|e| format!("unmarshal {}: {}", path, e))?;
    Ok(g)
}

/// Encodes a graph and writes it to stdout.
pub fn write_graph_stdout(ctx: &gocompat::Ctx, g: &layoutgraph::GraphRef) -> Result<(), String> {
    let out = graphjson::Marshal(ctx, Some(g)).map_err(|e| format!("marshal: {}", e))?;
    let stdout = std::io::stdout();
    let mut lock = stdout.lock();
    lock.write_all(&out).map_err(|e| e.to_string())?;
    lock.write_all(b"\n").map_err(|e| e.to_string())?;
    Ok(())
}

/// `orthoflow roundtrip in.json` → Marshal(Clone(Unmarshal(in))).
fn mode_roundtrip(args: &[String]) -> Result<(), String> {
    if args.len() != 1 {
        return Err(usage());
    }
    let ctx = gocompat::context::Background();
    let g = read_graph(&ctx, &args[0])?;
    let cloned = layoutgraph::Clone(&ctx, Some(&g)).map_err(|e| format!("clone: {}", e))?;
    write_graph_stdout(&ctx, &cloned)
}

/// Go `flag` subset used by talaref: -seed, -seeds, -iters, -stage (as
/// `-name value`, `-name=value` or with `--`). Parsing stops at the first
/// non-flag argument (or after `--`).
struct Flags {
    seed: i64,
    seeds: String,
    iters: i64,
    stage: i64,
    args: Vec<String>,
}

fn parse_flags(args: &[String]) -> Result<Flags, String> {
    let mut f = Flags { seed: 1, seeds: "1,2,3".into(), iters: 5, stage: 0, args: Vec::new() };
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if a == "--" {
            i += 1;
            break;
        }
        if !a.starts_with('-') || a == "-" {
            break;
        }
        let name_val = a.trim_start_matches('-');
        let (name, inline) = match name_val.split_once('=') {
            Some((n, v)) => (n.to_string(), Some(v.to_string())),
            None => (name_val.to_string(), None),
        };
        let value = match inline {
            Some(v) => v,
            None => {
                i += 1;
                args.get(i).cloned().ok_or_else(|| format!("flag needs an argument: -{}", name))?
            }
        };
        let int = |v: &str| v.parse::<i64>().map_err(|_| format!("invalid value {:?} for flag -{}", v, name));
        match name.as_str() {
            "seed" => f.seed = int(&value)?,
            "seeds" => f.seeds = value,
            "iters" => f.iters = int(&value)?,
            "stage" => f.stage = int(&value)?,
            _ => return Err(format!("flag provided but not defined: -{}", name)),
        }
        i += 1;
    }
    f.args = args[i..].to_vec();
    Ok(f)
}

fn parse_seeds(s: &str) -> Result<Vec<i64>, String> {
    s.split(',').map(|f| f.trim().parse::<i64>().map_err(|e| format!("error: strconv.ParseInt: parsing {:?}: {}", f.trim(), e))).collect()
}

fn err_line(e: impl std::fmt::Display) -> String {
    format!("error: {}", e)
}

fn read_zero_graph(ctx: &gocompat::Ctx, path: &str) -> Result<layoutgraph::GraphRef, String> {
    let data = std::fs::read(path).map_err(err_line)?;
    orthoflow::ReadGraph(ctx, &data).map_err(err_line)
}

fn write_stdout_line(data: &[u8]) -> Result<(), String> {
    let stdout = std::io::stdout();
    let mut lock = stdout.lock();
    lock.write_all(data).map_err(|e| e.to_string())?;
    lock.write_all(b"\n").map_err(|e| e.to_string())?;
    Ok(())
}

/// `orthoflow stages -seed S in.json outdir` → outdir/NN_Stage.json after every stage.
fn mode_stages(args: &[String]) -> Result<(), String> {
    let f = parse_flags(args).map_err(err_line)?;
    if f.args.len() < 2 {
        return Err(usage());
    }
    let ctx = gocompat::context::Background();
    let g = read_zero_graph(&ctx, &f.args[0])?;
    let out = std::path::PathBuf::from(&f.args[1]);
    std::fs::create_dir_all(&out).map_err(err_line)?;
    let res = engine::DebugRunStages(&ctx, &g, f.seed, |i, name, g| {
        let data = graphjson::Marshal(&ctx, Some(g))
            .map_err(|e| gocompat::GoError::new(format!("marshal after {}: {}", name, e)))?;
        std::fs::write(out.join(format!("{:02}_{}.json", i, name)), data)
            .map_err(|e| gocompat::GoError::new(e.to_string()))
    });
    match res {
        Ok(ws) => {
            layoutgraph::Teardown(&ws);
            layoutgraph::Teardown(&g);
            Ok(())
        }
        Err(e) => Err(err_line(e)),
    }
}

/// `orthoflow replay -seed S -stage N snapshot.json` → graph after running only stage N.
fn mode_replay(args: &[String]) -> Result<(), String> {
    let f = parse_flags(args).map_err(err_line)?;
    if f.args.is_empty() {
        return Err(usage());
    }
    let ctx = gocompat::context::Background();
    let g = read_zero_graph(&ctx, &f.args[0])?;
    engine::DebugReplayStage(&ctx, &g, f.seed, f.stage).map_err(err_line)?;
    let data = graphjson::Marshal(&ctx, Some(&g)).map_err(err_line)?;
    write_stdout_line(&data)
}

/// `orthoflow score graph.json` → {"area":..,"penalty":..}
fn mode_score(args: &[String]) -> Result<(), String> {
    let f = parse_flags(args).map_err(err_line)?;
    if f.args.is_empty() {
        return Err(usage());
    }
    let ctx = gocompat::context::Background();
    let g = read_zero_graph(&ctx, &f.args[0])?;
    let (p, a) = quality::EvaluateWithArea(&ctx, Some(&g)).map_err(err_line)?;
    let s = format!("{{\"area\":{},\"penalty\":{}}}", orthoflow::GoJSONFloat(a), orthoflow::GoJSONFloat(p));
    write_stdout_line(s.as_bytes())
}

/// `orthoflow layout -seeds 1,2,3 in.json` → layout report JSON.
fn mode_layout(args: &[String]) -> Result<(), String> {
    let f = parse_flags(args).map_err(err_line)?;
    if f.args.is_empty() {
        return Err(usage());
    }
    let seeds = parse_seeds(&f.seeds)?;
    let ctx = gocompat::context::Background();
    let g = read_zero_graph(&ctx, &f.args[0])?;
    let rep = orthoflow::LayoutBest(&ctx, &g, &seeds).map_err(err_line)?;
    layoutgraph::Teardown(&g);
    write_stdout_line(rep.to_json().as_bytes())
}

/// `orthoflow bench -seeds 1,2,3 -iters N files…` → {"file":..,"ms":[..]} per file.
fn mode_bench(args: &[String]) -> Result<(), String> {
    let f = parse_flags(args).map_err(err_line)?;
    let seeds = parse_seeds(&f.seeds)?;
    let ctx = gocompat::context::Background();
    for path in &f.args {
        let g = read_zero_graph(&ctx, path)?;
        orthoflow::LayoutBest(&ctx, &g, &seeds).map_err(err_line)?; // warm-up
        let mut times: Vec<String> = Vec::new();
        for _ in 0..f.iters {
            let start = std::time::Instant::now();
            orthoflow::LayoutBest(&ctx, &g, &seeds).map_err(err_line)?;
            let micros = start.elapsed().as_micros() as i64;
            times.push(orthoflow::GoJSONFloat(micros as f64 / 1000.0));
        }
        layoutgraph::Teardown(&g);
        let file = std::path::Path::new(path).file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let ms = if times.is_empty() { "null".to_string() } else { format!("[{}]", times.join(",")) };
        let line = format!("{{\"file\":{},\"ms\":{}}}", orthoflow::GoJSONString(&file), ms);
        write_stdout_line(line.as_bytes())?;
    }
    Ok(())
}
