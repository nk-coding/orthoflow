// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/placement/edge_abduction_scratch.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of placement/edge_abduction_scratch.go
//!
//! Go pools a `[]bool` scratch buffer; the port keeps a thread-local free list
//! so hot optimizer paths reuse the allocation.
use crate::*;
use std::cell::RefCell;

pub struct edgeAbductionBoolScratch {
    pub values: Vec<bool>,
}

thread_local! {
    static edgeAbductionBoolScratchPool: RefCell<Vec<edgeAbductionBoolScratch>> = RefCell::new(Vec::new());
}

pub fn borrowEdgeAbductionBools(length: usize) -> edgeAbductionBoolScratch {
    let mut scratch = edgeAbductionBoolScratchPool
        .with(|p| p.borrow_mut().pop())
        .unwrap_or_else(|| edgeAbductionBoolScratch { values: Vec::with_capacity(16) });
    scratch.values.clear();
    scratch.values.resize(length, false);
    scratch
}

pub fn returnEdgeAbductionBools(mut scratch: edgeAbductionBoolScratch) {
    scratch.values.clear();
    edgeAbductionBoolScratchPool.with(|p| p.borrow_mut().push(scratch));
}
