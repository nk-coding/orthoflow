// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/packing/guard.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

pub(crate) fn newWorkGuard(ctx: &Ctx, limit: i64) -> GoResult<WorkGuardRef> {
    let guard = limits::NewWorkGuard(ctx, "BinPack", limits::MaxEngineWorkUnits)?;
    guard.SetLimit(limit);
    Ok(guard)
}

pub(crate) fn binPackScoreGuarded(nodes: &[NodeRef], root: Option<&NodeRef>, guard: &WorkGuardRef) -> GoResult<f64> {
    let (topLeft, bottomRight) = graphbounds::FixedBoundingBox(nodes, guard)?;
    let (topLeft, bottomRight) = (*topLeft.b(), *bottomRight.b());
    let width = bottomRight.X - topLeft.X;
    let height = bottomRight.Y - topLeft.Y;
    let mut desiredAxisPenalty = 0.0;
    if let Some(root) = root {
        let (dw, dh) = {
            let r = root.b();
            (r.DesiredWidth, r.DesiredHeight)
        };
        if dw.is_some() && dh.is_none() && width < dw.unwrap() {
            desiredAxisPenalty = dw.unwrap() - width;
        } else if dw.is_none() && dh.is_some() && height < dh.unwrap() {
            desiredAxisPenalty = dh.unwrap() - height;
        }
    }
    let score = width * height
        + math::Pow(width - height, 2.0) * subgraphSquareDampener
        + math::Pow(desiredAxisPenalty, 2.0) * subgraphSquareDampener;
    guard.Finish()?;
    Ok(score)
}

pub(crate) fn binPackContainerTopLeft(g: &GraphRef, container: Option<&NodeRef>, children: &[NodeRef], guard: &WorkGuardRef) -> GoResult<geo::PointRef> {
    let (topLeft, bottomRight) = graphbounds::FixedBoundingBox(children, guard)?;
    let Some(container) = container else {
        guard.Finish()?;
        return Ok(topLeft);
    };
    let padding = g.ContainerPadding(Some(container), false);
    let (tl, br) = (*topLeft.b(), *bottomRight.b());
    let inside = container.InsidePlacement(br.X - tl.X, br.Y - tl.Y, padding);
    guard.Finish()?;
    Ok(geo::PointRef::new(inside))
}

pub(crate) fn binPackIsDescendantOf(descendant: Option<&NodeRef>, ancestor: Option<&NodeRef>, guard: &WorkGuardRef) -> GoResult<bool> {
    let mut seen: Set<NodeRef> = Set::default();
    let mut descendant = descendant.cloned();
    loop {
        guard.Step()?;
        if descendant.as_ref() == ancestor {
            return Ok(true);
        }
        let Some(d) = descendant else {
            return Ok(false);
        };
        if seen.contains(&d) {
            return Err(invariant::New("BinPack found a cycle in node ancestry"));
        }
        seen.insert(d.clone());
        let (container, cluster, sequence) = {
            let n = d.b();
            (n.Container.clone(), n.Cluster.clone(), n.Sequence.clone())
        };
        if let Some(c) = container {
            descendant = Some(c);
        } else if let Some(cl) = cluster {
            descendant = Some(cl.b().Vessel.clone());
        } else if let Some(s) = sequence {
            descendant = Some(s.b().Vessel.clone());
        } else {
            return Ok(ancestor.is_none());
        }
    }
}

pub(crate) fn binPackHasExternalConnection(
    g: &GraphRef,
    node: &NodeRef,
    container: Option<&NodeRef>,
    near: bool,
    containerIsExternal: bool,
    guard: &WorkGuardRef,
) -> GoResult<bool> {
    let Some(container) = container else {
        return Ok(false);
    };
    let descendants = g.AllDescendantNodesWithWorkGuard(Some(node), true, guard)?;
    let mut toVisit: Vec<NodeRef> = Vec::with_capacity(descendants.len() + 1);
    toVisit.push(node.clone());
    for descendant in descendants {
        guard.Step()?;
        toVisit.push(descendant);
    }
    for current in &toVisit {
        guard.Step()?;
        if near {
            let orderedNears = binPackOrderedNears(current, guard)?;
            for adjacent in &orderedNears {
                let inside = binPackIsDescendantOf(Some(adjacent), Some(container), guard)?;
                if !inside {
                    return Ok(true);
                }
            }
            continue;
        }
        for edge in current.edges() {
            guard.Step()?;
            let adjacent = current.Adjacent(&edge);
            // Once routes exist, moving a descendant connected to this container
            // translates the complete route and pulls the container endpoint off its
            // fixed border. Treat that attachment like a cross-container constraint.
            if containerIsExternal && adjacent == *container {
                return Ok(true);
            }
            let inside = binPackIsDescendantOf(Some(&adjacent), Some(container), guard)?;
            if !inside {
                return Ok(true);
            }
        }
    }
    guard.Finish()?;
    Ok(false)
}

