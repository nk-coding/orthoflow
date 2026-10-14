// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/limits/work_test.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;
use gocompat::context::{self, CancelFunc, Ctx};

struct CancelCase {
    name: &'static str,
    stride: i64,
    newContext: fn() -> (Ctx, CancelFunc),
}

fn workGuardCancelCases() -> Vec<CancelCase> {
    vec![
        CancelCase { name: "standard context", stride: cancellableContextCheckStride, newContext: || context::WithCancel(&context::Background()) },
        CancelCase { name: "nil Done context", stride: contextCheckStride, newContext: || context::WithErrOnlyCancel(&context::Background()) },
    ]
}

#[test]
fn TestWorkGuardPreservesLimitAndCancellationErrors() {
    let guard = NewWorkGuard(&context::Background(), "test", 2).unwrap();
    guard.Step().unwrap();
    guard.Step().unwrap();
    let err = guard.Step().unwrap_err();
    assert!(err.Error().contains("TALA test work exceeds limit 2"));

    let (canceled, cancel) = context::WithCancel(&context::Background());
    cancel();
    let err = NewWorkGuard(&canceled, "canceled test", 2).unwrap_err();
    assert!(err.Is(&context::Canceled));
    assert!(err.Error().contains("canceled test"));
}

#[test]
fn TestWorkGuardRejectsInvalidOrExceededZeroLimit() {
    let err = NewWorkGuard(&context::Background(), "negative test", -1).unwrap_err();
    assert!(err.Error().contains("work limit must not be negative"));
    let guard = NewWorkGuard(&context::Background(), "zero test", 0).unwrap();
    assert!(guard.Step().unwrap_err().Error().contains("work exceeds limit 0"));
}

#[test]
fn TestWorkGuardAddObservesCrossedCancellationStride() {
    for tt in workGuardCancelCases() {
        for (cname, limit) in [("within limit", tt.stride + 1), ("overflow", tt.stride)] {
            let (ctx, cancel) = (tt.newContext)();
            let guard = NewWorkGuard(&ctx, "crossed stride", limit).unwrap();
            guard.Step().unwrap();
            cancel();
            let err = guard.Add(tt.stride).unwrap_err();
            assert!(err.Is(&context::Canceled), "{}/{}: {}", tt.name, cname, err);
            assert!(err.Error().contains("crossed stride"));
            assert_eq!(guard.Used(), tt.stride + 1);
        }
    }
}

#[test]
fn TestWorkGuardAddLimitPrecedesCancellationWithoutAcceptedBoundary() {
    for cc in workGuardCancelCases() {
        let charges: [(&str, i64, i64, bool); 4] = [
            ("no accepted boundary", 1, cc.stride - 1, false),
            ("already at limit", cc.stride - 1, 1, false),
            ("already over limit", cc.stride, 0, true),
            ("huge charge", 1, i64::MAX, false),
        ];
        for (name, initialUnits, addUnits, initialOverLimit) in charges {
            let (ctx, cancel) = (cc.newContext)();
            let guard = NewWorkGuard(&ctx, "overflow ordering", cc.stride - 1).unwrap();
            let r = guard.Add(initialUnits);
            assert_eq!(r.is_err(), initialOverLimit, "{}/{}", cc.name, name);
            cancel();
            let err = guard.Add(addUnits).unwrap_err();
            assert!(err.Error().contains("TALA overflow ordering work exceeds limit"), "{}/{}: {}", cc.name, name, err);
            assert!(!err.Is(&context::Canceled));
            assert_eq!(guard.Used(), cc.stride);
        }
    }
}

#[test]
fn TestWorkGuardAddZeroPreservesBoundaryPolling() {
    for (steps, wantCanceled) in [(0, true), (1, false)] {
        let (ctx, cancel) = context::WithCancel(&context::Background());
        let guard = NewWorkGuard(&ctx, "zero charge", 10_000).unwrap();
        for _ in 0..steps {
            guard.Step().unwrap();
        }
        cancel();
        let r = guard.Add(0);
        if wantCanceled {
            assert!(r.unwrap_err().Is(&context::Canceled));
        } else {
            assert!(r.is_ok());
        }
        assert_eq!(guard.Used(), steps);
    }
}

#[test]
fn TestCheckedUint64Arithmetic() {
    assert_eq!(CheckedAddUint64(40, 2), (42, true));
    assert!(!CheckedAddUint64(u64::MAX, 1).1);
    assert_eq!(CheckedMulUint64(21, 2), (42, true));
    assert!(!CheckedMulUint64(u64::MAX, 2).1);
}
