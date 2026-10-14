// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/placement/direct.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of placement/direct.go
use crate::*;
use gocompat::Ctx;
use gocompat::{sort, GoResult, Map, Set};
use layoutgraph::{EdgeRef, GraphRef, NodeRef, TreeRef};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct directionCounts {
    pub left: i64,
    pub right: i64,
    pub top: i64,
    pub bottom: i64,
}

impl directionCounts {
    pub fn add(&mut self, other: directionCounts) {
        self.left += other.left;
        self.right += other.right;
        self.top += other.top;
        self.bottom += other.bottom;
    }
}

pub fn edgeDirectionCounts(edge: &EdgeRef) -> directionCounts {
    let mut counts = directionCounts::default();
    let (from, to, directed) = edge.DirectedEndpoints();
    if !directed {
        return counts;
    }
    let (from, to) = (from.expect("directed endpoint"), to.expect("directed endpoint"));
    match from.Orientation(&to).GetOpposite() {
        geo::Left => counts.left += 1,
        geo::TopLeft => {
            counts.top += 1;
            counts.left += 1;
        }
        geo::BottomLeft => {
            counts.bottom += 1;
            counts.left += 1;
        }
        geo::Right => counts.right += 1,
        geo::TopRight => {
            counts.top += 1;
            counts.right += 1;
        }
        geo::BottomRight => {
            counts.bottom += 1;
            counts.right += 1;
        }
        geo::Top => counts.top += 1,
        geo::Bottom => counts.bottom += 1,
        _ => {}
    }
    counts
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct directionTransforms {
    pub mirrorX: bool,
    pub mirrorY: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct directionCount {
    pub direction: geo::Orientation,
    pub count: i64,
}

pub fn compareDirectionCounts(a: &directionCount, b: &directionCount, preferred: geo::Orientation) -> i64 {
    let order = sort::cmpCompare(b.count, a.count);
    if order != 0 {
        return order;
    }
    let aPreferred = a.direction == preferred;
    let bPreferred = b.direction == preferred;
    if aPreferred && !bPreferred {
        -1
    } else if !aPreferred && bPreferred {
        1
    } else {
        0
    }
}

impl directionCounts {
    pub fn transformsTo(&self, mut direction: geo::Orientation) -> directionTransforms {
        let mut values = vec![
            directionCount { direction: geo::Right, count: self.right },
            directionCount { direction: geo::Bottom, count: self.bottom },
            directionCount { direction: geo::Left, count: self.left },
            directionCount { direction: geo::Top, count: self.top },
        ];
        sort::SlicesSortStableFunc(&mut values, |a, b| compareDirectionCounts(a, b, direction));
        let primary = values[0];
        let mut secondary = values[1];
        if secondary.direction == primary.direction.GetOpposite() {
            secondary = values[2];
        }
        if direction == geo::NONE {
            if primary.direction.IsHorizontal() {
                direction = geo::Right;
            } else {
                direction = geo::Bottom;
            }
        }
        let (mut xDirection, mut yDirection) = (geo::Right, geo::Bottom);
        if direction.IsHorizontal() {
            xDirection = direction;
        } else {
            yDirection = direction;
        }
        let mut transforms = directionTransforms::default();
        if primary.direction.IsHorizontal() {
            transforms.mirrorX = primary.direction != xDirection;
        } else {
            transforms.mirrorY = primary.direction != yDirection;
        }
        if secondary.count > values[3].count {
            if secondary.direction.IsHorizontal() {
                transforms.mirrorX = secondary.direction != xDirection;
            } else {
                transforms.mirrorY = secondary.direction != yDirection;
            }
        }
        transforms
    }
}

pub fn containerEdgeDirections(graph: &GraphRef, container: Option<&NodeRef>) -> directionCounts {
    let mut counted: Set<EdgeRef> = Set::default();
    let mut counts = directionCounts::default();
    let children = graph.b().Containers.at(&container.cloned());
    for node in &children {
        if node.graph().as_ref() != Some(graph) {
            continue;
        }
        for edge in node.edges() {
            if counted.contains(&edge) {
                continue;
            }
            counts.add(edgeDirectionCounts(&edge));
            counted.insert(edge);
        }
    }
    counts
}

pub fn hasFixedDescendant(node: &NodeRef) -> bool {
    if node.b().FixedTopLeft.is_some() {
        return true;
    }
    let g = node.g();
    if node.IsContainer() {
        let children = g.b().Containers.at(&Some(node.clone()));
        for child in &children {
            if hasFixedDescendant(child) {
                return true;
            }
        }
    }
    if node.IsClusterVessel() {
        let cluster = g.b().Clusters.get(node).expect("nil pointer dereference: cluster");
        let nodes = cluster.b().Nodes.clone();
        for child in &nodes {
            if hasFixedDescendant(child) {
                return true;
            }
        }
    } else {
        let sequence = g.b().Sequences.get(node);
        if let Some(sequence) = sequence {
            let nodes = sequence.b().Nodes.clone();
            for child in &nodes {
                if hasFixedDescendant(child) {
                    return true;
                }
            }
        }
    }
    false
}

pub fn mirrorAxes(ctx: &Ctx, graph: &GraphRef, mirrorX: bool, mirrorY: bool) -> GoResult<()> {
    let guard = limits::NewWorkGuard(ctx, "MirrorAxesReachability", limits::MaxEngineWorkUnits)?;
    let mut reachableContainers: Set<Option<NodeRef>> = Set::default();
    let mut visited: Set<NodeRef> = Set::default();
    let nodes = graph.b().Nodes.clone();
    for node in &nodes {
        if visited.contains(node) {
            continue;
        }
        let reachable = node.AllReachableNodesContext(true, true, false, None, &guard)?;
        for current in &reachable {
            visited.insert(current.clone());
            reachableContainers.insert(current.containerField());
        }
    }
    guard.Finish()?;

    let originalPositions = snapshotNodePositionsContext(ctx, "MirrorAxes", &nodes)?;
    let mut originalTreeOrientations: Vec<(TreeRef, geo::Orientation)> = Vec::new();
    {
        let nodeToTree = graph.b().NodeToTree.clone();
        let mut seenTrees: Set<TreeRef> = Set::default();
        for snapshot in &originalPositions {
            if let Some(tree) = nodeToTree.get(&snapshot.node) {
                if seenTrees.insert(tree.clone()) {
                    let o = tree.b().Orientation;
                    originalTreeOrientations.push((tree, o));
                }
            }
        }
    }

    let result = (|| -> GoResult<()> {
        let nodeToTree = graph.b().NodeToTree.clone();
        let mirrored: std::cell::RefCell<Set<NodeRef>> = std::cell::RefCell::new(Set::default());
        let mutationErr: std::cell::RefCell<Option<gocompat::GoError>> = std::cell::RefCell::new(None);
        let mut mirrorNode = |node: &NodeRef| {
            if mutationErr.borrow().is_some() {
                return;
            }
            if let Err(err) = guard.Step() {
                *mutationErr.borrow_mut() = Some(err);
                return;
            }
            if mirrored.borrow().contains(node) {
                return;
            }
            if reachableContainers.contains(&node.EffectiveContainer()) {
                node.Mirror(mirrorX, mirrorY);
                mirrored.borrow_mut().insert(node.clone());
                if let Err(err) = guard.Finish() {
                    *mutationErr.borrow_mut() = Some(err);
                    return;
                }
                if let Some(tree) = nodeToTree.get(node) {
                    let o = tree.b().Orientation;
                    if mirrorX && o.IsHorizontal() || mirrorY && o.IsVertical() {
                        tree.bm().Orientation = o.GetOpposite();
                        if let Err(err) = guard.Finish() {
                            *mutationErr.borrow_mut() = Some(err);
                            return;
                        }
                    }
                }
            }
            node.PositionContainerChildren(true);
            if let Err(err) = guard.Finish() {
                *mutationErr.borrow_mut() = Some(err);
            }
        };
        for node in &nodes {
            if mutationErr.borrow().is_some() {
                break;
            }
            let seen = mirrored.borrow().contains(node);
            if !seen {
                node.WalkRDFS(&mut mirrorNode);
            }
        }
        let mutationErr = mutationErr.into_inner();
        if let Some(err) = mutationErr {
            return Err(err);
        }
        guard.Finish()
    })();
    if result.is_err() {
        restoreNodePositions(&originalPositions);
        for (tree, orientation) in &originalTreeOrientations {
            tree.bm().Orientation = *orientation;
        }
    }
    result
}

// direct mirrors a placed subgraph toward its dominant edge direction when
// doing so preserves or improves the placement cost.
#[derive(Clone, Copy, Debug, Default)]
pub struct directOptions {
    pub checkEdgeLength: bool,
}

pub fn direct(ctx: &Ctx, graph: &GraphRef, nodes: &[NodeRef], container: Option<&NodeRef>, options: directOptions) -> GoResult<()> {
    if let Some(err) = ctx.Err() {
        return Err(err);
    }
    if nodes.is_empty() || nodes[0].b().Hierarchy.is_some() {
        return Ok(());
    }
    for node in nodes {
        if hasFixedDescendant(node) {
            return Ok(());
        }
        let sequence = graph.b().Sequences.get(node);
        if let Some(sequence) = sequence {
            if !sequence.b().EdgeAbductions.is_empty() {
                return Ok(());
            }
        }
        if let Some(sequence) = node.sequenceField() {
            if !sequence.b().EdgeAbductions.is_empty() {
                return Ok(());
            }
        }
    }
    let transforms = containerEdgeDirections(graph, container).transformsTo(graph.Direction(container));
    if !transforms.mirrorX && !transforms.mirrorY {
        return Ok(());
    }
    if !options.checkEdgeLength {
        return mirrorAxes(ctx, graph, transforms.mirrorX, transforms.mirrorY);
    }
    let currentLength = placementcost::EdgeLength(ctx, graph, &edgeLengthOptions(None, true))?;
    let transaction = graph.NewRequestTransaction(ctx, layoutgraph::TransactionOptions { AffectContainers: true, ..Default::default() })?;
    {
        let (ctx2, graph2) = (ctx.clone(), graph.clone());
        transaction.AddOp(move || mirrorAxes(&ctx2, &graph2, transforms.mirrorX, transforms.mirrorY));
    }
    if let Err(err) = transaction.Commit(ctx) {
        if layoutgraph::IsCandidateRejection(Some(&err)) {
            return Ok(());
        }
        return Err(err);
    }
    let newLength = match placementcost::EdgeLength(ctx, graph, &edgeLengthOptions(None, true)) {
        Ok(v) => v,
        Err(err) => {
            transaction.Rollback();
            return Err(err);
        }
    };
    if geo::PrecisionCompare(newLength, currentLength, geo::PRECISION) > 0 {
        transaction.Rollback();
    }
    Ok(())
}
