// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/placement/optimization_guard.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of placement/optimization_guard.go
use crate::*;
use gocompat::{errorf, math, sort, GoError, GoResult, Map, Set};
use layoutgraph::{
    nodeEq, ClusterArrangement, ClusterRef, EdgeAbductionRef, GraphRef, HerdAssignment, HerdAssignmentRef, NodeRef,
    PlacementCostSnapshotRef, TreeRef,
};
use limits::OptimizationWorkGuardRef;

// A work limit bounds CPU, but retaining every unique placement until that
// limit is reached could still consume too much memory. This independent cap
// keeps the reusable placement buffer and its deduplication map predictable.
pub const maxOptimizerPlacementCandidates: i64 = 1_000_000;
pub const maxRetainedPlacementCandidates: i64 = 16_384;

/// `fmt.Errorf("%w: ...", limits.ErrOptimizationResourceLimit, ...)`
pub(crate) fn optimizationResourceLimit(msg: String) -> GoError {
    GoError::wrap(
        format!("{}: {}", limits::ErrOptimizationResourceLimit.Error(), msg),
        limits::ErrOptimizationResourceLimit.err(),
    )
}

/// Go `cluster.Vessel` / `sequence.Vessel` parent walk used by the guarded
/// ancestry helpers.
#[inline]
fn ancestryParent(current: &NodeRef) -> Option<NodeRef> {
    let n = current.b();
    if let Some(c) = &n.Container {
        return Some(c.clone());
    }
    if let Some(c) = &n.Cluster {
        return Some(c.b().Vessel.clone());
    }
    if let Some(s) = &n.Sequence {
        return Some(s.b().Vessel.clone());
    }
    None
}

pub fn optimizerIsDescendantOf(
    descendant: Option<&NodeRef>,
    ancestor: Option<&NodeRef>,
    guard: &OptimizationWorkGuardRef,
) -> GoResult<bool> {
    // Ancestry chains are short; a small vector replaces Go's seen map with the
    // same membership and size semantics.
    let mut seen: Vec<NodeRef> = Vec::new();
    let mut current: Option<NodeRef> = descendant.cloned();
    loop {
        guard.Step()?;
        if nodeEq(current.as_ref(), ancestor) {
            return Ok(true);
        }
        let Some(cur) = current else {
            return Ok(ancestor.is_none());
        };
        if seen.iter().any(|s| *s == cur) {
            return Err(errorf!("TALA {} found a cycle in node ancestry at {}", guard.Location(), cur.DebugID()));
        }
        if seen.len() as i64 >= limits::MaxEngineNodes {
            return Err(errorf!("TALA {} ancestry exceeds node limit {}", guard.Location(), limits::MaxEngineNodes));
        }
        current = ancestryParent(&cur);
        seen.push(cur);
    }
}

pub fn optimizerTreeRoot(tree: Option<&TreeRef>, guard: &OptimizationWorkGuardRef) -> GoResult<Option<TreeRef>> {
    let mut seen: Vec<TreeRef> = Vec::new();
    let mut tree: Option<TreeRef> = tree.cloned();
    loop {
        let parent = match &tree {
            Some(t) => t.b().Parent.clone(),
            None => None,
        };
        let Some(parent) = parent else { break };
        guard.Step()?;
        let t = tree.unwrap();
        if seen.iter().any(|s| *s == t) {
            return Err(errorf!("TALA {} found a cycle in tree ancestry", guard.Location()));
        }
        if seen.len() as i64 >= limits::MaxEngineNodes {
            return Err(errorf!("TALA {} tree ancestry exceeds node limit {}", guard.Location(), limits::MaxEngineNodes));
        }
        seen.push(t);
        tree = Some(parent);
    }
    Ok(tree)
}

/// Returns a fresh point (Go `node.FixedOrigin()` allocates), so callers may
/// modify the returned value.
pub fn optimizerFixedOrigin(
    g: Option<&GraphRef>,
    container: Option<&NodeRef>,
    guard: &OptimizationWorkGuardRef,
) -> GoResult<Option<geo::Point>> {
    let Some(g) = g else {
        return Err(errorf!("TALA {} fixed-origin lookup requires a graph", guard.Location()));
    };
    let gb = g.b();
    if gb.Nodes.len() as i64 > limits::MaxEngineNodes {
        return Err(errorf!("TALA {} fixed-origin node count exceeds limit {}", guard.Location(), limits::MaxEngineNodes));
    }
    for node in gb.Nodes.iter() {
        guard.Step()?;
        if !nodeEq(node.OwningContainer().as_ref(), container) {
            continue;
        }
        if let Some(origin) = node.FixedOrigin() {
            let v = *origin.b();
            return Ok(Some(v));
        }
    }
    Ok(None)
}

pub fn optimizerOrderedNears(node: &NodeRef, guard: &OptimizationWorkGuardRef) -> GoResult<Vec<NodeRef>> {
    let nb = node.b();
    if nb.Nears.len() as i64 > limits::MaxEngineNodes {
        return Err(errorf!("TALA {} near count exceeds limit {}", guard.Location(), limits::MaxEngineNodes));
    }
    let mut nears: Vec<NodeRef> = Vec::with_capacity(nb.Nears.len());
    for near in nb.Nears.iter() {
        guard.Step()?;
        nears.push(near.clone());
    }
    drop(nb);
    guard.AddSort(nears.len() as i64)?;
    layoutgraph::SortNodesByID(&mut nears);
    Ok(nears)
}

