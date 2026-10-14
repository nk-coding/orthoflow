// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from lib/geo/orientation.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

/// Go `geo.Orientation` (an int enum).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct Orientation(pub i64);

pub const TopLeft: Orientation = Orientation(0);
pub const TopRight: Orientation = Orientation(1);
pub const BottomLeft: Orientation = Orientation(2);
pub const BottomRight: Orientation = Orientation(3);
pub const Top: Orientation = Orientation(4);
pub const Right: Orientation = Orientation(5);
pub const Bottom: Orientation = Orientation(6);
pub const Left: Orientation = Orientation(7);
pub const NONE: Orientation = Orientation(8);

impl Orientation {
    pub fn ToString(&self) -> String {
        match *self {
            TopLeft => "TopLeft",
            TopRight => "TopRight",
            BottomLeft => "BottomLeft",
            BottomRight => "BottomRight",
            Top => "Top",
            Right => "Right",
            Bottom => "Bottom",
            Left => "Left",
            _ => "",
        }
        .to_string()
    }

    pub fn SameSide(&self, o2: Orientation) -> bool {
        let o1 = *self;
        let sides: [[Orientation; 3]; 4] = [
            [TopLeft, Top, TopRight],
            [BottomLeft, Bottom, BottomRight],
            [Left, TopLeft, BottomLeft],
            [Right, TopRight, BottomRight],
        ];
        for sameSides in sides.iter() {
            let isO1 = sameSides.iter().any(|s| *s == o1);
            if isO1 {
                for side in sameSides {
                    if *side == o2 {
                        return true;
                    }
                }
            }
        }
        false
    }

    pub fn IsDiagonal(&self) -> bool {
        let o = *self;
        o == TopLeft || o == TopRight || o == BottomLeft || o == BottomRight
    }

    pub fn IsHorizontal(&self) -> bool {
        *self == Left || *self == Right
    }

    pub fn IsVertical(&self) -> bool {
        *self == Top || *self == Bottom
    }

    pub fn GetOpposite(&self) -> Orientation {
        match *self {
            TopLeft => BottomRight,
            TopRight => BottomLeft,
            BottomLeft => TopRight,
            BottomRight => TopLeft,
            Top => Bottom,
            Bottom => Top,
            Right => Left,
            Left => Right,
            o => o,
        }
    }
}
