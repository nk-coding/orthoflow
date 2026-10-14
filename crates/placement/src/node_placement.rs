// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/placement/node_placement.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of placement/node_placement.go
use crate::*;
use gocompat::Ctx;
use gocompat::rand::{self, RandRef};
use gocompat::{math, wrapf, GoResult, Map, Set};
use layoutgraph::{nodeEq, EdgeAbductionRef, GraphRef, NodeRef, NodesMethods, TreeRef};

// placeNodes is phase 1 of the algorithm
pub fn placeNodes(
    ctx: &Ctx,
    g: &GraphRef,
    root: Option<&NodeRef>,
    randomSeed: i64,
    prevEdgeAbductions: Option<&[EdgeAbductionRef]>,
    ancestorObstacles: &[geo::Box],
) -> GoResult<()> {
    // Edge cases
    let gNodes = g.b().Nodes.clone();
    if gNodes.is_empty() {
        return Ok(());
    }

    if gNodes.len() == 1 {
        let node = &gNodes[0];
        match node.fixedTL() {
            Some(ftl) => node.set_tl(Some(ftl.Copy())),
            None => node.set_tl(Some(geo::NewPoint(0.0, 0.0))),
        }
        g.SyncNestedGeometry();
        // there may be trees that need to be reconnected to this node
        trees::Place(ctx, g, root)?;
        let nodes = g.b().Nodes.clone();
        return direct(ctx, g, &nodes, root, directOptions::default());
    }

    // This graph is one undergoing node placement in this call
    let childrenGraph = layoutgraph::NewGraph();
    childrenGraph.CopyEntitiesFrom(g);

    // Add every direct child of the current root to the new graph
    let rootChildren = g.b().Containers.at(&root.cloned());
    for child in &rootChildren {
        childrenGraph.AddNodeUnchecked(child);
    }
    if childrenGraph.b().Nodes.is_empty() {
        return Ok(());
    }

    // When performing layout for a fixed child A, if child B is also fixed it creates an obstacle to try to avoid for A's children layout
    // we want to pass each child these other obstacles to try to avoid
    let mut fixedObstacles: Vec<geo::Box> = Vec::new();
    for n in rootChildren.FixedNodes() {
        let ftl = n.fixedTL().expect("nil pointer dereference: FixedTopLeft");
        fixedObstacles.push(geo::Box { TopLeft: Some(ftl.Copy()), Width: n.w(), Height: n.h() });
    }

    // we want to take the ancestor obstacles for layout at this level, and translate them to be relative to this root node
    let mut shift: Option<(f64, f64)> = None;
    if let Some(root) = root {
        if let Some(rftl) = root.fixedTL() {
            if ancestorObstacles.len() > 1 {
                let rftl = *rftl.b();
                let padding = layoutgraph::ContainerPadding as f64;
                let (dx, dy) = (-rftl.X - padding, -rftl.Y - padding);
                if dx != 0.0 || dy != 0.0 {
                    for b in ancestorObstacles {
                        let mut p = b.TL().bm();
                        p.X += dx;
                        p.Y += dy;
                    }
                    shift = Some((dx, dy));
                }
            }
        }
    }

    let result = placeNodesAbducted(ctx, g, root, randomSeed, prevEdgeAbductions, ancestorObstacles, &childrenGraph, &fixedObstacles);

    if let Some((dx, dy)) = shift {
        for b in ancestorObstacles {
            let mut p = b.TL().bm();
            p.X -= dx;
            p.Y -= dy;
        }
    }
    result
}

fn placeNodesAbducted(
    ctx: &Ctx,
    g: &GraphRef,
    root: Option<&NodeRef>,
    randomSeed: i64,
    prevEdgeAbductions: Option<&[EdgeAbductionRef]>,
    ancestorObstacles: &[geo::Box],
    childrenGraph: &GraphRef,
    fixedObstacles: &[geo::Box],
) -> GoResult<()> {
    let edgeAbductions = g.AbductEdges(root, childrenGraph);
    let result = placeNodesSplit(ctx, g, root, randomSeed, prevEdgeAbductions, ancestorObstacles, childrenGraph, fixedObstacles, &edgeAbductions);
    if result.is_err() {
        g.RestoreEdgeAbductions(&edgeAbductions);
        let nodes = g.b().Nodes.clone();
        nodes.SetGraphReference(g);
    }
    result
}

