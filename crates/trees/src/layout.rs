// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/trees/layout.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

fn isTerminalNode(isTerminal: &Map<NodeRef, bool>, n: &NodeRef) -> bool {
    isTerminal.contains_key(n)
}

pub(crate) fn extractTreesInContainer(
    g: &GraphRef,
    container: Option<&NodeRef>,
    guard: &WorkGuardRef,
) -> GoResult<Vec<(NodeRef, Vec<TreeRef>)>> {
    let containerNodes = g.containers().at(&container.cloned());
    let mut nodeToTree: Map<NodeRef, TreeRef> = Map::default();
    for node in &containerNodes {
        guard.Step()?;
        if node.IsTable() {
            return Ok(Vec::new());
        }
        nodeToTree.insert(node.clone(), newTree(node));
    }
    let adjacency = buildTreeAdjacencyMatrix(g, guard)?;

    let mut isTerminal: Map<NodeRef, bool> = Map::default();
    let mut sentinelToTreeRoots: Map<NodeRef, Vec<TreeRef>> = Map::default();
    let mut sentinelToArrowheads: Map<NodeRef, Set<layoutgraph::Arrowhead>> = Map::default();

    for c in g.containers().keys() {
        guard.Step()?;
        // Go's map key may be nil (root container); nil is never a node.
        if let Some(c) = c {
            isTerminal.insert(c, true);
        }
    }
    for vessel in g.sequences().keys() {
        guard.Step()?;
        isTerminal.insert(vessel, true);
    }
    for n in g.nodes() {
        guard.Step()?;
        let flag = {
            let nb = n.b();
            !nb.Nears.is_empty() || nb.FixedTopLeft.is_some()
        };
        if flag {
            isTerminal.insert(n.clone(), true);
        }
    }

    loop {
        let mut fringeNodes: Vec<NodeRef> = Vec::new();
        // GO-ALIAS: Go captured containerNodes once; removeNodeFromContainer
        // replaces the map entry with a fresh slice, so the captured slice is unchanged.
        let candidates = treeFringeNodes(&containerNodes, guard)?;
        for fnode in candidates {
            guard.Step()?;
            if !isTerminalNode(&isTerminal, &fnode) {
                fringeNodes.push(fnode);
            }
        }
        if fringeNodes.is_empty() {
            break;
        }

        for fnode in &fringeNodes {
            guard.Step()?;
            if isTerminalNode(&isTerminal, fnode) {
                continue;
            }
            let edges = fnode.edges();
            if edges.len() != 1 {
                return Err(treePreprocessBadState(format!("fringe node {} has invalid degree", fnode.id())));
            }
            let sentinelNode = fnode.Adjacent(&edges[0]);
            let sentinelEdges = sentinelNode.edges();
            if sentinelEdges.len() == 1 && sentinelNode.Adjacent(&sentinelEdges[0]) == *fnode {
                if !isTerminalNode(&isTerminal, &sentinelNode) {
                    isTerminal.insert(fnode.clone(), true);
                    continue;
                }
            }
            let Some(tree) = nodeToTree.get(fnode).cloned() else {
                return Err(treePreprocessBadState(format!("fringe node {} has no tree record", fnode.id())));
            };
            let sentinelEdge = adjacency.edgeBetween(fnode, &sentinelNode);
            tree.bm().SentinelEdge = sentinelEdge.clone();
            let Some(sentinelEdge) = sentinelEdge else {
                return Err(treePreprocessBadState(format!("fringe node {} has no graph sentinel edge", fnode.id())));
            };

            sentinelToTreeRoots.entry(sentinelNode.clone()).or_default();
            sentinelToArrowheads.entry(sentinelNode.clone()).or_default();

            let ah = sentinelEdge.ArrowheadTo(&sentinelNode);
            sentinelToTreeRoots.get_mut(&sentinelNode).unwrap().push(tree.clone());
            sentinelToArrowheads.get_mut(&sentinelNode).unwrap().insert(ah);

            if let Some(treeRoots) = sentinelToTreeRoots.get(fnode).cloned() {
                let treeRoots = filterTerminalTrees(&treeRoots, &isTerminal, guard)?;
                if treeRoots.is_empty() {
                    continue;
                }
                let directionCounts = countTreeDirections(&treeRoots, guard)?;
                let arrowheadCount = sentinelToArrowheads.get(fnode).map_or(0, |s| s.len());
                if directionCounts.len() > 1 || arrowheadCount > 1 {
                    isTerminal.insert(fnode.clone(), true);
                } else {
                    for t in &treeRoots {
                        addTreeChild(&tree, t, guard)?;
                    }
                    let hasUndirected = directionCounts.contains_key(&Undirected);
                    let sentinelEdgeDirection = treeEdgeDirection(fnode, &sentinelEdge);
                    let hasSentinelDirection = directionCounts.contains_key(&sentinelEdgeDirection);
                    let sentinelIsTerminal = !(hasUndirected || sentinelEdgeDirection == Undirected) && !hasSentinelDirection;
                    if sentinelIsTerminal {
                        isTerminal.insert(sentinelNode.clone(), true);
                    }
                }
            }
        }

        for fnode in &fringeNodes {
            guard.Step()?;
            if !isTerminalNode(&isTerminal, fnode) {
                removeNodeFromGraph(fnode, g, guard)?;
                let edge = nodeToTree.get(fnode).unwrap().b().SentinelEdge.clone();
                disconnectTreeEdge(g, edge.as_ref(), guard)?;
                removeNodeFromContainer(g, container, fnode, guard)?;
            }
        }
    }

    // Go returns a map; the caller only merges it into another map, so order is irrelevant.
    let mut terminalNodeToTreeRoots: Vec<(NodeRef, Vec<TreeRef>)> = Vec::new();
    for node in g.nodes() {
        guard.Step()?;
        if let Some(treeRoots) = sentinelToTreeRoots.get(&node).cloned() {
            let filteredRoots = filterTerminalTrees(&treeRoots, &isTerminal, guard)?;
            if filteredRoots.is_empty() {
                continue;
            }
            if g.sequences().has(&node) {
                setPair(&mut terminalNodeToTreeRoots, node.clone(), filteredRoots);
            } else {
                let (rootSentinel, roots) = handleTreeSubgraph(g, &node, container, filteredRoots, guard)?;
                setPair(&mut terminalNodeToTreeRoots, rootSentinel, roots);
            }
        }
    }
    guard.Check()?;
    Ok(terminalNodeToTreeRoots)
}

