// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/routing/priority_queue.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of routing/priority_queue.go.
#[allow(unused_imports)]
use crate::*;
use gocompat::{errorf, GoResult};

pub(crate) const priorityQueueEntryChunkSize: usize = 256;

/// priorityQueue is an indexed binary min-heap. Entries keep their heap index
/// so Dijkstra's algorithm can lower a queued distance without a map lookup.
/// Equal priorities are popped in insertion order.
///
/// Rust: Go's `*priorityQueueEntry` handles are indexes into `entries` (the
/// chunked entry arena); `items` holds the heap of entry handles.
#[derive(Default)]
pub struct priorityQueue {
    items: Vec<usize>,
    entries: Vec<priorityQueueEntry>,
    entryCursor: usize,
    nextOrder: u64,
}

/// Handle of a queued entry (Go `*priorityQueueEntry`).
pub(crate) type priorityQueueEntryRef = usize;

#[derive(Clone, Default)]
pub struct priorityQueueEntry {
    pub(crate) node: Option<OVGNodeRef>,
    pub(crate) isHorizontal: bool,
    pub(crate) priority: f64,
    pub(crate) index: i64,
    pub(crate) order: u64,
}

impl priorityQueue {
    /// reset empties the queue while retaining storage. Entries returned before a
    /// reset must not be used afterward.
    pub(crate) fn reset(&mut self) {
        self.items.clear();
        self.entryCursor = 0;
        self.nextOrder = 0;
    }

    #[inline]
    pub(crate) fn empty(&self) -> bool {
        self.items.is_empty()
    }

    #[inline]
    pub(crate) fn entry(&self, e: priorityQueueEntryRef) -> &priorityQueueEntry {
        &self.entries[e]
    }

    pub(crate) fn push(&mut self, priority: f64, node: &OVGNodeRef, isHorizontal: bool, guard: Option<&dyn workBudget>) -> GoResult<priorityQueueEntryRef> {
        reservePriorityQueueWork(guard, self.items.len() + 1, 1)?;

        let entry = self.allocateEntry(priority);
        let index = self.items.len();
        {
            let e = &mut self.entries[entry];
            e.node = Some(node.clone());
            e.isHorizontal = isHorizontal;
            e.index = index as i64;
            e.order = self.nextOrder;
        }
        self.nextOrder += 1;
        self.items.push(entry);
        self.siftUp(index);
        Ok(entry)
    }

    /// Returns (node, isHorizontal, priority) of the popped minimum.
    pub(crate) fn pop(&mut self, guard: Option<&dyn workBudget>) -> GoResult<priorityQueueEntryRef> {
        if self.empty() {
            return Err(errorf!("cannot dequeue minimum of empty priority queue"));
        }
        reservePriorityQueueWork(guard, self.items.len(), 2)?;

        let min = self.items[0];
        let lastIndex = self.items.len() - 1;
        let last = self.items[lastIndex];
        self.items.truncate(lastIndex);
        self.entries[min].index = -1;

        if lastIndex != 0 {
            self.items[0] = last;
            self.entries[last].index = 0;
            self.siftDown(0);
        }
        Ok(min)
    }

    pub(crate) fn decrease(&mut self, entry: Option<priorityQueueEntryRef>, priority: f64, guard: Option<&dyn workBudget>) -> GoResult<()> {
        if self.empty() {
            return Err(errorf!("cannot decrease priority in an empty priority queue"));
        }
        let entry = match entry {
            None => return Err(errorf!("cannot decrease priority: entry is nil")),
            Some(e) => e,
        };
        let idx = self.entries[entry].index;
        if idx < 0 || idx as usize >= self.items.len() || self.items[idx as usize] != entry {
            return Err(errorf!("cannot decrease priority: entry is not in the priority queue"));
        }
        let old = self.entries[entry].priority;
        if priority >= old {
            return Err(errorf!(
                "new priority {} is not less than old priority {}",
                gocompat::strconv::FormatFloatV(priority),
                gocompat::strconv::FormatFloatV(old)
            ));
        }
        reservePriorityQueueWork(guard, idx as usize + 1, 1)?;

        self.entries[entry].priority = priority;
        self.siftUp(idx as usize);
        Ok(())
    }

    fn allocateEntry(&mut self, priority: f64) -> priorityQueueEntryRef {
        if self.entryCursor == self.entries.len() {
            // Go allocates chunks of priorityQueueEntryChunkSize entries.
            let n = self.entries.len() + priorityQueueEntryChunkSize;
            self.entries.resize(n, priorityQueueEntry::default());
        }
        let entry = self.entryCursor;
        self.entryCursor += 1;
        self.entries[entry] = priorityQueueEntry { node: None, isHorizontal: false, priority, index: -1, order: 0 };
        entry
    }

    fn siftUp(&mut self, mut index: usize) {
        while index > 0 {
            let parent = (index - 1) / 2;
            if !self.less(index, parent) {
                return;
            }
            self.swap(index, parent);
            index = parent;
        }
    }

    fn siftDown(&mut self, mut index: usize) {
        loop {
            let left = index * 2 + 1;
            if left >= self.items.len() {
                return;
            }
            let mut smallest = left;
            let right = left + 1;
            if right < self.items.len() && self.less(right, left) {
                smallest = right;
            }
            if !self.less(smallest, index) {
                return;
            }
            self.swap(index, smallest);
            index = smallest;
        }
    }

    #[inline]
    fn less(&self, left: usize, right: usize) -> bool {
        priorityQueueEntryLess(&self.entries[self.items[left]], &self.entries[self.items[right]])
    }

    #[inline]
    fn swap(&mut self, left: usize, right: usize) {
        self.items.swap(left, right);
        let (l, r) = (self.items[left], self.items[right]);
        self.entries[l].index = left as i64;
        self.entries[r].index = right as i64;
    }
}

pub(crate) fn reservePriorityQueueWork(guard: Option<&dyn workBudget>, upperBound: usize, unitsPerLevel: u64) -> GoResult<()> {
    match guard {
        None => Ok(()),
        Some(g) => {
            let len = (usize::BITS - upperBound.leading_zeros()) as u64;
            g.add(len * unitsPerLevel)
        }
    }
}

#[inline]
pub(crate) fn priorityQueueEntryLess(a: &priorityQueueEntry, b: &priorityQueueEntry) -> bool {
    if a.priority != b.priority {
        return a.priority < b.priority;
    }
    // Preserve insertion order for equal-cost states. Route selection should not
    // depend on the binary heap's internal shape.
    a.order < b.order
}
