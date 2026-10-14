// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/nodeshape/shape_square.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

/// Go `nodeshape.shapeSquare` (wraps a D2 `shape.Shape`).
pub struct shapeSquare {
    pub Shape: ::shape::ShapeRc,
}

impl ::shape::Shape for shapeSquare {
    ::shape::delegate_shape_methods!(Shape);
}

impl Shape for shapeSquare {
    fn LabelPositionPreferences(&self, tier: LabelTier) -> Set<label::Position> {
        shapeSquare_LabelPositionPreferences(tier)
    }
    fn SnapPointPercentages(&self) -> Vec<Vec<geo::RelativePointRef>> {
        shapeSquare_SnapPointPercentages()
    }
    fn PortIndices(&self, orientation: geo::Orientation) -> Vec<i64> {
        shapeSquare_PortIndices(orientation)
    }
    fn MirroredPortIndices(&self) -> Map<i64, i64> {
        shapeSquare_MirroredPortIndices()
    }
    fn CenterPortIndices(&self) -> Vec<i64> {
        shapeSquare_CenterPortIndices()
    }
    fn CenterPortIndex(&self, o: geo::Orientation) -> i64 {
        shapeSquare_CenterPortIndex(o)
    }
}
