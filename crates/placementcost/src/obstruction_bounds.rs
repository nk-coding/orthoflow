// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/placementcost/obstruction_bounds.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

/// obstructionBounds encloses the straight, L-shaped, and alternate midpoint
/// routes considered by placement scoring. It is only a rejection filter.
#[derive(Clone, Copy, Default)]
pub(crate) struct obstructionBounds {
    pub(crate) left: f64,
    pub(crate) top: f64,
    pub(crate) right: f64,
    pub(crate) bottom: f64,
    pub(crate) usable: bool,
}

const scoringBound: f64 = f64::MAX / 8.0;

pub(crate) fn scoringNodeBounds(node: Option<&NodeRef>) -> obstructionBounds {
    let Some(node) = node else {
        return obstructionBounds::default();
    };
    let Some(tl) = node.tl() else {
        return obstructionBounds::default();
    };
    let p = *tl.b();
    let (x, y) = (p.X, p.Y);
    let (right, bottom) = (x + node.w(), y + node.h());
    let bound = scoringBound;
    let usable = x >= -bound
        && x <= bound
        && y >= -bound
        && y <= bound
        && right >= -bound
        && right <= bound
        && bottom >= -bound
        && bottom <= bound;
    obstructionBounds {
        left: math::BuiltinMin(x, right),
        top: math::BuiltinMin(y, bottom),
        right: math::BuiltinMax(x, right),
        bottom: math::BuiltinMax(y, bottom),
        usable,
    }
}

impl obstructionBounds {
    pub(crate) fn including(&self, other: obstructionBounds) -> obstructionBounds {
        obstructionBounds {
            left: math::BuiltinMin(self.left, other.left),
            top: math::BuiltinMin(self.top, other.top),
            right: math::BuiltinMax(self.right, other.right),
            bottom: math::BuiltinMax(self.bottom, other.bottom),
            usable: self.usable && other.usable,
        }
    }

    pub(crate) fn excludes(&self, node: Option<&NodeRef>) -> bool {
        if !self.usable {
            return false;
        }
        let Some(node) = node else {
            return false;
        };
        let Some(tl) = node.tl() else {
            return false;
        };
        let p = *tl.b();
        let (x, y) = (p.X, p.Y);
        let (right, bottom) = (x + node.w(), y + node.h());
        if x.is_infinite() || y.is_infinite() || right.is_infinite() || bottom.is_infinite() || x.is_nan() || y.is_nan() || right.is_nan() || bottom.is_nan() {
            return false;
        }
        (x < self.left && right < self.left)
            || (x > self.right && right > self.right)
            || (y < self.top && bottom < self.top)
            || (y > self.bottom && bottom > self.bottom)
    }
}
