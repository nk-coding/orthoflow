// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/hierarchy/discovery.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;
use layoutgraph::{EdgeAbductionRef, GoMap, HierarchyRef};

pub(crate) const minHierarchyLevels: i64 = 3;
pub(crate) const maxAutomaticWorkflowNodes: i64 = 128;
pub(crate) const maxAutomaticWorkflowEdges: i64 = 256;

/// Self-loops are routed, but they do not describe a relationship between hierarchy levels.
pub(crate) fn isHierarchyStructuralEdge(edge: &EdgeRef) -> bool {
    !edge.IsLoop()
}

pub(crate) fn countHierarchyStructuralEdges(node: &NodeRef) -> i64 {
    let mut count = 0;
    for edge in node.b().Edges.iter() {
        if isHierarchyStructuralEdge(edge) {
            count += 1;
        }
    }
    count
}

/// Assign identifies hierarchical structures within the subgraphs of this graph.
pub fn Assign(ctx: &Ctx, g: &GraphRef, root: Option<&NodeRef>, hierarchicalNodes: &Set<NodeRef>) -> GoResult<()> {
    layoutgraph::Validate(ctx, "AssignNodeHierarchy", g)?;
    let (ctx, guard) = layoutgraph::EnsureTransactionWorkGuard(ctx, "AssignNodeHierarchyTransactions")?;
    let state = layoutgraph::NewGraphStateSnapshot(layoutgraph::GraphStateSnapshotOptions {
        CaptureTopology: true,
        CaptureEdgeRoutes: true,
    });
    state.UpdateWithWorkGuard(g, &guard)?;
    let r = (|| -> GoResult<()> {
        let ownedNodes = state.OwnedNodes(g, &guard)?;
        // Deterministic iteration (Go iterates the map; every step below is order-independent).
        let mut owned: Vec<NodeRef> = ownedNodes.iter().cloned().collect();
        layoutgraph::SortNodesByID(&mut owned);
        let mut seenHierarchies: Vec<HierarchyRef> = Vec::new();
        let mut seenSet: Set<HierarchyRef> = Set::default();
        for node in &owned {
            guard.Step()?;
            if let Some(h) = node.b().Hierarchy.clone() {
                if seenSet.insert(h.clone()) {
                    seenHierarchies.push(h);
                }
            }
        }
        for hierarchy in &seenHierarchies {
            let levels = hierarchy.Levels();
            for node in levels.keys() {
                guard.Step()?;
                if ownedNodes.contains(&node) {
                    levels.delete(&node);
                }
            }
        }
        for node in &owned {
            guard.Step()?;
            node.bm().Hierarchy = None;
        }
        assign(&ctx, g, root, hierarchicalNodes)?;
        guard.Finish()?;
        Ok(())
    })();
    if r.is_err() {
        layoutgraph::RestoreGraphState(g, &state);
    }
    r
}

/// RemoveIsolatedMemberships removes leaf nodes that inherited a container's
/// hierarchy but have no edges of their own.
pub fn RemoveIsolatedMemberships(g: &GraphRef) {
    let nodes = g.b().Nodes.clone();
    for node in &nodes {
        let h = node.b().Hierarchy.clone();
        if let Some(h) = h {
            if node.b().Edges.is_empty() && !node.IsContainer() {
                h.Levels().delete(node);
                node.bm().Hierarchy = None;
            }
        }
    }
}