fn placeNodesSplit(
    ctx: &Ctx,
    g: &GraphRef,
    root: Option<&NodeRef>,
    randomSeed: i64,
    prevEdgeAbductions: Option<&[EdgeAbductionRef]>,
    ancestorObstacles: &[geo::Box],
    childrenGraph: &GraphRef,
    fixedObstacles: &[geo::Box],
    edgeAbductions: &[EdgeAbductionRef],
) -> GoResult<()> {
    let rootChildren = g.b().Containers.at(&root.cloned());
    let childrenOrder = PlaceChildrenOrder(ctx, &rootChildren, edgeAbductions)?;
    for child in &childrenOrder {
        if child.b().Hierarchy.is_some() {
            continue;
        }
        // If the direct child is a container with more children, then recurse first
        // So we go from most nested to least (root)
        if child.IsContainer() {
            placeNodes(ctx, g, Some(child), randomSeed, Some(edgeAbductions), fixedObstacles)?;
            loops::UpdateOffsets(child);
            // descendant may have placed trees so we need to update containers map
            let containers = g.b().Containers.clone();
            childrenGraph.bm().Containers = containers;
        }
        if child.IsClusterVessel() {
            let cluster = g.b().Clusters.get(child).expect("nil pointer dereference: cluster");
            let clusterNodes = cluster.b().Nodes.clone();
            for n in &clusterNodes {
                if n.IsContainer() {
                    placeNodes(ctx, g, Some(n), randomSeed, Some(edgeAbductions), fixedObstacles)?;
                    let containers = g.b().Containers.clone();
                    childrenGraph.bm().Containers = containers;
                }
            }
        }
    }

    // we don't know how the subgraphs will be combined (unless it is only one),
    // but we can try to layout each subgraph assuming it is the only one and therefore it should try to avoid fixed ancestors
    // then when we combine subgraphs, we can also avoid fixed ancestors

    let prev: &[EdgeAbductionRef] = prevEdgeAbductions.unwrap_or(&[]);
    proximity::AssignNears(ctx, childrenGraph, root, prev)?;
    proximity::AssignHerds(ctx, childrenGraph, root, prev)?;
    let (subgraphs, splitOwnership) = childrenGraph.SplitSubgraphsTracked(
        ctx,
        layoutgraph::SplitOptions { IncludeNears: true, TraverseTrees: true, ..Default::default() },
        None,
    );
    let subgraphs = subgraphs?;
    let result = placeSubgraphs(ctx, g, root, randomSeed, ancestorObstacles, &subgraphs, edgeAbductions);
    if result.is_err() {
        // SplitSubgraphs redirects every reachable node to a temporary graph,
        // including Near-only nodes outside g.Nodes. Restore their exact prior
        // owners if any later placement work fails or panics.
        splitOwnership.Restore();
    }
    result
}

fn placeSubgraphs(
    ctx: &Ctx,
    g: &GraphRef,
    root: Option<&NodeRef>,
    randomSeed: i64,
    ancestorObstacles: &[geo::Box],
    subgraphs: &[GraphRef],
    edgeAbductions: &[EdgeAbductionRef],
) -> GoResult<()> {
    let rootKey = root.cloned();
    for subgraph in subgraphs {
        subgraph.ComputeCellSize();
        // Put all the nested nodes Graph reference as the subgraph
        // There are calls that use the node's graph reference
        let nodes = subgraph.b().Nodes.clone();
        nodes.SetGraphReference(subgraph);

        // TODO: this must be a better check, like only select the nodes that must be placed
        // probably, we want to skip placing nodes that were already placed
        if nodes[0].b().Hierarchy.is_none() {
            let rng = rand::New(rand::NewSource(randomSeed));
            placeNodesOrthogonally(ctx, root, subgraph, Some(edgeAbductions), &rng, ancestorObstacles, Some(randomSeed))?;
            let nodes = subgraph.b().Nodes.clone();
            for n in &nodes {
                if n.IsClusterVessel() {
                    let cluster = subgraph.b().Clusters.get(n).expect("nil pointer dereference: cluster");
                    optimizeCluster(ctx, &cluster, true)?;
                }
            }
        }

        trees::Place(ctx, subgraph, root)?;
        // Tree nodes and edges have been restored within subgraph but we need to also add these back to g
        let nodes = subgraph.b().Nodes.clone();
        for n in &nodes {
            let treeRoots = g.b().Trees.at(&Some(n.clone()));
            for treeRoot in &treeRoots {
                let mut all: Vec<TreeRef> = trees::Descendants(Some(treeRoot));
                all.push(treeRoot.clone());
                for tree in &all {
                    let (tnode, sentinelEdge) = {
                        let t = tree.b();
                        (t.Node.clone(), t.SentinelEdge.clone())
                    };
                    g.AddNodeUnchecked(&tnode);
                    g.AddEdge(&sentinelEdge.expect("nil pointer dereference: tree.SentinelEdge"));
                }
            }
        }
        // reconnectTree restores the node to subgraph.Containers[root] but if the slice grows from an append,
        // subgraph.Containers[root] will be a different slice than g.Containers[root] so we need to sync the entry in g.Containers
        let rootEntry = subgraph.b().Containers.at(&rootKey);
        g.b().Containers.set(rootKey.clone(), rootEntry.clone());
        for otherSubgraph in subgraphs {
            otherSubgraph.b().Containers.set(rootKey.clone(), rootEntry.clone());
        }

        // To determine within mirroring which container contents are reachable and should be recursively mirrored,
        // the edges need to be temporarily restored
        subgraph.RestoreEdgeAbductions(edgeAbductions);
        let nodes = subgraph.b().Nodes.clone();
        let directErr = direct(ctx, subgraph, &nodes, root, directOptions::default());
        subgraph.ApplyEdgeAbductions(edgeAbductions);
        directErr?;

        let nodes = subgraph.b().Nodes.clone();
        validatePlacedNodes(root, &nodes)?;
    }
    let combined = packing::CombineSubgraphs(ctx, g, subgraphs, ancestorObstacles)?;

    orientSourceInterior(ctx, &combined, root, ancestorObstacles)?;
    // Set the dimensions of the root according to outcome of node placement
    if let Some(root) = root {
        root.FitToGraph(&combined, g.ContainerPadding(Some(root), false));
        // The root may be a cluster node, in which case all the other cluster nodes have to also be resized
        let clusters = g.b().Clusters.values();
        for c in &clusters {
            let (is, vessel) = {
                let cb = c.b();
                (cb.Nodes.iter().any(|n| n == root), cb.Vessel.clone())
            };
            if is {
                c.Resize(&vessel);
            }
        }
    }

    g.RestoreEdgeAbductions(edgeAbductions);
    let nodes = g.b().Nodes.clone();
    nodes.SetGraphReference(g);
    Ok(())
}

