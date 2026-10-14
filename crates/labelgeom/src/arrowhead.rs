// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/labelgeom/arrowhead.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;
use geo::PointRef;

/// `labelgeom.ArrowheadTopLeft(route, isTarget, sourceArrowhead, targetArrowhead, width, height)`
pub fn ArrowheadTopLeft(route: &[PointRef], isTarget: bool, sourceArrowhead: &str, targetArrowhead: &str, width: f64, height: f64) -> PointRef {
    let mut connection = d2target::BaseConnection();
    connection.Route = route.to_vec();
    connection.SrcArrow = d2target::Arrowhead::new(sourceArrowhead);
    connection.DstArrow = d2target::Arrowhead::new(targetArrowhead);

    let text = d2target::Text {
        LabelWidth: gocompat::math::F64ToInt(width),
        LabelHeight: gocompat::math::F64ToInt(height),
        ..Default::default()
    };
    if isTarget {
        connection.DstLabel = Some(text);
    } else {
        connection.SrcLabel = Some(text);
    }
    connection.GetArrowheadLabelPosition(isTarget)
}
