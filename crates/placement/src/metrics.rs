// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/placement/metrics.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of placement/metrics.go
use crate::*;
use gocompat::sort;
use layoutgraph::{nodeEq, EdgeAbductionRef, GraphRef, NodeRef, TreeRef};

// occupied reports whether point is the top-left position of a graph node.
pub fn occupied(graph: &GraphRef, point: &geo::Point) -> (Option<NodeRef>, bool) {
    let gb = graph.b();
    for node in gb.Nodes.iter() {
        if let Some(tl) = node.tl() {
            if *tl.b() == *point {
                return (Some(node.clone()), true);
            }
        }
    }
    (None, false)
}

// withinMaxSize reports whether the graph fits inside the supported placement
// bounds.
pub fn withinMaxSize(graph: &GraphRef) -> bool {
    let (topLeft, bottomRight) = graph.BoundingBox();
    let (tl, br) = (*topLeft.unwrap().b(), *bottomRight.unwrap().b());
    let width = br.X - tl.X;
    let height = br.Y - tl.Y;
    width <= limits::MaxGraphSize as f64 && height <= limits::MaxGraphSize as f64
}

// intersectsOtherNode reports whether the center-to-center segment crosses an
// unrelated graph node.
pub fn intersectsOtherNode(graph: &GraphRef, first: &NodeRef, second: &NodeRef) -> bool {
    let firstCenter = first.Center();
    let secondCenter = second.Center();
    let nodes = graph.b().Nodes.clone();
    for other in &nodes {
        if other == first || other == second {
            continue;
        }
        if first.IsDescendantOf(Some(other)) || second.IsDescendantOf(Some(other)) {
            continue;
        }
        if other.IsDescendantOf(Some(first)) || other.IsDescendantOf(Some(second)) {
            continue;
        }
        if other.PassesThrough(Some(&firstCenter), Some(&secondCenter)) {
            return true;
        }
    }
    false
}

pub fn median(nodes: &[NodeRef], includeSizes: bool) -> (f64, f64) {
    let mut orderedByX: Vec<NodeRef> = nodes.to_vec();
    let mut orderedByY: Vec<NodeRef> = nodes.to_vec();
    sort::SortSlice(&mut orderedByX, |left, right| {
        let (mut leftX, mut rightX) = (left.x(), right.x());
        if includeSizes {
            leftX += left.w() / 2.0;
            rightX += right.w() / 2.0;
        }
        if leftX == rightX {
            return left.id() < right.id();
        }
        leftX < rightX
    });
    sort::SortSlice(&mut orderedByY, |left, right| {
        let (mut leftY, mut rightY) = (left.y(), right.y());
        if includeSizes {
            leftY += left.h() / 2.0;
            rightY += right.h() / 2.0;
        }
        if leftY == rightY {
            return left.id() < right.id();
        }
        leftY < rightY
    });

    let middle = nodes.len() / 2;
    if includeSizes {
        let mut medianX = orderedByX[middle].x() + orderedByX[middle].w() / 2.0;
        let mut medianY = orderedByY[middle].y() + orderedByY[middle].h() / 2.0;
        if nodes.len() % 2 == 0 {
            medianX = (medianX + orderedByX[middle - 1].x() + orderedByX[middle - 1].w() / 2.0) / 2.0;
            medianY = (medianY + orderedByY[middle - 1].y() + orderedByY[middle - 1].h() / 2.0) / 2.0;
        }
        let cellSize = nodes[0].g().b().CellSize;
        return (medianX / cellSize, medianY / cellSize);
    }
    let mut medianX = orderedByX[middle].x() + 0.5;
    let mut medianY = orderedByY[middle].y() + 0.5;
    if nodes.len() % 2 == 0 {
        medianX = (medianX + orderedByX[middle - 1].x()) / 2.0;
        medianY = (medianY + orderedByY[middle - 1].y()) / 2.0;
    }
    (medianX, medianY)
}

