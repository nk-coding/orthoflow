// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from lib/label/label.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;
use geo::{BoxRef, PointRef, Route, RouteExt};
use gocompat::math;

pub const LEFT_LABEL_POSITION: f64 = 1.0 / 4.0;
pub const CENTER_LABEL_POSITION: f64 = 2.0 / 4.0;
pub const RIGHT_LABEL_POSITION: f64 = 3.0 / 4.0;

pub const PADDING: i64 = 5;

/// Go `label.Position` (int8 enum).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct Position(pub i8);

pub const Unset: Position = Position(0);
pub const OutsideTopLeft: Position = Position(1);
pub const OutsideTopCenter: Position = Position(2);
pub const OutsideTopRight: Position = Position(3);
pub const OutsideLeftTop: Position = Position(4);
pub const OutsideLeftMiddle: Position = Position(5);
pub const OutsideLeftBottom: Position = Position(6);
pub const OutsideRightTop: Position = Position(7);
pub const OutsideRightMiddle: Position = Position(8);
pub const OutsideRightBottom: Position = Position(9);
pub const OutsideBottomLeft: Position = Position(10);
pub const OutsideBottomCenter: Position = Position(11);
pub const OutsideBottomRight: Position = Position(12);
pub const InsideTopLeft: Position = Position(13);
pub const InsideTopCenter: Position = Position(14);
pub const InsideTopRight: Position = Position(15);
pub const InsideMiddleLeft: Position = Position(16);
pub const InsideMiddleCenter: Position = Position(17);
pub const InsideMiddleRight: Position = Position(18);
pub const InsideBottomLeft: Position = Position(19);
pub const InsideBottomCenter: Position = Position(20);
pub const InsideBottomRight: Position = Position(21);
pub const BorderTopLeft: Position = Position(22);
pub const BorderTopCenter: Position = Position(23);
pub const BorderTopRight: Position = Position(24);
pub const BorderLeftTop: Position = Position(25);
pub const BorderLeftMiddle: Position = Position(26);
pub const BorderLeftBottom: Position = Position(27);
pub const BorderRightTop: Position = Position(28);
pub const BorderRightMiddle: Position = Position(29);
pub const BorderRightBottom: Position = Position(30);
pub const BorderBottomLeft: Position = Position(31);
pub const BorderBottomCenter: Position = Position(32);
pub const BorderBottomRight: Position = Position(33);
pub const UnlockedTop: Position = Position(34);
pub const UnlockedMiddle: Position = Position(35);
pub const UnlockedBottom: Position = Position(36);

pub fn FromString(s: &str) -> Position {
    match s {
        "OUTSIDE_TOP_LEFT" => OutsideTopLeft,
        "OUTSIDE_TOP_CENTER" => OutsideTopCenter,
        "OUTSIDE_TOP_RIGHT" => OutsideTopRight,
        "OUTSIDE_LEFT_TOP" => OutsideLeftTop,
        "OUTSIDE_LEFT_MIDDLE" => OutsideLeftMiddle,
        "OUTSIDE_LEFT_BOTTOM" => OutsideLeftBottom,
        "OUTSIDE_RIGHT_TOP" => OutsideRightTop,
        "OUTSIDE_RIGHT_MIDDLE" => OutsideRightMiddle,
        "OUTSIDE_RIGHT_BOTTOM" => OutsideRightBottom,
        "OUTSIDE_BOTTOM_LEFT" => OutsideBottomLeft,
        "OUTSIDE_BOTTOM_CENTER" => OutsideBottomCenter,
        "OUTSIDE_BOTTOM_RIGHT" => OutsideBottomRight,
        "INSIDE_TOP_LEFT" => InsideTopLeft,
        "INSIDE_TOP_CENTER" => InsideTopCenter,
        "INSIDE_TOP_RIGHT" => InsideTopRight,
        "INSIDE_MIDDLE_LEFT" => InsideMiddleLeft,
        "INSIDE_MIDDLE_CENTER" => InsideMiddleCenter,
        "INSIDE_MIDDLE_RIGHT" => InsideMiddleRight,
        "INSIDE_BOTTOM_LEFT" => InsideBottomLeft,
        "INSIDE_BOTTOM_CENTER" => InsideBottomCenter,
        "INSIDE_BOTTOM_RIGHT" => InsideBottomRight,
        "BORDER_TOP_LEFT" => BorderTopLeft,
        "BORDER_TOP_CENTER" => BorderTopCenter,
        "BORDER_TOP_RIGHT" => BorderTopRight,
        "BORDER_LEFT_TOP" => BorderLeftTop,
        "BORDER_LEFT_MIDDLE" => BorderLeftMiddle,
        "BORDER_LEFT_BOTTOM" => BorderLeftBottom,
        "BORDER_RIGHT_TOP" => BorderRightTop,
        "BORDER_RIGHT_MIDDLE" => BorderRightMiddle,
        "BORDER_RIGHT_BOTTOM" => BorderRightBottom,
        "BORDER_BOTTOM_LEFT" => BorderBottomLeft,
        "BORDER_BOTTOM_CENTER" => BorderBottomCenter,
        "BORDER_BOTTOM_RIGHT" => BorderBottomRight,
        "UNLOCKED_TOP" => UnlockedTop,
        "UNLOCKED_MIDDLE" => UnlockedMiddle,
        "UNLOCKED_BOTTOM" => UnlockedBottom,
        _ => Unset,
    }
}