pub(crate) fn binPackOrderedNears(node: &NodeRef, guard: &WorkGuardRef) -> GoResult<Vec<NodeRef>> {
    let mut nears: Vec<NodeRef> = {
        let n = node.b();
        if n.Nears.len() as i64 > limits::MaxEngineNodes {
            return Err(invariant::New("BinPack node near count exceeds node limit"));
        }
        n.Nears.iter().cloned().collect()
    };
    for _ in 0..nears.len() {
        guard.Step()?;
    }
    // Account for the comparison sort before entering the small, bounded legacy
    // sorter, so every recursive BinPack helper consumes the same stage budget.
    let mut width = nears.len();
    while width > 1 {
        for _ in 0..nears.len() {
            guard.Step()?;
        }
        width = (width + 1) / 2;
    }
    layoutgraph::SortNodesByID(&mut nears);
    guard.Finish()?;
    Ok(nears)
}

#[derive(Clone)]
pub(crate) struct binPackHierarchyBox {
    pub(crate) topLeft: geo::PointRef,
    pub(crate) bottomRight: geo::PointRef,
}

pub(crate) fn binPackHierarchyBoxes(packed: &[Vec<NodeRef>], guard: &WorkGuardRef) -> GoResult<Vec<binPackHierarchyBox>> {
    let mut boxes: Vec<binPackHierarchyBox> = Vec::new();
    for nodes in packed {
        guard.Step()?;
        if nodes.is_empty() {
            return Err(invariant::New("BinPack contains an empty packed subgraph"));
        }
        if nodes[0].b().Hierarchy.is_none() {
            continue;
        }
        let (topLeft, bottomRight) = graphbounds::BoundingBox(nodes, guard)?;
        boxes.push(binPackHierarchyBox { topLeft, bottomRight });
    }
    guard.Finish()?;
    Ok(boxes)
}

pub(crate) fn binPackPointInHierarchy(boxes: &[binPackHierarchyBox], point: &geo::Point, guard: &WorkGuardRef) -> GoResult<bool> {
    for bx in boxes {
        guard.Step()?;
        let (tl, br) = (*bx.topLeft.b(), *bx.bottomRight.b());
        if tl.X <= point.X && point.X <= br.X && tl.Y <= point.Y && point.Y <= br.Y {
            return Ok(true);
        }
    }
    guard.Finish()?;
    Ok(false)
}

/// Widen the child bounds for labels wider than their node (shared by
/// binPackWrapChildren and binPackPositionContainerChildren).
fn widenForChildLabels(children: &[NodeRef], topLeft: &geo::PointRef, bottomRight: &geo::PointRef, guard: &WorkGuardRef, checkPlaced: bool) -> GoResult<()> {
    for child in children {
        guard.Step()?;
        if checkPlaced && !child.has_tl() {
            return Err(invariant::New("BinPack container contains an unplaced child"));
        }
        let Some(lbl) = child.label() else { continue };
        let lw = lbl.b().Width;
        let (cx, cw) = (child.x(), child.w());
        let (tlx, brx) = (topLeft.b().X, bottomRight.b().X);
        if (cx == tlx || cx + cw == brx) && lw > cw {
            topLeft.bm().X = math::Min(tlx, (cx + (cw / 2.0 - lw / 2.0)).floor());
            let brx = bottomRight.b().X;
            bottomRight.bm().X = math::Max(brx, (cx + cw - (cw / 2.0 - lw / 2.0)).ceil());
        }
    }
    Ok(())
}

