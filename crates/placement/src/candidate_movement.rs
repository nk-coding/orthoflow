// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/placement/candidate_movement.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of placement/candidate_movement.go
use crate::*;
use gocompat::{errorf, GoResult};
use layoutgraph::NodeRef;
use limits::OptimizationWorkGuardRef;

// optimizerCandidateMovement reuses the candidate search's existing rollback
// snapshot as its stable movement set. Scoring only changes geometry, so every
// trial visits the same descendants in the same order. Its caller must restore
// positions if the search fails or panics, including an error midway through a
// move; it must not retain this movement set across topology changes.
pub struct optimizerCandidateMovement {
    pub positions: Vec<nodePositionSnapshot>,
    pub descendantWork: u64,
}

pub fn captureOptimizerCandidateMovement(node: &NodeRef, guard: &OptimizationWorkGuardRef) -> GoResult<optimizerCandidateMovement> {
    let before = guard.Used();
    let positions = captureOptimizerNodePositions(std::slice::from_ref(node), guard)?;
    // Single-root capture charges one Step for each retained position in addition
    // to the descendant walk. Retain the latter's charge so every trial preserves
    // the original work boundary and cancellation polling sequence.
    let descendantWork = guard.Used() - before - positions.len() as u64;
    Ok(optimizerCandidateMovement { positions, descendantWork })
}

impl optimizerCandidateMovement {
    pub fn moveAbs(&self, x: f64, y: f64, guard: &OptimizationWorkGuardRef) -> GoResult<()> {
        let node = &self.positions[0].node;
        let Some(tl) = node.tl() else {
            return Err(errorf!("TALA {} cannot move an unpositioned node", guard.Location()));
        };
        let cur = *tl.b();
        if cur.X == x && cur.Y == y {
            return guard.Step();
        }
        // Replay the original walk's Step calls instead of batching them: custom
        // contexts and resource failures keep the same observation boundaries.
        for _ in 0..self.descendantWork {
            guard.Step()?;
        }
        let descendants = &self.positions[1..];
        for position in descendants {
            guard.Step()?;
            if !position.node.has_tl() {
                return Err(errorf!("TALA {} cannot move an unpositioned descendant", guard.Location()));
            }
        }
        let (dx, dy) = (x - cur.X, y - cur.Y);
        node.Translate(dx, dy);
        for position in descendants {
            guard.Step()?;
            position.node.Translate(dx, dy);
        }
        Ok(())
    }
}
