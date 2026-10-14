// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/routing/turns.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of routing/turns.go.
#[allow(unused_imports)]
use crate::*;
use layoutgraph::NodeRef;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct idealTurnAxis {
    pub(crate) isX: bool,
    pub(crate) val: f64,
}

/// idealTurnAxes returns the midpoint axes where a route can turn between two
/// node boxes without adding avoidable distance.
pub(crate) fn idealTurnAxes(nodeA: &NodeRef, nodeB: &NodeRef) -> Vec<idealTurnAxis> {
    let (tla, wa, ha) = (nodeA.tlv(), nodeA.w(), nodeA.h());
    let (tlb, wb, hb) = (nodeB.tlv(), nodeB.w(), nodeB.h());
    let x1 = tla.X;
    let y1 = tla.Y;
    let x1b = tla.X + wa;
    let y1b = tla.Y + ha;

    let x2 = tlb.X;
    let y2 = tlb.Y;
    let x2b = tlb.X + wb;
    let y2b = tlb.Y + hb;

    let left = x2b < x1;
    let right = x1b < x2;
    let top = y2b < y1;
    let bottom = y1b < y2;

    let mut axes = Vec::with_capacity(2);
    let ax = |isX: bool, val: f64| idealTurnAxis { isX, val };
    if top && left {
        axes.push(ax(true, (x1 + x2b) / 2.0));
        axes.push(ax(false, (y1 + y2b) / 2.0));
    } else if left && bottom {
        axes.push(ax(true, (x1 + x2b) / 2.0));
        axes.push(ax(false, (y2 + y1b) / 2.0));
    } else if bottom && right {
        axes.push(ax(true, (x2 + x1b) / 2.0));
        axes.push(ax(false, (y2 + y1b) / 2.0));
    } else if right && top {
        axes.push(ax(true, (x2 + x1b) / 2.0));
        axes.push(ax(false, (y1 + y2b) / 2.0));
    } else if left {
        axes.push(ax(true, (x1 + x2b) / 2.0));
    } else if right {
        axes.push(ax(true, (x2 + x1b) / 2.0));
    } else if bottom {
        axes.push(ax(false, (y2 + y1b) / 2.0));
    } else if top {
        axes.push(ax(false, (y1 + y2b) / 2.0));
    }
    axes
}
