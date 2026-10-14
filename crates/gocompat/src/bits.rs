// SPDX-License-Identifier: BSD-3-Clause
// Copyright 2009 The Go Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSES file.
//
// Ported from Go 1.27.2 (https://go.dev/src):
//   - src/math/bits/bits.go
// Rust port: Copyright 2026 Niklas Krieger.

//! `math/bits`
/// `bits.Len64(x)`: minimum number of bits to represent x.
#[inline]
pub fn Len64(x: u64) -> i64 {
    64 - x.leading_zeros() as i64
}
/// `bits.Len(uint(x))` (Go uint is 64-bit).
#[inline]
pub fn Len(x: u64) -> i64 {
    Len64(x)
}
#[inline]
pub fn LeadingZeros64(x: u64) -> i64 {
    x.leading_zeros() as i64
}
#[inline]
pub fn TrailingZeros64(x: u64) -> i64 {
    x.trailing_zeros() as i64
}
#[inline]
pub fn OnesCount64(x: u64) -> i64 {
    x.count_ones() as i64
}
/// `bits.Mul64(x, y) (hi, lo)`
#[inline]
pub fn Mul64(x: u64, y: u64) -> (u64, u64) {
    let p = (x as u128) * (y as u128);
    ((p >> 64) as u64, p as u64)
}
/// `bits.Add64(x, y, carry) (sum, carryOut)`
#[inline]
pub fn Add64(x: u64, y: u64, carry: u64) -> (u64, u64) {
    let sum = x.wrapping_add(y).wrapping_add(carry);
    let carry_out = ((x & y) | ((x | y) & !sum)) >> 63;
    (sum, carry_out)
}