pub(crate) fn assign(ctx: &Ctx, g: &GraphRef, root: Option<&NodeRef>, hierarchicalNodes: &Set<NodeRef>) -> GoResult<()> {
    let force = match root {
        None => g.b().IsRootHierarchy,
        Some(r) => r.b().ForceHierarchy,
    };

    let hierarchyGraph = layoutgraph::NewGraph();
    hierarchyGraph.CopyEntitiesFrom(g);

    let rootChildren = g.b().Containers.at(&root.cloned());
    for node in &rootChildren {
        hierarchyGraph.AddNodeUnchecked(node);
    }
    let edgeAbductions = g.AbductEdges(root, &hierarchyGraph);
    let mut edgesRestored = false;
    let mut splitOwnership = layoutgraph::NodeGraphOwnershipJournal::default();

    let r = (|| -> GoResult<()> {
        for child in &rootChildren {
            if !hierarchicalNodes.contains(child) && child.IsContainer() {
                assign(ctx, g, Some(child), hierarchicalNodes)?;
            }
        }

        let (subgraphs, journal) = hierarchyGraph.SplitSubgraphsTracked(ctx, layoutgraph::SplitOptions::default(), None);
        splitOwnership = journal;
        let subgraphs = subgraphs?;
        for subgraph in &subgraphs {
            let hierarchy = build(ctx, subgraph, force, hierarchicalNodes, Some(&edgeAbductions))?;
            let Some(hierarchy) = hierarchy else { continue };
            let snodes = subgraph.b().Nodes.clone();
            for node in &snodes {
                node.bm().Hierarchy = Some(hierarchy.clone());
                for descendant in g.AllDescendantNodes(Some(node), false) {
                    let levels = hierarchy.Levels();
                    let l = levels.at(node);
                    levels.set(descendant.clone(), l);
                    descendant.bm().Hierarchy = Some(hierarchy.clone());
                }
            }
        }
        g.RestoreEdgeAbductions(&edgeAbductions);
        edgesRestored = true;

        // remove Nears of nodes in hierarchies
        if root.is_none() {
            let nodes = g.b().Nodes.clone();
            for node in &nodes {
                let nears = std::mem::take(&mut node.bm().Nears);
                if node.b().Hierarchy.is_none() {
                    for near in &nears {
                        if near.b().Hierarchy.is_none() {
                            node.AddNear(Some(near));
                        }
                    }
                }
            }
        }
        Ok(())
    })();

    if !edgesRestored {
        g.RestoreEdgeAbductions(&edgeAbductions);
    }
    splitOwnership.Restore();
    let nodes = g.b().Nodes.clone();
    nodes.SetGraphReference(g);
    r
}

/// build assigns a hierarchy level to each node in this graph given that all
/// nodes can be in a hierarchy.
pub(crate) fn build(
    ctx: &Ctx,
    g: &GraphRef,
    force: bool,
    hierarchicalNodes: &Set<NodeRef>,
    edgeAbductions: Option<&[EdgeAbductionRef]>,
) -> GoResult<Option<HierarchyRef>> {
    let nodes = g.b().Nodes.clone();
    if nodes.len() <= 1 {
        return Ok(None);
    }
    let mut sourceCount: i64 = 0;
    let mut sinkCount: i64 = 0;
    let mut allNodesAreTable = true;
    for node in &nodes {
        if !hierarchicalNodes.contains(node) {
            return Ok(None);
        }
        allNodesAreTable = allNodesAreTable && node.IsTable();
        if isSource(node) {
            sourceCount += 1;
        } else if isSink(node) {
            sinkCount += 1;
        }
    }

    if !force && (allNodesAreTable || sourceCount == 0 || sinkCount == 0) {
        return Ok(None);
    }

    let hierarchy = layoutgraph::NewHierarchy();
    let dag = makeSimpleDAG(ctx, g)?;
    let ranks = rankDAG(ctx, Some(&dag));
    let ranks = match ranks {
        Ok(r) => r,
        Err(e) => {
            layoutgraph::Teardown(&dag);
            return Err(e);
        }
    };
    hierarchy.ReplaceLevels(mapDAGToGraphLevels(g, &ranks.nodeToLevel));
    hierarchy.bm().LevelCount = ranks.levelCount;
    // Rust-only: the temporary DAG owns only its own nodes/edges; break its Rc cycles.
    drop(ranks);
    layoutgraph::Teardown(&dag);
    if force || isValid(&hierarchy, g, edgeAbductions.unwrap_or(&[]), sourceCount, sinkCount) {
        return Ok(Some(hierarchy));
    }
    Ok(None)
}

