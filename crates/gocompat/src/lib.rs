// SPDX-License-Identifier: BSD-3-Clause
// Copyright 2026 Niklas Krieger
//
// Crate root of the Go standard library ports (licensed like the Go sources
// it accompanies; see the LICENSES file).

//! Go runtime / stdlib semantics shared by every crate of the TALA port.
//!
//! * [`go_ptr!`]: Go `*T` → `Rc<RefCell<T>>` newtype with pointer identity.
//! * [`Map`]/[`Set`]: deterministic hash maps (FxHash).
//! * [`GoError`], [`Sentinel`], [`GoResult`]: Go `error` values.
//! * [`Ctx`] (module [`context`]): `context.Context`.
//! * [`rand`]: exact port of `math/rand` v1.
//! * [`sort`] (and [`slices`]): exact ports of Go's pdqsort / stable sorts / searches.
//! * [`math`]: exact ports of Go `math` as executed natively on linux/amd64 (AVX+FMA).
//! * [`bits`]: `math/bits` helpers.
#![allow(non_snake_case, non_camel_case_types, non_upper_case_globals, dead_code, unused_imports, clippy::all)]

pub mod bits;
pub mod context;
pub mod errors;
pub mod fxhash;
pub mod math;
pub mod rand;
pub mod slices;
pub mod sort;
pub mod strconv;

pub use context::{Ctx, CtxKey};
pub use errors::{GoError, GoResult, Sentinel};
pub use fxhash::{FxBuildHasher, FxHasher};

/// Go `map[K]V` (iteration order is deterministic run-to-run but unrelated to Go's).
pub type Map<K, V> = std::collections::HashMap<K, V, FxBuildHasher>;
/// Go `map[K]struct{}` / `map[K]bool` used as a set.
pub type Set<K> = std::collections::HashSet<K, FxBuildHasher>;

/// `make(map[K]V)`
pub fn NewMap<K, V>() -> Map<K, V> {
    Map::default()
}
/// `make(map[K]V, n)`
pub fn NewMapCap<K, V>(n: usize) -> Map<K, V> {
    Map::with_capacity_and_hasher(n, FxBuildHasher::default())
}
pub fn NewSet<K>() -> Set<K> {
    Set::default()
}
pub fn NewSetCap<K>(n: usize) -> Set<K> {
    Set::with_capacity_and_hasher(n, FxBuildHasher::default())
}

/// Declares a Go pointer type `*T` as a newtype over `Rc<RefCell<T>>`.
///
/// ```text
/// gocompat::go_ptr!(NodeRef, Node);
/// let n = NodeRef::new(Node { .. });
/// n.b().Width;            // read
/// n.bm().Width = 3.0;     // write
/// let alias = n.clone();  // pointer copy
/// assert!(alias == n);    // pointer equality
/// ```
#[macro_export]
macro_rules! go_ptr {
    ($name:ident, $t:ty) => {
        #[derive(Clone)]
        pub struct $name(pub ::std::rc::Rc<::std::cell::RefCell<$t>>);

        impl $name {
            #[inline]
            pub fn new(v: $t) -> Self {
                $name(::std::rc::Rc::new(::std::cell::RefCell::new(v)))
            }
            /// Shared borrow (`*p` read).
            #[inline]
            #[track_caller]
            pub fn b(&self) -> ::std::cell::Ref<'_, $t> {
                self.0.borrow()
            }
            /// Mutable borrow (`*p` write).
            #[inline]
            #[track_caller]
            pub fn bm(&self) -> ::std::cell::RefMut<'_, $t> {
                self.0.borrow_mut()
            }
            /// Address of the pointee (stable for the lifetime of the object).
            #[inline]
            pub fn addr(&self) -> usize {
                ::std::rc::Rc::as_ptr(&self.0) as *const () as usize
            }
            /// Go `p == q`.
            #[inline]
            pub fn ptr_eq(&self, other: &Self) -> bool {
                ::std::rc::Rc::ptr_eq(&self.0, &other.0)
            }
        }
        impl ::std::cmp::PartialEq for $name {
            #[inline]
            fn eq(&self, other: &Self) -> bool {
                ::std::rc::Rc::ptr_eq(&self.0, &other.0)
            }
        }
        impl ::std::cmp::Eq for $name {}
        impl ::std::hash::Hash for $name {
            #[inline]
            fn hash<H: ::std::hash::Hasher>(&self, state: &mut H) {
                state.write_usize(self.addr());
            }
        }
        impl ::std::fmt::Debug for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                write!(f, "{}({:#x})", stringify!($name), self.addr())
            }
        }
    };
}

/// Helper: Go `p1 == p2` for `Option<XRef>` (nil-aware pointer equality).
#[inline]
pub fn opt_ptr_eq<T: PartialEq>(a: &Option<T>, b: &Option<T>) -> bool {
    a == b
}