impl std::fmt::Display for Position {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.String())
    }
}

impl Position {
    pub fn String(&self) -> String {
        match *self {
            OutsideTopLeft => "OUTSIDE_TOP_LEFT",
            OutsideTopCenter => "OUTSIDE_TOP_CENTER",
            OutsideTopRight => "OUTSIDE_TOP_RIGHT",
            OutsideLeftTop => "OUTSIDE_LEFT_TOP",
            OutsideLeftMiddle => "OUTSIDE_LEFT_MIDDLE",
            OutsideLeftBottom => "OUTSIDE_LEFT_BOTTOM",
            OutsideRightTop => "OUTSIDE_RIGHT_TOP",
            OutsideRightMiddle => "OUTSIDE_RIGHT_MIDDLE",
            OutsideRightBottom => "OUTSIDE_RIGHT_BOTTOM",
            OutsideBottomLeft => "OUTSIDE_BOTTOM_LEFT",
            OutsideBottomCenter => "OUTSIDE_BOTTOM_CENTER",
            OutsideBottomRight => "OUTSIDE_BOTTOM_RIGHT",
            InsideTopLeft => "INSIDE_TOP_LEFT",
            InsideTopCenter => "INSIDE_TOP_CENTER",
            InsideTopRight => "INSIDE_TOP_RIGHT",
            InsideMiddleLeft => "INSIDE_MIDDLE_LEFT",
            InsideMiddleCenter => "INSIDE_MIDDLE_CENTER",
            InsideMiddleRight => "INSIDE_MIDDLE_RIGHT",
            InsideBottomLeft => "INSIDE_BOTTOM_LEFT",
            InsideBottomCenter => "INSIDE_BOTTOM_CENTER",
            InsideBottomRight => "INSIDE_BOTTOM_RIGHT",
            BorderTopLeft => "BORDER_TOP_LEFT",
            BorderTopCenter => "BORDER_TOP_CENTER",
            BorderTopRight => "BORDER_TOP_RIGHT",
            BorderLeftTop => "BORDER_LEFT_TOP",
            BorderLeftMiddle => "BORDER_LEFT_MIDDLE",
            BorderLeftBottom => "BORDER_LEFT_BOTTOM",
            BorderRightTop => "BORDER_RIGHT_TOP",
            BorderRightMiddle => "BORDER_RIGHT_MIDDLE",
            BorderRightBottom => "BORDER_RIGHT_BOTTOM",
            BorderBottomLeft => "BORDER_BOTTOM_LEFT",
            BorderBottomCenter => "BORDER_BOTTOM_CENTER",
            BorderBottomRight => "BORDER_BOTTOM_RIGHT",
            UnlockedTop => "UNLOCKED_TOP",
            UnlockedMiddle => "UNLOCKED_MIDDLE",
            UnlockedBottom => "UNLOCKED_BOTTOM",
            _ => "",
        }
        .to_string()
    }