pub fn appendBoundedOptimizerNodes(
    dst: &mut Vec<NodeRef>,
    src: Vec<NodeRef>,
    guard: &OptimizationWorkGuardRef,
) -> GoResult<()> {
    if dst.len() as i64 > layoutgraph::MaxTopologyReferences - src.len() as i64 {
        return Err(errorf!(
            "TALA {} optimizer node references exceed limit {}",
            guard.Location(),
            layoutgraph::MaxTopologyReferences
        ));
    }
    dst.extend(src);
    Ok(())
}

pub fn appendOptimizerTreeNears(
    nears: &mut Vec<NodeRef>,
    tree: Option<&TreeRef>,
    guard: &OptimizationWorkGuardRef,
) -> GoResult<()> {
    let Some(tree) = tree else { return Ok(()) };
    let mut stack: Vec<TreeRef> = vec![tree.clone()];
    let mut seen: Set<TreeRef> = Set::default();
    seen.insert(tree.clone());
    while let Some(current) = {
        if stack.is_empty() {
            None
        } else {
            guard.Step()?;
            stack.pop()
        }
    } {
        let (cnode, children) = {
            let c = current.b();
            (c.Node.clone(), c.Children.clone())
        };
        let ordered = optimizerOrderedNears(&cnode, guard)?;
        appendBoundedOptimizerNodes(nears, ordered, guard)?;
        if children.len() as i64 > limits::MaxEngineNodes {
            return Err(errorf!("TALA {} tree child references exceed limit {}", guard.Location(), limits::MaxEngineNodes));
        }
        for child in children.iter().rev() {
            guard.Step()?;
            if seen.contains(child) {
                return Err(errorf!("TALA {} found a cycle or shared child in a tree", guard.Location()));
            }
            if seen.len() as i64 >= limits::MaxEngineNodes {
                return Err(errorf!("TALA {} tree exceeds node limit {}", guard.Location(), limits::MaxEngineNodes));
            }
            seen.insert(child.clone());
            stack.push(child.clone());
        }
    }
    Ok(())
}

/// Resolves an active cluster/sequence vessel, then a tree's root sentinel
/// (shared by both near passes of optimizerAdjacents).
fn resolveOptimizerNear(
    g: &GraphRef,
    near: &NodeRef,
    guard: &OptimizationWorkGuardRef,
) -> GoResult<NodeRef> {
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
    let tree = g.b().NodeToTree.get(&near);
    if let Some(tree) = tree {
        let root = optimizerTreeRoot(Some(&tree), guard)?;
        let Some(root) = root else {
            return Err(errorf!("TALA {} found a tree without a sentinel", guard.Location()));
        };
        near = root.SentinelNode();
    }
    Ok(near)
}