fn setPair(v: &mut Vec<(NodeRef, Vec<TreeRef>)>, k: NodeRef, val: Vec<TreeRef>) {
    if let Some(e) = v.iter_mut().find(|(n, _)| *n == k) {
        e.1 = val;
    } else {
        v.push((k, val));
    }
}

pub(crate) fn extractTrees(g: &GraphRef, guard: &WorkGuardRef) -> GoResult<layoutgraph::GoMap<Option<NodeRef>, Vec<TreeRef>>> {
    let terminalNodeToTreeRoots: layoutgraph::GoMap<Option<NodeRef>, Vec<TreeRef>> = layoutgraph::GoMap::new();
    let containerOrder = treeContainerRDFSOrder(g, None, guard)?;
    let mut order: Vec<Option<NodeRef>> = containerOrder.into_iter().map(Some).collect();
    order.push(None);
    for container in &order {
        guard.Step()?;
        let containerTrees = extractTreesInContainer(g, container.as_ref(), guard)?;
        for (rootSentinel, roots) in containerTrees {
            guard.Step()?;
            terminalNodeToTreeRoots.set(Some(rootSentinel), roots);
        }
    }
    Ok(terminalNodeToTreeRoots)
}

/// handleTreeSubgraph merges tree parts that share an internal root sentinel.
pub(crate) fn handleTreeSubgraph(
    g: &GraphRef,
    rootSentinel: &NodeRef,
    container: Option<&NodeRef>,
    roots: Vec<TreeRef>,
    guard: &WorkGuardRef,
) -> GoResult<(NodeRef, Vec<TreeRef>)> {
    if !(rootSentinel.b().Edges.is_empty() && roots.len() > 1) {
        return Ok((rootSentinel.clone(), roots));
    }
    if rootSentinel.IsContainer() {
        return Ok((rootSentinel.clone(), roots));
    }

    let mut newRootSentinel: Option<NodeRef> = None;
    let mut newTreeRoot: Option<TreeRef> = None;
    let candidateRoots = candidateTreeRoots(&roots, guard)?;
    for candidateRoot in &candidateRoots {
        guard.Step()?;
        let newRoot = treeEndOfLine(candidateRoot, guard)?;
        if let Some(newRoot) = newRoot {
            let jointNode = newTree(rootSentinel);
            for otherRoot in &roots {
                guard.Step()?;
                if *otherRoot != newRoot {
                    addTreeChild(&jointNode, otherRoot, guard)?;
                }
            }
            let edgeToRootSentinel = candidateRoot.b().SentinelEdge.clone();
            reverseTreeChain(candidateRoot, guard)?;
            addTreeChild(candidateRoot, &jointNode, guard)?;
            jointNode.bm().SentinelEdge = edgeToRootSentinel;

            newRootSentinel = Some(newRoot.b().Node.clone());
            let ntr = newRoot.b().Children[0].clone();
            ntr.bm().Parent = None;
            newTreeRoot = Some(ntr);
            break;
        }
    }

    if let (Some(newRootSentinel), Some(newTreeRoot)) = (newRootSentinel, newTreeRoot) {
        removeNodeFromGraph(rootSentinel, g, guard)?;
        g.AddNodeUnchecked(&newRootSentinel);
        guard.Step()?;
        let containers = g.containers();
        let mut filtered: Vec<NodeRef> = Vec::new();
        for n in containers.at(&container.cloned()) {
            guard.Step()?;
            if n == *rootSentinel {
                continue;
            }
            filtered.push(n);
        }
        filtered.push(newRootSentinel.clone());
        containers.set(container.cloned(), filtered);
        return Ok((newRootSentinel, vec![newTreeRoot]));
    }
    Ok((rootSentinel.clone(), roots))
}

