// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/layoutgraph/icon.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

/// Icon is the icon-placement state attached to a layout node.
#[derive(Clone, Debug, Default)]
pub struct Icon {
    pub Position: label::Position,

    pub(crate) positionFixed: bool,
}

gocompat::go_ptr!(IconRef, Icon);

impl Icon {
    /// Go `&layoutgraph.Icon{Position: p}`.
    pub fn new(Position: label::Position) -> Icon {
        Icon { Position, positionFixed: false }
    }
    /// PositionFixed reports whether Position was supplied by the caller and must
    /// be preserved by automatic icon placement.
    pub fn PositionFixed(&self) -> bool {
        self.positionFixed
    }

    /// FixPosition marks Position as supplied by the caller.
    pub fn FixPosition(&mut self) {
        self.positionFixed = true;
    }
}

impl IconRef {
    pub fn PositionFixed(&self) -> bool {
        self.b().positionFixed
    }
    pub fn FixPosition(&self) {
        self.bm().positionFixed = true;
    }
}
