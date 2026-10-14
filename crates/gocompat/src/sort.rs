// SPDX-License-Identifier: BSD-3-Clause
// Copyright 2009 The Go Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSES file.
//
// Ported from Go 1.27.2 (https://go.dev/src):
//   - src/sort/sort.go
//   - src/sort/slice.go
//   - src/sort/search.go
//   - src/sort/zsortfunc.go
//   - src/sort/zsortinterface.go
//   - src/slices/sort.go
//   - src/slices/zsortanyfunc.go
//   - src/slices/zsortordered.go
// Rust port: Copyright 2026 Niklas Krieger.

//! Exact ports of Go's `sort` package and the sorting part of `slices`
//! (`sort/zsortfunc.go`, `slices/zsortanyfunc.go`, `slices/zsortordered.go`,
//! which share one algorithm and therefore one implementation here).
//!
//! | Go | Rust |
//! |---|---|
//! | `sort.Slice(x, func(i, j int) bool { return x[i].K < x[j].K })` | `SortSlice(&mut x, \|a, b\| a.K < b.K)` |
//! | same, when `less` needs indices | `SortSliceIdx(&mut x, \|x, i, j\| ...)` |
//! | `sort.SliceStable` | `SortSliceStable` |
//! | `sort.Sort(data)` / `sort.Stable(data)` | `Sort(&mut data)` / `Stable(&mut data)` with `impl Interface` |
//! | `slices.SortFunc(x, cmp)` | `SlicesSortFunc(&mut x, \|a, b\| cmp)` (cmp returns i64/i32/Ordering) |
//! | `slices.SortStableFunc` | `SlicesSortStableFunc` |
//! | `slices.Sort(x)`, `sort.Ints`, `sort.Float64s` | `SlicesSort`, `Ints`, `Float64s` |
//! | `sort.Search(n, f)` | `Search(n, f)` |
//! | `slices.BinarySearch(Func)` | `SlicesBinarySearch(Func)` |
//! | `slices.Reverse` | `SlicesReverse` |
use std::cmp::Ordering;

/// Go `sort.Interface`.
pub trait Interface {
    fn Len(&self) -> i64;
    fn Less(&mut self, i: usize, j: usize) -> bool;
    fn Swap(&mut self, i: usize, j: usize);
}

/// The `lessSwap` pair used by the generic algorithm.
pub trait LessSwap {
    fn less(&mut self, i: isize, j: isize) -> bool;
    fn swap(&mut self, i: isize, j: isize);
}

struct IfaceLS<'a, D: Interface + ?Sized>(&'a mut D);
impl<'a, D: Interface + ?Sized> LessSwap for IfaceLS<'a, D> {
    #[inline]
    fn less(&mut self, i: isize, j: isize) -> bool {
        self.0.Less(i as usize, j as usize)
    }
    #[inline]
    fn swap(&mut self, i: isize, j: isize) {
        self.0.Swap(i as usize, j as usize)
    }
}

struct ElemLS<'a, T, F: FnMut(&T, &T) -> bool> {
    v: &'a mut [T],
    less: F,
}
impl<'a, T, F: FnMut(&T, &T) -> bool> LessSwap for ElemLS<'a, T, F> {
    #[inline]
    fn less(&mut self, i: isize, j: isize) -> bool {
        (self.less)(&self.v[i as usize], &self.v[j as usize])
    }
    #[inline]
    fn swap(&mut self, i: isize, j: isize) {
        self.v.swap(i as usize, j as usize)
    }
}

struct IdxLS<'a, T, F: FnMut(&[T], usize, usize) -> bool> {
    v: &'a mut [T],
    less: F,
}
impl<'a, T, F: FnMut(&[T], usize, usize) -> bool> LessSwap for IdxLS<'a, T, F> {
    #[inline]
    fn less(&mut self, i: isize, j: isize) -> bool {
        (self.less)(&*self.v, i as usize, j as usize)
    }
    #[inline]
    fn swap(&mut self, i: isize, j: isize) {
        self.v.swap(i as usize, j as usize)
    }
}