pub fn optimizerAdjacents(
    node: &NodeRef,
    edgeAbductions: &[EdgeAbductionRef],
    guard: &OptimizationWorkGuardRef,
) -> GoResult<Vec<NodeRef>> {
    let Some(g) = node.graph() else {
        return Err(errorf!("TALA {} adjacency lookup requires a node with a graph", guard.Location()));
    };
    let edges = node.edges();
    if edges.len() as i64 > limits::MaxEngineEdges || edgeAbductions.len() as i64 > limits::MaxEngineEdges {
        return Err(errorf!("TALA {} adjacency inputs exceed edge limit {}", guard.Location(), limits::MaxEngineEdges));
    }
    let mut adjacents: Vec<NodeRef> = Vec::with_capacity(edges.len());
    {
        let mut usedScratch = borrowEdgeAbductionBools(edgeAbductions.len());
        let result = (|| -> GoResult<()> {
            let used = &mut usedScratch.values;
            for edge in &edges {
                guard.Step()?;
                let (from, to) = {
                    let e = edge.b();
                    (e.From.clone(), e.To.clone())
                };
                if from != *node && to != *node {
                    return Err(errorf!("TALA {} found a malformed incident edge on {}", guard.Location(), node.DebugID()));
                }
                let adjacentNode = node.Adjacent(edge);
                if !adjacentNode.has_tl() {
                    continue;
                }
                let mut addNode = adjacentNode.clone();
                for (i, edgeAbduction) in edgeAbductions.iter().enumerate() {
                    guard.Step()?;
                    if used[i] {
                        continue;
                    }
                    let ea = edgeAbduction.b();
                    if nodeEq(ea.CurrentFrom.as_ref(), Some(node))
                        && nodeEq(ea.CurrentTo.as_ref(), Some(&adjacentNode))
                        && ea.OriginallyTo.is_some()
                    {
                        used[i] = true;
                        addNode = ea.OriginallyTo.clone().unwrap();
                        break;
                    }
                    if nodeEq(ea.CurrentFrom.as_ref(), Some(&adjacentNode))
                        && nodeEq(ea.CurrentTo.as_ref(), Some(node))
                        && ea.OriginallyFrom.is_some()
                    {
                        used[i] = true;
                        addNode = ea.OriginallyFrom.clone().unwrap();
                        break;
                    }
                }
                adjacents.push(addNode);
            }
            Ok(())
        })();
        returnEdgeAbductionBools(usedScratch);
        result?;
    }
    if !adjacents.is_empty() {
        return Ok(adjacents);
    }

    let orderedNears = optimizerOrderedNears(node, guard)?;
    let container = node.containerField();
    for near in &orderedNears {
        let near = resolveOptimizerNear(&g, near, guard)?;
        if !near.has_tl() {
            continue;
        }
        let isDescendant = optimizerIsDescendantOf(Some(&near), container.as_ref(), guard)?;
        if isDescendant {
            adjacents.push(near);
        }
    }
    if !adjacents.is_empty() {
        return Ok(adjacents);
    }

    let mut nears: Vec<NodeRef> = Vec::new();
    if node.IsClusterVessel() {
        let cluster = g.b().Clusters.get(node);
        let Some(cluster) = cluster else {
            return Err(errorf!("TALA {} found a cluster vessel without a cluster", guard.Location()));
        };
        let clusterNodes = cluster.b().Nodes.clone();
        for clusterNode in &clusterNodes {
            guard.Step()?;
            let ordered = optimizerOrderedNears(clusterNode, guard)?;
            appendBoundedOptimizerNodes(&mut nears, ordered, guard)?;
        }
    }
    let sequence = g.b().Sequences.get(node);
    if let Some(sequence) = sequence {
        let sequenceNodes = sequence.b().Nodes.clone();
        for sequenceNode in &sequenceNodes {
            guard.Step()?;
            let ordered = optimizerOrderedNears(sequenceNode, guard)?;
            appendBoundedOptimizerNodes(&mut nears, ordered, guard)?;
        }
    }
    let trees = g.b().Trees.at(&Some(node.clone()));
    if trees.len() as i64 > limits::MaxEngineNodes {
        return Err(errorf!("TALA {} tree roots exceed node limit {}", guard.Location(), limits::MaxEngineNodes));
    }
    for tree in &trees {
        appendOptimizerTreeNears(&mut nears, Some(tree), guard)?;
    }
    for near in &nears {
        guard.Step()?;
        let addNode = resolveOptimizerNear(&g, near, guard)?;
        if addNode.has_tl() {
            adjacents.push(addNode);
        }
    }
    Ok(adjacents)
}

#[derive(Clone, Copy)]
struct medianKey {
    value: f64,
    id: layoutgraph::EntityID,
    index: usize,
}

pub fn optimizerMedian(nodes: &[NodeRef], includeSizes: bool, guard: &OptimizationWorkGuardRef) -> GoResult<(f64, f64)> {
    if nodes.is_empty() {
        return Err(errorf!("TALA {} cannot compute a median without positioned neighbors", guard.Location()));
    }
    if nodes.len() as i64 > layoutgraph::MaxTopologyReferences {
        return Err(errorf!(
            "TALA {} median inputs exceed limit {}",
            guard.Location(),
            layoutgraph::MaxTopologyReferences
        ));
    }
    if nodes.len() == 1 {
        let node = &nodes[0];
        guard.Step()?;
        let Some(tl) = node.tl() else {
            return Err(errorf!("TALA {} cannot compute a median with an unpositioned neighbor", guard.Location()));
        };
        guard.AddSort(1)?;
        guard.AddSort(1)?;
        let tl = *tl.b();
        if includeSizes {
            let cellSize = node.g().b().CellSize;
            if cellSize <= 0.0 || math::IsNaN(cellSize) || math::IsInf(cellSize, 0) {
                return Err(errorf!("TALA {} requires a finite positive cell size", guard.Location()));
            }
            return Ok(((tl.X + node.w() / 2.0) / cellSize, (tl.Y + node.h() / 2.0) / cellSize));
        }
        return Ok((tl.X + 0.5, tl.Y + 0.5));
    }
    // Sorting (key, ID) records with the same comparator yields the same
    // permutation as sorting the nodes themselves.
    let mut orderedByX: Vec<medianKey> = Vec::with_capacity(nodes.len());
    let mut orderedByY: Vec<medianKey> = Vec::with_capacity(nodes.len());
    let mut geom: Vec<(f64, f64, f64, f64)> = Vec::with_capacity(nodes.len());
    for (i, node) in nodes.iter().enumerate() {
        guard.Step()?;
        let n = node.b();
        let bx = n.Box.b();
        let Some(tl) = bx.TopLeft.as_ref() else {
            return Err(errorf!("TALA {} cannot compute a median with an unpositioned neighbor", guard.Location()));
        };
        let tl = *tl.b();
        let (w, h) = (bx.Width, bx.Height);
        geom.push((tl.X, tl.Y, w, h));
        let (mut kx, mut ky) = (tl.X, tl.Y);
        if includeSizes {
            kx += w / 2.0;
            ky += h / 2.0;
        }
        orderedByX.push(medianKey { value: kx, id: n.ID, index: i });
        orderedByY.push(medianKey { value: ky, id: n.ID, index: i });
    }
    let less = |a: &medianKey, b: &medianKey| {
        if a.value == b.value {
            return a.id < b.id;
        }
        a.value < b.value
    };
    guard.AddSort(nodes.len() as i64)?;
    sort::SortSlice(&mut orderedByX, less);
    guard.AddSort(nodes.len() as i64)?;
    sort::SortSlice(&mut orderedByY, less);

    let middle = nodes.len() / 2;
    let (mut medianX, mut medianY);
    let gx = |k: &medianKey| geom[k.index];
    if includeSizes {
        let (x, _, w, _) = gx(&orderedByX[middle]);
        let (_, y, _, h) = gx(&orderedByY[middle]);
        medianX = x + w / 2.0;
        medianY = y + h / 2.0;
        if nodes.len() % 2 == 0 {
            let (x1, _, w1, _) = gx(&orderedByX[middle - 1]);
            let (_, y1, _, h1) = gx(&orderedByY[middle - 1]);
            medianX = (medianX + x1 + w1 / 2.0) / 2.0;
            medianY = (medianY + y1 + h1 / 2.0) / 2.0;
        }
        let cellSize = nodes[0].g().b().CellSize;
        if cellSize <= 0.0 || math::IsNaN(cellSize) || math::IsInf(cellSize, 0) {
            return Err(errorf!("TALA {} requires a finite positive cell size", guard.Location()));
        }
        medianX /= cellSize;
        medianY /= cellSize;
    } else {
        medianX = gx(&orderedByX[middle]).0 + 0.5;
        medianY = gx(&orderedByY[middle]).1 + 0.5;
        if nodes.len() % 2 == 0 {
            medianX = (medianX + gx(&orderedByX[middle - 1]).0) / 2.0;
            medianY = (medianY + gx(&orderedByY[middle - 1]).1) / 2.0;
        }
    }
    Ok((medianX, medianY))
}