/// isValid prefers compact hierarchies with a dominant edge direction.
pub(crate) fn isValid(h: &HierarchyRef, g: &GraphRef, edgeAbductions: &[EdgeAbductionRef], sourceCount: i64, sinkCount: i64) -> bool {
    let levelCount = h.b().LevelCount;
    if levelCount < minHierarchyLevels {
        return false;
    }
    let levels = h.Levels();
    let mut unique: Set<NodeRef> = Set::default();
    let mut uniqueOrder: Vec<NodeRef> = Vec::new();
    let mut levelNodes = levels.keys();
    layoutgraph::SortNodesByID(&mut levelNodes);
    for node in &levelNodes {
        if unique.insert(node.clone()) {
            uniqueOrder.push(node.clone());
        }
        for descendant in g.AllDescendantNodes(Some(node), false) {
            if unique.insert(descendant.clone()) {
                uniqueOrder.push(descendant);
            }
        }
    }
    let aspectRatio = unique.len() as f64 / (levelCount * levelCount) as f64;
    let tooTall = aspectRatio < 0.5;
    let tooWide = aspectRatio > 2.0;

    let (forwardEdges, backOrNeutralEdges) = countEdgeDirection(h, g);
    let edgesFlowInOneDirection = forwardEdges as f64 >= 1.5 * backOrNeutralEdges as f64;
    let allEdgesForward = backOrNeutralEdges == 0;
    let gnodeCount = g.b().Nodes.len() as i64;
    let mut branchedWorkflow = gnodeCount > levelCount
        && sourceCount == 1
        && sinkCount == 1
        && allEdgesForward
        && unique.len() as i64 <= maxAutomaticWorkflowNodes
        && forwardEdges + backOrNeutralEdges <= maxAutomaticWorkflowEdges;
    if tooTall && branchedWorkflow && !minimumWorkflowExtentFits(h, g) {
        branchedWorkflow = false;
    }

    let mut nEdgesAbductions: Map<NodeRef, i64> = Map::default();
    for ea in edgeAbductions {
        let ea = ea.b();
        if !isHierarchyStructuralEdge(&ea.Edge) {
            continue;
        }
        if ea.OriginallyFrom.is_some() {
            if let Some(cf) = &ea.CurrentFrom {
                *nEdgesAbductions.entry(cf.clone()).or_insert(0) += 1;
            }
        }
        if ea.OriginallyTo.is_some() {
            if let Some(ct) = &ea.CurrentTo {
                *nEdgesAbductions.entry(ct.clone()).or_insert(0) += 1;
            }
        }
    }
    let maxEdges = (2.0 * math::Ceil(math::Sqrt(unique.len() as f64))) as i64;
    let mut hasDenselyConnectedNode = false;
    for node in &uniqueOrder {
        let edgeCount = countHierarchyStructuralEdges(node) - nEdgesAbductions.get(node).copied().unwrap_or(0);
        if edgeCount > maxEdges {
            hasDenselyConnectedNode = true;
            break;
        }
    }

    !tooWide && (!tooTall || branchedWorkflow) && edgesFlowInOneDirection && !hasDenselyConnectedNode && !is1N1(h, edgeAbductions)
}

/// minimumWorkflowExtentFits screens current rank dimensions with the minimum placement gaps.
pub(crate) fn minimumWorkflowExtentFits(h: &HierarchyRef, g: &GraphRef) -> bool {
    let nodes = g.b().Nodes.clone();
    let horizontal = IsHorizontal(&nodes);
    let levelCount = h.b().LevelCount;
    let mut levelSize = vec![0.0f64; levelCount as usize];
    for (node, level) in h.Levels().entries() {
        let size = if horizontal { node.w() } else { node.h() };
        levelSize[level as usize] = math::Max(levelSize[level as usize], size);
    }
    let mut extent = (levelCount - 1) as f64 * (crossingSpacing + 2.0 * layoutgraph::MinPortClearance);
    for &size in &levelSize {
        extent += math::Ceil(size);
    }
    extent <= limits::MaxGraphSize as f64
}

