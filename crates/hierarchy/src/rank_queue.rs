// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/hierarchy/rank_queue.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.
//
/// nodeMinHeap is a small priority queue for stable Kahn traversal.
#[derive(Default)]
pub(crate) struct nodeMinHeap(pub(crate) Vec<usize>);

impl nodeMinHeap {
    pub(crate) fn push(&mut self, node: usize) {
        let h = &mut self.0;
        h.push(node);
        let mut child = h.len() - 1;
        while child > 0 {
            let parent = (child - 1) / 2;
            if h[parent] <= h[child] {
                break;
            }
            h.swap(parent, child);
            child = parent;
        }
    }

    pub(crate) fn pop(&mut self) -> usize {
        let old = &mut self.0;
        let root = old[0];
        let last = old[old.len() - 1];
        old.pop();
        if !old.is_empty() {
            old[0] = last;
            let mut parent = 0;
            loop {
                let left = parent * 2 + 1;
                if left >= old.len() {
                    break;
                }
                let mut child = left;
                let right = left + 1;
                if right < old.len() && old[right] < old[left] {
                    child = right;
                }
                if old[parent] <= old[child] {
                    break;
                }
                old.swap(parent, child);
                parent = child;
            }
        }
        root
    }

    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }
}