/// prepare tree for operation defined to Bottom, when we want it to work for the provided orientation
pub(crate) fn invertOrientationToBottom(t: &TreeRef, orientation: geo::Orientation, guard: &WorkGuardRef) -> GoResult<()> {
    if orientation == geo::Top {
        return flip(t, guard);
    }
    if orientation == geo::Right {
        return swapDimensions(t, guard);
    }
    if orientation == geo::Left {
        swapDimensions(t, guard)?;
        return flip(t, guard);
    }
    guard.Check()
}

pub(crate) fn constructToOrientation(placementTree: &TreeRef, orientation: geo::Orientation, guard: &WorkGuardRef) -> GoResult<()> {
    setOrientation(placementTree, orientation, guard)?;
    invertOrientationToBottom(placementTree, orientation, guard)?;
    layoutTree(placementTree, guard)?;
    centerAlignChildren(placementTree, guard)?;
    invertOrientationToBottom(placementTree, orientation, guard)
}

pub(crate) fn treePlacementFixedOrigin(g: &GraphRef, container: Option<&NodeRef>, guard: &WorkGuardRef) -> GoResult<Option<geo::Point>> {
    for node in g.nodes() {
        guard.Step()?;
        if node.EffectiveContainer().as_ref() != container {
            continue;
        }
        if let Some(point) = node.FixedOrigin() {
            return Ok(Some(*point.b()));
        }
    }
    Ok(None)
}

fn positionOf(orientation: geo::Orientation, n: &NodeRef) -> f64 {
    if orientation == geo::Left {
        n.x()
    } else if orientation == geo::Right {
        n.x() + n.w()
    } else if orientation == geo::Top {
        n.y()
    } else {
        n.y() + n.h()
    }
}

fn moveTree(placementTree: &TreeRef, orientation: geo::Orientation, from: f64, to: f64, guard: &WorkGuardRef) -> GoResult<()> {
    let (mut dx, mut dy) = (0.0f64, 0.0f64);
    if orientation == geo::Left || orientation == geo::Right {
        dx = (to - from).round();
    } else if orientation == geo::Top || orientation == geo::Bottom {
        dy = (to - from).round();
    }
    let children = placementTree.b().Children.clone();
    for child in &children {
        guard.Step()?;
        offsetSubtree(child, dx, dy, guard)?;
    }
    guard.Finish()
}

