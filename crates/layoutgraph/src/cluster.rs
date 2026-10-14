// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/layoutgraph/cluster.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;
use std::borrow::Cow;

/// Cluster is a group of nodes that
/// - have the same size
/// - are all adjacent to another node in the cluster
/// - are evenly spaced apart from each other
/// - are arranged according to a ClusterArrangement
#[derive(Clone)]
pub struct Cluster {
    /// Vessel is the node that gets moved around during node placement and encapsulates the cluster nodes
    pub Vessel: NodeRef,
    pub Nodes: Vec<NodeRef>,
    pub Arrangement: ClusterArrangement,
    pub DesiredArrangement: ClusterArrangement,
    pub Graph: Option<GraphRef>,
    pub EdgeAbductions: Vec<EdgeAbductionRef>,
    pub Padding: f64,
    pub FixedSize: bool,

    pub Container: Option<NodeRef>,
}

gocompat::go_ptr!(ClusterRef, Cluster);

/// Go `type ClusterArrangement string`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct ClusterArrangement(pub Cow<'static, str>);

impl ClusterArrangement {
    pub fn from_string(s: impl Into<String>) -> ClusterArrangement {
        ClusterArrangement(Cow::Owned(s.into()))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
    pub fn Flip(&self) -> ClusterArrangement {
        if *self == Row {
            return Column;
        }
        Row
    }
}

pub const Row: ClusterArrangement = ClusterArrangement(Cow::Borrowed("Row"));
pub const Column: ClusterArrangement = ClusterArrangement(Cow::Borrowed("Column"));

impl ClusterRef {
    /// Resize makes the cluster nodes consistent in size, and sizes the vessel to exact fit of cluster nodes
    /// It should be called after any potential resizing, which may happen if a child node is a container and gets resized
    pub fn Resize(&self, vessel: &NodeRef) {
        if let Err(err) = self.resizeWithWork(Some(vessel), &unmeteredGroupGeometry) {
            panic!("{}", err);
        }
    }

    pub(crate) fn resizeWithWork(&self, vessel: Option<&NodeRef>, work: &dyn workStepper) -> GoResult<()> {
        let Some(vessel) = vessel else {
            return Err(invariant::New("cluster is missing its vessel"));
        };
        let (fixedSize, nodes) = {
            let c = self.b();
            (c.FixedSize, c.Nodes.clone())
        };
        if !fixedSize {
            let (nodeWidth, nodeHeight) = self.maximumsWithWork(work)?;

            for node in &nodes {
                work.Step()?;
                node.set_w(nodeWidth);
                node.set_h(nodeHeight);
            }
        }
        let (nodeWidth, nodeHeight) = self.maximumsWithWork(work)?;
        let (arrangement, padding) = {
            let c = self.b();
            (c.Arrangement.clone(), c.Padding)
        };
        let n = nodes.len() as f64;
        if arrangement == Row {
            vessel.set_w(nodeWidth * n + padding * (nodes.len() as f64 - 1.0));
            vessel.set_h(nodeHeight);
        }

        if arrangement == Column {
            vessel.set_w(nodeWidth);
            vessel.set_h(nodeHeight * n + padding * (nodes.len() as f64 - 1.0));
        }
        Ok(())
    }

    pub(crate) fn maximumsWithWork(&self, work: &dyn workStepper) -> GoResult<(f64, f64)> {
        let nodes = self.b().Nodes.clone();
        let (mut maxWidth, mut maxHeight) = (0.0f64, 0.0f64);
        for node in &nodes {
            work.Step()?;
            maxWidth = fmax(maxWidth, node.w());
        }
        for node in &nodes {
            work.Step()?;
            maxHeight = fmax(maxHeight, node.h());
        }
        work.Finish()?;
        Ok((maxWidth, maxHeight))
    }

    /// ArrangeClusterNodes moves the inner cluster nodes to their respective positions within the cluster
    /// This should be called after vessels move
    /// This is idempotent -- calling it after it's in the right position results in 0's for movements
    pub fn ArrangeClusterNodes(&self) {
        if let Err(err) = self.arrangeNodesWithWork(&unmeteredGroupGeometry) {
            panic!("{}", err);
        }
    }