pub fn optimizerMedianToNeighbors(
    node: &NodeRef,
    includeSizes: bool,
    edgeAbductions: &[EdgeAbductionRef],
    guard: &OptimizationWorkGuardRef,
) -> GoResult<(f64, f64)> {
    let adjacents = optimizerAdjacents(node, edgeAbductions, guard)?;
    optimizerMedian(&adjacents, includeSizes, guard)
}

pub const maxSafeOverlapDelta: f64 = 500.0;

pub fn optimizerDoesOverlap(
    node: &NodeRef,
    point: &geo::PointRef,
    exceptions: &[NodeRef],
    guard: &OptimizationWorkGuardRef,
) -> GoResult<bool> {
    let Some(g) = node.graph() else {
        return Err(errorf!("TALA {} overlap check requires a node, graph, and point", guard.Location()));
    };
    let gb = g.b();
    if gb.Nodes.len() as i64 > limits::MaxEngineNodes || exceptions.len() as i64 > limits::MaxEngineNodes {
        return Err(errorf!("TALA {} overlap inputs exceed node limit {}", guard.Location(), limits::MaxEngineNodes));
    }
    let p = *point.b();
    let (nodeWidth, nodeHeight, nodeEdges) = {
        let n = node.b();
        let bx = n.Box.b();
        (bx.Width, bx.Height, n.Edges.len())
    };
    let right = p.X + nodeWidth;
    let bottom = p.Y + nodeHeight;
    for otherNode in gb.Nodes.iter() {
        guard.Step()?;
        if otherNode == node {
            continue;
        }
        let mut excluded = false;
        for exception in exceptions {
            guard.Step()?;
            if exception == otherNode {
                excluded = true;
                break;
            }
        }
        if excluded {
            continue;
        }
        let (ox, oy, ow, oh) = {
            let o = otherNode.b();
            let bx = o.Box.b();
            let Some(tl) = bx.TopLeft.as_ref() else { continue };
            let tl = *tl.b();
            (tl.X, tl.Y, bx.Width, bx.Height)
        };
        if p.X > ox + ow + maxSafeOverlapDelta
            || p.X + nodeWidth + maxSafeOverlapDelta < ox
            || p.Y > oy + oh + maxSafeOverlapDelta
            || p.Y + nodeHeight + maxSafeOverlapDelta < oy
        {
            continue;
        }
        guard.Add(nodeEdges as u64)?;
        let delta = node.DeltaTo(otherNode, Some(point)) as f64;
        if p.X < ox + ow + delta && right + delta > ox && p.Y < oy + oh + delta && bottom + delta > oy {
            return Ok(true);
        }
    }
    Ok(false)
}

pub fn optimizerIsOccupied(
    g: &GraphRef,
    point: &geo::Point,
    guard: &OptimizationWorkGuardRef,
) -> GoResult<(Option<NodeRef>, bool)> {
    let gb = g.b();
    if gb.Nodes.len() as i64 > limits::MaxEngineNodes {
        return Err(errorf!("TALA {} occupancy node count exceeds limit {}", guard.Location(), limits::MaxEngineNodes));
    }
    for node in gb.Nodes.iter() {
        guard.Step()?;
        let n = node.b();
        let bx = n.Box.b();
        if let Some(tl) = bx.TopLeft.as_ref() {
            if nonNilEquals(&tl.b(), point) {
                return Ok((Some(node.clone()), true));
            }
        }
    }
    Ok((None, false))
}