/// countEdgeDirection counts the direction of the edges in the given graph considering the node levels.
pub(crate) fn countEdgeDirection(h: &HierarchyRef, g: &GraphRef) -> (i64, i64) {
    let mut forward = 0;
    let mut structural = 0;
    let levels = h.Levels();
    let edges = g.b().Edges.clone();
    for e in &edges {
        if !isHierarchyStructuralEdge(e) {
            continue;
        }
        structural += 1;
        if !e.IsDirected() {
            continue;
        }
        let (from, to, _) = e.DirectedEndpoints();
        let lt = to.map(|n| levels.at(&n)).unwrap_or(0);
        let lf = from.map(|n| levels.at(&n)).unwrap_or(0);
        if lt > lf {
            forward += 1;
        }
    }
    (forward, structural - forward)
}

/// is1N1 identifies a 1 -> N -> 1 hierarchy.
pub(crate) fn is1N1(h: &HierarchyRef, edgeAbductions: &[EdgeAbductionRef]) -> bool {
    if h.b().LevelCount != 3 {
        return false;
    }
    let levels = h.Levels();
    let mut nodesByLevel: Map<i64, Vec<NodeRef>> = Map::default();
    let mut entries = levels.entries();
    entries.sort_by_key(|(n, _)| n.id());
    for (node, level) in entries {
        nodesByLevel.entry(level).or_default().push(node);
    }
    let lvl = |l: i64| -> Vec<NodeRef> { nodesByLevel.get(&l).cloned().unwrap_or_default() };
    if lvl(0).len() != 1 || lvl(2).len() != 1 {
        return false;
    }
    let l0 = lvl(0)[0].clone();
    let l2 = lvl(2)[0].clone();

    let mut sourceEdgeAbduction: Option<EdgeAbductionRef> = None;
    let mut sinkEdgeAbduction: Option<EdgeAbductionRef> = None;
    for eaRef in edgeAbductions {
        let ea = eaRef.b().clone();
        if !isHierarchyStructuralEdge(&ea.Edge) {
            continue;
        }
        if ea.CurrentFrom.as_ref() == Some(&l0) {
            if let Some(s) = &sourceEdgeAbduction {
                if ea.OriginallyFrom != s.b().OriginallyFrom {
                    return false;
                }
            } else {
                sourceEdgeAbduction = Some(eaRef.clone());
            }
        }
        if ea.CurrentTo.as_ref() == Some(&l0) {
            if let Some(s) = &sourceEdgeAbduction {
                if ea.OriginallyTo != s.b().OriginallyTo {
                    return false;
                }
            } else {
                sourceEdgeAbduction = Some(eaRef.clone());
            }
        }

        if ea.CurrentFrom.as_ref() == Some(&l2) {
            if let Some(s) = &sinkEdgeAbduction {
                if ea.OriginallyFrom != s.b().OriginallyFrom {
                    return false;
                }
            } else {
                sinkEdgeAbduction = Some(eaRef.clone());
            }
        }
        if ea.CurrentTo.as_ref() == Some(&l2) {
            if let Some(s) = &sinkEdgeAbduction {
                if ea.OriginallyTo != s.b().OriginallyTo {
                    return false;
                }
            } else {
                sinkEdgeAbduction = Some(eaRef.clone());
            }
        }
    }

    for node in &lvl(1) {
        let mut hasEdgeAbove = false;
        let mut hasEdgeBelow = false;
        for e in node.edges() {
            if !isHierarchyStructuralEdge(&e) {
                continue;
            }
            let adj = node.Adjacent(&e);
            let l = levels.at(&adj);
            if l == 0 {
                hasEdgeAbove = true;
            } else if l == 2 {
                hasEdgeBelow = true;
            } else {
                return false;
            }
            if hasEdgeAbove && hasEdgeBelow {
                break;
            }
        }
        if !hasEdgeAbove || !hasEdgeBelow {
            return false;
        }
    }

    let source = l0;
    for e in source.edges() {
        if !isHierarchyStructuralEdge(&e) {
            continue;
        }
        let adj = source.Adjacent(&e);
        if levels.at(&adj) != 1 {
            return false;
        }
    }

    true
}

