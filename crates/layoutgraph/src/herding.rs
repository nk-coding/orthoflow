// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/layoutgraph/herding.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

/// HerdAssignment records the side of a container toward which a node should
/// be placed, together with the uncles paired on the same and opposite sides.
/// The proximity package owns the algorithms that produce and synchronize
/// these assignments.
#[derive(Clone, Debug)]
pub struct HerdAssignment {
    // The whole point of herding to one side is so another herd can pair on the
    // opposite side. Keep unique counts of connected uncles on each side.
    pub(crate) oppositeSidePaired: Set<NodeRef>,
    pub(crate) sameSidePaired: Set<NodeRef>,

    /// Orientation is the side to herd to: TOP, LEFT, BOTTOM, or RIGHT.
    pub Orientation: geo::Orientation,

    /// Val is the y-axis coordinate for TOP/BOTTOM and the x-axis coordinate for
    /// LEFT/RIGHT.
    pub Val: f64,
}

gocompat::go_ptr!(HerdAssignmentRef, HerdAssignment);

pub fn NewHerdAssignment() -> HerdAssignmentRef {
    HerdAssignmentRef::new(HerdAssignment {
        oppositeSidePaired: Set::default(),
        sameSidePaired: Set::default(),
        Orientation: geo::Orientation::default(),
        Val: 0.0,
    })
}

impl HerdAssignmentRef {
    pub fn Copy(&self) -> HerdAssignmentRef {
        let ha = self.b();
        HerdAssignmentRef::new(HerdAssignment {
            oppositeSidePaired: ha.oppositeSidePaired.clone(),
            sameSidePaired: ha.sameSidePaired.clone(),
            Orientation: ha.Orientation,
            Val: ha.Val,
        })
    }
}