pub(crate) fn binPackWrapChildren(root: &NodeRef, guard: &WorkGuardRef) -> GoResult<()> {
    if !root.IsContainer() {
        return Ok(());
    }
    let graph = root.g();
    let children = graph.b().Containers.at(&Some(root.clone()));
    let padding = graph.ContainerPadding(Some(root), false);
    let (topLeft, bottomRight) = graphbounds::FixedBoundingBox(&children, guard)?;
    widenForChildLabels(&children, &topLeft, &bottomRight, guard, false)?;
    root.FitToBoundingBox(&topLeft, &bottomRight, padding);
    let (tl, br) = (*topLeft.b(), *bottomRight.b());
    let inside = root.InsidePlacement(br.X - tl.X, br.Y - tl.Y, padding);
    root.Translate(tl.X - inside.X, tl.Y - inside.Y);
    guard.Finish()
}

pub(crate) fn binPackMoveNodeWithChildren(node: &NodeRef, dx: f64, dy: f64, guard: &WorkGuardRef) -> GoResult<()> {
    guard.Step()?;
    if dx == 0.0 && dy == 0.0 {
        return Ok(());
    }
    if !node.has_tl() || node.graph().is_none() {
        return Err(invariant::New("BinPack cannot move an unplaced node or a node without a graph"));
    }
    node.Translate(dx, dy);
    let descendants = node.g().AllDescendantNodesWithWorkGuard(Some(node), true, guard)?;
    for child in &descendants {
        guard.Step()?;
        if !child.has_tl() {
            return Err(invariant::New("BinPack cannot move an unplaced descendant"));
        }
        child.Translate(dx, dy);
    }
    guard.Finish()
}

pub(crate) fn binPackPositionContainerChildren(node: &NodeRef, guard: &WorkGuardRef) -> GoResult<()> {
    guard.Step()?;
    if !node.IsContainer() {
        return Ok(());
    }
    let Some(graph) = node.graph() else {
        return Err(invariant::New("BinPack container has no graph"));
    };
    let children = graph.b().Containers.at(&Some(node.clone()));
    let (topLeft, bottomRight) = graphbounds::FixedBoundingBox(&children, guard)?;
    widenForChildLabels(&children, &topLeft, &bottomRight, guard, true)?;
    let (tl, br) = (*topLeft.b(), *bottomRight.b());
    let inside = node.InsidePlacement(br.X - tl.X, br.Y - tl.Y, layoutgraph::Spacing::default());
    let dx = inside.X - tl.X;
    let dy = inside.Y - tl.Y;
    for child in &children {
        binPackMoveNodeWithChildren(child, dx, dy, guard)?;
    }
    guard.Finish()
}

pub(crate) struct binPackClusterGeometryWork {
    pub(crate) guard: WorkGuardRef,
}

impl layoutgraph::SharedWorkStepper for binPackClusterGeometryWork {
    fn Step(&self) -> GoResult<()> {
        self.guard.Step()
    }
    fn Finish(&self) -> GoResult<()> {
        self.guard.Finish()
    }
}

impl layoutgraph::ClusterGeometryWork for binPackClusterGeometryWork {
    fn MoveNodeWithChildren(&self, node: &NodeRef, dx: f64, dy: f64) -> GoResult<()> {
        binPackMoveNodeWithChildren(node, dx, dy, &self.guard)
    }
    fn PositionContainerChildren(&self, node: &NodeRef) -> GoResult<()> {
        binPackPositionContainerChildren(node, &self.guard)
    }
}

struct binPackRDFSFrame {
    node: NodeRef,
    phase: u8,
    index: usize,
}

