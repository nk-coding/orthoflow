// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/quality/score.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

/// Score preserves the original TALA penalty and separate area tie-breaker.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Score {
    pub Penalty: f64,
    pub Area: f64,
}

/// Evaluate adapts the original evaluator for bounded refinement callers.
/// Go returns the (zero) Score together with the error; Rust returns Err.
pub fn Evaluate(ctx: &Ctx, graph: Option<&GraphRef>) -> GoResult<Score> {
    let (penalty, area) = EvaluateWithArea(ctx, graph)?;
    Ok(Score { Penalty: penalty, Area: area })
}

impl Score {
    /// Compare orders penalties exactly, then areas.
    pub fn Compare(&self, other: Score) -> i64 {
        let comparison = compareNumber(self.Penalty, other.Penalty, false);
        if comparison != 0 {
            return comparison;
        }
        compareNumber(self.Area, other.Area, true)
    }
}

pub(crate) fn compareNumber(left: f64, right: f64, nonnegative: bool) -> i64 {
    let leftValid = finite(left) && (!nonnegative || left >= 0.0);
    let rightValid = finite(right) && (!nonnegative || right >= 0.0);
    if leftValid && !rightValid {
        return -1;
    }
    if !leftValid && rightValid {
        return 1;
    }
    if !leftValid {
        return 0;
    }
    // cmp.Compare on (finite) floats
    if left < right {
        -1
    } else if left > right {
        1
    } else {
        0
    }
}

pub(crate) fn finite(value: f64) -> bool {
    !math::IsNaN(value) && !math::IsInf(value, 0)
}
