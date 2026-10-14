// SPDX-License-Identifier: BSD-3-Clause
// Copyright 2009 The Go Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSES file.
//
// Ported from Go 1.27.2 (https://go.dev/src):
//   - src/context/context.go
// Rust port: Copyright 2026 Niklas Krieger.

//! `context.Context` → [`Ctx`] (cheap-clone handle; pass `&Ctx`).
//!
//! ```text
//! use gocompat::context::{self, Ctx, CtxKey};
//! static transactionWorkGuardKey: CtxKey = CtxKey::new("transactionWorkGuardKey");
//! let ctx = context::Background();
//! let ctx2 = ctx.WithValue(&transactionWorkGuardKey, guard.clone()); // any 'static value
//! let g: Option<WorkGuardRef> = ctx2.ValueAs::<WorkGuardRef>(&transactionWorkGuardKey);
//! let (wctx, cancel) = context::WithCancel(&ctx);
//! cancel();
//! assert!(wctx.Err().unwrap().Is(&context::Canceled));
//! if ctx.Done().is_none() { /* Go: ctx.Done() == nil */ }
//! ```
use crate::errors::{GoError, Sentinel};
use std::any::Any;
use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

/// `context.Canceled`
pub static Canceled: Sentinel = Sentinel::new("context canceled");
/// `context.DeadlineExceeded`
pub static DeadlineExceeded: Sentinel = Sentinel::new("context deadline exceeded");

/// A context key. Declare one `static` per Go key type; identity is the static's address.
pub struct CtxKey {
    pub name: &'static str,
}
impl CtxKey {
    pub const fn new(name: &'static str) -> CtxKey {
        CtxKey { name }
    }
}

enum Node {
    Background,
    Value { parent: Ctx, key: &'static CtxKey, val: Rc<dyn Any> },
    /// `context.WithCancel`: has a Done channel.
    Cancel { parent: Ctx, err: RefCell<Option<GoError>> },
    /// A custom context whose `Done()` is nil but whose `Err()` can become non-nil
    /// (used by tests that model such Go contexts).
    ErrOnly { parent: Ctx, err: RefCell<Option<GoError>> },
}

/// Go `context.Context`.
#[derive(Clone)]
pub struct Ctx(Rc<Node>);

/// Go `context.CancelFunc`.
pub type CancelFunc = Rc<dyn Fn()>;

/// The Done channel of a context (`<-chan struct{}`), only usable for non-blocking polls.
#[derive(Clone)]
pub struct Done(Ctx);

impl Done {
    /// `select { case <-done: true; default: false }`
    #[inline]
    pub fn IsClosed(&self) -> bool {
        self.0.Err().is_some()
    }
}

/// `context.Background()` (also `context.TODO()`).
pub fn Background() -> Ctx {
    thread_local! {
        static BG: Ctx = Ctx(Rc::new(Node::Background));
    }
    BG.with(|c| c.clone())
}

pub fn TODO() -> Ctx {
    Background()
}

/// `context.WithValue(parent, key, val)`
pub fn WithValue<V: Any>(parent: &Ctx, key: &'static CtxKey, val: V) -> Ctx {
    parent.WithValue(key, val)
}

/// `context.WithCancel(parent)`
pub fn WithCancel(parent: &Ctx) -> (Ctx, CancelFunc) {
    WithCancelErr(parent, Canceled.err())
}

/// Like `WithCancel` but calling the returned function cancels with `err`
/// (e.g. `context::DeadlineExceeded.err()` to model an expired deadline).
pub fn WithCancelErr(parent: &Ctx, err: GoError) -> (Ctx, CancelFunc) {
    let ctx = Ctx(Rc::new(Node::Cancel { parent: parent.clone(), err: RefCell::new(None) }));
    let c2 = ctx.clone();
    let cancel: CancelFunc = Rc::new(move || c2.set_err(err.clone()));
    (ctx, cancel)
}

/// A context whose `Done()` is nil but that can still be cancelled through `Err()`
/// (a custom Go `Context` implementation). Used by tests.
pub fn WithErrOnlyCancel(parent: &Ctx) -> (Ctx, CancelFunc) {
    let ctx = Ctx(Rc::new(Node::ErrOnly { parent: parent.clone(), err: RefCell::new(None) }));
    let c2 = ctx.clone();
    let cancel: CancelFunc = Rc::new(move || c2.set_err(Canceled.err()));
    (ctx, cancel)
}

impl Ctx {
    fn set_err(&self, err: GoError) {
        match &*self.0 {
            Node::Cancel { err: e, .. } | Node::ErrOnly { err: e, .. } => {
                let mut b = e.borrow_mut();
                if b.is_none() {
                    // Go: a cancelCtx keeps the first error (including one inherited
                    // from its parent).
                    let inherited = match &*self.0 {
                        Node::Cancel { parent, .. } => {
                            if parent.Done().is_some() {
                                parent.Err()
                            } else {
                                None
                            }
                        }
                        _ => None,
                    };
                    *b = Some(inherited.unwrap_or(err));
                }
            }
            _ => {}
        }
    }

