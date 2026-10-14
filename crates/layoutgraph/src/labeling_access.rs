// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/layoutgraph/labeling_access.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

/// LabelBoxFits reports whether outer completely contains inner.
pub fn LabelBoxFits(outer: &geo::Box, inner: &geo::Box) -> bool {
    covers(outer, inner)
}

impl NodeRef {
    /// PadLabelCandidate expands (or shrinks, for a negative value) a temporary
    /// node box by the same amount on every side.
    pub fn PadLabelCandidate(&self, padding: i64) {
        self.pad(padding);
    }

    /// LabelBoxesOverlap reports whether two node boxes overlap with the requested
    /// padding.
    pub fn LabelBoxesOverlap(&self, other: &NodeRef, padding: f64) -> bool {
        self.doesOverlapCalc(other, padding)
    }

    /// LabelOverlapArea returns the rectangular overlap area. Callers must first prove
    /// that the nodes overlap.
    pub fn LabelOverlapArea(&self, other: &NodeRef) -> f64 {
        self.overlapArea(other)
    }
}

impl EdgeRef {
    /// IsClusterEdge reports whether either endpoint is currently represented by
    /// a cluster grouping.
    pub fn IsClusterEdge(&self) -> bool {
        self.isClusterEdge()
    }
}