// medianToNeighbors approximates the two-dimensional geometric median of a
// node's currently positioned placement neighbors.
pub fn medianToNeighbors(node: &NodeRef, includeSizes: bool, abductions: &[EdgeAbductionRef]) -> (f64, f64) {
    median(&adjacents(node, abductions), includeSizes)
}

/// Active cluster/sequence vessel, then the root sentinel of its tree.
fn resolveNear(graph: &GraphRef, near: &NodeRef) -> NodeRef {
    let mut near = near.clone();
    let (cluster, sequence) = {
        let n = near.b();
        (n.Cluster.clone(), n.Sequence.clone())
    };
    if layoutgraph::clusterIsActive(cluster.as_ref()) {
        near = cluster.unwrap().b().Vessel.clone();
    } else if layoutgraph::sequenceIsActive(sequence.as_ref()) {
        near = sequence.unwrap().b().Vessel.clone();
    }
    let tree = graph.b().NodeToTree.get(&near);
    if let Some(mut tree) = tree {
        loop {
            let parent = tree.b().Parent.clone();
            match parent {
                Some(p) => tree = p,
                None => break,
            }
        }
        near = tree.SentinelNode();
    }
    near
}

pub fn adjacents(node: &NodeRef, abductions: &[EdgeAbductionRef]) -> Vec<NodeRef> {
    let edges = node.edges();
    let mut result: Vec<NodeRef> = Vec::with_capacity(edges.len());
    let mut usedScratch = borrowEdgeAbductionBools(abductions.len());
    {
        let used = &mut usedScratch.values;
        for edge in &edges {
            let adjacent = node.Adjacent(edge);
            if !adjacent.has_tl() {
                continue;
            }
            let mut add = adjacent.clone();
            for (i, abduction) in abductions.iter().enumerate() {
                if used[i] {
                    continue;
                }
                let a = abduction.b();
                if nodeEq(a.CurrentFrom.as_ref(), Some(node)) && nodeEq(a.CurrentTo.as_ref(), Some(&adjacent)) && a.OriginallyTo.is_some() {
                    used[i] = true;
                    add = a.OriginallyTo.clone().unwrap();
                    break;
                }
                if nodeEq(a.CurrentFrom.as_ref(), Some(&adjacent)) && nodeEq(a.CurrentTo.as_ref(), Some(node)) && a.OriginallyFrom.is_some() {
                    used[i] = true;
                    add = a.OriginallyFrom.clone().unwrap();
                    break;
                }
            }
            result.push(add);
        }
    }
    returnEdgeAbductionBools(usedScratch);
    if !result.is_empty() {
        return result;
    }

    let graph = node.g();
    let container = node.containerField();
    for near in node.OrderedNears() {
        let near = resolveNear(&graph, &near);
        if near.has_tl() && near.IsDescendantOf(container.as_ref()) {
            result.push(near);
        }
    }
    if !result.is_empty() {
        return result;
    }

    let mut nears: Vec<NodeRef> = Vec::new();
    if node.IsClusterVessel() {
        let cluster = graph.b().Clusters.get(node).expect("nil pointer dereference: cluster");
        let cn = cluster.b().Nodes.clone();
        for child in &cn {
            nears.extend(child.OrderedNears());
        }
    }
    let sequence = graph.b().Sequences.get(node);
    if let Some(sequence) = sequence {
        let sn = sequence.b().Nodes.clone();
        for child in &sn {
            nears.extend(child.OrderedNears());
        }
    }
    let trees = graph.b().Trees.at(&Some(node.clone()));
    for tree in &trees {
        let mut queue: std::collections::VecDeque<TreeRef> = std::collections::VecDeque::new();
        queue.push_back(tree.clone());
        while let Some(current) = queue.pop_front() {
            let (cn, children) = {
                let c = current.b();
                (c.Node.clone(), c.Children.clone())
            };
            nears.extend(cn.OrderedNears());
            queue.extend(children);
        }
    }
    for near in &nears {
        let add = resolveNear(&graph, near);
        if add.has_tl() {
            result.push(add);
        }
    }
    result
}