pub(crate) fn placeAtOrientationGuarded(
    ctx: &Ctx,
    g: &GraphRef,
    placementTree: &TreeRef,
    orientation: geo::Orientation,
    guard: &WorkGuardRef,
) -> GoResult<f64> {
    let ctx = layoutgraph::ContextWithTransactionWorkGuard(ctx, Some(guard));
    let rootNode = placementTree.b().Node.clone();
    if !rootNode.has_tl() {
        return Err(treePreprocessBadState("tree placement encountered an incomplete placement tree"));
    }
    guard.Finish()?;
    let isFurtherOut = |a: f64, b: f64| -> bool {
        if orientation == geo::Left || orientation == geo::Top {
            a < b
        } else {
            a > b
        }
    };

    let rootPosition = positionOf(orientation, &rootNode);
    let mut borderPosition = rootPosition;
    let descendants = treeDescendants(placementTree, guard)?;
    let mut isNodeOfTree: Set<NodeRef> = gocompat::NewSetCap(descendants.len() + 1);
    isNodeOfTree.insert(rootNode.clone());
    for d in &descendants {
        guard.Step()?;
        isNodeOfTree.insert(d.b().Node.clone());
    }
    for n in g.nodes() {
        guard.Step()?;
        if !n.has_tl() {
            return Err(treePreprocessBadState("tree placement encountered an incomplete graph node"));
        }
        if !isNodeOfTree.contains(&n) {
            let position = positionOf(orientation, &n);
            if isFurtherOut(position, borderPosition) {
                borderPosition = position;
            }
        }
    }

    let mut currentBestPosition = borderPosition;
    let mut currentBestDistance = (currentBestPosition - rootPosition).abs();

    moveTree(placementTree, orientation, 0.0, borderPosition, guard)?;
    let fixedOrigin = treePlacementFixedOrigin(g, rootNode.EffectiveContainer().as_ref(), guard)?;
    if let Some(fo) = fixedOrigin {
        let mut hasNegative = false;
        let mut placementNodes = vec![placementTree.clone()];
        placementNodes.extend(descendants.iter().cloned());
        for tree in &placementNodes {
            guard.Step()?;
            let n = tree.b().Node.clone();
            if n.x() < fo.X || n.y() < fo.Y {
                hasNegative = true;
                break;
            }
        }
        if hasNegative {
            currentBestDistance = f64::INFINITY;
        }
    }

    let mut obstaclePositions: Vec<f64> = Vec::new();
    for n in g.nodes() {
        guard.Step()?;
        if !isNodeOfTree.contains(&n) {
            let position = positionOf(orientation, &n);
            if isFurtherOut(borderPosition, position) && isFurtherOut(position, rootPosition) {
                obstaclePositions.push(position);
            }
        }
    }
    obstaclePositions.push(rootPosition);

    let txn = g.NewRequestTransaction(&ctx, layoutgraph::TransactionOptions::default())?;
    for obstaclePosition in obstaclePositions {
        guard.Step()?;
        {
            let pt = placementTree.clone();
            let gd = guard.clone();
            let from = currentBestPosition;
            txn.AddOp(move || moveTree(&pt, orientation, from, obstaclePosition, &gd));
        }
        if let Err(err) = txn.Commit(&ctx) {
            txn.Clear();
            if layoutgraph::IsCandidateRejection(Some(&err)) {
                continue;
            }
            return Err(err);
        }
        if let Err(err) = guard.Finish() {
            txn.Clear();
            return Err(err);
        }
        let obstacleDistance = (obstaclePosition - rootPosition).abs();
        if obstacleDistance < currentBestDistance {
            currentBestPosition = obstaclePosition;
            currentBestDistance = obstacleDistance;
            if let Err(err) = txn.UpdateState() {
                txn.Clear();
                return Err(err);
            }
        } else {
            txn.Rollback();
        }
        txn.Clear();
    }

    guard.Finish()?;
    Ok(currentBestDistance)
}

pub(crate) fn putBackNonBranchingTrees(g: &GraphRef, guard: &WorkGuardRef) -> GoResult<()> {
    let containerOrder = treeContainerRDFSOrder(g, None, guard)?;
    let mut order: Vec<Option<NodeRef>> = containerOrder.into_iter().map(Some).collect();
    order.push(None);
    for container in &order {
        guard.Step()?;
        let containerNodes = g.containers().at(container);
        for node in &containerNodes {
            guard.Step()?;
            let Some(roots) = g.trees().get(&Some(node.clone())) else { continue };
            if roots.is_empty() {
                return Err(treePreprocessBadState(format!("tree sentinel {} has no roots", node.id())));
            }
            let isolated = isIsolatedTreeGuarded(g, node, guard)?;
            let mut firstBranches = false;
            if isolated && roots.len() == 1 {
                firstBranches = treeBranches(&roots[0], guard)?;
            }
            if isolated && (roots.len() > 1 || firstBranches) {
                if !g.sequences().has(node) {
                    continue;
                }
            }
            let mut branchingRoots: Vec<TreeRef> = Vec::with_capacity(roots.len());
            for root in &roots {
                guard.Step()?;
                let branches = treeBranches(root, guard)?;
                if !branches {
                    let reconnectionTree = newTree(node);
                    addTreeChild(&reconnectionTree, root, guard)?;
                    reconnectTreeGuarded(g, &reconnectionTree, container.as_ref(), guard)?;
                } else {
                    branchingRoots.push(root.clone());
                }
            }
            if !branchingRoots.is_empty() {
                g.trees().set(Some(node.clone()), branchingRoots);
            } else {
                g.trees().delete(&Some(node.clone()));
            }
        }
    }
    guard.Check()
}

