// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/layoutgraph/label.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

/// Label is the label state attached to a layout node or edge.
#[derive(Clone, Debug, Default)]
pub struct Label {
    pub Text: String,
    pub Position: label::Position,
    pub Width: f64,
    pub Height: f64,

    pub(crate) positionFixed: bool,
}

gocompat::go_ptr!(LabelRef, Label);

impl Label {
    /// Go `&layoutgraph.Label{Text, Position, Width, Height}` (positionFixed false).
    pub fn new(Text: impl Into<String>, Position: label::Position, Width: f64, Height: f64) -> Label {
        Label { Text: Text.into(), Position, Width, Height, positionFixed: false }
    }
    /// PositionFixed reports whether Position was supplied by the caller and must
    /// be preserved by automatic label placement.
    pub fn PositionFixed(&self) -> bool {
        self.positionFixed
    }

    /// FixPosition marks Position as supplied by the caller.
    pub fn FixPosition(&mut self) {
        self.positionFixed = true;
    }
}

impl LabelRef {
    pub fn PositionFixed(&self) -> bool {
        self.b().positionFixed
    }
    pub fn FixPosition(&self) {
        self.bm().positionFixed = true;
    }
}