/// Result of a Go `cmp` function: negative / zero / positive.
pub trait CmpResult {
    fn is_neg(&self) -> bool;
}
impl CmpResult for i64 {
    #[inline]
    fn is_neg(&self) -> bool {
        *self < 0
    }
}
impl CmpResult for i32 {
    #[inline]
    fn is_neg(&self) -> bool {
        *self < 0
    }
}
impl CmpResult for isize {
    #[inline]
    fn is_neg(&self) -> bool {
        *self < 0
    }
}
impl CmpResult for Ordering {
    #[inline]
    fn is_neg(&self) -> bool {
        *self == Ordering::Less
    }
}

/// Go `cmp.Ordered` types with `cmp.Less` / `cmp.Compare` semantics (NaN sorts first).
pub trait GoOrdered {
    fn cmpLess(a: &Self, b: &Self) -> bool;
    fn cmpCompare(a: &Self, b: &Self) -> i64;
}
macro_rules! ordered_int {
    ($($t:ty),*) => {$(
        impl GoOrdered for $t {
            #[inline] fn cmpLess(a: &Self, b: &Self) -> bool { a < b }
            #[inline] fn cmpCompare(a: &Self, b: &Self) -> i64 { if a < b { -1 } else if a > b { 1 } else { 0 } }
        }
    )*};
}
ordered_int!(i8, i16, i32, i64, isize, u8, u16, u32, u64, usize, String, &str);
macro_rules! ordered_float {
    ($($t:ty),*) => {$(
        impl GoOrdered for $t {
            #[inline] fn cmpLess(a: &Self, b: &Self) -> bool { (a.is_nan() && !b.is_nan()) || a < b }
            #[inline] fn cmpCompare(a: &Self, b: &Self) -> i64 {
                let xn = a.is_nan(); let yn = b.is_nan();
                if xn { if yn { return 0; } return -1; }
                if yn { return 1; }
                if a < b { -1 } else if a > b { 1 } else { 0 }
            }
        }
    )*};
}
ordered_float!(f64, f32);

/// `cmp.Less(a, b)`
#[inline]
pub fn cmpLess<T: GoOrdered>(a: T, b: T) -> bool {
    T::cmpLess(&a, &b)
}
/// `cmp.Compare(a, b)`
#[inline]
pub fn cmpCompare<T: GoOrdered>(a: T, b: T) -> i64 {
    T::cmpCompare(&a, &b)
}

#[inline]
fn bitsLen(x: usize) -> isize {
    (usize::BITS - x.leading_zeros()) as isize
}

// ---------------------------------------------------------------- algorithm

const unknownHint: i32 = 0;
const increasingHint: i32 = 1;
const decreasingHint: i32 = 2;

