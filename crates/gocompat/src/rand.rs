// SPDX-License-Identifier: BSD-3-Clause
// Copyright 2009 The Go Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSES file.
//
// Ported from Go 1.27.2 (https://go.dev/src):
//   - src/math/rand/rand.go
//   - src/math/rand/rng.go
// Rust port: Copyright 2026 Niklas Krieger.

//! Exact port of Go `math/rand` (v1) with `rngSource`.
//!
//! ```text
//! use gocompat::rand;
//! let r: rand::RandRef = rand::New(rand::NewSource(seed)); // Go *rand.Rand (shared)
//! let x = r.bm().Intn(10);
//! let mut plain = rand::Rand::new(seed);                    // owned value
//! plain.Shuffle(n, |i, j| v.swap(i, j));
//! ```
#[path = "rand_cooked.rs"]
mod rand_cooked;
use rand_cooked::rngCooked;

const rngLen: usize = 607;
const rngTap: usize = 273;
const rngMax: u64 = 1 << 63;
const rngMask: u64 = rngMax - 1;
const int32max: i32 = i32::MAX;

fn seedrand(x: i32) -> i32 {
    const A: i32 = 48271;
    const Q: i32 = 44488;
    const R: i32 = 3399;
    let hi = x / Q;
    let lo = x % Q;
    let mut x = A.wrapping_mul(lo).wrapping_sub(R.wrapping_mul(hi));
    if x < 0 {
        x = x.wrapping_add(int32max);
    }
    x
}

/// Go `rngSource` (what `rand.NewSource` returns).
#[derive(Clone)]
pub struct Source {
    tap: i64,
    feed: i64,
    vec: Box<[i64; rngLen]>,
}

impl Source {
    pub fn Seed(&mut self, seed: i64) {
        self.tap = 0;
        self.feed = (rngLen - rngTap) as i64;

        let mut seed = seed % (int32max as i64);
        if seed < 0 {
            seed += int32max as i64;
        }
        if seed == 0 {
            seed = 89482311;
        }

        let mut x = seed as i32;
        let mut i: i64 = -20;
        while i < rngLen as i64 {
            x = seedrand(x);
            if i >= 0 {
                let mut u: i64 = (x as i64) << 40;
                x = seedrand(x);
                u ^= (x as i64) << 20;
                x = seedrand(x);
                u ^= x as i64;
                u ^= rngCooked[i as usize];
                self.vec[i as usize] = u;
            }
            i += 1;
        }
    }

    #[inline]
    pub fn Int63(&mut self) -> i64 {
        (self.Uint64() & rngMask) as i64
    }

    #[inline]
    pub fn Uint64(&mut self) -> u64 {
        self.tap -= 1;
        if self.tap < 0 {
            self.tap += rngLen as i64;
        }
        self.feed -= 1;
        if self.feed < 0 {
            self.feed += rngLen as i64;
        }
        let x = self.vec[self.feed as usize].wrapping_add(self.vec[self.tap as usize]);
        self.vec[self.feed as usize] = x;
        x as u64
    }
}

/// `rand.NewSource(seed)`
pub fn NewSource(seed: i64) -> Source {
    let mut s = Source { tap: 0, feed: 0, vec: Box::new([0; rngLen]) };
    s.Seed(seed);
    s
}

/// Go `rand.Rand` (with an `rngSource`).
#[derive(Clone)]
pub struct Rand {
    src: Source,
}

crate::go_ptr!(RandRef, Rand);

/// `rand.New(src)` → `*rand.Rand`.
pub fn New(src: Source) -> RandRef {
    RandRef::new(Rand { src })
}

/// Alias used in some ports.
pub type GoRand = Rand;

impl Rand {
    /// `rand.New(rand.NewSource(seed))` as an owned value.
    pub fn new(seed: i64) -> Rand {
        Rand { src: NewSource(seed) }
    }
    pub fn from_source(src: Source) -> Rand {
        Rand { src }
    }

    pub fn Seed(&mut self, seed: i64) {
        self.src.Seed(seed);
    }

    #[inline]
    pub fn Int63(&mut self) -> i64 {
        self.src.Int63()
    }

