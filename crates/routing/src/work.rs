// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/routing/work.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of routing/work.go.
#[allow(unused_imports)]
use crate::*;
use gocompat::context::{Ctx, Done};
use gocompat::{GoError, GoResult};

/// cachedContextErr avoids repeatedly traversing context value wrappers in hot
/// guarded loops. Standard cancellable contexts close Done before Err becomes
/// observable; synthetic contexts without Done retain per-call Err polling.
#[inline]
pub(crate) fn cachedContextErr(ctx: &Ctx, done: &Option<Done>) -> Option<GoError> {
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

/// chargeSkippedRouteWork retains the logical work of a linear scan after its
/// irrelevant prefix has been found by binary search. Nonstandard budgets and
/// synthetic contexts retain exact step-by-step accounting and cancellation.
pub(crate) fn chargeSkippedRouteWork(guard: Option<&dyn workBudget>, amount: usize) -> GoResult<()> {
    let guard = match guard {
        None => return Ok(()),
        Some(g) => g,
    };
    if amount == 0 {
        return Ok(());
    }
    let search = guard.asRouteSearchWorkGuard();
    let exact = match search {
        None => true,
        Some(s) => {
            let s = s.b();
            s.done.is_none() || s.aggregate.is_some() && !s.deferAggregate
        }
    };
    if exact {
        for _ in 0..amount {
            guard.step()?;
        }
        return Ok(());
    }
    let search = search.unwrap();
    let units = amount as u64;
    let (used, limit) = {
        let s = search.b();
        (s.used, s.limit)
    };
    if used >= limit {
        return search.step();
    }
    let remaining = limit - used;
    if units > remaining {
        search.add(remaining)?;
        return search.step();
    }
    search.add(units)
}