pub fn optimizerCanMove(
    node: &NodeRef,
    point: &geo::PointRef,
    includeSizes: bool,
    guard: &OptimizationWorkGuardRef,
) -> GoResult<bool> {
    let Some(g) = node.graph() else {
        return Err(errorf!("TALA {} movement check requires a node, graph, and point", guard.Location()));
    };
    let p = *point.b();
    if nonNilEquals(&node.tlv(), &p) {
        return Ok(true);
    }
    let (_, occupied) = optimizerIsOccupied(&g, &p, guard)?;
    if occupied || !includeSizes {
        return Ok(!occupied);
    }
    let overlaps = optimizerDoesOverlap(node, point, &[], guard)?;
    Ok(!overlaps)
}

// chargeOptimizerScoring accounts for the nested edge, abduction, blocker, and
// symmetry scans performed by the legacy scoring helpers. Those helpers poll
// context internally; this conservative charge additionally makes repeated
// candidate scoring obey the shared operation budget.
pub fn chargeOptimizerScoring(
    node: &NodeRef,
    edgeAbductionsLen: usize,
    includeSymmetry: bool,
    guard: &OptimizationWorkGuardRef,
) -> GoResult<()> {
    let (edgeCount, nodeCount) = {
        let n = node.b();
        let Some(g) = n.Graph.as_ref() else {
            return Err(errorf!("TALA {} scoring requires a node with a graph", guard.Location()));
        };
        let c = g.b().Nodes.len();
        (n.Edges.len(), c)
    };
    let edges = (edgeCount + 1) as u64;
    let mut searchSpace = (nodeCount + edgeAbductionsLen + 1) as u64;
    if includeSymmetry {
        if u64::MAX - searchSpace < edges {
            return Err(optimizationResourceLimit(format!("{} scoring work arithmetic overflow", guard.Location())));
        }
        searchSpace += edges;
    }
    guard.AddProduct(edges, searchSpace)
}

// chargeOptimizerTranspose bounds the independent reachability, rotation, and
// scoring guards used by transpose under the optimizer's single operation
// budget. Cheap eligibility checks avoid charging the worst case when
// transpose will return immediately.
pub fn chargeOptimizerTranspose(
    g: &GraphRef,
    node: &NodeRef,
    edgeAbductions: Option<&[EdgeAbductionRef]>,
    guard: &OptimizationWorkGuardRef,
) -> GoResult<()> {
    if node.graph().is_none() {
        return Err(errorf!("TALA {} transpose requires a node with a graph", guard.Location()));
    }
    let abductionCount = edgeAbductions.map_or(0, |a| a.len());
    let (gNodes, gEdges) = {
        let gb = g.b();
        (gb.Nodes.len(), gb.Edges.len())
    };
    if gNodes as i64 > limits::MaxEngineNodes
        || gEdges as i64 > limits::MaxEngineEdges
        || abductionCount as i64 > limits::MaxEngineEdges
    {
        return Err(errorf!("TALA {} transpose inputs exceed engine limits", guard.Location()));
    }
    for _ in 0..abductionCount {
        guard.Step()?;
    }
    let (hasHierarchy, hasFixed, edgeCount) = {
        let n = node.b();
        (n.Hierarchy.is_some(), n.FixedTopLeft.is_some(), n.Edges.len())
    };
    if hasHierarchy || hasFixed || edgeCount < 1 || edgeCount > 2 {
        return guard.Step();
    }
    if g.b().NodeToTree.has(node) || g.IsTreeSentinel(Some(node)) {
        return guard.Step();
    }

    let nodes = (gNodes + 1) as u64;
    let edges = (gEdges + 1) as u64;
    let abductions = (abductionCount + 1) as u64;
    // Reachability is performed several times while selecting a rotation side.
    for _ in 0..6 {
        guard.Add(nodes + edges + abductions)?;
    }
    // At most four trial rotations and one committed rotation are scored.
    for _ in 0..5 {
        if edgeAbductions.is_none() {
            guard.AddProduct(nodes, edges)?;
            guard.AddProduct(edges, edges)?;
        } else {
            guard.AddProduct((edgeCount + 1) as u64, nodes + abductions)?;
        }
    }
    // Rotating containers can translate descendants for each trial.
    for _ in 0..4 {
        guard.AddProduct(nodes, nodes)?;
    }
    Ok(())
}

