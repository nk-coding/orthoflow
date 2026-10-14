// SPDX-License-Identifier: BSD-3-Clause
// Copyright 2009 The Go Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSES file.
//
// Ported from Go 1.27.2 (https://go.dev/src):
//   - src/slices/sort.go
//   - src/slices/slices.go
// Rust port: Copyright 2026 Niklas Krieger.

//! Go `slices` package (Go-named wrappers; algorithms live in [`crate::sort`]).
pub use crate::sort::{CmpResult, GoOrdered, IsZero};
use crate::sort as s;

/// `slices.Sort`
pub fn Sort<T: GoOrdered>(x: &mut [T]) {
    s::SlicesSort(x)
}
/// `slices.SortFunc`
pub fn SortFunc<T, R: CmpResult>(x: &mut [T], cmp: impl FnMut(&T, &T) -> R) {
    s::SlicesSortFunc(x, cmp)
}
/// `slices.SortStableFunc`
pub fn SortStableFunc<T, R: CmpResult>(x: &mut [T], cmp: impl FnMut(&T, &T) -> R) {
    s::SlicesSortStableFunc(x, cmp)
}
/// `slices.BinarySearch`
pub fn BinarySearch<T: GoOrdered>(x: &[T], target: &T) -> (i64, bool) {
    s::SlicesBinarySearch(x, target)
}
/// `slices.BinarySearchFunc`
pub fn BinarySearchFunc<E, T: ?Sized, R: CmpResult + IsZero>(x: &[E], target: &T, cmp: impl FnMut(&E, &T) -> R) -> (i64, bool) {
    s::SlicesBinarySearchFunc(x, target, cmp)
}
/// `slices.Reverse`
pub fn Reverse<T>(x: &mut [T]) {
    s::SlicesReverse(x)
}
/// `slices.IsSorted`
pub fn IsSorted<T: GoOrdered>(x: &[T]) -> bool {
    s::SlicesIsSorted(x)
}
/// `slices.Contains`
pub fn Contains<T: PartialEq>(x: &[T], v: &T) -> bool {
    x.iter().any(|e| e == v)
}
/// `slices.Index` (-1 when absent)
pub fn Index<T: PartialEq>(x: &[T], v: &T) -> i64 {
    x.iter().position(|e| e == v).map(|p| p as i64).unwrap_or(-1)
}
/// `slices.IndexFunc` (-1 when absent)
pub fn IndexFunc<T>(x: &[T], f: impl FnMut(&T) -> bool) -> i64 {
    x.iter().position(f).map(|p| p as i64).unwrap_or(-1)
}
/// `slices.ContainsFunc`
pub fn ContainsFunc<T>(x: &[T], f: impl FnMut(&T) -> bool) -> bool {
    x.iter().any(f)
}
/// `slices.Equal` (uses `==`; for floats, NaN != NaN and -0 == +0 like Go)
pub fn Equal<T: PartialEq>(a: &[T], b: &[T]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x == y)
}
/// `slices.DeleteFunc` (in place, returns new length semantics as a Vec truncate)
pub fn DeleteFunc<T>(x: &mut Vec<T>, mut del: impl FnMut(&T) -> bool) {
    x.retain(|e| !del(e));
}
/// `cmp.Compare`
pub fn Compare<T: GoOrdered>(a: T, b: T) -> i64 {
    s::cmpCompare(a, b)
}
