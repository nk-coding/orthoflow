// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/placement/types.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of placement/types.go
use crate::*;
use gocompat::Ctx;
use gocompat::{errorf, math, GoResult, Map, Set};
use layoutgraph::{GraphRef, NodeRef};

pub const maxCompactionCandidateCount: i64 = limits::MaxGraphSize + 3;

/// Go `pointerSnapshot[geo.Point]` (the only instantiation used).
#[derive(Clone)]
pub struct pointerSnapshot {
    pub pointer: Option<geo::PointRef>,
    pub value: geo::Point,
}

pub fn snapshotPointer(pointer: Option<&geo::PointRef>) -> pointerSnapshot {
    match pointer {
        None => pointerSnapshot { pointer: None, value: geo::Point::default() },
        Some(p) => {
            let value = *p.b();
            pointerSnapshot { pointer: Some(p.clone()), value }
        }
    }
}

impl pointerSnapshot {
    pub fn restore(&self) -> Option<geo::PointRef> {
        let p = self.pointer.as_ref()?;
        *p.bm() = self.value;
        Some(p.clone())
    }
}

#[derive(Clone)]
pub struct nodePositionSnapshot {
    pub node: NodeRef,
    pub topLeft: pointerSnapshot,
}

impl nodePositionSnapshot {
    #[inline]
    pub fn of(node: &NodeRef) -> nodePositionSnapshot {
        let tl = node.tl();
        nodePositionSnapshot { node: node.clone(), topLeft: snapshotPointer(tl.as_ref()) }
    }
}

pub fn restoreNodePositions(snapshots: &[nodePositionSnapshot]) {
    for snapshot in snapshots {
        snapshot.node.set_tl(snapshot.topLeft.restore());
    }
}

pub fn snapshotNodePositionsContext(ctx: &Ctx, location: &str, nodes: &[NodeRef]) -> GoResult<Vec<nodePositionSnapshot>> {
    let guard = limits::NewWorkGuard(ctx, location, limits::MaxEngineWorkUnits)?;
    let mut seen: Set<NodeRef> = Set::default();
    let mut snapshots: Vec<nodePositionSnapshot> = Vec::with_capacity(nodes.len());
    let mut queue: Vec<(NodeRef, Option<GraphRef>)> = Vec::with_capacity(nodes.len());
    let mut enqueue = |node: &NodeRef,
                       graph: Option<GraphRef>,
                       seen: &mut Set<NodeRef>,
                       snapshots: &mut Vec<nodePositionSnapshot>,
                       queue: &mut Vec<(NodeRef, Option<GraphRef>)>|
     -> GoResult<()> {
        if seen.contains(node) {
            return Ok(());
        }
        if seen.len() as i64 >= limits::MaxEngineNodes {
            return Err(errorf!("TALA {} unique node count exceeds limit {}", location, limits::MaxEngineNodes));
        }
        guard.Step()?;
        seen.insert(node.clone());
        snapshots.push(nodePositionSnapshot::of(node));
        queue.push((node.clone(), graph));
        Ok(())
    };
    for node in nodes {
        enqueue(node, node.graph(), &mut seen, &mut snapshots, &mut queue)?;
    }
    let mut index = 0;
    while index < queue.len() {
        let (node, graph) = queue[index].clone();
        index += 1;
        let Some(graph) = graph else { continue };
        if node.IsContainer() {
            let children = graph.b().Containers.at(&Some(node.clone()));
            for child in &children {
                enqueue(child, Some(graph.clone()), &mut seen, &mut snapshots, &mut queue)?;
            }
        }
        if node.IsClusterVessel() {
            let cluster = graph.b().Clusters.get(&node);
            if let Some(cluster) = cluster {
                let children = cluster.b().Nodes.clone();
                for child in &children {
                    enqueue(child, Some(graph.clone()), &mut seen, &mut snapshots, &mut queue)?;
                }
            }
        }
        let sequence = graph.b().Sequences.get(&node);
        if let Some(sequence) = sequence {
            let children = sequence.b().Nodes.clone();
            for child in &children {
                enqueue(child, Some(graph.clone()), &mut seen, &mut snapshots, &mut queue)?;
            }
        }
    }
    guard.Finish()?;
    Ok(snapshots)
}

#[inline]
pub fn nonNilEquals(first: &geo::Point, second: &geo::Point) -> bool {
    first.X == second.X && first.Y == second.Y
}

#[inline]
pub fn roundToPreviousCellSize(value: f64, cellSize: f64) -> f64 {
    math::Floor(value / cellSize) * cellSize
}

#[inline]
pub fn roundToNearestCellSize(value: f64, cellSize: f64) -> f64 {
    math::Round(value / cellSize) * cellSize
}

pub fn numPointsWithinManhattanDistance(distance: f64) -> i64 {
    math::F64ToInt(math::Ceil(math::Pow(distance, 2.0) + math::Pow(distance + 1.0, 2.0)))
}

/// `placementcost.EdgeLengthOptions{EdgeAbductions: abductions, IncludeNodeSizes:
/// includeSizes, EnforceMinimumGap: false, PenalizeDirection: true}` (the only
/// option combination the optimizers use). `None` is Go's nil slice.
#[inline]
pub(crate) fn edgeLengthOptions(abductions: Option<&[layoutgraph::EdgeAbductionRef]>, includeSizes: bool) -> placementcost::EdgeLengthOptions {
    placementcost::EdgeLengthOptions {
        EdgeAbductions: abductions.map(|a| a.to_vec()),
        IncludeNodeSizes: includeSizes,
        EnforceMinimumGap: false,
        PenalizeDirection: true,
    }
}