// This routine is heavily based on the algorithm proposed in Graph Compact Orthogonal Layout Algorithm by Freivalds and Glagolevs
pub fn placeNodesOrthogonally(
    ctx: &Ctx,
    root: Option<&NodeRef>,
    subgraph: &GraphRef,
    edgeAbductions: Option<&[EdgeAbductionRef]>,
    randGenerator: &RandRef,
    obstacles: &[geo::Box],
    seed: Option<i64>,
) -> GoResult<()> {
    let fixed = subgraph.FixedNodes();
    let mut initialized = false;
    let mut res: GoResult<()> = Ok(());
    if let Some(seed) = seed {
        if seed % 2 == 0 {
            match initializeByGraphDistance(ctx, subgraph) {
                Ok(v) => initialized = v,
                Err(err) => res = Err(err),
            }
        }
    }
    if res.is_ok() && !initialized {
        res = initializeNodes(ctx, subgraph);
    }
    res?;
    // After nodes in this nesting level have been initialized, the inner cluster nodes and container children need to also move
    subgraph.SyncNestedGeometry();

    let nodeCount = subgraph.b().Nodes.len() as f64;
    let numIterations = math::F64ToInt(nodePlacementIterations as f64 * math::Sqrt(nodeCount));
    let mut temp = 2.0 * math::Sqrt(nodeCount);
    let coolingFactor = math::Pow(0.2 / temp, 1.0 / numIterations as f64);

    // run compaction every 9 iterations alternating between horizontal and vertical compaction
    let compactionIteration = 9i64;
    let mut compactionAxis = horizontalAxis;

    let mut sizeless = newSizelessOptimizer(ctx, subgraph, Some(randGenerator))?;
    let mut i = 0i64;
    while i < numIterations / 2 {
        if let Some(err) = ctx.Err() {
            return Err(wrapf!(err, "PlaceNodes: {}", err));
        }
        sizeless.optimize(ctx, temp)?;

        if i % compactionIteration == 0 {
            compaction(ctx, subgraph, compactionOptions { axis: compactionAxis, factor: compactionFactor, ..Default::default() })?;
            compactionAxis = compactionAxis.opposite();
            sizeless.resetOccupied();
        }
        temp = temp * coolingFactor;
        i += 1;
    }

    let abductionsOwned: Option<Vec<EdgeAbductionRef>> = edgeAbductions.map(|a| a.to_vec());
    compaction(
        ctx,
        subgraph,
        compactionOptions {
            edgeAbductions: abductionsOwned.clone(),
            axis: horizontalAxis,
            includeSizes: true,
            factor: compactionFactor,
            transition: true,
            ..Default::default()
        },
    )?;
    compaction(
        ctx,
        subgraph,
        compactionOptions {
            edgeAbductions: abductionsOwned.clone(),
            axis: verticalAxis,
            includeSizes: true,
            factor: compactionFactor,
            transition: true,
            ..Default::default()
        },
    )?;

    // if there are fixed nodes, we want to shift them to their fixed top left positions
    if !fixed.is_empty() {
        let ftl = *fixed[0].fixedTL().expect("FixedTopLeft").b();
        let tl = fixed[0].tlv();
        let dx = ftl.X - tl.X;
        let dy = ftl.Y - tl.Y;
        let nodes = subgraph.b().Nodes.clone();
        for n in &nodes {
            n.MoveWithChildren(dx, dy);
        }
        for n in &fixed {
            // adjust precisely ignoring cell size alignment
            let f = *n.fixedTL().expect("FixedTopLeft").b();
            n.MoveAbsWithChildren(f.X, f.Y);
        }
    }

    validateCellSize(subgraph)?;
    let cellSize = subgraph.b().CellSize;
    let nodes = subgraph.b().Nodes.clone();
    for n in &nodes {
        if n.b().FixedTopLeft.is_some() {
            continue;
        }

        let tl = n.tlv();
        let mut x = tl.X;
        let mut y = tl.Y;
        if math::Mod(tl.X, cellSize) != 0.0 {
            x = roundToPreviousCellSize(tl.X, cellSize);
        }
        if math::Mod(tl.Y, cellSize) != 0.0 {
            y = roundToPreviousCellSize(tl.Y, cellSize);
        }
        if x != tl.X || y != tl.Y {
            n.MoveAbsWithChildren(x, y);
        }
    }

    validateGridAlignment(subgraph)?;

    // The turn cost is cached for edge length estimates, and we compute here
    subgraph.TurnCost();
    // Since the compaction factor is rather high, we estimate the overall maxLength would actually be ~half it
    subgraph.HalveTurnCost();

    let mut sized = newSizedOptimizer(ctx, subgraph, root, edgeAbductions, Some(randGenerator), obstacles)?;

    proximity::SyncHerdFences(subgraph);
    let mut i = (numIterations / 2) + 1;
    while i < numIterations {
        if let Some(err) = ctx.Err() {
            return Err(wrapf!(err, "PlaceNodes: {}", err));
        }
        sized.optimize(ctx, temp)?;

        if i % compactionIteration == 0 {
            let factor = math::Max(
                1.0,
                1.0 + ((2.0 * (numIterations - i - 30) as f64) / (0.5 * numIterations as f64)),
            );
            compaction(
                ctx,
                subgraph,
                compactionOptions {
                    edgeAbductions: abductionsOwned.clone(),
                    axis: compactionAxis,
                    includeSizes: true,
                    factor,
                    ..Default::default()
                },
            )?;
            compactionAxis = compactionAxis.opposite();

            grouping::JoinDistancedClusters(ctx, subgraph)?;
            proximity::SyncHerdFences(subgraph);
        }
        temp = temp * coolingFactor;
        i += 1;
    }
    grouping::JoinDistancedClusters(ctx, subgraph)?;
    proximity::SyncHerdFences(subgraph);

    for _ in 0..10 {
        let changed = sized.optimize(ctx, 0.0)?;
        if !changed {
            break;
        }
    }

    if !fixed.is_empty() {
        let ftl = *fixed[0].fixedTL().expect("FixedTopLeft").b();
        let tl = fixed[0].tlv();
        let dx = ftl.X - tl.X;
        let dy = ftl.Y - tl.Y;
        let nodes = subgraph.b().Nodes.clone();
        for n in &nodes {
            n.MoveWithChildren(dx, dy);
        }
        for n in &fixed {
            // adjust precisely ignoring cell size alignment
            let f = n.fixedTL().expect("FixedTopLeft");
            n.set_tl(Some(f.Copy()));
        }
    }
    Ok(())
}

