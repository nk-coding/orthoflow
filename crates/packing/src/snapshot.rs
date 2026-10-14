// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/packing/snapshot.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

/// Go `pointerSnapshot[geo.Point]`: the pointer plus a copy of its pointee.
#[derive(Clone, Default)]
pub(crate) struct pointerSnapshot {
    pub(crate) pointer: Option<geo::PointRef>,
    pub(crate) value: geo::Point,
}

pub(crate) fn snapshotPointer(pointer: Option<&geo::PointRef>) -> pointerSnapshot {
    match pointer {
        None => pointerSnapshot::default(),
        Some(p) => pointerSnapshot { pointer: Some(p.clone()), value: *p.b() },
    }
}

impl pointerSnapshot {
    pub(crate) fn restore(&self) -> Option<geo::PointRef> {
        let p = self.pointer.as_ref()?;
        *p.bm() = self.value;
        Some(p.clone())
    }
}