    pub(crate) fn arrangeNodesWithWork(&self, work: &dyn ClusterGeometryWork) -> GoResult<()> {
        let (vessel, arrangement, nodes, padding) = {
            let c = self.b();
            (c.Vessel.clone(), c.Arrangement.clone(), c.Nodes.clone(), c.Padding)
        };
        // Hasn't been initialized yet, which is fine since we arrange all cluster nodes every time for simplicity
        if !vessel.has_tl() {
            return work.Finish();
        }

        if arrangement == Row {
            let mut position = vessel.x();
            let vesselCenter = vessel.y() + vessel.h() / 2.0;

            for node in &nodes {
                work.Step()?;
                if node.has_tl() {
                    let dx = position - node.x();
                    let dy = (vesselCenter - (node.y() + node.h() / 2.0)).round();
                    work.MoveNodeWithChildren(node, dx, dy)?;
                } else {
                    node.set_tl(Some(newPoint(position, (vesselCenter - node.h() / 2.0).round())));
                    work.PositionContainerChildren(node)?;
                }
                position += node.w() + padding;
            }
        }

        if arrangement == Column {
            let mut position = vessel.y();
            let vesselCenter = vessel.x() + vessel.w() / 2.0;

            for node in &nodes {
                work.Step()?;
                if node.has_tl() {
                    let dx = (vesselCenter - (node.x() + node.w() / 2.0)).round();
                    let dy = position - node.y();
                    work.MoveNodeWithChildren(node, dx, dy)?;
                } else {
                    node.set_tl(Some(newPoint((vesselCenter - node.w() / 2.0).round(), position)));
                    work.PositionContainerChildren(node)?;
                }
                position += node.h() + padding;
            }
        }
        work.Finish()
    }

    pub fn DebugID(&self) -> String {
        let (nodes, arrangement) = {
            let c = self.b();
            (c.Nodes.clone(), c.Arrangement.clone())
        };
        let nodeIDs: Vec<String> = nodes.iter().map(|n| n.DebugID()).collect();
        format!("[{}]; Arrangement: {}", nodeIDs.join(", "), arrangement.as_str())
    }

    /// SyncGeometry resizes the cluster vessel and arranges its visible members.
    pub fn SyncGeometry(&self) {
        if let Err(err) = self.SyncGeometryWithWork(&unmeteredGroupGeometry) {
            panic!("{}", err);
        }
    }

    /// SyncGeometryWithWork resizes the cluster vessel and arranges its visible
    /// members through caller-owned cancellation and movement operations.
    pub fn SyncGeometryWithWork(&self, work: &dyn ClusterGeometryWork) -> GoResult<()> {
        work.Step()?;
        let vessel = self.b().Vessel.clone();
        self.resizeWithWork(Some(&vessel), work)?;
        self.arrangeNodesWithWork(work)
    }

    /// isActive is true while a Cluster's vessel is in the graph replacing the cluster nodes.
    pub(crate) fn isActive(&self) -> bool {
        let v = self.b().Vessel.clone();
        let active = v.b().Graph.is_some();
        active
    }
}

/// Go `c.isActive()` on a possibly-nil cluster.
#[inline]
pub fn clusterIsActive(c: Option<&ClusterRef>) -> bool {
    match c {
        None => false,
        Some(c) => c.isActive(),
    }
}

impl GraphRef {
    /// SyncClusters synchronizes every active cluster in nested graph order.
    pub fn SyncClusters(&self) {
        let clusters = self.b().Clusters.clone();
        if clusters.len() == 0 {
            return;
        }

        let mut sync = |n: &NodeRef| {
            if n.b().isClusterVessel {
                let c = clusters.get(n).expect("nil pointer dereference: cluster");
                c.SyncGeometry();
            }
        };
        for n in self.nodes() {
            n.rdfsWalk(&mut sync);
        }
    }
}