pub fn validatePlacedNodes(root: Option<&NodeRef>, nodes: &[NodeRef]) -> GoResult<()> {
    for node in nodes {
        if !node.has_tl() {
            return Err(invariant::Errorf(format!(
                "node {} was not placed under container {}",
                node.id(),
                layoutgraph::entityIDOf(root)
            )));
        }
    }
    Ok(())
}

pub fn validateGridAlignment(g: &GraphRef) -> GoResult<()> {
    validateCellSize(g)?;
    let (nodes, cellSize) = {
        let gb = g.b();
        (gb.Nodes.clone(), gb.CellSize)
    };
    for node in &nodes {
        if node.b().FixedTopLeft.is_some() {
            continue;
        }
        let Some(tl) = node.tl() else {
            return Err(invariant::Errorf(format!("node {} has no position", node.id())));
        };
        let tl = *tl.b();
        if math::Mod(tl.X, cellSize) != 0.0 || math::Mod(tl.Y, cellSize) != 0.0 {
            return Err(invariant::Errorf(format!(
                "node {} at ({}, {}) is not aligned to cell size {}",
                node.id(),
                tl.X,
                tl.Y,
                cellSize
            )));
        }
    }
    Ok(())
}

pub fn validateCellSize(g: &GraphRef) -> GoResult<()> {
    let cellSize = g.b().CellSize;
    if cellSize < 1.0 || math::IsNaN(cellSize) || math::IsInf(cellSize, 0) || math::Trunc(cellSize) != cellSize {
        return Err(invariant::Errorf(format!("invalid cell size {}", cellSize)));
    }
    Ok(())
}

