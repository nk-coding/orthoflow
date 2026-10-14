// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/routing/tuning.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of routing/tuning.go.
#[allow(unused_imports)]
use crate::*;

pub(crate) const basicallyInfinity: f64 = 10_000_000.0;
pub(crate) const extraInterestingPointLayers: f64 = 3.0;
pub(crate) const ovgPadding: f64 = 20.0;
pub(crate) const overshootAmount: f64 = 100.0;
pub(crate) const pathNodeProximityFloor: f64 = 5.0;
pub(crate) const nodeProximityThreshold: f64 = 30.0;
pub(crate) const idealTurnAxisTolerance: f64 = 4.0;
pub(crate) const idealTurnMultiplier: f64 = 0.98;
pub(crate) const idealTurnEvenClusterMultiplier: f64 = 0.10;
pub(crate) const nodeProximityPenalty: f64 = 1.0;
pub(crate) const turnEndpointClearance: f64 = 20.0;
pub(crate) const lowerJitterThreshold: f64 = 5.0;
pub(crate) const minStraightEdgeAngle: f64 = 30.0;
pub(crate) const nonOrthogonalFactor: f64 = 4.0;
pub(crate) const turnPenalty: f64 = 1.4;
pub(crate) const segmentSpacingBuffer: f64 = 40.0;
pub(crate) const unboundedSegmentMove: f64 = 100.0;
pub(crate) const treeChildAlignmentTolerance: f64 = 10.0;