pub fn optimizerDescendants(node: &NodeRef, guard: &OptimizationWorkGuardRef) -> GoResult<Vec<NodeRef>> {
    let Some(g) = node.graph() else {
        return Err(errorf!("TALA {} movement requires a node with a graph", guard.Location()));
    };
    let mut seen: Set<NodeRef> = Set::default();
    seen.insert(node.clone());
    let mut stack: Vec<NodeRef> = Vec::new();
    let push = |child: &NodeRef, seen: &mut Set<NodeRef>, stack: &mut Vec<NodeRef>| -> GoResult<()> {
        guard.Step()?;
        if seen.contains(child) {
            return Ok(());
        }
        if seen.len() as i64 >= limits::MaxEngineNodes {
            return Err(errorf!("TALA {} descendant count exceeds limit {}", guard.Location(), limits::MaxEngineNodes));
        }
        seen.insert(child.clone());
        stack.push(child.clone());
        Ok(())
    };
    let pushChildren = |parent: &NodeRef, seen: &mut Set<NodeRef>, stack: &mut Vec<NodeRef>| -> GoResult<()> {
        let sequence = g.b().Sequences.get(parent);
        if let Some(sequence) = sequence {
            let nodes = sequence.b().Nodes.clone();
            if nodes.len() as i64 > limits::MaxEngineNodes {
                return Err(errorf!(
                    "TALA {} sequence child references exceed node limit {}",
                    guard.Location(),
                    limits::MaxEngineNodes
                ));
            }
            for v in nodes.iter().rev() {
                push(v, seen, stack)?;
            }
        }
        if parent.IsClusterVessel() {
            let cluster = g.b().Clusters.get(parent);
            if let Some(cluster) = cluster {
                let nodes = cluster.b().Nodes.clone();
                if nodes.len() as i64 > limits::MaxEngineNodes {
                    return Err(errorf!(
                        "TALA {} cluster child references exceed node limit {}",
                        guard.Location(),
                        limits::MaxEngineNodes
                    ));
                }
                for v in nodes.iter().rev() {
                    push(v, seen, stack)?;
                }
            }
        }
        if parent.IsContainer() {
            let children = g.b().Containers.at(&Some(parent.clone()));
            if children.len() as i64 > limits::MaxEngineNodes {
                return Err(errorf!(
                    "TALA {} container child references exceed node limit {}",
                    guard.Location(),
                    limits::MaxEngineNodes
                ));
            }
            for c in children.iter().rev() {
                push(c, seen, stack)?;
            }
        }
        Ok(())
    };
    pushChildren(node, &mut seen, &mut stack)?;
    let mut descendants: Vec<NodeRef> = Vec::with_capacity(stack.len());
    while !stack.is_empty() {
        guard.Step()?;
        let current = stack.pop().unwrap();
        descendants.push(current.clone());
        pushChildren(&current, &mut seen, &mut stack)?;
    }
    Ok(descendants)
}

pub fn optimizerMoveNodeAbs(node: &NodeRef, x: f64, y: f64, guard: &OptimizationWorkGuardRef) -> GoResult<()> {
    let Some(tl) = node.tl() else {
        return Err(errorf!("TALA {} cannot move an unpositioned node", guard.Location()));
    };
    let cur = *tl.b();
    if cur.X == x && cur.Y == y {
        return guard.Step();
    }
    // Most optimizer candidates are ordinary leaves. The general path below
    // walks ownership maps and snapshots descendants for atomic rollback, but
    // those structures are provably empty for a non-container/non-vessel leaf.
    if node.graph().is_some() && !node.IsContainer() && !node.IsClusterVessel() && !node.g().b().Sequences.has(node) {
        node.Translate(x - cur.X, y - cur.Y);
        return Ok(());
    }
    let descendants = optimizerDescendants(node, guard)?;
    let mut snapshots: Vec<nodePositionSnapshot> = Vec::with_capacity(descendants.len() + 1);
    snapshots.push(nodePositionSnapshot::of(node));
    for child in &descendants {
        guard.Step()?;
        if !child.has_tl() {
            return Err(errorf!("TALA {} cannot move an unpositioned descendant", guard.Location()));
        }
        snapshots.push(nodePositionSnapshot::of(child));
    }
    let cur = node.tlv();
    let (dx, dy) = (x - cur.X, y - cur.Y);
    node.Translate(dx, dy);
    for child in &descendants {
        if let Err(err) = guard.Step() {
            restoreNodePositions(&snapshots);
            return Err(err);
        }
        child.Translate(dx, dy);
    }
    Ok(())
}

pub fn optimizerSwapPositions(nodeA: &NodeRef, nodeB: &NodeRef, guard: &OptimizationWorkGuardRef) -> GoResult<()> {
    if !nodeA.has_tl() || !nodeB.has_tl() {
        return Err(errorf!("TALA {} cannot swap unpositioned nodes", guard.Location()));
    }
    let snapshots = captureOptimizerNodePositions(&[nodeA.clone(), nodeB.clone()], guard)?;
    let result = (|| -> GoResult<()> {
        let a = nodeA.tlv();
        let b = nodeB.tlv();
        optimizerMoveNodeAbs(nodeA, b.X, b.Y, guard)?;
        optimizerMoveNodeAbs(nodeB, a.X, a.Y, guard)?;
        Ok(())
    })();
    if result.is_err() {
        restoreNodePositions(&snapshots);
    }
    result
}

pub struct optimizerNodeMutationSnapshot {
    pub node: NodeRef,
    pub topLeft: pointerSnapshot,
    pub width: f64,
    pub height: f64,
}

pub struct optimizerClusterMutationSnapshot {
    pub cluster: ClusterRef,
    pub arrangement: ClusterArrangement,
    pub desired: ClusterArrangement,
    pub padding: f64,
}

pub struct optimizerHerdMutationSnapshot {
    pub herd: HerdAssignmentRef,
    pub value: HerdAssignment,
}