    #[inline]
    pub fn Uint32(&mut self) -> u32 {
        (self.Int63() >> 31) as u32
    }

    #[inline]
    pub fn Uint64(&mut self) -> u64 {
        // rngSource implements Source64.
        self.src.Uint64()
    }

    #[inline]
    pub fn Int31(&mut self) -> i32 {
        (self.Int63() >> 32) as i32
    }

    #[inline]
    pub fn Int(&mut self) -> i64 {
        let u = self.Int63() as u64;
        ((u << 1) >> 1) as i64
    }

    pub fn Int63n(&mut self, n: i64) -> i64 {
        if n <= 0 {
            panic!("invalid argument to Int63n");
        }
        if n & (n - 1) == 0 {
            return self.Int63() & (n - 1);
        }
        let max = ((1u64 << 63) - 1 - (1u64 << 63) % (n as u64)) as i64;
        let mut v = self.Int63();
        while v > max {
            v = self.Int63();
        }
        v % n
    }

    pub fn Int31n(&mut self, n: i32) -> i32 {
        if n <= 0 {
            panic!("invalid argument to Int31n");
        }
        if n & (n - 1) == 0 {
            return self.Int31() & (n - 1);
        }
        let max = ((1u32 << 31) - 1 - (1u32 << 31) % (n as u32)) as i32;
        let mut v = self.Int31();
        while v > max {
            v = self.Int31();
        }
        v % n
    }

    /// unexported `int31n` (Lemire), used by Shuffle.
    pub fn int31n(&mut self, n: i32) -> i32 {
        let mut v = self.Uint32();
        let mut prod = (v as u64).wrapping_mul(n as u64);
        let mut low = prod as u32;
        if low < n as u32 {
            let thresh = (n.wrapping_neg() as u32) % (n as u32);
            while low < thresh {
                v = self.Uint32();
                prod = (v as u64).wrapping_mul(n as u64);
                low = prod as u32;
            }
        }
        (prod >> 32) as i32
    }

    pub fn Intn(&mut self, n: i64) -> i64 {
        if n <= 0 {
            panic!("invalid argument to Intn");
        }
        if n <= (1i64 << 31) - 1 {
            return self.Int31n(n as i32) as i64;
        }
        self.Int63n(n)
    }

    pub fn Float64(&mut self) -> f64 {
        loop {
            let f = self.Int63() as f64 / 9223372036854775808.0;
            if f == 1.0 {
                continue;
            }
            return f;
        }
    }

    pub fn Float32(&mut self) -> f32 {
        loop {
            let f = self.Float64() as f32;
            if f == 1.0 {
                continue;
            }
            return f;
        }
    }

    pub fn Perm(&mut self, n: i64) -> Vec<i64> {
        let mut m = vec![0i64; n as usize];
        for i in 0..n {
            let j = self.Intn(i + 1);
            m[i as usize] = m[j as usize];
            m[j as usize] = i;
        }
        m
    }

    pub fn Shuffle(&mut self, n: i64, mut swap: impl FnMut(usize, usize)) {
        if n < 0 {
            panic!("invalid argument to Shuffle");
        }
        let mut i = n - 1;
        while i > (1i64 << 31) - 1 - 1 {
            let j = self.Int63n(i + 1);
            swap(i as usize, j as usize);
            i -= 1;
        }
        while i > 0 {
            let j = self.int31n((i + 1) as i32) as i64;
            swap(i as usize, j as usize);
            i -= 1;
        }
    }

    /// Convenience: `r.Shuffle(len(v), func(i, j) { v[i], v[j] = v[j], v[i] })`.
    pub fn ShuffleSlice<T>(&mut self, v: &mut [T]) {
        let n = v.len() as i64;
        self.Shuffle(n, |i, j| v.swap(i, j));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn known_values() {
        // Go: rand.New(rand.NewSource(1)).Int63() == 5577006791947779410
        let mut r = Rand::new(1);
        assert_eq!(r.Int63(), 5577006791947779410);
        assert_eq!(r.Int63(), 8674665223082153551);
    }
}