/// makeSimpleDAG transforms the input into a DAG and removes duplicate edges.
pub(crate) fn makeSimpleDAG(ctx: &Ctx, g: &GraphRef) -> GoResult<GraphRef> {
    let guard = limits::NewWorkGuard(ctx, "MakeSimpleDAG", limits::MaxEngineWorkUnits)?;
    let dag = layoutgraph::NewGraph();
    let r = makeSimpleDAGInto(ctx, g, &dag, &guard);
    if r.is_err() {
        // Rust-only: free the temporary DAG's Rc cycles on error paths.
        layoutgraph::Teardown(&dag);
    }
    r.map(|_| dag)
}

fn makeSimpleDAGInto(ctx: &Ctx, g: &GraphRef, dag: &GraphRef, guard: &WorkGuardRef) -> GoResult<()> {
    let connect = |from: &NodeRef, to: &NodeRef, rankWeight: i64| {
        let e = dag.Connect(from, to);
        let n = dag.b().Edges.len() as i64;
        {
            let mut eb = e.bm();
            eb.SourceArrowhead = layoutgraph::NoArrowhead.into();
            eb.TargetArrowhead = layoutgraph::TriangleArrowhead.into();
            eb.ID = n;
        }
        e.SetHierarchyRankWeight(rankWeight);
    };

    let mut idToNode: Map<layoutgraph::EntityID, NodeRef> = Map::default();
    let (gnodes, gedges) = {
        let gb = g.b();
        (gb.Nodes.clone(), gb.Edges.clone())
    };
    for n in &gnodes {
        guard.Step()?;
        let newN = dag.AddNode(&layoutgraph::NewNode(n.id(), n.w(), n.h()));
        newN.bm().D2ID = n.b().D2ID.clone();
        idToNode.insert(n.id(), newN);
    }

    for e in &gedges {
        guard.Step()?;
        if !isHierarchyStructuralEdge(e) {
            continue;
        }
        if e.IsDirected() {
            let (from, to, _) = e.DirectedEndpoints();
            let (from, to) = (from.unwrap(), to.unwrap());
            connect(&idToNode[&from.id()], &idToNode[&to.id()], 1);
        } else {
            connect(&idToNode[&e.from().id()], &idToNode[&e.to().id()], 1);
            connect(&idToNode[&e.to().id()], &idToNode[&e.from().id()], 0);
        }
    }

    let cycleEdges = findCycleEdges(ctx, dag)?;
    reverseEdges(ctx, &cycleEdges)?;
    removeDuplicateEdges(ctx, dag)?;
    let n = dag.b().Nodes.len();
    for _ in 0..n {
        guard.Step()?;
    }

    guard.Finish()
}