pub(crate) fn preprocessTreesWithWorkLimit(ctx: &Ctx, g: &GraphRef, workLimit: i64) -> GoResult<()> {
    layoutgraph::Validate(ctx, treePreprocessLocation, g)?;
    let guard = newWorkGuard(ctx, treePreprocessLocation)?;
    guard.SetLimit(workLimit);

    let state = layoutgraph::NewGraphStateSnapshot(layoutgraph::GraphStateSnapshotOptions {
        CaptureTopology: true,
        CaptureEdgeRoutes: true,
    });
    state.UpdateWithWorkGuard(g, &guard)?;
    let result = (|| -> GoResult<()> {
        let trees = extractTrees(g, &guard)?;
        g.bm().Trees = trees;
        guard.Step()?;
        guard.Check()?;
        putBackNonBranchingTrees(g, &guard)?;
        buildNodeToTreeGuarded(g, &guard)?;
        guard.Finish()
    })();
    if result.is_err() {
        layoutgraph::RestoreGraphState(g, &state);
    }
    result
}

pub fn Preprocess(ctx: &Ctx, g: &GraphRef) -> GoResult<()> {
    withTreeRecording(g, || preprocessTreesWithWorkLimit(ctx, g, limits::MaxEngineWorkUnits))
}

/// if width >= height this is the aspect ratio, otherwise it is 1/the aspect ratio
pub(crate) fn normalizedAspectRatio(g: &GraphRef, guard: &WorkGuardRef) -> GoResult<f64> {
    let nodes = g.nodes();
    for node in &nodes {
        guard.Step()?;
        if !node.has_tl() {
            return Err(treePreprocessBadState("tree placement encountered an incomplete graph node"));
        }
        for _ in 0..nodes.len() {
            guard.Step()?;
        }
    }
    for edge in g.edges() {
        guard.Step()?;
        let n = edge.b().Points.len();
        for _ in 0..n {
            guard.Step()?;
        }
    }
    let (tl, br) = g.BoundingBox();
    let (Some(tl), Some(br)) = (tl, br) else {
        return Err(treePreprocessBadState("tree placement could not compute a graph bounding box"));
    };
    guard.Finish()?;
    let (tl, br) = (*tl.b(), *br.b());
    let width = br.X - tl.X;
    let height = br.Y - tl.Y;
    if width >= height {
        return Ok(width / height);
    }
    Ok(height / width)
}