#[derive(Default)]
pub struct optimizerMutationSnapshot {
    pub nodes: Vec<optimizerNodeMutationSnapshot>,
    pub clusters: Vec<optimizerClusterMutationSnapshot>,
    pub herds: Vec<optimizerHerdMutationSnapshot>,
    pub costSnapshot: Option<PlacementCostSnapshotRef>,

    // Scratch is owned by one optimizer invocation at a time. Reusing storage
    // avoids rebuilding allocation capacity on every annealing iteration; the
    // topology and rollback values are still captured afresh on each call.
    pub seenNodes: Set<NodeRef>,
    pub seenClusters: Set<ClusterRef>,
    pub seenHerds: Set<HerdAssignmentRef>,
}

pub fn captureOptimizerMutationState(g: &GraphRef, guard: &OptimizationWorkGuardRef) -> GoResult<optimizerMutationSnapshot> {
    let mut snapshot = optimizerMutationSnapshot::default();
    captureOptimizerMutationStateInto(g, guard, &mut snapshot)?;
    Ok(snapshot)
}

impl optimizerMutationSnapshot {
    // release clears graph references while retaining request-local buffer capacity.
    // It is also safe after an interrupted capture, when only a prefix was recorded.
    pub fn release(&mut self) {
        self.nodes.clear();
        self.clusters.clear();
        self.herds.clear();
        self.costSnapshot = None;
        self.seenNodes.clear();
        self.seenClusters.clear();
        self.seenHerds.clear();
    }

    pub fn restore(&self) {
        for node in &self.nodes {
            node.node.set_tl(node.topLeft.restore());
            node.node.set_w(node.width);
            node.node.set_h(node.height);
        }
        for cluster in &self.clusters {
            let mut c = cluster.cluster.bm();
            c.Arrangement = cluster.arrangement.clone();
            c.DesiredArrangement = cluster.desired.clone();
            c.Padding = cluster.padding;
        }
        for herd in &self.herds {
            *herd.herd.bm() = herd.value.clone();
        }
        if let Some(cs) = &self.costSnapshot {
            cs.Restore();
        }
    }
}

pub fn captureOptimizerMutationStateInto(
    g: &GraphRef,
    guard: &OptimizationWorkGuardRef,
    snapshot: &mut optimizerMutationSnapshot,
) -> GoResult<()> {
    snapshot.release();
    let (nodes, containers, clusters, sequences) = {
        let gb = g.b();
        (gb.Nodes.clone(), gb.Containers.clone(), gb.Clusters.clone(), gb.Sequences.clone())
    };
    if nodes.len() as i64 > limits::MaxEngineNodes {
        return Err(errorf!("TALA {} snapshot node count exceeds limit {}", guard.Location(), limits::MaxEngineNodes));
    }
    if containers.len() as i64 > limits::MaxEngineNodes + 1
        || clusters.len() as i64 > limits::MaxEngineNodes
        || sequences.len() as i64 > limits::MaxEngineNodes
    {
        return Err(errorf!(
            "TALA {} optimizer topology maps exceed node limit {}",
            guard.Location(),
            limits::MaxEngineNodes
        ));
    }
    let cacheEntries = g.EdgeLengthCacheEntries();
    if cacheEntries as i64 > layoutgraph::MaxTopologyReferences {
        return Err(errorf!(
            "TALA {} edge-length cache entries exceed limit {}",
            guard.Location(),
            layoutgraph::MaxTopologyReferences
        ));
    }
    for _ in 0..cacheEntries {
        guard.Step()?;
    }
    let costSnapshot = g.SnapshotPlacementCosts();
    guard.Finish()?;
    snapshot.costSnapshot = Some(costSnapshot);

    fn enqueue(node: &NodeRef, snapshot: &mut optimizerMutationSnapshot, guard: &OptimizationWorkGuardRef) -> GoResult<()> {
        guard.Step()?;
        if snapshot.seenNodes.contains(node) {
            return Ok(());
        }
        if snapshot.seenNodes.len() as i64 >= limits::MaxEngineNodes {
            return Err(errorf!("TALA {} snapshot node count exceeds limit {}", guard.Location(), limits::MaxEngineNodes));
        }
        snapshot.seenNodes.insert(node.clone());
        let (tl, w, h) = {
            let n = node.b();
            let bx = n.Box.b();
            (bx.TopLeft.clone(), bx.Width, bx.Height)
        };
        snapshot.nodes.push(optimizerNodeMutationSnapshot {
            node: node.clone(),
            topLeft: snapshotPointer(tl.as_ref()),
            width: w,
            height: h,
        });
        Ok(())
    }
    for node in &nodes {
        enqueue(node, snapshot, guard)?;
    }
    // The snapshot already contains the queue in discovery order.
    let mut i = 0;
    while i < snapshot.nodes.len() {
        let node = snapshot.nodes[i].node.clone();
        i += 1;
        let children = containers.at(&Some(node.clone()));
        if children.len() as i64 > limits::MaxEngineNodes {
            return Err(errorf!(
                "TALA {} container child references exceed node limit {}",
                guard.Location(),
                limits::MaxEngineNodes
            ));
        }
        for child in &children {
            enqueue(child, snapshot, guard)?;
        }
        if let Some(cluster) = clusters.get(&node) {
            let cn = cluster.b().Nodes.clone();
            if cn.len() as i64 > limits::MaxEngineNodes {
                return Err(errorf!(
                    "TALA {} cluster child references exceed node limit {}",
                    guard.Location(),
                    limits::MaxEngineNodes
                ));
            }
            for child in &cn {
                enqueue(child, snapshot, guard)?;
            }
        }
        if let Some(sequence) = sequences.get(&node) {
            let sn = sequence.b().Nodes.clone();
            if sn.len() as i64 > limits::MaxEngineNodes {
                return Err(errorf!(
                    "TALA {} sequence child references exceed node limit {}",
                    guard.Location(),
                    limits::MaxEngineNodes
                ));
            }
            for child in &sn {
                enqueue(child, snapshot, guard)?;
            }
        }
    }

    for cluster in clusters.values() {
        guard.Step()?;
        if snapshot.seenClusters.contains(&cluster) {
            continue;
        }
        snapshot.seenClusters.insert(cluster.clone());
        let (arrangement, desired, padding) = {
            let c = cluster.b();
            (c.Arrangement.clone(), c.DesiredArrangement.clone(), c.Padding)
        };
        snapshot.clusters.push(optimizerClusterMutationSnapshot { cluster, arrangement, desired, padding });
    }

    for idx in 0..snapshot.nodes.len() {
        guard.Step()?;
        let herd = snapshot.nodes[idx].node.b().HerdAssignment.clone();
        let Some(herd) = herd else { continue };
        if snapshot.seenHerds.contains(&herd) {
            continue;
        }
        snapshot.seenHerds.insert(herd.clone());
        let value = herd.b().clone();
        snapshot.herds.push(optimizerHerdMutationSnapshot { herd, value });
    }
    Ok(())
}