    pub fn IsShapePosition(&self) -> bool {
        matches!(*self, OutsideTopLeft | OutsideTopCenter | OutsideTopRight | OutsideBottomLeft | OutsideBottomCenter | OutsideBottomRight | OutsideLeftTop | OutsideLeftMiddle | OutsideLeftBottom | OutsideRightTop | OutsideRightMiddle | OutsideRightBottom | InsideTopLeft | InsideTopCenter | InsideTopRight | InsideMiddleLeft | InsideMiddleCenter | InsideMiddleRight | InsideBottomLeft | InsideBottomCenter | InsideBottomRight | BorderTopLeft | BorderTopCenter | BorderTopRight | BorderLeftTop | BorderLeftMiddle | BorderLeftBottom | BorderRightTop | BorderRightMiddle | BorderRightBottom | BorderBottomLeft | BorderBottomCenter | BorderBottomRight)
    }

    pub fn IsEdgePosition(&self) -> bool {
        matches!(*self, OutsideTopLeft | OutsideTopCenter | OutsideTopRight | InsideMiddleLeft | InsideMiddleCenter | InsideMiddleRight | OutsideBottomLeft | OutsideBottomCenter | OutsideBottomRight | UnlockedTop | UnlockedMiddle | UnlockedBottom)
    }

    pub fn IsOutside(&self) -> bool {
        matches!(*self, OutsideTopLeft | OutsideTopCenter | OutsideTopRight | OutsideBottomLeft | OutsideBottomCenter | OutsideBottomRight | OutsideLeftTop | OutsideLeftMiddle | OutsideLeftBottom | OutsideRightTop | OutsideRightMiddle | OutsideRightBottom)
    }

    pub fn IsUnlocked(&self) -> bool {
        matches!(*self, UnlockedTop | UnlockedMiddle | UnlockedBottom)
    }

    pub fn IsBorder(&self) -> bool {
        matches!(*self, BorderTopLeft | BorderTopCenter | BorderTopRight | BorderLeftTop | BorderLeftMiddle | BorderLeftBottom | BorderRightTop | BorderRightMiddle | BorderRightBottom | BorderBottomLeft | BorderBottomCenter | BorderBottomRight)
    }

    pub fn IsOnEdge(&self) -> bool {
        matches!(*self, InsideMiddleLeft | InsideMiddleCenter | InsideMiddleRight | UnlockedMiddle)
    }

    pub fn Mirrored(&self) -> Position {
        match *self {
            OutsideTopLeft => OutsideBottomRight,
            OutsideTopCenter => OutsideBottomCenter,
            OutsideTopRight => OutsideBottomLeft,
            OutsideLeftTop => OutsideRightBottom,
            OutsideLeftMiddle => OutsideRightMiddle,
            OutsideLeftBottom => OutsideRightTop,
            OutsideRightTop => OutsideLeftBottom,
            OutsideRightMiddle => OutsideLeftMiddle,
            OutsideRightBottom => OutsideLeftTop,
            OutsideBottomLeft => OutsideTopRight,
            OutsideBottomCenter => OutsideTopCenter,
            OutsideBottomRight => OutsideTopLeft,
            InsideTopLeft => InsideBottomRight,
            InsideTopCenter => InsideBottomCenter,
            InsideTopRight => InsideBottomLeft,
            InsideMiddleLeft => InsideMiddleRight,
            InsideMiddleCenter => InsideMiddleCenter,
            InsideMiddleRight => InsideMiddleLeft,
            InsideBottomLeft => InsideTopRight,
            InsideBottomCenter => InsideTopCenter,
            InsideBottomRight => InsideTopLeft,
            BorderTopLeft => BorderBottomRight,
            BorderTopCenter => BorderBottomCenter,
            BorderTopRight => BorderBottomLeft,
            BorderLeftTop => BorderRightBottom,
            BorderLeftMiddle => BorderRightMiddle,
            BorderLeftBottom => BorderRightTop,
            BorderRightTop => BorderLeftBottom,
            BorderRightMiddle => BorderLeftMiddle,
            BorderRightBottom => BorderLeftTop,
            BorderBottomLeft => BorderTopRight,
            BorderBottomCenter => BorderTopCenter,
            BorderBottomRight => BorderTopLeft,
            UnlockedTop => UnlockedBottom,
            UnlockedBottom => UnlockedTop,
            UnlockedMiddle => UnlockedMiddle,
            _ => Unset,
        }
    }

