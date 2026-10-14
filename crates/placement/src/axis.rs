// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/placement/axis.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of placement/axis.go
use crate::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct layoutAxis(pub u8);

pub const invalidAxis: layoutAxis = layoutAxis(0);
pub const horizontalAxis: layoutAxis = layoutAxis(1);
pub const verticalAxis: layoutAxis = layoutAxis(2);

impl layoutAxis {
    pub fn valid(self) -> bool {
        self == horizontalAxis || self == verticalAxis
    }

    pub fn isHorizontal(self) -> bool {
        self == horizontalAxis
    }

    pub fn opposite(self) -> layoutAxis {
        match self {
            horizontalAxis => verticalAxis,
            verticalAxis => horizontalAxis,
            _ => invalidAxis,
        }
    }
}

pub fn axisForArrangement(arrangement: &layoutgraph::ClusterArrangement) -> layoutAxis {
    if *arrangement == layoutgraph::Column {
        return horizontalAxis;
    }
    verticalAxis
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct traversalDirection(pub u8);

pub const invalidDirection: traversalDirection = traversalDirection(0);
pub const forwardDirection: traversalDirection = traversalDirection(1);
pub const backwardDirection: traversalDirection = traversalDirection(2);

impl traversalDirection {
    pub fn valid(self) -> bool {
        self == forwardDirection || self == backwardDirection
    }

    pub fn isForward(self) -> bool {
        self == forwardDirection
    }

    pub fn opposite(self) -> traversalDirection {
        match self {
            forwardDirection => backwardDirection,
            backwardDirection => forwardDirection,
            _ => invalidDirection,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn TestAxisOpposite() {
        assert_eq!(horizontalAxis.opposite(), verticalAxis);
        assert_eq!(verticalAxis.opposite(), horizontalAxis);
        assert_eq!(invalidAxis.opposite(), invalidAxis);
        assert!(!invalidAxis.valid());
        assert_eq!(forwardDirection.opposite(), backwardDirection);
        assert_eq!(invalidDirection.opposite(), invalidDirection);
    }
}
