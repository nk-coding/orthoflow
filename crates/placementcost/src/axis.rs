// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/placementcost/axis.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

/// AxisScore measures how strongly nodes share one visual axis.
pub fn AxisScore(nodes: &[NodeRef]) -> f64 {
    if nodes.len() < 2 {
        return 1.0;
    }
    if nodes.len() == 2 {
        if nodes[0].Orientation(&nodes[1]).IsDiagonal() {
            return 0.0;
        }
        return 1.0;
    }

    let mut largestWidth = f64::NEG_INFINITY;
    let mut largestWidthNode: Option<&NodeRef> = None;
    let mut largestHeight = f64::NEG_INFINITY;
    let mut largestHeightNode: Option<&NodeRef> = None;
    for node in nodes {
        let (w, h) = (node.w(), node.h());
        if w > largestWidth {
            largestWidth = w;
            largestWidthNode = Some(node);
        }
        if h > largestHeight {
            largestHeight = h;
            largestHeightNode = Some(node);
        }
    }

    let mut verticallyAligned = true;
    for node in nodes {
        if Some(node) == largestHeightNode {
            continue;
        }
        let lh = largestHeightNode.expect("nil pointer dereference");
        if !node.Orientation(lh).IsHorizontal() {
            verticallyAligned = false;
            break;
        }
    }
    if verticallyAligned {
        let lh = largestHeightNode.expect("nil pointer dereference");
        let center = *lh.Center().b();
        let mut score = 0.0;
        for node in nodes {
            if node == lh {
                continue;
            }
            let mut nodeScore = 0.0;
            for fraction in [0.25, 0.5, 0.75] {
                let y = lh.y() + fraction * lh.h();
                let nc = *node.Center().b();
                if node.OverlapsLine(&geo::Point { X: center.X, Y: y }, &geo::Point { X: nc.X, Y: y }, 0.0) {
                    nodeScore += 0.33;
                }
            }
            if node.h() < 0.25 * lh.h() {
                nodeScore *= 3.0;
            } else if node.h() < 0.75 * lh.h() {
                nodeScore *= 2.0;
            }
            if nodeScore == 0.99 {
                score += 1.0;
            } else {
                score += nodeScore;
            }
        }
        return math::Max(0.33, score / (nodes.len() - 1) as f64);
    }

    let mut horizontallyAligned = true;
    for node in nodes {
        if Some(node) == largestWidthNode {
            continue;
        }
        let lw = largestWidthNode.expect("nil pointer dereference");
        if !node.Orientation(lw).IsVertical() {
            horizontallyAligned = false;
            break;
        }
    }
    if horizontallyAligned {
        let lw = largestWidthNode.expect("nil pointer dereference");
        let center = *lw.Center().b();
        let mut score = 0.0;
        for node in nodes {
            if node == lw {
                continue;
            }
            let mut nodeScore = 0.0;
            for fraction in [0.25, 0.5, 0.75] {
                let x = lw.x() + fraction * lw.w();
                let nc = *node.Center().b();
                if node.OverlapsLine(&geo::Point { X: x, Y: center.Y }, &geo::Point { X: x, Y: nc.Y }, 0.0) {
                    nodeScore += 0.33;
                }
            }
            if node.w() < 0.25 * lw.w() {
                nodeScore *= 3.0;
            } else if node.w() < 0.75 * lw.w() {
                nodeScore *= 2.0;
            }
            if nodeScore == 0.99 {
                score += 1.0;
            } else {
                score += nodeScore;
            }
        }
        return math::Max(0.33, score / (nodes.len() - 1) as f64);
    }
    0.0
}
