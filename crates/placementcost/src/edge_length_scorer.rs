// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/placementcost/edge_length_scorer.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

/// NodeEdgeLengthScorer reuses incident-edge topology during one candidate sweep.
/// Geometry is read from live nodes on every Score. The caller must keep edges,
/// abductions, container/cluster membership, labels and directions unchanged until
/// Close.
#[derive(Default)]
pub struct NodeEdgeLengthScorer {
    node: Option<NodeRef>,
    options: EdgeLengthOptions,
    scratch: Option<Box<edgeScratch>>,
    setup: edgeLengthStatic,
    prepChecks: i64,
}

/// NewNodeEdgeLengthScorer starts an unprepared sweep. Preparation is lazy so
/// cancellation and the caller's work charge occur at the original score point.
pub fn NewNodeEdgeLengthScorer(node: &NodeRef, options: EdgeLengthOptions) -> NodeEdgeLengthScorer {
    NodeEdgeLengthScorer { node: Some(node.clone()), options, scratch: None, setup: edgeLengthStatic::default(), prepChecks: 0 }
}

impl NodeEdgeLengthScorer {
    /// Score has the same value and cancellation checkpoints as NodeEdgeLength while
    /// the scorer's topology remains fixed.
    pub fn Score(&mut self, ctx: &Ctx) -> GoResult<f64> {
        ctxCheck(ctx)?;
        let Some(node) = self.node.clone() else {
            return Err(GoError::new("EdgeLength: scorer is closed"));
        };
        if self.scratch.is_none() {
            let mut s = Box::new(edgeScratch::default());
            let mut checks = prepChecker { ctx, count: 0 };
            let setup = prepareNodeEdgeLength(&mut checks, &node, &self.options, &mut s)?;
            self.setup = setup;
            self.prepChecks = checks.count;
            self.scratch = Some(s);
        } else {
            // Preparation only inspects topology. Replay its exact polls.
            for _ in 0..self.prepChecks {
                checkScoringCancellation(ctx)?;
            }
        }
        evaluateNodeEdgeLength(ctx, &node, &self.options, self.scratch.as_ref().unwrap(), self.setup)
    }

    /// Close releases scratch and makes the scorer unusable. It is safe to call more
    /// than once, including after an unsuccessful Score.
    pub fn Close(&mut self) {
        *self = NodeEdgeLengthScorer::default();
    }
}
