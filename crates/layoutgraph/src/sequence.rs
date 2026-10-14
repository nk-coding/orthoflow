// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/layoutgraph/sequence.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

#[derive(Clone)]
pub struct Sequence {
    pub Vessel: NodeRef,
    pub Nodes: Vec<NodeRef>,
    pub Graph: Option<GraphRef>,
    pub EdgeAbductions: Vec<EdgeAbductionRef>,
    pub Container: Option<NodeRef>,
}

gocompat::go_ptr!(SequenceRef, Sequence);

impl SequenceRef {
    // findAbductedNodeByEdge searches for the edge abduction related to the given edge and then returns the original node
    pub(crate) fn findAbductedNodeByEdge(&self, e: &EdgeRef) -> Option<NodeRef> {
        let (abductions, vessel) = {
            let s = self.b();
            (s.EdgeAbductions.clone(), s.Vessel.clone())
        };
        for ea in &abductions {
            let ea = ea.b();
            if *e != ea.Edge {
                continue;
            }
            if ea.CurrentFrom.as_ref() == Some(&vessel) {
                return ea.OriginallyFrom.clone();
            }
            if ea.CurrentTo.as_ref() == Some(&vessel) {
                return ea.OriginallyTo.clone();
            }
        }
        None
    }

    /// SyncGeometry resizes the sequence vessel and arranges its visible steps.
    pub fn SyncGeometry(&self) {
        if let Err(err) = self.SyncGeometryWithWork(&unmeteredGroupGeometry) {
            panic!("{}", err);
        }
    }

    /// SyncGeometryWithWork resizes the sequence vessel and arranges its visible
    /// steps through caller-owned cancellation and movement operations.
    pub fn SyncGeometryWithWork(&self, work: &dyn SharedWorkStepper) -> GoResult<()> {
        work.Step()?;
        self.resizeWithWork(work)?;
        self.arrangeStepsWithWork(work)
    }

    pub(crate) fn resizeWithWork(&self, work: &dyn workStepper) -> GoResult<()> {
        let (nodes, vessel) = {
            let s = self.b();
            (s.Nodes.clone(), s.Vessel.clone())
        };
        let mut width = 0.0f64;
        let mut offset = 0.0f64;
        let mut height = 0.0f64;
        for n in &nodes {
            work.Step()?;
            let (w, h) = (n.w(), n.h());
            width = fmax(width, offset + fmax(0.0, w));
            offset += SequenceAdvance(w);
            height = fmax(height, fmax(0.0, h));
        }
        vessel.set_w(width);
        vessel.set_h(height);
        Ok(())
    }

    /// ArrangeSteps places visible sequence steps relative to the vessel.
    pub fn ArrangeSteps(&self) {
        if let Err(err) = self.arrangeStepsWithWork(&unmeteredGroupGeometry) {
            panic!("{}", err);
        }
    }

    pub(crate) fn arrangeStepsWithWork(&self, work: &dyn workStepper) -> GoResult<()> {
        let (nodes, vessel) = {
            let s = self.b();
            (s.Nodes.clone(), s.Vessel.clone())
        };
        // hasn't been initialized yet
        if !vessel.has_tl() {
            return work.Finish();
        }

        let mut tl = vessel.tlv();
        for n in &nodes {
            work.Step()?;
            n.set_tl(Some(geo::PointRef::new(tl)));
            tl.X += SequenceAdvance(n.w());
        }
        work.Finish()
    }

    /// PlaceVessel positions the sequence vessel at the top-left of its steps.
    pub fn PlaceVessel(&self) {
        let (nodes, vessel) = {
            let s = self.b();
            (s.Nodes.clone(), s.Vessel.clone())
        };
        let mut topLeft = geo::Point { X: f64::INFINITY, Y: f64::INFINITY };
        for node in &nodes {
            let Some(tl) = node.tl() else { return };
            let t = *tl.b();
            topLeft.X = fmin(topLeft.X, t.X);
            topLeft.Y = fmin(topLeft.Y, t.Y);
        }
        vessel.set_tl(Some(geo::PointRef::new(topLeft)));
    }
}

/// SequenceAdvance is the horizontal distance from one step's top-left to
/// the next. The rendered Step shape clamps its wedge to half of the width when
/// the requested width is at most STEP_WEDGE_WIDTH, so fixed-size narrow steps
/// must use the same geometry instead of always overlapping by the full wedge.
pub fn SequenceAdvance(width: f64) -> f64 {
    if width <= 0.0 {
        return 0.0;
    }
    let mut wedge = shape::STEP_WEDGE_WIDTH;
    if width <= wedge {
        wedge = width / 2.0;
    }
    width - wedge
}

impl GraphRef {
    /// SyncSequences synchronizes every active sequence in nested graph order.
    pub fn SyncSequences(&self) {
        let sequences = self.b().Sequences.clone();
        if sequences.len() == 0 {
            return;
        }

        let mut sync = |n: &NodeRef| {
            if let Some(sequence) = sequences.get(n) {
                sequence.SyncGeometry();
            }
        };
        for n in self.nodes() {
            n.rdfsWalk(&mut sync);
        }
    }
}

impl SequenceRef {
    pub(crate) fn first(&self) -> NodeRef {
        self.b().Nodes[0].clone()
    }
    pub(crate) fn last(&self) -> NodeRef {
        let s = self.b();
        s.Nodes[s.Nodes.len() - 1].clone()
    }

    /// First and Last expose the endpoints needed by sequence-aware routing.
    pub fn First(&self) -> NodeRef {
        self.first()
    }
    pub fn Last(&self) -> NodeRef {
        self.last()
    }

    pub(crate) fn isActive(&self) -> bool {
        let v = self.b().Vessel.clone();
        let active = v.b().Graph.is_some();
        active
    }

    pub fn DebugID(&self) -> String {
        let nodes = self.b().Nodes.clone();
        let nodeIDs: Vec<String> = nodes.iter().map(|n| n.DebugID()).collect();
        format!("[{}]", nodeIDs.join(", "))
    }
}

/// Go `s.isActive()` on a possibly-nil sequence.
#[inline]
pub fn sequenceIsActive(s: Option<&SequenceRef>) -> bool {
    match s {
        None => false,
        Some(s) => s.isActive(),
    }
}