    pub fn WithValue<V: Any>(&self, key: &'static CtxKey, val: V) -> Ctx {
        Ctx(Rc::new(Node::Value { parent: self.clone(), key, val: Rc::new(val) }))
    }

    /// `ctx.Err()`
    pub fn Err(&self) -> Option<GoError> {
        match &*self.0 {
            Node::Background => None,
            Node::Value { parent, .. } => parent.Err(),
            Node::Cancel { parent, err } => {
                if let Some(e) = &*err.borrow() {
                    return Some(e.clone());
                }
                // Parent cancellation propagates only when the parent has a Done channel.
                if parent.Done().is_some() {
                    parent.Err()
                } else {
                    None
                }
            }
            Node::ErrOnly { err, .. } => err.borrow().clone(),
        }
    }

    /// `ctx.Done()`; `None` is Go's nil channel.
    pub fn Done(&self) -> Option<Done> {
        match &*self.0 {
            Node::Background => None,
            Node::Value { parent, .. } => parent.Done(),
            Node::Cancel { .. } => Some(Done(self.clone())),
            Node::ErrOnly { .. } => None,
        }
    }

    /// `ctx.Value(key)` (untyped).
    pub fn Value(&self, key: &'static CtxKey) -> Option<Rc<dyn Any>> {
        let mut cur = self;
        loop {
            match &*cur.0 {
                Node::Background => return None,
                Node::Value { parent, key: k, val } => {
                    if std::ptr::eq(*k, key) {
                        return Some(val.clone());
                    }
                    cur = parent;
                }
                Node::Cancel { parent, .. } | Node::ErrOnly { parent, .. } => cur = parent,
            }
        }
    }

    /// `v, ok := ctx.Value(key).(T)`: the value if present and of type `T`.
    pub fn ValueAs<T: Any + Clone>(&self, key: &'static CtxKey) -> Option<T> {
        self.Value(key).and_then(|v| v.downcast_ref::<T>().cloned())
    }

    /// Identity of two contexts (Go interface `==` on the same context value).
    pub fn ptr_eq(&self, other: &Ctx) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl fmt::Debug for Ctx {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind = match &*self.0 {
            Node::Background => "Background",
            Node::Value { .. } => "Value",
            Node::Cancel { .. } => "Cancel",
            Node::ErrOnly { .. } => "ErrOnly",
        };
        write!(f, "Ctx({kind})")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    static K1: CtxKey = CtxKey::new("k1");
    static K2: CtxKey = CtxKey::new("k2");

    #[test]
    fn values_and_cancel() {
        let bg = Background();
        assert!(bg.Done().is_none());
        assert!(bg.Err().is_none());
        let v = bg.WithValue(&K1, 5i64);
        assert!(v.Done().is_none());
        assert_eq!(v.ValueAs::<i64>(&K1), Some(5));
        assert_eq!(v.ValueAs::<i64>(&K2), None);
        assert_eq!(v.ValueAs::<bool>(&K1), None);
        let (c, cancel) = WithCancel(&v);
        let c2 = c.WithValue(&K2, true);
        assert_eq!(c2.ValueAs::<i64>(&K1), Some(5));
        assert_eq!(c2.ValueAs::<bool>(&K2), Some(true));
        let d = c2.Done().unwrap();
        assert!(!d.IsClosed());
        assert!(c2.Err().is_none());
        let (child, _cc) = WithCancel(&c2);
        cancel();
        assert!(d.IsClosed());
        assert!(c2.Err().unwrap().Is(&Canceled));
        assert!(child.Err().unwrap().Is(&Canceled));
        assert!(!c2.Err().unwrap().Is(&DeadlineExceeded));
    }

    #[test]
    fn err_only() {
        let (c, cancel) = WithErrOnlyCancel(&Background());
        assert!(c.Done().is_none());
        let (child, _) = WithCancel(&c);
        cancel();
        assert!(c.Err().unwrap().Is(&Canceled));
        // No propagation from a parent whose Done() is nil.
        assert!(child.Err().is_none());
    }
}