/// findCycleEdges implements Eades' feedback-arc-set heuristic.
///
/// Go iterates maps here; the result is independent of iteration order (the
/// sink/source closures are unique and the max-node choice is a total order
/// on (out-in, degree, -ID)). Iteration here uses ID order.
pub(crate) fn findCycleEdges(ctx: &Ctx, g: &GraphRef) -> GoResult<Vec<EdgeRef>> {
    let guard = limits::NewWorkGuard(ctx, "FindCycleEdges", limits::MaxEngineWorkUnits)?;
    let mut incomingEdges: Map<NodeRef, Set<EdgeRef>> = Map::default();
    let mut outgoingEdges: Map<NodeRef, Set<EdgeRef>> = Map::default();
    fn addEdgeToSet(e: &EdgeRef, n: &NodeRef, set: &mut Map<NodeRef, Set<EdgeRef>>) {
        set.entry(n.clone()).or_default().insert(e.clone());
    }
    fn lenOf(set: &Map<NodeRef, Set<EdgeRef>>, n: &NodeRef) -> usize {
        set.get(n).map_or(0, |s| s.len())
    }

    let mut nodes: Set<NodeRef> = Set::default();
    let mut edges: Vec<EdgeRef> = Vec::new();
    let mut edgeSet: Set<EdgeRef> = Set::default();
    let gedges = g.b().Edges.clone();
    for e in &gedges {
        guard.Step()?;
        if edgeSet.insert(e.clone()) {
            edges.push(e.clone());
        }
        let (from, to) = (e.from(), e.to());
        nodes.insert(from.clone());
        nodes.insert(to.clone());
        if e.IsTargetedTo(&from) {
            addEdgeToSet(e, &from, &mut incomingEdges);
            addEdgeToSet(e, &to, &mut outgoingEdges);
        } else {
            addEdgeToSet(e, &from, &mut outgoingEdges);
            addEdgeToSet(e, &to, &mut incomingEdges);
        }
    }

    let mut nonCycleEdges: Set<EdgeRef> = Set::default();
    let pruneNode = |n: &NodeRef,
                     nodes: &mut Set<NodeRef>,
                     incomingEdges: &mut Map<NodeRef, Set<EdgeRef>>,
                     outgoingEdges: &mut Map<NodeRef, Set<EdgeRef>>,
                     nonCycleEdges: &mut Set<EdgeRef>|
     -> GoResult<()> {
        let inc: Vec<EdgeRef> = incomingEdges.get(n).map(|s| s.iter().cloned().collect()).unwrap_or_default();
        for e in &inc {
            guard.Step()?;
            nonCycleEdges.insert(e.clone());
            let adj = n.Adjacent(e);
            if let Some(adjEdges) = outgoingEdges.get_mut(&adj) {
                adjEdges.remove(e);
            }
            if lenOf(outgoingEdges, &adj) == 0 && lenOf(incomingEdges, &adj) == 0 {
                nodes.remove(&adj);
                outgoingEdges.remove(&adj);
                incomingEdges.remove(&adj);
            }
        }
        let out: Vec<EdgeRef> = outgoingEdges.get(n).map(|s| s.iter().cloned().collect()).unwrap_or_default();
        for e in &out {
            guard.Step()?;
            nonCycleEdges.insert(e.clone());
            let adj = n.Adjacent(e);
            if let Some(adjEdges) = incomingEdges.get_mut(&adj) {
                adjEdges.remove(e);
            }
            if lenOf(incomingEdges, &adj) == 0 && lenOf(outgoingEdges, &adj) == 0 {
                nodes.remove(&adj);
                outgoingEdges.remove(&adj);
                incomingEdges.remove(&adj);
            }
        }
        nodes.remove(n);
        incomingEdges.remove(n);
        outgoingEdges.remove(n);
        Ok(())
    };
    let sortedNodes = |nodes: &Set<NodeRef>| -> Vec<NodeRef> {
        let mut v: Vec<NodeRef> = nodes.iter().cloned().collect();
        layoutgraph::SortNodesByID(&mut v);
        v
    };
    while !nodes.is_empty() {
        guard.Step()?;
        loop {
            let mut foundSink = false;
            for n in sortedNodes(&nodes) {
                if !nodes.contains(&n) {
                    continue;
                }
                guard.Step()?;
                if lenOf(&outgoingEdges, &n) == 0 {
                    foundSink = true;
                    pruneNode(&n, &mut nodes, &mut incomingEdges, &mut outgoingEdges, &mut nonCycleEdges)?;
                }
            }
            if !foundSink {
                break;
            }
        }

        loop {
            let mut foundSource = false;
            for n in sortedNodes(&nodes) {
                if !nodes.contains(&n) {
                    continue;
                }
                guard.Step()?;
                if lenOf(&incomingEdges, &n) == 0 {
                    foundSource = true;
                    pruneNode(&n, &mut nodes, &mut incomingEdges, &mut outgoingEdges, &mut nonCycleEdges)?;
                }
            }
            if !foundSource {
                break;
            }
        }

        if !nodes.is_empty() {
            let mut maxNode: Option<NodeRef> = None;
            let mut maxDiff = math::Inf(-1);
            for node in sortedNodes(&nodes) {
                guard.Step()?;
                let diff = (lenOf(&outgoingEdges, &node) as i64 - lenOf(&incomingEdges, &node) as i64) as f64;
                if diff > maxDiff {
                    maxDiff = diff;
                    maxNode = Some(node);
                } else if diff == maxDiff {
                    let mn = maxNode.clone().unwrap();
                    let nEdges = lenOf(&outgoingEdges, &node) + lenOf(&incomingEdges, &node);
                    let nMaxEdges = lenOf(&outgoingEdges, &mn) + lenOf(&incomingEdges, &mn);
                    if nEdges > nMaxEdges {
                        maxDiff = diff;
                        maxNode = Some(node);
                    } else if lenOf(&incomingEdges, &node) == lenOf(&incomingEdges, &mn) && node.id() < mn.id() {
                        maxDiff = diff;
                        maxNode = Some(node);
                    }
                }
            }
            let maxNode = maxNode.unwrap();
            let inc: Vec<EdgeRef> = incomingEdges.get(&maxNode).map(|s| s.iter().cloned().collect()).unwrap_or_default();
            for e in &inc {
                guard.Step()?;
                if let Some(s) = incomingEdges.get_mut(&maxNode) {
                    s.remove(e);
                }
                let adj = maxNode.Adjacent(e);
                if let Some(s) = outgoingEdges.get_mut(&adj) {
                    s.remove(e);
                }
            }
            pruneNode(&maxNode, &mut nodes, &mut incomingEdges, &mut outgoingEdges, &mut nonCycleEdges)?;
        }
    }

    for _ in nonCycleEdges.iter() {
        guard.Step()?;
    }
    let result: Vec<EdgeRef> = edges.into_iter().filter(|e| !nonCycleEdges.contains(e)).collect();

    guard.Finish()?;
    Ok(result)
}