/// reconnect tree to the graph with the best placement we can find
pub(crate) fn placeTree(
    ctx: &Ctx,
    g: &GraphRef,
    placementTree: &TreeRef,
    container: Option<&NodeRef>,
    existingPlacements: &Set<geo::Orientation>,
    guard: &WorkGuardRef,
) -> GoResult<geo::Orientation> {
    let descendants = treeDescendants(placementTree, guard)?;
    for t in &descendants {
        guard.Step()?;
        t.b().Node.set_tl(Some(geo::PointRef::new(geo::Point::default())));
        guard.Finish()?;
    }

    reconnectTreeGuarded(g, placementTree, container, guard)?;

    let mut bestOrientation = geo::Orientation::default();
    let mut bestDistance = f64::INFINITY;
    let mut bestRatio = f64::INFINITY;

    let defaultOrder = [geo::Bottom, geo::Top, geo::Left, geo::Right];
    let mut order: Vec<geo::Orientation> = Vec::new();

    let rootNode = placementTree.b().Node.clone();
    let isIsolatedTree = isIsolatedTreeGuarded(g, &rootNode, guard)?;
    if existingPlacements.len() == 1 {
        for o in defaultOrder {
            if existingPlacements.contains(&o) {
                bestOrientation = o.GetOpposite();
                break;
            }
        }
    } else {
        if isIsolatedTree {
            for o in defaultOrder {
                if existingPlacements.contains(&o) {
                    continue;
                }
                order.push(o);
            }
            if order.is_empty() {
                order = defaultOrder.to_vec();
            }
        } else {
            order = defaultOrder.to_vec();
        }

        let direction = g.Direction(container);
        for orientation in order {
            guard.Step()?;
            constructToOrientation(placementTree, orientation, guard)?;
            let mut distance = placeAtOrientationGuarded(ctx, g, placementTree, orientation, guard)?;
            let ratio = normalizedAspectRatio(g, guard)?;
            if orientation != direction {
                distance += directionPenalty;
            }
            if distance < bestDistance {
                bestOrientation = orientation;
                bestDistance = distance;
                bestRatio = ratio;
            } else if distance == bestDistance && ratio < bestRatio {
                bestOrientation = orientation;
                bestRatio = ratio;
            }
        }
    }

    guard.Step()?;
    constructToOrientation(placementTree, bestOrientation, guard)?;
    placeAtOrientationGuarded(ctx, g, placementTree, bestOrientation, guard)?;
    positionTreeEdgeLabels(placementTree, isIsolatedTree, guard)?;
    Ok(bestOrientation)
}

pub(crate) fn positionTreeEdgeLabels(placementTree: &TreeRef, isIsolatedTree: bool, guard: &WorkGuardRef) -> GoResult<()> {
    let orientation = placementTree.b().Orientation;
    invertOrientationToBottom(placementTree, orientation, guard)?;
    let result = (|| -> GoResult<()> {
        let mut labelTrees: Vec<TreeRef> = Vec::new();
        let children = placementTree.b().Children.clone();
        for root in &children {
            guard.Step()?;
            if isIsolatedTree {
                labelTrees.push(root.clone());
            } else {
                let rc = root.b().Children.clone();
                for child in rc {
                    guard.Step()?;
                    labelTrees.push(child);
                }
            }
        }
        for tree in &labelTrees {
            guard.Step()?;
            validateBottomOrientation(tree, guard)?;
        }
        for tree in &labelTrees {
            guard.Step()?;
            positionEdgeLabels(tree, guard)?;
        }
        guard.Finish()
    })();
    let restoreErr = invertOrientationToBottom(placementTree, orientation, guard);
    match result {
        Err(e) => Err(e),
        Ok(()) => restoreErr,
    }
}

pub(crate) fn placeTrees(ctx: &Ctx, g: &GraphRef, container: Option<&NodeRef>, guard: &WorkGuardRef) -> GoResult<()> {
    let mut placementTrees = buildPlacementTrees(g, guard)?;
    let mut treeSizes: Map<TreeRef, i64> = Map::default();
    for pt in &placementTrees {
        let size = treeSize(pt, guard)?;
        treeSizes.insert(pt.clone(), size);
    }
    let n = placementTrees.len();
    let mut width = 1usize;
    while width < n {
        for _ in 0..n {
            guard.Step()?;
        }
        if width > n / 2 {
            break;
        }
        width *= 2;
    }
    gocompat::sort::SlicesSortFunc(&mut placementTrees, |a: &TreeRef, b: &TreeRef| {
        let order = gocompat::sort::cmpCompare(treeSizes[b], treeSizes[a]);
        if order != 0 {
            return order;
        }
        let aid = a.b().Children[0].b().Node.id();
        let bid = b.b().Children[0].b().Node.id();
        gocompat::sort::cmpCompare(aid, bid)
    });
    let mut rootSentinelPlacements: Map<NodeRef, Set<geo::Orientation>> = Map::default();
    for pt in &placementTrees {
        guard.Step()?;
        let node = pt.b().Node.clone();
        let existing = rootSentinelPlacements.entry(node.clone()).or_default().clone();
        let placed = placeTree(ctx, g, pt, container, &existing, guard)?;
        rootSentinelPlacements.get_mut(&node).unwrap().insert(placed);
        guard.Finish()?;
    }
    let trees = g.trees();
    for rootSentinel in g.nodes() {
        guard.Step()?;
        for root in trees.at(&Some(rootSentinel.clone())) {
            guard.Step()?;
            root.bm().Parent = None;
            guard.Finish()?;
        }
    }
    guard.Check()
}

