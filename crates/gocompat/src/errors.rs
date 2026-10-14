// SPDX-License-Identifier: BSD-3-Clause
// Copyright 2009 The Go Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSES file.
//
// Ported from Go 1.27.2 (https://go.dev/src):
//   - src/errors/errors.go
//   - src/errors/wrap.go
//   - src/errors/join.go
// Rust port: Copyright 2026 Niklas Krieger.

//! Go `error` values.
//!
//! * `var ErrX = errors.New("...")` → `pub static ErrX: Sentinel = Sentinel::new("...");`
//! * `return ErrX` → `Err(ErrX.err())` (or `Err((&ErrX).into())`)
//! * `errors.New(msg)` / `fmt.Errorf` without `%w` → `GoError::new(msg)`
//! * `fmt.Errorf("...%w...", err)` → `GoError::wrap(full_message, err)` where
//!   `full_message` is the complete formatted text (format `err` into it yourself).
//! * `errors.Join(errs...)` → `GoError::join(errs)`
//! * `errors.Is(err, ErrX)` → `err.Is(&ErrX)`; for a possibly nil error
//!   `errors::Is(err.as_ref(), &ErrX)`.
use std::fmt;
use std::rc::Rc;

/// A package-level `errors.New` sentinel. Identity is the static's address.
pub struct Sentinel {
    msg: &'static str,
}

impl Sentinel {
    pub const fn new(msg: &'static str) -> Sentinel {
        Sentinel { msg }
    }
    /// The error value `ErrX` (as returned by `return ErrX`).
    #[inline]
    pub fn err(&'static self) -> GoError {
        GoError(Rc::new(Inner::Sentinel(self)))
    }
    pub fn Error(&self) -> String {
        self.msg.to_string()
    }
    #[inline]
    fn same(&self, other: &Sentinel) -> bool {
        std::ptr::eq(self, other)
    }
}

impl fmt::Debug for Sentinel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Sentinel({:?})", self.msg)
    }
}

impl fmt::Display for Sentinel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.msg)
    }
}

enum Inner {
    Sentinel(&'static Sentinel),
    Msg(String),
    Wrap { msg: String, wrapped: Vec<GoError> },
    Join(Vec<GoError>),
}

/// A Go `error` (non-nil). Cheap to clone; identity (`==` in Go) is pointer identity.
#[derive(Clone)]
pub struct GoError(Rc<Inner>);

/// `(T, error)` → `GoResult<T>`; `error` alone → `GoResult<()>`.
pub type GoResult<T> = Result<T, GoError>;

impl GoError {
    /// `errors.New(msg)` / `fmt.Errorf(...)` without `%w`.
    pub fn new(msg: impl Into<String>) -> GoError {
        GoError(Rc::new(Inner::Msg(msg.into())))
    }
    /// `fmt.Errorf("... %w ...", err)`: `msg` is the full message text.
    pub fn wrap(msg: impl Into<String>, err: GoError) -> GoError {
        GoError(Rc::new(Inner::Wrap { msg: msg.into(), wrapped: vec![err] }))
    }
    /// `fmt.Errorf` with several `%w` verbs.
    pub fn wrap_many(msg: impl Into<String>, errs: Vec<GoError>) -> GoError {
        GoError(Rc::new(Inner::Wrap { msg: msg.into(), wrapped: errs }))
    }
    /// `errors.Join(errs...)`: nil entries are dropped; returns None when all are nil.
    pub fn join<I: IntoIterator<Item = Option<GoError>>>(errs: I) -> Option<GoError> {
        let v: Vec<GoError> = errs.into_iter().flatten().collect();
        if v.is_empty() {
            None
        } else {
            Some(GoError(Rc::new(Inner::Join(v))))
        }
    }
    /// `errors.Join` over non-nil errors.
    pub fn join_errs(errs: Vec<GoError>) -> Option<GoError> {
        if errs.is_empty() {
            None
        } else {
            Some(GoError(Rc::new(Inner::Join(errs))))
        }
    }
    /// `errors.Is(err, sentinel)`.
    pub fn Is(&self, target: &Sentinel) -> bool {
        match &*self.0 {
            Inner::Sentinel(s) => s.same(target),
            Inner::Msg(_) => false,
            Inner::Wrap { wrapped, .. } => wrapped.iter().any(|e| e.Is(target)),
            Inner::Join(errs) => errs.iter().any(|e| e.Is(target)),
        }
    }
    /// `errors.Is(err, target)` for a non-sentinel target error value (pointer identity,
    /// or same sentinel).
    pub fn IsErr(&self, target: &GoError) -> bool {
        if let Inner::Sentinel(t) = &*target.0 {
            return self.Is(t);
        }
        if Rc::ptr_eq(&self.0, &target.0) {
            return true;
        }
        match &*self.0 {
            Inner::Sentinel(_) | Inner::Msg(_) => false,
            Inner::Wrap { wrapped, .. } => wrapped.iter().any(|e| e.IsErr(target)),
            Inner::Join(errs) => errs.iter().any(|e| e.IsErr(target)),
        }
    }
    /// The sentinel if this error *is* (not wraps) a sentinel.
    pub fn AsSentinel(&self) -> Option<&'static Sentinel> {
        match &*self.0 {
            Inner::Sentinel(s) => Some(s),
            _ => None,
        }
    }
    /// `errors.Unwrap` / `Unwrap() []error`.
    pub fn Unwrap(&self) -> Vec<GoError> {
        match &*self.0 {
            Inner::Wrap { wrapped, .. } => wrapped.clone(),
            Inner::Join(errs) => errs.clone(),
            _ => Vec::new(),
        }
    }
    /// Go pointer identity of two error values (`err1 == err2`).
    pub fn ptr_eq(&self, other: &GoError) -> bool {
        match (&*self.0, &*other.0) {
            (Inner::Sentinel(a), Inner::Sentinel(b)) => a.same(b),
            _ => Rc::ptr_eq(&self.0, &other.0),
        }
    }
    /// `err.Error()`
    pub fn Error(&self) -> String {
        self.to_string()
    }
}