pub fn captureOptimizerNodePositions(nodes: &[NodeRef], guard: &OptimizationWorkGuardRef) -> GoResult<Vec<nodePositionSnapshot>> {
    if nodes.len() <= 2 {
        let mut leaves = true;
        for node in nodes {
            let isLeaf = node.graph().is_some()
                && !node.IsContainer()
                && !node.IsClusterVessel()
                && !node.g().b().Sequences.has(node);
            if !isLeaf {
                leaves = false;
                break;
            }
        }
        if leaves {
            let mut snapshots: Vec<nodePositionSnapshot> = Vec::with_capacity(nodes.len());
            for node in nodes {
                if snapshots.iter().any(|s| s.node == *node) {
                    continue;
                }
                guard.Step()?;
                snapshots.push(nodePositionSnapshot::of(node));
            }
            return Ok(snapshots);
        }
    }
    if nodes.len() == 1 {
        // optimizerDescendants already deduplicates its traversal, including
        // the root. A single root needs no second map or combined node slice.
        let descendants = optimizerDescendants(&nodes[0], guard)?;
        let mut snapshots: Vec<nodePositionSnapshot> = Vec::with_capacity(descendants.len() + 1);
        for i in 0..descendants.len() + 1 {
            guard.Step()?;
            let node = if i > 0 { &descendants[i - 1] } else { &nodes[0] };
            snapshots.push(nodePositionSnapshot::of(node));
        }
        return Ok(snapshots);
    }
    let mut seen: Set<NodeRef> = Set::default();
    let mut snapshots: Vec<nodePositionSnapshot> = Vec::with_capacity(nodes.len());
    for node in nodes {
        let descendants = optimizerDescendants(node, guard)?;
        for current in std::iter::once(node).chain(descendants.iter()) {
            if seen.contains(current) {
                continue;
            }
            if seen.len() as i64 >= limits::MaxEngineNodes {
                return Err(errorf!("TALA {} position snapshot exceeds node limit {}", guard.Location(), limits::MaxEngineNodes));
            }
            guard.Step()?;
            seen.insert(current.clone());
            snapshots.push(nodePositionSnapshot::of(current));
        }
    }
    Ok(snapshots)
}

// withOptimizerPositionsSwapped owns the rollback point for a speculative swap
// and its scoring callback. The former nested atomic swap captured the same
// geometry twice without any intervening mutation. Reuse this rollback point,
// replaying the second capture's exact work and cancellation checkpoints.
pub fn withOptimizerPositionsSwapped(
    nodeA: &NodeRef,
    nodeB: &NodeRef,
    guard: &OptimizationWorkGuardRef,
    f: impl FnOnce() -> GoResult<()>,
) -> GoResult<()> {
    let before = guard.Used();
    let snapshots = captureOptimizerNodePositions(&[nodeA.clone(), nodeB.clone()], guard)?;
    let captureWork = guard.Used() - before;
    let result = (|| -> GoResult<()> {
        if !nodeA.has_tl() || !nodeB.has_tl() {
            return Err(errorf!("TALA {} cannot swap unpositioned nodes", guard.Location()));
        }
        for _ in 0..captureWork {
            guard.Step()?;
        }
        let a = nodeA.tlv();
        let b = nodeB.tlv();
        optimizerMoveNodeAbs(nodeA, b.X, b.Y, guard)?;
        optimizerMoveNodeAbs(nodeB, a.X, a.Y, guard)?;
        f()
    })();
    restoreNodePositions(&snapshots);
    result
}