pub(crate) fn binPackReverseDFSWalk(root: &NodeRef, apply: &mut dyn FnMut(&NodeRef) -> GoResult<()>, guard: &WorkGuardRef) -> GoResult<()> {
    let mut stack: Vec<binPackRDFSFrame> = vec![binPackRDFSFrame { node: root.clone(), phase: 0, index: 0 }];
    let mut active: Set<NodeRef> = Set::default();
    active.insert(root.clone());
    while !stack.is_empty() {
        guard.Step()?;
        let top = stack.len() - 1;
        let frameNode = stack[top].node.clone();
        let Some(graph) = frameNode.graph() else {
            return Err(invariant::New("BinPack reverse traversal found a node without a graph"));
        };
        let children: Vec<NodeRef>;
        match stack[top].phase {
            0 => {
                if frameNode.IsContainer() {
                    children = graph.b().Containers.at(&Some(frameNode.clone()));
                } else {
                    children = Vec::new();
                }
            }
            1 => {
                if frameNode.IsClusterVessel() {
                    let cluster = graph.b().Clusters.get(&frameNode);
                    let Some(cluster) = cluster else {
                        return Err(invariant::New("BinPack reverse traversal found a cluster vessel without a cluster"));
                    };
                    children = cluster.b().Nodes.clone();
                } else if let Some(sequence) = graph.b().Sequences.get(&frameNode) {
                    children = sequence.b().Nodes.clone();
                } else {
                    children = Vec::new();
                }
            }
            _ => {
                apply(&frameNode)?;
                active.remove(&frameNode);
                stack.pop();
                continue;
            }
        }
        let frame = &mut stack[top];
        if frame.index >= children.len() {
            frame.phase += 1;
            frame.index = 0;
            continue;
        }
        let child = children[frame.index].clone();
        frame.index += 1;
        if active.contains(&child) {
            return Err(invariant::New("BinPack reverse traversal found a topology cycle"));
        }
        active.insert(child.clone());
        stack.push(binPackRDFSFrame { node: child, phase: 0, index: 0 });
    }
    guard.Finish()
}

pub(crate) fn binPackSyncClusters(graph: &GraphRef, guard: &WorkGuardRef) -> GoResult<()> {
    let clusters = graph.b().Clusters.clone();
    if clusters.len() == 0 {
        return guard.Finish();
    }
    let work = binPackClusterGeometryWork { guard: guard.clone() };
    for root in graph.nodes() {
        guard.Step()?;
        binPackReverseDFSWalk(
            &root,
            &mut |node: &NodeRef| -> GoResult<()> {
                if !node.IsClusterVessel() {
                    return Ok(());
                }
                let cluster = clusters.get(node).expect("nil pointer dereference: cluster");
                cluster.SyncGeometryWithWork(&work)
            },
            guard,
        )?;
    }
    guard.Finish()
}

pub(crate) fn binPackSyncSequences(graph: &GraphRef, guard: &WorkGuardRef) -> GoResult<()> {
    let sequences = graph.b().Sequences.clone();
    if sequences.len() == 0 {
        return guard.Finish();
    }
    for root in graph.nodes() {
        guard.Step()?;
        binPackReverseDFSWalk(
            &root,
            &mut |node: &NodeRef| -> GoResult<()> {
                let Some(sequence) = sequences.get(node) else {
                    return Ok(());
                };
                sequence.SyncGeometryWithWork(guard)
            },
            guard,
        )?;
    }
    guard.Finish()
}

pub(crate) fn binPackSmallestDeltas(subgraphs: &[Vec<NodeRef>], guard: &WorkGuardRef) -> GoResult<(f64, f64)> {
    let (mut x, mut y) = (f64::INFINITY, f64::INFINITY);
    for subgraph in subgraphs {
        guard.Step()?;
        let (topLeft, bottomRight) = graphbounds::BoundingBox(subgraph, guard)?;
        let (tl, br) = (*topLeft.b(), *bottomRight.b());
        x = math::Min(x, br.X - tl.X);
        y = math::Min(y, br.Y - tl.Y);
    }
    guard.Finish()?;
    Ok((x, y))
}