/// reverseEdges exchanges from, to = to, from.
pub(crate) fn reverseEdges(ctx: &Ctx, edges: &[EdgeRef]) -> GoResult<()> {
    let guard = limits::NewWorkGuard(ctx, "ReverseEdges", limits::MaxEngineWorkUnits)?;
    for _ in edges {
        guard.Step()?;
    }
    guard.Finish()?;
    for e in edges {
        let mut eb = e.bm();
        let from = eb.From.clone();
        eb.From = eb.To.clone();
        eb.To = from;
    }
    Ok(())
}

/// removeDuplicateEdges combines multiple instances of the same edge (from -> to).
pub(crate) fn removeDuplicateEdges(ctx: &Ctx, g: &GraphRef) -> GoResult<()> {
    let guard = limits::NewWorkGuard(ctx, "RemoveDuplicateEdges", limits::MaxEngineWorkUnits)?;
    let mut edges: Map<(NodeRef, NodeRef), EdgeRef> = Map::default();
    let gedges = g.b().Edges.clone();
    let mut uniqueEdges: Vec<EdgeRef> = Vec::with_capacity(gedges.len());
    let mut rankWeights: Map<EdgeRef, i64> = Map::default();
    for e in &gedges {
        guard.Step()?;
        let key = (e.from(), e.to());
        if let Some(retained) = edges.get(&key) {
            *rankWeights.entry(retained.clone()).or_insert(0) += e.HierarchyRankWeight();
        } else {
            edges.insert(key, e.clone());
            uniqueEdges.push(e.clone());
            rankWeights.insert(e.clone(), e.HierarchyRankWeight());
        }
    }

    guard.Finish()?;
    for e in &uniqueEdges {
        e.SetHierarchyRankWeight(rankWeights.get(e).copied().unwrap_or(0));
    }
    g.ReplaceEdgesUnchecked(uniqueEdges);
    Ok(())
}

pub(crate) fn mapDAGToGraphLevels(g: &GraphRef, dagNodeToLevel: &Map<NodeRef, i64>) -> GoMap<NodeRef, i64> {
    let mut idToNode: Map<layoutgraph::EntityID, NodeRef> = Map::default();
    for n in g.b().Nodes.iter() {
        idToNode.insert(n.id(), n.clone());
    }
    let nodeToLevel: GoMap<NodeRef, i64> = GoMap::new();
    for (dagN, &level) in dagNodeToLevel {
        nodeToLevel.set(idToNode[&dagN.id()].clone(), level);
    }
    nodeToLevel
}
