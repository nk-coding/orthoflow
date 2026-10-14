// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Copyright 2026 Niklas Krieger

//! Leak check: replays every stage on Go's snapshots and reports bytes that
//! stay allocated after the graph is torn down. Run with
//! `ORTHOFLOW_GOREF=<dir> cargo test --release -p orthoflow --test leak -- --nocapture --ignored`
//! (`ORTHOFLOW_FIXTURE=<name>` selects the fixture, default `ovg_larger_tree`).
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicIsize, Ordering};

struct Counting;
static LIVE: AtomicIsize = AtomicIsize::new(0);
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        LIVE.fetch_add(l.size() as isize, Ordering::Relaxed);
        System.alloc(l)
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        LIVE.fetch_sub(l.size() as isize, Ordering::Relaxed);
        System.dealloc(p, l)
    }
}
#[global_allocator]
static A: Counting = Counting;

fn live() -> isize {
    LIVE.load(Ordering::Relaxed)
}

#[test]
#[ignore = "needs Go reference data (ORTHOFLOW_GOREF), see README"]
fn leak_per_stage() {
    let root = testkit::goref();
    let fixture = std::env::var("ORTHOFLOW_FIXTURE").unwrap_or("ovg_larger_tree".into());
    let dir = root.join("corpus/stages").join(&fixture);
    let mut stages: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
    stages.sort();
    for (i, st) in stages.iter().enumerate() {
        let prev = if i == 0 { root.join("corpus/inputs").join(format!("{fixture}.json")) } else { dir.join(&stages[i - 1]) };
        let data = std::fs::read_to_string(prev).unwrap();
        // warm-up (thread-local pools etc.)
        let _ = orthoflow::debug_replay(&data, 1, i as i64);
        let before = live();
        for _ in 0..3 {
            let _ = orthoflow::debug_replay(&data, 1, i as i64);
        }
        let leaked = (live() - before) / 3;
        if leaked != 0 {
            println!("{st}: {leaked} bytes/run");
        }
    }
    let data = std::fs::read_to_string(root.join("corpus/inputs").join(format!("{fixture}.json"))).unwrap();
    let _ = orthoflow::layout(&data, &[1]);
    let before = live();
    let _ = orthoflow::layout(&data, &[1]);
    println!("full layout: {} bytes/run", live() - before);
}
