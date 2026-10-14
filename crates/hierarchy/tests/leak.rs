// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Copyright 2026 Niklas Krieger

//! Leak check for stage 04 PreprocessHierarchies (and PlaceCompound): bytes
//! still allocated after the graph is torn down. Run with --ignored --nocapture.
extern crate orthoflow_hierarchy as hierarchy;
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

fn run(path: &std::path::Path, compound: bool) {
    let ctx0 = gocompat::context::Background();
    let g = testkit::load_graph(&ctx0, path);
    {
        let (ctx, _guard) = layoutgraph::EnsureTransactionWorkGuard(&ctx0, "AutolayoutTransactions").unwrap();
        if compound {
            let _ = hierarchy::PlaceCompound(&ctx, &g, Some(&gocompat::rand::New(gocompat::rand::NewSource(0))));
        } else {
            let r = gocompat::rand::New(gocompat::rand::NewSource(1));
            let _ = (|| -> gocompat::GoResult<()> {
                hierarchy::Assign(&ctx, &g, None, &hierarchy::Candidates(&g))?;
                hierarchy::Place(&ctx, &g, None, Some(&r))?;
                hierarchy::RemoveIsolatedMemberships(&g);
                Ok(())
            })();
        }
    }
    layoutgraph::Teardown(&g);
}

#[test]
#[ignore]
fn leak_stage04() {
    let root = testkit::goref();
    let mut fixtures: Vec<_> = std::fs::read_dir(root.join("corpus/stages")).unwrap().map(|e| e.unwrap().path()).collect();
    fixtures.sort();
    let mut total = 0;
    for f in fixtures {
        for (snap, compound) in [("03_PreprocessTrees.json", false), ("20_Rescale.json", true)] {
            let p = f.join(snap);
            run(&p, compound);
            let before = LIVE.load(Ordering::Relaxed);
            for _ in 0..3 {
                run(&p, compound);
            }
            let leaked = (LIVE.load(Ordering::Relaxed) - before) / 3;
            if leaked != 0 {
                total += 1;
                println!("{}/{}: {} bytes/run", f.file_name().unwrap().to_str().unwrap(), if compound { "compound" } else { "stage04" }, leaked);
            }
        }
    }
    println!("leaking runs: {total}");
}