pub(crate) fn placeTreesWithWorkLimit(ctx: &Ctx, g: &GraphRef, container: Option<&NodeRef>, workLimit: i64) -> GoResult<()> {
    layoutgraph::Validate(ctx, "PlaceTrees", g)?;
    let guard = newWorkGuard(ctx, "PlaceTrees")?;
    guard.SetLimit(workLimit);
    let state = layoutgraph::NewGraphStateSnapshot(layoutgraph::GraphStateSnapshotOptions {
        CaptureTopology: true,
        CaptureEdgeRoutes: true,
    });
    state.UpdateWithWorkGuard(g, &guard)?;
    if let Some(c) = container {
        if !state.TracksNode(c) {
            return Err(treePreprocessBadState("cannot place trees into an unknown container"));
        }
    }
    let result = placeTrees(ctx, g, container, &guard).and_then(|_| guard.Finish());
    if result.is_err() {
        layoutgraph::RestoreGraphState(g, &state);
    }
    result
}

pub fn Place(ctx: &Ctx, g: &GraphRef, container: Option<&NodeRef>) -> GoResult<()> {
    withTreeRecording(g, || placeTreesWithWorkLimit(ctx, g, container, limits::MaxPlaceTreesWorkUnits))
}

pub(crate) fn buildPlacementTrees(g: &GraphRef, guard: &WorkGuardRef) -> GoResult<Vec<TreeRef>> {
    let mut placementTrees: Vec<TreeRef> = Vec::new();
    let trees = g.trees();
    for rootSentinel in g.nodes() {
        guard.Step()?;
        let firstPlacementTree = placementTrees.len();
        let Some(roots) = trees.get(&Some(rootSentinel.clone())) else { continue };

        let isIsolated = isIsolatedTreeGuarded(g, &rootSentinel, guard)?;
        if isIsolated {
            let mut byDirection = rootsByTreeDirection(&roots, guard)?;
            let undirected = byDirection.remove(&Undirected).unwrap_or_default();
            // GO-MAP-ORDER: Go ranges over this map (random order). The port uses a
            // fixed direction order; see DEVIATIONS.md.
            for dir in [Outwards, Inwards, Bidirectional] {
                let Some(matchingRoots) = byDirection.get(&dir) else { continue };
                let mut arrowheadSentinels: Vec<(layoutgraph::Arrowhead, TreeRef)> = Vec::new();
                for root in matchingRoots {
                    guard.Step()?;
                    let Some(se) = root.b().SentinelEdge.clone() else {
                        return Err(treePreprocessBadState("tree placement encountered an incomplete root"));
                    };
                    let arrowhead = se.ArrowheadTo(&rootSentinel);
                    let sentinel = match arrowheadSentinels.iter().find(|(a, _)| *a == arrowhead) {
                        Some((_, t)) => t.clone(),
                        None => {
                            let t = newTree(&rootSentinel);
                            arrowheadSentinels.push((arrowhead, t.clone()));
                            placementTrees.push(t.clone());
                            t
                        }
                    };
                    addTreeChild(&sentinel, root, guard)?;
                }
            }
            if !undirected.is_empty() {
                let mut mergeWith: Option<TreeRef> = None;
                for tree in placementTrees[firstPlacementTree..].to_vec() {
                    guard.Step()?;
                    let treeChildren = tree.b().Children.clone();
                    if let Some(m) = &mergeWith {
                        if treeChildren.len() <= m.b().Children.len() {
                            continue;
                        }
                    }
                    if !treeChildren.is_empty()
                        && !treeChildren[0].b().SentinelEdge.clone().unwrap().HasArrowTo(&rootSentinel)
                    {
                        mergeWith = Some(tree.clone());
                    }
                }
                let mergeWith = match mergeWith {
                    Some(m) => m,
                    None => {
                        let m = newTree(&rootSentinel);
                        placementTrees.push(m.clone());
                        m
                    }
                };
                for root in &undirected {
                    addTreeChild(&mergeWith, root, guard)?;
                }
            }
        } else {
            for root in &roots {
                guard.Step()?;
                let placementTree = newTree(&rootSentinel);
                addTreeChild(&placementTree, root, guard)?;
                placementTrees.push(placementTree);
            }
        }
    }
    guard.Check()?;
    Ok(placementTrees)
}
