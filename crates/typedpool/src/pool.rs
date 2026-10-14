// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/typedpool/pool.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;
use std::cell::RefCell;

/// Go `typedpool.Pool[T]` (a typed `sync.Pool`). Single-threaded LIFO free list:
/// `Put` then `Get` returns the same value, like sync.Pool's per-P private slot.
/// Go declares pools as package-level vars; in Rust use a `thread_local!`:
///
/// ```text
/// thread_local! {
///     static scratchPool: typedpool::Pool<EdgeScratchRef> =
///         typedpool::New(|| EdgeScratchRef::new(EdgeScratch::default()));
/// }
/// let s = scratchPool.with(|p| p.Get());
/// // ...
/// scratchPool.with(|p| p.Put(s));
/// ```
pub struct Pool<T> {
    newValue: Box<dyn Fn() -> T>,
    free: RefCell<Vec<T>>,
}

/// `typedpool.New(newValue)`
pub fn New<T>(newValue: impl Fn() -> T + 'static) -> Pool<T> {
    Pool { newValue: Box::new(newValue), free: RefCell::new(Vec::new()) }
}

impl<T> Pool<T> {
    pub fn Get(&self) -> T {
        let v = self.free.borrow_mut().pop();
        match v {
            Some(v) => v,
            None => (self.newValue)(),
        }
    }

    pub fn Put(&self, value: T) {
        self.free.borrow_mut().push(value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn get_put() {
        let p = New(|| vec![0u8; 3]);
        let mut a = p.Get();
        a.push(1);
        p.Put(a);
        assert_eq!(p.Get().len(), 4);
        assert_eq!(p.Get().len(), 3);
    }
}