impl PartialEq for GoError {
    fn eq(&self, other: &Self) -> bool {
        self.ptr_eq(other)
    }
}

impl From<&'static Sentinel> for GoError {
    fn from(s: &'static Sentinel) -> GoError {
        s.err()
    }
}

impl fmt::Display for GoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &*self.0 {
            Inner::Sentinel(s) => f.write_str(s.msg),
            Inner::Msg(m) => f.write_str(m),
            Inner::Wrap { msg, .. } => f.write_str(msg),
            Inner::Join(errs) => {
                for (i, e) in errs.iter().enumerate() {
                    if i > 0 {
                        f.write_str("\n")?;
                    }
                    write!(f, "{}", e)?;
                }
                Ok(())
            }
        }
    }
}

impl fmt::Debug for GoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "GoError({:?})", self.to_string())
    }
}

impl std::error::Error for GoError {}

/// `errors.Is(err, target)` for a possibly-nil `err`.
#[inline]
pub fn Is(err: Option<&GoError>, target: &Sentinel) -> bool {
    match err {
        Some(e) => e.Is(target),
        None => false,
    }
}

/// `errors.Is` on a result: true iff it is `Err(e)` with `e` matching `target`.
#[inline]
pub fn IsResult<T>(r: &GoResult<T>, target: &Sentinel) -> bool {
    match r {
        Err(e) => e.Is(target),
        Ok(_) => false,
    }
}

/// `errors.New(msg)`
#[inline]
pub fn New(msg: impl Into<String>) -> GoError {
    GoError::new(msg)
}

/// `errors.Join(errs...)`
#[inline]
pub fn Join<I: IntoIterator<Item = Option<GoError>>>(errs: I) -> Option<GoError> {
    GoError::join(errs)
}

/// `fmt.Errorf` without `%w`: `errorf!("x {}", y)` → `GoError::new(format!(...))`.
#[macro_export]
macro_rules! errorf {
    ($($arg:tt)*) => { $crate::GoError::new(format!($($arg)*)) };
}

/// `fmt.Errorf` with one `%w`: `wrapf!(err, "{}: {}", loc, err)`; the format must
/// produce the full message (include `err` where Go has `%w`).
#[macro_export]
macro_rules! wrapf {
    ($err:expr, $($arg:tt)*) => { $crate::GoError::wrap(format!($($arg)*), ($err).clone()) };
}
