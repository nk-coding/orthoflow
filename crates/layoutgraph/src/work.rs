// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/layoutgraph/work.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

// Keep the established engine-local names while the surrounding layout
// domains migrate independently. The values and their implementation now
// have one cycle-free owner.
pub(crate) const maxEngineNodes: i64 = limits::MaxEngineNodes as i64;
pub(crate) const maxEngineEdges: i64 = limits::MaxEngineEdges as i64;
pub(crate) const maxEngineWorkUnits: i64 = limits::MaxEngineWorkUnits as i64;
pub(crate) const maxTransactionWorkUnits: i64 = limits::MaxTransactionWorkUnits as i64;
pub(crate) const maxTransactionOverlapReferences: i64 = limits::MaxTransactionOverlapReferences as i64;
pub(crate) const maxBinPackWorkUnits: i64 = limits::MaxBinPackWorkUnits as i64;
pub(crate) const maxGraphSize: i64 = limits::MaxGraphSize as i64;
pub(crate) const maxCompactionCandidateCount: i64 = limits::MaxGraphSize as i64 + 3;

/// workStepper is the narrow accounting contract used by graph traversals.
/// In the port it is the same trait as [`SharedWorkStepper`] (Go interfaces
/// with identical method sets are interchangeable).
pub(crate) use crate::SharedWorkStepper as workStepper;

pub(crate) struct unboundedWorkStepper;

impl SharedWorkStepper for unboundedWorkStepper {
    fn Step(&self) -> GoResult<()> {
        Ok(())
    }
    fn Finish(&self) -> GoResult<()> {
        Ok(())
    }
}

pub(crate) static unboundedWork: unboundedWorkStepper = unboundedWorkStepper;

impl SharedWorkStepper for limits::WorkGuardRef {
    #[inline]
    fn Step(&self) -> GoResult<()> {
        limits::WorkGuardRef::Step(self)
    }
    #[inline]
    fn Finish(&self) -> GoResult<()> {
        limits::WorkGuardRef::Finish(self)
    }
}