/// allEdgesHaveCompleteRoutesGuarded enforces the all-or-none route state that
/// BinPack relies on.
pub(crate) fn allEdgesHaveCompleteRoutesGuarded(graph: &GraphRef, root: Option<&NodeRef>, guard: &WorkGuardRef) -> GoResult<(bool, Vec<EdgeRef>)> {
    let descendants: Vec<NodeRef> = match root {
        None => graph.nodes(),
        Some(root) => graph.AllDescendantNodesWithWorkGuard(Some(root), true, guard)?,
    };
    let mut affectedNodes: Vec<NodeRef> = Vec::with_capacity(descendants.len() + 1);
    if let Some(root) = root {
        affectedNodes.push(root.clone());
    }
    affectedNodes.extend(descendants);
    let mut affectedNodeSet: Set<NodeRef> = Set::default();
    for node in &affectedNodes {
        guard.Step()?;
        if node.graph().as_ref() != Some(graph) {
            return Err(invariant::New("BinPack affected-node inventory is malformed"));
        }
        if affectedNodeSet.contains(node) {
            return Err(invariant::Errorf(format!("BinPack affected-node inventory repeats node {}", node.id())));
        }
        affectedNodeSet.insert(node.clone());
    }

    let mut expectedNodeEdges: Map<NodeRef, Map<EdgeRef, i64>> = Map::default();
    let mut expectedNodeEdgeTotals: Map<NodeRef, i64> = Map::default();
    for node in &affectedNodes {
        expectedNodeEdges.insert(node.clone(), Map::default());
    }
    let mut affectedGraphEdges: Set<EdgeRef> = Set::default();
    let mut routedEdges: i64 = 0;
    let mut incidentEdges: Vec<EdgeRef> = Vec::new();
    let edges = graph.edges();
    for edge in &edges {
        guard.Step()?;
        let (from, to, npoints, id) = {
            let e = edge.b();
            (e.From.clone(), e.To.clone(), e.Points.len(), e.ID)
        };
        if from.graph().as_ref() != Some(graph) || to.graph().as_ref() != Some(graph) {
            return Err(invariant::Errorf(format!("BinPack edge {} has an endpoint owned by another graph", id)));
        }
        let mut edgeAffectsRoot = false;
        if affectedNodeSet.contains(&from) {
            edgeAffectsRoot = true;
            *expectedNodeEdges.get_mut(&from).unwrap().entry(edge.clone()).or_insert(0) += 1;
            *expectedNodeEdgeTotals.entry(from.clone()).or_insert(0) += 1;
        }
        if to != from {
            if affectedNodeSet.contains(&to) {
                edgeAffectsRoot = true;
                *expectedNodeEdges.get_mut(&to).unwrap().entry(edge.clone()).or_insert(0) += 1;
                *expectedNodeEdgeTotals.entry(to.clone()).or_insert(0) += 1;
            }
        }
        if edgeAffectsRoot {
            if affectedGraphEdges.contains(edge) {
                return Err(invariant::Errorf(format!("BinPack graph edge inventory repeats affected edge {}", id)));
            }
            affectedGraphEdges.insert(edge.clone());
        }
        if npoints == 0 {
            continue;
        }
        if npoints < 2 {
            return Err(invariant::Errorf(format!("BinPack edge {} has an incomplete route with {} point(s)", id, npoints)));
        }
        for _ in 0..npoints {
            guard.Step()?;
        }
        routedEdges += 1;
        if let Some(root) = root {
            if from == *root || to == *root {
                incidentEdges.push(edge.clone());
            }
        }
    }
    if routedEdges != 0 && routedEdges != edges.len() as i64 {
        return Err(invariant::Errorf(format!(
            "BinPack graph is partially routed: {} of {} edges have routes",
            routedEdges,
            edges.len()
        )));
    }

    for node in &affectedNodes {
        let mut actualNodeEdges: Map<EdgeRef, i64> = Map::default();
        let nodeEdges = node.edges();
        for edge in &nodeEdges {
            guard.Step()?;
            let (from, to) = (edge.from(), edge.to());
            if from != *node && to != *node {
                return Err(invariant::Errorf(format!(
                    "BinPack node {} edge inventory contains non-incident edge {}",
                    node.id(),
                    edge.id()
                )));
            }
            let actual = {
                let a = actualNodeEdges.entry(edge.clone()).or_insert(0);
                *a += 1;
                *a
            };
            let expectedOccurrences = expectedNodeEdges.get(node).and_then(|m| m.get(edge)).copied().unwrap_or(0);
            if actual > expectedOccurrences {
                return Err(invariant::Errorf(format!(
                    "BinPack node {} edge inventory has {} occurrence(s) of edge {}; graph requires {}",
                    node.id(),
                    actual,
                    edge.id(),
                    expectedOccurrences
                )));
            }
        }
        let total = expectedNodeEdgeTotals.get(node).copied().unwrap_or(0);
        if nodeEdges.len() as i64 != total {
            return Err(invariant::Errorf(format!(
                "BinPack node {} edge inventory has {} occurrence(s); graph requires {}",
                node.id(),
                nodeEdges.len(),
                total
            )));
        }
    }
    if routedEdges == 0 {
        guard.Finish()?;
        return Ok((false, Vec::new()));
    }
    guard.Finish()?;
    Ok((true, incidentEdges))
}
