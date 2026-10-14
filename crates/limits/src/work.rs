// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/limits/work.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;
use gocompat::context::{Ctx, Done};
use gocompat::{GoError, GoResult};

pub const MaxEngineNodes: i64 = 10_000;
pub const MaxEngineEdges: i64 = 50_000;
pub const MaxEngineRoutePoints: i64 = 1_000_000;
pub const MaxEngineTreeDepth: i64 = 256;
pub const MaxGraphSize: i64 = 30_000;

pub(crate) const contextCheckStride: i64 = 64;
pub(crate) const cancellableContextCheckStride: i64 = 1024;
pub(crate) const workUnitsPerEntity: i64 = 1_024;

pub const MaxEngineWorkUnits: i64 = (MaxEngineNodes + MaxEngineEdges) * workUnitsPerEntity;
pub const MaxTransactionWorkUnits: i64 = 1_000_000_000;
pub const MaxTransactionOverlapReferences: i64 = 4_000_000;
pub const MaxBinPackWorkUnits: i64 = 1_000_000_000;
pub const MaxPlaceTreesWorkUnits: i64 = 68_000_000;
pub const MaxLabelPlacementWorkUnits: i64 = 50_000_000;

/// Go `limits.WorkGuard`. Go always uses it through `*WorkGuard` → [`WorkGuardRef`];
/// the methods exist on both (`&mut self` on the struct, `&self` on the Ref).
#[derive(Clone)]
pub struct WorkGuard {
    pub(crate) ctx: Ctx,
    pub(crate) done: Option<Done>,
    pub(crate) location: String,
    pub(crate) used: i64,
    pub(crate) limit: i64,
}

gocompat::go_ptr!(WorkGuardRef, WorkGuard);

/// `limits.NewWorkGuard(ctx, location, limit)`
pub fn NewWorkGuard(ctx: &Ctx, location: &str, limit: i64) -> GoResult<WorkGuardRef> {
    if limit < 0 {
        return Err(GoError::new(format!("TALA {} work limit must not be negative", location)));
    }
    let guard = WorkGuard { ctx: ctx.clone(), done: ctx.Done(), location: location.to_string(), used: 0, limit };
    guard.Finish()?;
    Ok(WorkGuardRef::new(guard))
}

impl WorkGuard {
    pub fn Step(&mut self) -> GoResult<()> {
        self.used += 1;
        if self.used > self.limit {
            return Err(GoError::new(format!("TALA {} work exceeds limit {}", self.location, self.limit)));
        }
        self.checkAtStride()
    }

    pub fn Add(&mut self, units: i64) -> GoResult<()> {
        if units < 0 {
            return Err(GoError::new(format!("TALA {} work charge must not be negative", self.location)));
        }
        if units > self.limit || self.used > self.limit - units {
            let previous = self.used;
            self.used = self.limit + 1;
            let stride = self.pollingStride();
            if previous <= self.limit && previous / stride != self.limit / stride {
                self.Check()?;
            }
            return Err(GoError::new(format!("TALA {} work exceeds limit {}", self.location, self.limit)));
        }
        let previous = self.used;
        self.used += units;
        self.checkAfterAdd(previous)
    }

    pub(crate) fn checkAfterAdd(&self, previous: i64) -> GoResult<()> {
        let stride = self.pollingStride();
        if self.used % stride != 0 && previous / stride == self.used / stride {
            return Ok(());
        }
        self.Check()
    }

    pub(crate) fn checkAtStride(&self) -> GoResult<()> {
        let stride = self.pollingStride();
        if self.used % stride != 0 {
            return Ok(());
        }
        self.Check()
    }

    pub(crate) fn pollingStride(&self) -> i64 {
        if self.done.is_none() {
            return contextCheckStride;
        }
        cancellableContextCheckStride
    }

    pub fn Check(&self) -> GoResult<()> {
        if let Some(err) = cachedContextErr(&self.ctx, self.done.as_ref()) {
            return Err(GoError::wrap(format!("{}: {}", self.location, err), err));
        }
        Ok(())
    }

    pub fn Finish(&self) -> GoResult<()> {
        self.Check()
    }

    pub fn SetLimit(&mut self, limit: i64) {
        self.limit = limit;
    }

    pub fn Used(&self) -> i64 {
        self.used
    }
}

impl WorkGuardRef {
    #[inline]
    pub fn Step(&self) -> GoResult<()> {
        self.bm().Step()
    }
    #[inline]
    pub fn Add(&self, units: i64) -> GoResult<()> {
        self.bm().Add(units)
    }
    pub fn Check(&self) -> GoResult<()> {
        self.b().Check()
    }
    pub fn Finish(&self) -> GoResult<()> {
        self.b().Finish()
    }
    pub fn SetLimit(&self, limit: i64) {
        self.bm().SetLimit(limit)
    }
    pub fn Used(&self) -> i64 {
        self.b().Used()
    }
}

pub(crate) fn cachedContextErr(ctx: &Ctx, done: Option<&Done>) -> Option<GoError> {
    match done {
        None => ctx.Err(),
        Some(d) => {
            if d.IsClosed() {
                ctx.Err()
            } else {
                None
            }
        }
    }
}