    pub fn GetPointOnBox(&self, box_: &geo::Box, padding: f64, width: f64, height: f64) -> PointRef {
        let pr = box_.TL().Copy();
        let boxCenter = box_.Center().get();
        let mut p = pr.bm();
        match *self {
        OutsideTopLeft => {
            p.X -= padding;
            p.Y -= padding + height;
        }
        OutsideTopCenter => {
            p.X = boxCenter.X - width/ 2.0;
            p.Y -= padding + height;
        }
        OutsideTopRight => {
            p.X += box_.Width - width - padding;
            p.Y -= padding + height;
        }
        OutsideLeftTop => {
            p.X -= padding + width;
            p.Y += padding;
        }
        OutsideLeftMiddle => {
            p.X -= padding + width;
            p.Y = boxCenter.Y - height/ 2.0;
        }
        OutsideLeftBottom => {
            p.X -= padding + width;
            p.Y += box_.Height - height - padding;
        }
        OutsideRightTop => {
            p.X += box_.Width + padding;
            p.Y += padding;
        }
        OutsideRightMiddle => {
            p.X += box_.Width + padding;
            p.Y = boxCenter.Y - height/ 2.0;
        }
        OutsideRightBottom => {
            p.X += box_.Width + padding;
            p.Y += box_.Height - height - padding;
        }
        OutsideBottomLeft => {
            p.X += padding;
            p.Y += box_.Height + padding;
        }
        OutsideBottomCenter => {
            p.X = boxCenter.X - width/ 2.0;
            p.Y += box_.Height + padding;
        }
        OutsideBottomRight => {
            p.X += box_.Width - width - padding;
            p.Y += box_.Height + padding;
        }
        InsideTopLeft => {
            p.X += padding;
            p.Y += padding;
        }
        InsideTopCenter => {
            p.X = boxCenter.X - width/ 2.0;
            p.Y += padding;
        }
        InsideTopRight => {
            p.X += box_.Width - width - padding;
            p.Y += padding;
        }
        InsideMiddleLeft => {
            p.X += padding;
            p.Y = boxCenter.Y - height/ 2.0;
        }
        InsideMiddleCenter => {
            p.X = boxCenter.X - width/ 2.0;
            p.Y = boxCenter.Y - height/ 2.0;
        }
        InsideMiddleRight => {
            p.X += box_.Width - width - padding;
            p.Y = boxCenter.Y - height/ 2.0;
        }
        InsideBottomLeft => {
            p.X += padding;
            p.Y += box_.Height - height - padding;
        }
        InsideBottomCenter => {
            p.X = boxCenter.X - width/ 2.0;
            p.Y += box_.Height - height - padding;
        }
        InsideBottomRight => {
            p.X += box_.Width - width - padding;
            p.Y += box_.Height - height - padding;
        }
        BorderTopLeft => {
            p.X += padding;
            p.Y -= height / 2.0;
        }
        BorderTopCenter => {
            p.X = boxCenter.X - width/ 2.0;
            p.Y -= height / 2.0;
        }
        BorderTopRight => {
            p.X += box_.Width - width - padding;
            p.Y -= height / 2.0;
        }
        BorderLeftTop => {
            p.X -= width / 2.0;
            p.Y += padding;
        }
        BorderLeftMiddle => {
            p.X -= width / 2.0;
            p.Y = boxCenter.Y - height/ 2.0;
        }
        BorderLeftBottom => {
            p.X -= width / 2.0;
            p.Y += box_.Height - height - padding;
        }
        BorderRightTop => {
            p.X += box_.Width - width/ 2.0;
            p.Y += padding;
        }
        BorderRightMiddle => {
            p.X += box_.Width - width/ 2.0;
            p.Y = boxCenter.Y - height/ 2.0;
        }
        BorderRightBottom => {
            p.X += box_.Width - width/ 2.0;
            p.Y += box_.Height - height - padding;
        }
        BorderBottomLeft => {
            p.X += padding;
            p.Y += box_.Height - height/ 2.0;
        }
        BorderBottomCenter => {
            p.X = boxCenter.X - width/ 2.0;
            p.Y += box_.Height - height/ 2.0;
        }
        BorderBottomRight => {
            p.X += box_.Width - width - padding;
            p.Y += box_.Height - height/ 2.0;
        }
        _ => {}
        }
        drop(p);
        pr
    }

