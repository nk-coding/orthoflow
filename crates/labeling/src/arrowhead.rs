// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/labeling/arrowhead.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

/// PositionedArrowheadLabel is the rendered box reserved beside one end of an
/// edge route.
#[derive(Clone)]
pub struct PositionedArrowheadLabel {
    pub Box: geo::Box,
    pub Edge: EdgeRef,
    pub IsTarget: bool,
    pub Text: String,
}

/// PositionArrowheadLabel derives the renderer-compatible label box for one
/// arrowhead. A nil result means that endpoint has no arrowhead label.
/// (Go's nil-edge check is unreachable: every caller passes a non-nil edge.)
pub fn PositionArrowheadLabel(edge: &EdgeRef, isTarget: bool, route: &[geo::PointRef]) -> Option<PositionedArrowheadLabel> {
    let (value, sa, ta) = {
        let e = edge.b();
        let v = if isTarget { e.TargetArrowheadLabel.clone() } else { e.SourceArrowheadLabel.clone() };
        (v, e.SourceArrowhead.clone(), e.TargetArrowhead.clone())
    };
    let value = value?;
    let (w, h, text) = {
        let v = value.b();
        (v.Width, v.Height, v.Text.clone())
    };
    let topLeft = labelgeom::ArrowheadTopLeft(route, isTarget, &sa, &ta, w, h);
    let bx = geo::NewBox(topLeft, w, h).b().clone();
    Some(PositionedArrowheadLabel { Box: bx, Edge: edge.clone(), IsTarget: isTarget, Text: text })
}
