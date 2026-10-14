// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/labeling/model.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;
use label::Position;

/// searchRange is the fraction of an edge route searched for an unlocked label.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct searchRange {
    pub(crate) start: f64,
    pub(crate) end: f64,
}

/// The order of preferred edge-label positions.
pub(crate) const edgeLabelPreferenceOrder: [Position; 9] = [
    label::OutsideTopCenter,
    label::OutsideBottomCenter,
    label::OutsideTopLeft,
    label::OutsideTopRight,
    label::OutsideBottomLeft,
    label::OutsideBottomRight,
    label::InsideMiddleCenter,
    label::InsideMiddleLeft,
    label::InsideMiddleRight,
];

pub(crate) const nodeLabelPositionOrder: [Position; 21] = [
    label::InsideMiddleCenter,
    label::InsideTopCenter,
    label::InsideBottomCenter,
    label::InsideMiddleLeft,
    label::InsideMiddleRight,
    label::InsideTopLeft,
    label::InsideTopRight,
    label::InsideBottomLeft,
    label::InsideBottomRight,
    label::OutsideTopCenter,
    label::OutsideBottomCenter,
    label::OutsideLeftMiddle,
    label::OutsideRightMiddle,
    label::OutsideTopLeft,
    label::OutsideTopRight,
    label::OutsideBottomLeft,
    label::OutsideBottomRight,
    label::OutsideLeftTop,
    label::OutsideLeftBottom,
    label::OutsideRightTop,
    label::OutsideRightBottom,
];

pub(crate) const containerLabelPositionOrder: [Position; 21] = [
    label::InsideTopCenter,
    label::OutsideTopCenter,
    label::InsideBottomCenter,
    label::OutsideBottomCenter,
    label::OutsideTopLeft,
    label::OutsideTopRight,
    label::OutsideBottomLeft,
    label::OutsideBottomRight,
    label::OutsideLeftMiddle,
    label::OutsideRightMiddle,
    label::OutsideLeftTop,
    label::OutsideLeftBottom,
    label::OutsideRightTop,
    label::OutsideRightBottom,
    label::InsideTopLeft,
    label::InsideTopRight,
    label::InsideBottomLeft,
    label::InsideBottomRight,
    label::InsideMiddleLeft,
    label::InsideMiddleRight,
    label::InsideMiddleCenter,
];

const tiers: [nodeshape::LabelTier; 4] = [nodeshape::Good, nodeshape::OK, nodeshape::Unideal, nodeshape::Bad];

/// Initialize reserves each explicit/default node-label and icon position
/// before placement begins.
pub fn Initialize(graph: &GraphRef) {
    let nodes = graph.b().Nodes.clone();
    for node in &nodes {
        setDefaultLabelPlacement(node);
        if let Some(icon) = node.icon() {
            if icon.b().Position != label::Unset {
                icon.FixPosition();
            }
        }
    }
}

pub(crate) fn setDefaultLabelPlacement(node: &NodeRef) {
    let Some(l) = node.label() else { return };
    if l.b().Position == label::Unset {
        let p = labelPositionPreferences(node)[0];
        l.bm().Position = p;
    } else {
        l.FixPosition();
    }
}

pub(crate) fn compareLabelPositions(node: &NodeRef, first: Position, second: Position) -> i64 {
    let shape = node.shape();
    let score = |position: Position| -> i64 {
        let mut score = -1;
        for (index, tier) in tiers.iter().enumerate() {
            if shape.LabelPositionPreferences(*tier).contains(&position) {
                score = index as i64 + 1;
            }
        }
        score
    };
    let (firstScore, secondScore) = (score(first), score(second));
    if firstScore < secondScore {
        return 1;
    }
    if firstScore > secondScore {
        return -1;
    }
    0
}

pub(crate) fn labelPositionPreferences(node: &NodeRef) -> Vec<Position> {
    let mut preferences = Vec::new();
    for tranche in labelPositionPreferenceTranches(node) {
        preferences.extend(tranche);
    }
    preferences
}

pub(crate) fn labelPositionPreferenceTranches(node: &NodeRef) -> Vec<Vec<Position>> {
    let baseOrder: &[Position] = if node.IsContainer() { &containerLabelPositionOrder } else { &nodeLabelPositionOrder };
    let shape = node.shape();
    let mut tranches = Vec::with_capacity(4);
    for tier in tiers {
        let allowed = shape.LabelPositionPreferences(tier);
        let mut tranche = Vec::new();
        for position in baseOrder {
            if allowed.contains(position) {
                tranche.push(*position);
            }
        }
        tranches.push(tranche);
    }
    tranches
}
