// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/limits/optimization.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;
use gocompat::context::Ctx;
use gocompat::rand::Rand;
use gocompat::{GoError, GoResult, Sentinel};

pub const MaxOptimizationWorkUnits: u64 = 250_000_000;
pub(crate) const optimizationContextCheckStride: u64 = 64;

pub static ErrOptimizationResourceLimit: Sentinel = Sentinel::new("TALA optimization resource limit exceeded");

/// Go `limits.OptimizationWorkGuard` (used through `*OptimizationWorkGuard` →
/// [`OptimizationWorkGuardRef`]; methods on both).
#[derive(Clone)]
pub struct OptimizationWorkGuard {
    pub(crate) ctx: Ctx,
    pub(crate) location: String,
    pub(crate) used: u64,
    pub(crate) limit: u64,
}

gocompat::go_ptr!(OptimizationWorkGuardRef, OptimizationWorkGuard);

pub fn NewOptimizationWorkGuard(ctx: &Ctx, location: &str, limit: u64) -> GoResult<OptimizationWorkGuardRef> {
    let guard = OptimizationWorkGuard { ctx: ctx.clone(), location: location.to_string(), used: 0, limit };
    guard.Finish()?;
    Ok(OptimizationWorkGuardRef::new(guard))
}

fn resourceLimit(msg: String) -> GoError {
    GoError::wrap(format!("{}: {}", ErrOptimizationResourceLimit, msg), ErrOptimizationResourceLimit.err())
}

impl OptimizationWorkGuard {
    pub fn Step(&mut self) -> GoResult<()> {
        self.Add(1)
    }

    pub fn Add(&mut self, amount: u64) -> GoResult<()> {
        if amount == 0 {
            return self.Check();
        }
        if u64::MAX - self.used < amount {
            return Err(resourceLimit(format!("{} work arithmetic overflow", self.location)));
        }
        let previous = self.used;
        let next = previous + amount;
        if next > self.limit {
            return Err(resourceLimit(format!("TALA {} work exceeds limit {}", self.location, self.limit)));
        }
        self.used = next;
        if previous / optimizationContextCheckStride != next / optimizationContextCheckStride {
            return self.Check();
        }
        Ok(())
    }

    pub fn AddProduct(&mut self, a: u64, b: u64) -> GoResult<()> {
        if a != 0 && b > u64::MAX / a {
            return Err(resourceLimit(format!("{} work arithmetic overflow", self.location)));
        }
        self.Add(a.wrapping_mul(b))
    }

    pub fn AddSort(&mut self, length: i64) -> GoResult<()> {
        if length <= 1 {
            return self.Check();
        }
        self.AddProduct(length as u64, gocompat::bits::Len64((length - 1) as u64) as u64)
    }

    pub fn Check(&self) -> GoResult<()> {
        if let Some(err) = self.ctx.Err() {
            return Err(GoError::wrap(format!("{}: {}", self.location, err), err));
        }
        Ok(())
    }

    pub fn Finish(&self) -> GoResult<()> {
        self.Check()
    }

    pub fn Location(&self) -> String {
        self.location.clone()
    }

    pub fn Used(&self) -> u64 {
        self.used
    }
}

impl OptimizationWorkGuardRef {
    pub fn Step(&self) -> GoResult<()> {
        self.bm().Step()
    }
    pub fn Add(&self, amount: u64) -> GoResult<()> {
        self.bm().Add(amount)
    }
    pub fn AddProduct(&self, a: u64, b: u64) -> GoResult<()> {
        self.bm().AddProduct(a, b)
    }
    pub fn AddSort(&self, length: i64) -> GoResult<()> {
        self.bm().AddSort(length)
    }
    pub fn Check(&self) -> GoResult<()> {
        self.b().Check()
    }
    pub fn Finish(&self) -> GoResult<()> {
        self.b().Finish()
    }
    pub fn Location(&self) -> String {
        self.b().Location()
    }
    pub fn Used(&self) -> u64 {
        self.b().Used()
    }
}

/// `limits.Shuffle(values, random, guard)`: math/rand Shuffle with a work guard.
/// (Go's nil-`random` error path is not representable: `random` is non-nil here.)
pub fn Shuffle<T>(values: &mut [T], random: &mut Rand, guard: &mut OptimizationWorkGuard) -> GoResult<()> {
    let mut i = values.len() as i64 - 1;
    while i > (1i64 << 31) - 1 - 1 {
        guard.Step()?;
        let j = random.Int63n(i + 1);
        values.swap(i as usize, j as usize);
        i -= 1;
    }
    while i > 0 {
        let j = shuffleIndex(random, (i + 1) as i32, guard)?;
        values.swap(i as usize, j as usize);
        i -= 1;
    }
    Ok(())
}

pub(crate) fn shuffleIndex(random: &mut Rand, n: i32, guard: &mut OptimizationWorkGuard) -> GoResult<i32> {
    guard.Step()?;
    let mut v = random.Uint32();
    let mut product = (v as u64).wrapping_mul(n as u32 as u64);
    let mut low = product as u32;
    if low < n as u32 {
        let threshold = (n.wrapping_neg() as u32) % (n as u32);
        while low < threshold {
            guard.Step()?;
            v = random.Uint32();
            product = (v as u64).wrapping_mul(n as u32 as u64);
            low = product as u32;
        }
    }
    Ok((product >> 32) as i32)
}