    pub fn GetPointOnRoute(&self, route: &[PointRef], strokeWidth: f64, labelPercentage: f64, width: f64, height: f64) -> (Option<PointRef>, i64) {
        let totalLength = route.Length();
        let leftPosition = LEFT_LABEL_POSITION * totalLength;
        let centerPosition = CENTER_LABEL_POSITION * totalLength;
        let rightPosition = RIGHT_LABEL_POSITION * totalLength;
        let unlockedPosition = labelPercentage * totalLength;

        let getOffsetLabelPosition = |basePoint: &PointRef, normStart: &PointRef, normEnd: &PointRef, flip: bool| -> PointRef {
            let (ns, ne) = (normStart.get(), normEnd.get());
            let (mut normalX, mut normalY) = geo::GetUnitNormalVector(ns.X, ns.Y, ne.X, ne.Y);
            if flip {
                normalX *= -1.0;
                normalY *= -1.0;
            }
            let offsetX = strokeWidth / 2.0 + PADDING as f64 + width / 2.0;
            let offsetY = strokeWidth / 2.0 + PADDING as f64 + height / 2.0;
            let bp = basePoint.get();
            geo::NewPoint(bp.X + normalX * offsetX, bp.Y + normalY * offsetY)
        };

        // GO-ALIAS: in the Outside*/UnlockedTop/UnlockedBottom cases Go declares a new
        // `index` with `:=`, shadowing the named result, which therefore stays 0.
        let mut index: i64 = 0;
        let labelCenter: PointRef;
        let offset = |dist: f64, flip: bool| -> PointRef {
            let (basePoint, idx) = route.GetPointAtDistance(dist);
            let i = idx as usize;
            getOffsetLabelPosition(&basePoint, &route[i], &route[i + 1], flip)
        };
        match *self {
            InsideMiddleLeft => {
                let (p, i) = route.GetPointAtDistance(leftPosition);
                labelCenter = p;
                index = i;
            }
            InsideMiddleCenter => {
                let (p, i) = route.GetPointAtDistance(centerPosition);
                labelCenter = p;
                index = i;
            }
            InsideMiddleRight => {
                let (p, i) = route.GetPointAtDistance(rightPosition);
                labelCenter = p;
                index = i;
            }
            OutsideTopLeft => labelCenter = offset(leftPosition, true),
            OutsideTopCenter => labelCenter = offset(centerPosition, true),
            OutsideTopRight => labelCenter = offset(rightPosition, true),
            OutsideBottomLeft => labelCenter = offset(leftPosition, false),
            OutsideBottomCenter => labelCenter = offset(centerPosition, false),
            OutsideBottomRight => labelCenter = offset(rightPosition, false),
            UnlockedTop => labelCenter = offset(unlockedPosition, true),
            UnlockedMiddle => {
                let (p, i) = route.GetPointAtDistance(unlockedPosition);
                labelCenter = p;
                index = i;
            }
            UnlockedBottom => labelCenter = offset(unlockedPosition, false),
            _ => return (None, -1),
        }
        {
            let mut lc = labelCenter.bm();
            lc.X = chopPrecision(lc.X - width / 2.0);
            lc.Y = chopPrecision(lc.Y - height / 2.0);
        }
        (Some(labelCenter), index)
    }
}

pub(crate) fn chopPrecision(f: f64) -> f64 {
    let result = math::Round(((f * 10000.0) as f32) as f64 / 10000.0);
    if result == 0.0 {
        return 0.0;
    }
    result
}