pub fn PlaceChildrenOrder(ctx: &Ctx, nodes: &[NodeRef], edgeAbductions: &[EdgeAbductionRef]) -> GoResult<Vec<NodeRef>> {
    let check = || -> GoResult<()> {
        if let Some(err) = ctx.Err() {
            return Err(wrapf!(err, "PlaceChildrenOrder: {}", err));
        }
        Ok(())
    };
    check()?;
    let mut expected: Set<NodeRef> = Set::default();
    let mut connected: Map<NodeRef, Set<NodeRef>> = Map::default();
    for node in nodes {
        check()?;
        if expected.contains(node) {
            return Err(invariant::Errorf(format!("container has duplicate child {}", node.DebugID())));
        }
        expected.insert(node.clone());
        connected.insert(node.clone(), Set::default());
    }
    for edgeAbduction in edgeAbductions {
        check()?;
        let (from, to) = {
            let ea = edgeAbduction.b();
            (ea.CurrentFrom.clone(), ea.CurrentTo.clone())
        };
        if let (Some(from), Some(to)) = (&from, &to) {
            if expected.contains(from) && expected.contains(to) {
                connected.get_mut(from).unwrap().insert(to.clone());
            }
            if expected.contains(to) && expected.contains(from) {
                connected.get_mut(to).unwrap().insert(from.clone());
            }
        }
    }

    let mut ordered: Vec<NodeRef> = Vec::with_capacity(nodes.len());
    let mut orderedSet: Set<NodeRef> = Set::default();
    let appendNode = |node: &NodeRef, ordered: &mut Vec<NodeRef>, orderedSet: &mut Set<NodeRef>, connected: &mut Map<NodeRef, Set<NodeRef>>| {
        ordered.push(node.clone());
        orderedSet.insert(node.clone());
        connected.remove(node);
    };
    for node in nodes {
        check()?;
        if connected.get(node).map_or(0, |s| s.len()) == 0 {
            appendNode(node, &mut ordered, &mut orderedSet, &mut connected);
        }
    }

    while ordered.len() < nodes.len() {
        check()?;
        let mut leastDegree = nodes.len() + 1;
        let mut start: Option<NodeRef> = None;
        for node in nodes {
            if let Some(adjacent) = connected.get(node) {
                if adjacent.len() < leastDegree {
                    leastDegree = adjacent.len();
                    start = Some(node.clone());
                }
            }
        }
        let Some(start) = start else {
            return Err(invariant::New("could not order all container children"));
        };

        let mut visited: Set<NodeRef> = Set::default();
        let mut queue: std::collections::VecDeque<NodeRef> = std::collections::VecDeque::new();
        queue.push_back(start);
        while let Some(current) = queue.front().cloned() {
            check()?;
            queue.pop_front();
            if visited.contains(&current) {
                continue;
            }
            visited.insert(current.clone());
            if orderedSet.contains(&current) {
                continue;
            }
            appendNode(&current, &mut ordered, &mut orderedSet, &mut connected);
            for edgeAbduction in edgeAbductions {
                let ea = edgeAbduction.b();
                let adjacent = if nodeEq(ea.CurrentFrom.as_ref(), Some(&current)) {
                    ea.CurrentTo.clone()
                } else if nodeEq(ea.CurrentTo.as_ref(), Some(&current)) {
                    ea.CurrentFrom.clone()
                } else {
                    None
                };
                if let Some(adjacent) = adjacent {
                    if expected.contains(&adjacent) {
                        queue.push_back(adjacent);
                    }
                }
            }
        }
    }
    check()?;
    Ok(ordered)
}