struct xorshift(u64);
impl xorshift {
    #[inline]
    fn Next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

#[inline]
fn nextPowerOfTwo(length: isize) -> u64 {
    let shift = bitsLen(length as usize) as u32;
    1u64 << shift
}

fn insertionSort<D: LessSwap + ?Sized>(data: &mut D, a: isize, b: isize) {
    let mut i = a + 1;
    while i < b {
        let mut j = i;
        while j > a && data.less(j, j - 1) {
            data.swap(j, j - 1);
            j -= 1;
        }
        i += 1;
    }
}

fn siftDown<D: LessSwap + ?Sized>(data: &mut D, lo: isize, hi: isize, first: isize) {
    let mut root = lo;
    loop {
        let mut child = 2 * root + 1;
        if child >= hi {
            break;
        }
        if child + 1 < hi && data.less(first + child, first + child + 1) {
            child += 1;
        }
        if !data.less(first + root, first + child) {
            return;
        }
        data.swap(first + root, first + child);
        root = child;
    }
}

fn heapSort<D: LessSwap + ?Sized>(data: &mut D, a: isize, b: isize) {
    let first = a;
    let lo = 0;
    let hi = b - a;
    let mut i = (hi - 1) / 2;
    while i >= 0 {
        siftDown(data, i, hi, first);
        i -= 1;
    }
    let mut i = hi - 1;
    while i >= 0 {
        data.swap(first, first + i);
        siftDown(data, lo, i, first);
        i -= 1;
    }
}

fn pdqsort<D: LessSwap + ?Sized>(data: &mut D, mut a: isize, mut b: isize, mut limit: isize) {
    const maxInsertion: isize = 12;
    let mut wasBalanced = true;
    let mut wasPartitioned = true;
    loop {
        let length = b - a;
        if length <= maxInsertion {
            insertionSort(data, a, b);
            return;
        }
        if limit == 0 {
            heapSort(data, a, b);
            return;
        }
        if !wasBalanced {
            breakPatterns(data, a, b);
            limit -= 1;
        }
        let (mut pivot, mut hint) = choosePivot(data, a, b);
        if hint == decreasingHint {
            reverseRange(data, a, b);
            pivot = (b - 1) - (pivot - a);
            hint = increasingHint;
        }
        if wasBalanced && wasPartitioned && hint == increasingHint {
            if partialInsertionSort(data, a, b) {
                return;
            }
        }
        if a > 0 && !data.less(a - 1, pivot) {
            let mid = partitionEqual(data, a, b, pivot);
            a = mid;
            continue;
        }
        let (mid, alreadyPartitioned) = partition(data, a, b, pivot);
        wasPartitioned = alreadyPartitioned;
        let (leftLen, rightLen) = (mid - a, b - mid);
        let balanceThreshold = length / 8;
        if leftLen < rightLen {
            wasBalanced = leftLen >= balanceThreshold;
            pdqsort(data, a, mid, limit);
            a = mid + 1;
        } else {
            wasBalanced = rightLen >= balanceThreshold;
            pdqsort(data, mid + 1, b, limit);
            b = mid;
        }
    }
}

fn partition<D: LessSwap + ?Sized>(data: &mut D, a: isize, b: isize, pivot: isize) -> (isize, bool) {
    data.swap(a, pivot);
    let (mut i, mut j) = (a + 1, b - 1);
    while i <= j && data.less(i, a) {
        i += 1;
    }
    while i <= j && !data.less(j, a) {
        j -= 1;
    }
    if i > j {
        data.swap(j, a);
        return (j, true);
    }
    data.swap(i, j);
    i += 1;
    j -= 1;
    loop {
        while i <= j && data.less(i, a) {
            i += 1;
        }
        while i <= j && !data.less(j, a) {
            j -= 1;
        }
        if i > j {
            break;
        }
        data.swap(i, j);
        i += 1;
        j -= 1;
    }
    data.swap(j, a);
    (j, false)
}

fn partitionEqual<D: LessSwap + ?Sized>(data: &mut D, a: isize, b: isize, pivot: isize) -> isize {
    data.swap(a, pivot);
    let (mut i, mut j) = (a + 1, b - 1);
    loop {
        while i <= j && !data.less(a, i) {
            i += 1;
        }
        while i <= j && data.less(a, j) {
            j -= 1;
        }
        if i > j {
            break;
        }
        data.swap(i, j);
        i += 1;
        j -= 1;
    }
    i
}

fn partialInsertionSort<D: LessSwap + ?Sized>(data: &mut D, a: isize, b: isize) -> bool {
    const maxSteps: isize = 5;
    const shortestShifting: isize = 50;
    let mut i = a + 1;
    for _ in 0..maxSteps {
        while i < b && !data.less(i, i - 1) {
            i += 1;
        }
        if i == b {
            return true;
        }
        if b - a < shortestShifting {
            return false;
        }
        data.swap(i, i - 1);
        if i - a >= 2 {
            let mut j = i - 1;
            while j >= 1 {
                if !data.less(j, j - 1) {
                    break;
                }
                data.swap(j, j - 1);
                j -= 1;
            }
        }
        if b - i >= 2 {
            let mut j = i + 1;
            while j < b {
                if !data.less(j, j - 1) {
                    break;
                }
                data.swap(j, j - 1);
                j += 1;
            }
        }
    }
    false
}

fn breakPatterns<D: LessSwap + ?Sized>(data: &mut D, a: isize, b: isize) {
    let length = b - a;
    if length >= 8 {
        let mut random = xorshift(length as u64);
        let modulus = nextPowerOfTwo(length);
        let mut idx = a + (length / 4) * 2 - 1;
        while idx <= a + (length / 4) * 2 + 1 {
            let mut other = (random.Next() & (modulus.wrapping_sub(1))) as isize;
            if other >= length {
                other -= length;
            }
            data.swap(idx, a + other);
            idx += 1;
        }
    }
}

fn choosePivot<D: LessSwap + ?Sized>(data: &mut D, a: isize, b: isize) -> (isize, i32) {
    const shortestNinther: isize = 50;
    const maxSwaps: i64 = 4 * 3;
    let l = b - a;
    let mut swaps: i64 = 0;
    let mut i = a + l / 4 * 1;
    let mut j = a + l / 4 * 2;
    let mut k = a + l / 4 * 3;
    if l >= 8 {
        if l >= shortestNinther {
            i = medianAdjacent(data, i, &mut swaps);
            j = medianAdjacent(data, j, &mut swaps);
            k = medianAdjacent(data, k, &mut swaps);
        }
        j = median(data, i, j, k, &mut swaps);
    }
    if swaps == 0 {
        (j, increasingHint)
    } else if swaps == maxSwaps {
        (j, decreasingHint)
    } else {
        (j, unknownHint)
    }
}

#[inline]
fn order2<D: LessSwap + ?Sized>(data: &mut D, a: isize, b: isize, swaps: &mut i64) -> (isize, isize) {
    if data.less(b, a) {
        *swaps += 1;
        return (b, a);
    }
    (a, b)
}

#[inline]
fn median<D: LessSwap + ?Sized>(data: &mut D, a: isize, b: isize, c: isize, swaps: &mut i64) -> isize {
    let (a, b) = order2(data, a, b, swaps);
    let (b, c) = order2(data, b, c, swaps);
    let _ = c;
    let (_a, b) = order2(data, a, b, swaps);
    b
}

#[inline]
fn medianAdjacent<D: LessSwap + ?Sized>(data: &mut D, a: isize, swaps: &mut i64) -> isize {
    median(data, a - 1, a, a + 1, swaps)
}

fn reverseRange<D: LessSwap + ?Sized>(data: &mut D, a: isize, b: isize) {
    let mut i = a;
    let mut j = b - 1;
    while i < j {
        data.swap(i, j);
        i += 1;
        j -= 1;
    }
}

fn swapRange<D: LessSwap + ?Sized>(data: &mut D, a: isize, b: isize, n: isize) {
    for i in 0..n {
        data.swap(a + i, b + i);
    }
}

fn stable<D: LessSwap + ?Sized>(data: &mut D, n: isize) {
    let mut blockSize: isize = 20;
    let (mut a, mut b) = (0, blockSize);
    while b <= n {
        insertionSort(data, a, b);
        a = b;
        b += blockSize;
    }
    insertionSort(data, a, n);
    while blockSize < n {
        a = 0;
        b = 2 * blockSize;
        while b <= n {
            symMerge(data, a, a + blockSize, b);
            a = b;
            b += 2 * blockSize;
        }
        let m = a + blockSize;
        if m < n {
            symMerge(data, a, m, n);
        }
        blockSize *= 2;
    }
}

fn symMerge<D: LessSwap + ?Sized>(data: &mut D, a: isize, m: isize, b: isize) {
    if m - a == 1 {
        let mut i = m;
        let mut j = b;
        while i < j {
            let h = ((i + j) as usize >> 1) as isize;
            if data.less(h, a) {
                i = h + 1;
            } else {
                j = h;
            }
        }
        let mut k = a;
        while k < i - 1 {
            data.swap(k, k + 1);
            k += 1;
        }
        return;
    }
    if b - m == 1 {
        let mut i = a;
        let mut j = m;
        while i < j {
            let h = ((i + j) as usize >> 1) as isize;
            if !data.less(m, h) {
                i = h + 1;
            } else {
                j = h;
            }
        }
        let mut k = m;
        while k > i {
            data.swap(k, k - 1);
            k -= 1;
        }
        return;
    }
    let mid = ((a + b) as usize >> 1) as isize;
    let n = mid + m;
    let (mut start, mut r);
    if m > mid {
        start = n - b;
        r = mid;
    } else {
        start = a;
        r = m;
    }
    let p = n - 1;
    while start < r {
        let c = ((start + r) as usize >> 1) as isize;
        if !data.less(p - c, c) {
            start = c + 1;
        } else {
            r = c;
        }
    }
    let end = n - start;
    if start < m && m < end {
        rotate(data, start, m, end);
    }
    if a < start && start < mid {
        symMerge(data, a, start, mid);
    }
    if mid < end && end < b {
        symMerge(data, mid, end, b);
    }
}

fn rotate<D: LessSwap + ?Sized>(data: &mut D, a: isize, m: isize, b: isize) {
    let mut i = m - a;
    let mut j = b - m;
    while i != j {
        if i > j {
            swapRange(data, m - i, m, j);
            i -= j;
        } else {
            swapRange(data, m - i, m + j - i, i);
            j -= i;
        }
    }
    swapRange(data, m - i, m, i);
}

// ---------------------------------------------------------------- entry points

/// Run Go's unstable pdqsort on any [`LessSwap`] (`limit = bits.Len(n)`).
pub fn PdqsortLessSwap<D: LessSwap + ?Sized>(data: &mut D, n: usize) {
    pdqsort(data, 0, n as isize, bitsLen(n));
}

/// Run Go's stable insertion+symMerge sort on any [`LessSwap`].
pub fn StableLessSwap<D: LessSwap + ?Sized>(data: &mut D, n: usize) {
    stable(data, n as isize);
}

/// `sort.Sort(data)`
pub fn Sort<D: Interface + ?Sized>(data: &mut D) {
    let n = data.Len();
    if n <= 1 {
        return;
    }
    let limit = bitsLen(n as usize);
    pdqsort(&mut IfaceLS(data), 0, n as isize, limit);
}

/// `sort.Stable(data)`
pub fn Stable<D: Interface + ?Sized>(data: &mut D) {
    let n = data.Len();
    stable(&mut IfaceLS(data), n as isize);
}

/// `sort.Slice(x, less)` with an element-wise less (`less(&x[i], &x[j])`).
pub fn SortSlice<T>(x: &mut [T], less: impl FnMut(&T, &T) -> bool) {
    let length = x.len();
    let limit = bitsLen(length);
    pdqsort(&mut ElemLS { v: x, less }, 0, length as isize, limit);
}

/// `sort.Slice(x, less)` with an index-based less (`less(&x, i, j)`).
pub fn SortSliceIdx<T>(x: &mut [T], less: impl FnMut(&[T], usize, usize) -> bool) {
    let length = x.len();
    let limit = bitsLen(length);
    pdqsort(&mut IdxLS { v: x, less }, 0, length as isize, limit);
}

/// `sort.SliceStable(x, less)` (element-wise less).
pub fn SortSliceStable<T>(x: &mut [T], less: impl FnMut(&T, &T) -> bool) {
    let n = x.len() as isize;
    stable(&mut ElemLS { v: x, less }, n);
}

/// `sort.SliceStable(x, less)` (index-based less).
pub fn SortSliceStableIdx<T>(x: &mut [T], less: impl FnMut(&[T], usize, usize) -> bool) {
    let n = x.len() as isize;
    stable(&mut IdxLS { v: x, less }, n);
}

/// `slices.SortFunc(x, cmp)`
pub fn SlicesSortFunc<T, R: CmpResult>(x: &mut [T], mut cmp: impl FnMut(&T, &T) -> R) {
    let n = x.len();
    pdqsort(&mut ElemLS { v: x, less: |a: &T, b: &T| cmp(a, b).is_neg() }, 0, n as isize, bitsLen(n));
}

/// `slices.SortStableFunc(x, cmp)`
pub fn SlicesSortStableFunc<T, R: CmpResult>(x: &mut [T], mut cmp: impl FnMut(&T, &T) -> R) {
    let n = x.len() as isize;
    stable(&mut ElemLS { v: x, less: |a: &T, b: &T| cmp(a, b).is_neg() }, n);
}

/// `slices.Sort(x)` for ordered element types (floats: NaNs first).
pub fn SlicesSort<T: GoOrdered>(x: &mut [T]) {
    let n = x.len();
    pdqsort(&mut ElemLS { v: x, less: |a: &T, b: &T| T::cmpLess(a, b) }, 0, n as isize, bitsLen(n));
}

/// `sort.Ints(x)`
pub fn Ints(x: &mut [i64]) {
    SlicesSort(x)
}

/// `sort.Float64s(x)`
pub fn Float64s(x: &mut [f64]) {
    SlicesSort(x)
}

/// `sort.Search(n, f)`
pub fn Search(n: i64, mut f: impl FnMut(i64) -> bool) -> i64 {
    let (mut i, mut j) = (0i64, n);
    while i < j {
        let h = ((i + j) as u64 >> 1) as i64;
        if !f(h) {
            i = h + 1;
        } else {
            j = h;
        }
    }
    i
}

/// `sort.SearchFloat64s(a, x)`
pub fn SearchFloat64s(a: &[f64], x: f64) -> i64 {
    Search(a.len() as i64, |i| a[i as usize] >= x)
}

/// `sort.SearchInts(a, x)`
pub fn SearchInts(a: &[i64], x: i64) -> i64 {
    Search(a.len() as i64, |i| a[i as usize] >= x)
}

/// `slices.BinarySearch(x, target)`
pub fn SlicesBinarySearch<T: GoOrdered>(x: &[T], target: &T) -> (i64, bool) {
    let n = x.len() as i64;
    let (mut i, mut j) = (0i64, n);
    while i < j {
        let h = ((i + j) as u64 >> 1) as i64;
        if T::cmpLess(&x[h as usize], target) {
            i = h + 1;
        } else {
            j = h;
        }
    }
    (i, i < n && (T::cmpCompare(&x[i as usize], target) == 0))
}

/// `slices.BinarySearchFunc(x, target, cmp)`
pub fn SlicesBinarySearchFunc<E, T: ?Sized, R: CmpResult + IsZero>(
    x: &[E],
    target: &T,
    mut cmp: impl FnMut(&E, &T) -> R,
) -> (i64, bool) {
    let n = x.len() as i64;
    let (mut i, mut j) = (0i64, n);
    while i < j {
        let h = ((i + j) as u64 >> 1) as i64;
        if cmp(&x[h as usize], target).is_neg() {
            i = h + 1;
        } else {
            j = h;
        }
    }
    (i, i < n && cmp(&x[i as usize], target).is_zero())
}

pub trait IsZero {
    fn is_zero(&self) -> bool;
}
impl IsZero for i64 {
    fn is_zero(&self) -> bool {
        *self == 0
    }
}
impl IsZero for i32 {
    fn is_zero(&self) -> bool {
        *self == 0
    }
}
impl IsZero for isize {
    fn is_zero(&self) -> bool {
        *self == 0
    }
}
impl IsZero for Ordering {
    fn is_zero(&self) -> bool {
        *self == Ordering::Equal
    }
}

/// `slices.Reverse(s)`
pub fn SlicesReverse<T>(s: &mut [T]) {
    let n = s.len();
    if n == 0 {
        return;
    }
    let (mut i, mut j) = (0usize, n - 1);
    while i < j {
        s.swap(i, j);
        i += 1;
        j -= 1;
    }
}

/// `slices.IsSorted(x)`
pub fn SlicesIsSorted<T: GoOrdered>(x: &[T]) -> bool {
    let mut i = x.len() as isize - 1;
    while i > 0 {
        if T::cmpLess(&x[i as usize], &x[i as usize - 1]) {
            return false;
        }
        i -= 1;
    }
    true
}

// Go-named aliases (`sort.Slice` → `sort::Slice`, etc.).
/// `sort.Slice`
pub fn Slice<T>(x: &mut [T], less: impl FnMut(&T, &T) -> bool) {
    SortSlice(x, less)
}
/// `sort.SliceStable`
pub fn SliceStable<T>(x: &mut [T], less: impl FnMut(&T, &T) -> bool) {
    SortSliceStable(x, less)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn basic() {
        let mut v: Vec<i64> = (0..300).map(|i| (i * 7919) % 101).collect();
        Ints(&mut v);
        assert!(v.windows(2).all(|w| w[0] <= w[1]));
        let mut f = vec![3.0, f64::NAN, 1.0, -0.0, 0.0];
        Float64s(&mut f);
        assert!(f[0].is_nan());
        let mut s: Vec<(i64, i64)> = (0..100).map(|i| (i % 5, i)).collect();
        SortSliceStable(&mut s, |a, b| a.0 < b.0);
        for w in s.windows(2) {
            assert!(w[0].0 < w[1].0 || (w[0].0 == w[1].0 && w[0].1 < w[1].1));
        }
        assert_eq!(Search(100, |i| i >= 37), 37);
    }
}
