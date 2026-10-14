// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/hierarchy/placement.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;
use gocompat::rand::RandRef;

pub(crate) const crossingSpacing: f64 = 50.0;
pub(crate) const parentSpacing: f64 = 300.0;
pub(crate) const siblingDummySpacing: f64 = 50.0;
pub(crate) const siblingSpacing: f64 = 60.0;

/// Index of a placementNode in the arena (Go `*placementNode`).
pub(crate) type PN = usize;
/// Index of a shared slice in the arena. Go slices of placement nodes
/// (`pn.children`, `byLevel[l]`) are aliased by `nodeToSiblings` and permuted
/// in place, so they live in the arena too. GO-ALIAS
pub(crate) type SL = usize;

pub(crate) struct placementNode {
    pub(crate) graphNode: Option<NodeRef>,
    pub(crate) level: i64,
    /// level index considering the order left to right as if it was a flatten array
    pub(crate) rank: i64,
    /// Go map sets; kept in insertion order (Go's order is random and the
    /// algorithm does not depend on it).
    pub(crate) aboves: Vec<PN>,
    pub(crate) belows: Vec<PN>,
    pub(crate) container: Option<PN>,
    pub(crate) children: SL,
    pub(crate) optimizeChildrenCrossings: bool,
    pub(crate) isChainningConnection: bool,
    pub(crate) isContainer: bool,
    pub(crate) isDummy: bool,
}

#[derive(Default)]
pub(crate) struct placementArena {
    pub(crate) nodes: Vec<placementNode>,
    pub(crate) slices: Vec<Vec<PN>>,
}

/// Go `map[int][]*placementNode`.
pub(crate) type byLevelMap = Map<i64, SL>;

pub(crate) fn setInsert(s: &mut Vec<PN>, v: PN) {
    if !s.contains(&v) {
        s.push(v);
    }
}

pub(crate) fn setRemove(s: &mut Vec<PN>, v: PN) {
    if let Some(i) = s.iter().position(|&x| x == v) {
        s.remove(i);
    }
}

impl placementArena {
    pub(crate) fn newSlice(&mut self, v: Vec<PN>) -> SL {
        self.slices.push(v);
        self.slices.len() - 1
    }

    pub(crate) fn newPlacementNode(&mut self, level: i64, node: Option<NodeRef>) -> PN {
        let children = self.newSlice(Vec::new());
        self.nodes.push(placementNode {
            graphNode: node,
            level,
            rank: 0,
            aboves: Vec::new(),
            belows: Vec::new(),
            container: None,
            children,
            optimizeChildrenCrossings: true,
            isChainningConnection: false,
            isContainer: false,
            isDummy: false,
        });
        self.nodes.len() - 1
    }

    #[inline]
    pub(crate) fn n(&self, pn: PN) -> &placementNode {
        &self.nodes[pn]
    }

    #[inline]
    pub(crate) fn gn(&self, pn: PN) -> NodeRef {
        self.nodes[pn].graphNode.clone().expect("nil pointer dereference: placementNode.graphNode")
    }

    pub(crate) fn children(&self, pn: PN) -> Vec<PN> {
        self.slices[self.nodes[pn].children].clone()
    }

    pub(crate) fn degree(&self, pn: PN) -> i64 {
        if self.nodes[pn].isDummy {
            return 2;
        }
        countHierarchyStructuralEdges(&self.gn(pn))
    }

    pub(crate) fn findNonDummyNode(&self, pn: PN, above: bool) -> Option<PN> {
        let mut n = Some(pn);
        while let Some(c) = n {
            let node = &self.nodes[c];
            if !(node.isDummy && node.isChainningConnection) {
                break;
            }
            let set = if above { &node.aboves } else { &node.belows };
            n = set.first().copied();
        }
        n
    }

    /// Go `below.connect(above)`.
    pub(crate) fn connect(&mut self, below: PN, above: PN) {
        let (mut below, mut above) = (below, above);
        if self.nodes[above].level == self.nodes[below].level {
            return;
        }
        if self.nodes[above].level > self.nodes[below].level {
            std::mem::swap(&mut below, &mut above);
        }
        setInsert(&mut self.nodes[below].aboves, above);
        setInsert(&mut self.nodes[above].belows, below);
    }

    pub(crate) fn levelSlice(&self, byLevel: &byLevelMap, level: i64) -> Vec<PN> {
        match byLevel.get(&level) {
            Some(&sl) => self.slices[sl].clone(),
            None => Vec::new(),
        }
    }

    pub(crate) fn DebugID(&self, pn: PN) -> String {
        let n = &self.nodes[pn];
        format!("{}[{}, {}]", layoutgraph::DebugIDOf(n.graphNode.as_ref()), n.level, n.rank)
    }
}

pub fn Place(ctx: &Ctx, g: &GraphRef, root: Option<&NodeRef>, randGenerator: Option<&RandRef>) -> GoResult<()> {
    layoutgraph::Validate(ctx, "PlaceHierarchies", g)?;
    if g.b().Containers.at(&root.cloned()).is_empty() {
        return Ok(());
    }
    let (ctx, guard) = layoutgraph::EnsureTransactionWorkGuard(ctx, "PlaceHierarchiesTransactions")?;
    let state = layoutgraph::NewGraphStateSnapshot(layoutgraph::GraphStateSnapshotOptions {
        CaptureTopology: true,
        CaptureEdgeRoutes: true,
    });
    state.UpdateWithWorkGuard(g, &guard)?;
    let r = (|| -> GoResult<()> {
        place(&ctx, g, root, randGenerator)?;
        if let Some(err) = ctx.Err() {
            return Err(wrapf!(err, "PlaceHierarchies: {}", err));
        }
        Ok(())
    })();
    if r.is_err() {
        layoutgraph::RestoreGraphState(g, &state);
    }
    r
}

pub(crate) fn place(ctx: &Ctx, g: &GraphRef, root: Option<&NodeRef>, randGenerator: Option<&RandRef>) -> GoResult<()> {
    let hGraph = layoutgraph::NewGraph();
    hGraph.CopyEntitiesFrom(g);
    let rootChildren = g.b().Containers.at(&root.cloned());
    for child in &rootChildren {
        hGraph.AddNodeUnchecked(child);
    }

    let mut edgeAbductions = g.AbductEdges(root, &hGraph);
    let mut edgesRestored = false;
    let mut splitOwnership = layoutgraph::NodeGraphOwnershipJournal::default();

    let r = (|| -> GoResult<()> {
        for child in &rootChildren {
            if child.b().Hierarchy.is_none() && child.IsContainer() {
                place(ctx, g, Some(child), randGenerator)?;
            }
        }

        let (subgraphs, journal) =
            hGraph.SplitSubgraphsTracked(ctx, layoutgraph::SplitOptions { IncludeNears: true, ..Default::default() }, None);
        splitOwnership = journal;
        let subgraphs = subgraphs?;
        for subgraph in &subgraphs {
            if subgraph.b().Nodes[0].b().Hierarchy.is_none() {
                continue;
            }
            subgraph.ComputeCellSize();
            let gnodes = g.b().Nodes.clone();
            gnodes.SetGraphReference(subgraph);
            edgeAbductions = subgraph.RestoreEdgeAbductions(&edgeAbductions);
            placeNodesInHierarchy(ctx, subgraph, randGenerator)?;
        }

        g.RestoreEdgeAbductions(&edgeAbductions);
        edgesRestored = true;
        Ok(())
    })();

    if !edgesRestored {
        g.RestoreEdgeAbductions(&edgeAbductions);
    }
    splitOwnership.Restore();
    let gnodes = g.b().Nodes.clone();
    gnodes.SetGraphReference(g);
    r
}

pub(crate) fn placeNodesInHierarchy(ctx: &Ctx, g: &GraphRef, rand: Option<&RandRef>) -> GoResult<()> {
    let first = g.b().Nodes[0].clone();
    if let Some(hierarchy) = first.b().Hierarchy.clone() {
        let mut members = hierarchy.Levels().keys();
        layoutgraph::SortNodesByID(&mut members);
        for node in &members {
            if let Some(err) = ctx.Err() {
                return Err(wrapf!(err, "PlaceHierarchies: {}", err));
            }
            if node.b().FixedTopLeft.is_some() {
                return Ok(());
            }
        }
    }
    let (ctx, guard) = layoutgraph::EnsureTransactionWorkGuard(ctx, "PlaceHierarchiesTransactions")?;
    let state = layoutgraph::NewGraphStateSnapshot(layoutgraph::GraphStateSnapshotOptions {
        CaptureEdgeRoutes: true,
        ..Default::default()
    });
    state.UpdateWithWorkGuard(g, &guard)?;
    let r = (|| -> GoResult<()> {
        let mut a = placementArena::default();
        let gnodes = g.b().Nodes.clone();
        let placementNodes = createPlacementNodes(&mut a, g, &gnodes, rand);
        connectPlacementNodes(&mut a, g, &placementNodes);
        let mut byLevel = groupPlacementNodesByLevel(&mut a, &placementNodes);
        initializeRanks(&mut a, &byLevel);
        breakLongConnections(&mut a, &placementNodes, &mut byLevel);
        minimizeHierarchyCrossings(&mut a, &byLevel);
        globalSifting(&mut a, &byLevel)?;
        let isHorizontal = IsHorizontal(&gnodes);
        if isHorizontal {
            transpose(g);
        }
        placeNodesByLevel(&mut a, g, &byLevel, isHorizontal);
        for level in 0..byLevel.len() as i64 {
            let cur = a.levelSlice(&byLevel, level);
            let newNodes = removeTableColumnNodes(&mut a, &cur);
            let sl = a.newSlice(newNodes);
            byLevel.insert(level, sl);
            let nodes = a.slices[sl].clone();
            computeLevelRanks(&mut a, &nodes);
        }
        alignHierarchy(&ctx, &mut a, g, &byLevel)?;
        if isHorizontal {
            transpose(g);
        }
        let d = g.b().Nodes[0].ContainerDirection();
        if d == geo::Left {
            mirrorX(g);
        } else if d == geo::Top {
            mirrorY(g);
        }
        Ok(())
    })();
    if r.is_err() {
        layoutgraph::RestoreGraphState(g, &state);
    }
    r
}

/// create placement nodes recursively and set the container whenever required
pub(crate) fn createPlacementNodes(a: &mut placementArena, g: &GraphRef, nodes: &[NodeRef], rand: Option<&RandRef>) -> Vec<PN> {
    let mut placementNodes: Vec<PN> = Vec::new();
    for node in nodes {
        let pn = a.newPlacementNode(node.HierarchyLevel(), Some(node.clone()));
        placementNodes.push(pn);
        if node.IsTable() {
            let numColumns = node.NumColumns();
            let level = a.nodes[pn].level;
            let mut children = Vec::with_capacity(numColumns.max(0) as usize);
            for _ in 0..numColumns {
                let c = a.newPlacementNode(level, None);
                a.nodes[c].container = Some(pn);
                a.nodes[c].isDummy = true;
                children.push(c);
            }
            let sl = a.newSlice(children);
            a.nodes[pn].children = sl;
            a.nodes[pn].optimizeChildrenCrossings = false;
            a.nodes[pn].isContainer = false;
        } else {
            a.nodes[pn].optimizeChildrenCrossings = true;
            let containerChildren = g.b().Containers.at(&Some(node.clone()));
            let mut children = Vec::with_capacity(containerChildren.len());
            for child in createPlacementNodes(a, g, &containerChildren, rand) {
                if a.nodes[child].container.is_none() {
                    a.nodes[child].container = Some(pn);
                    children.push(child);
                }
            }
            let isContainer = !children.is_empty();
            let sl = a.newSlice(children);
            a.nodes[pn].children = sl;
            a.nodes[pn].isContainer = isContainer;
        }
    }
    if let Some(r) = rand {
        let n = placementNodes.len() as i64;
        r.bm().Shuffle(n, |i, j| placementNodes.swap(i, j));
    }
    placementNodes
}

pub(crate) fn connectPlacementNodes(a: &mut placementArena, g: &GraphRef, nodes: &[PN]) {
    let mut nodeToPlacementNode: Map<NodeRef, PN> = Map::default();
    let mut queue: std::collections::VecDeque<PN> = nodes.iter().copied().collect();
    while let Some(pn) = queue.pop_front() {
        if let Some(gn) = a.nodes[pn].graphNode.clone() {
            nodeToPlacementNode.insert(gn, pn);
        }
        queue.extend(a.children(pn));
    }

    let edges = g.b().Edges.clone();
    for edge in &edges {
        let from = *nodeToPlacementNode.get(&edge.from()).expect("nil pointer dereference: placement node");
        let to = *nodeToPlacementNode.get(&edge.to()).expect("nil pointer dereference: placement node");
        a.connect(to, from);

        if edge.IsBetweenTableColumns() {
            let (fi, ti) = {
                let e = edge.b();
                (e.FromTableColumnIndex.unwrap(), e.ToTableColumnIndex.unwrap())
            };
            let from = a.slices[a.nodes[from].children][fi as usize];
            let to = a.slices[a.nodes[to].children][ti as usize];
            a.connect(to, from);
        }
    }
}

/// breakLongConnections creates chains of dummy nodes when two nodes are
/// connected but their level distance is greater than 1.
pub(crate) fn breakLongConnections(a: &mut placementArena, nodes: &[PN], byLevel: &mut byLevelMap) -> Vec<PN> {
    let mut dummies: Vec<PN> = Vec::new();
    for &pn in nodes {
        let pnLevel = a.nodes[pn].level;
        let mut belows: Vec<PN> = a.nodes[pn].belows.iter().copied().filter(|&b| a.nodes[b].level - pnLevel > 1).collect();
        gocompat::slices::SortStableFunc(&mut belows, |&x, &y| {
            let order = gocompat::slices::Compare(a.nodes[x].rank, a.nodes[y].rank);
            if order != 0 {
                return order;
            }
            gocompat::slices::Compare(a.nodes[x].level, a.nodes[y].level)
        });
        for below in belows {
            setRemove(&mut a.nodes[pn].belows, below);
            setRemove(&mut a.nodes[below].aboves, pn);

            let mut above = pn;
            let belowLevel = a.nodes[below].level;
            let mut level = pnLevel + 1;
            while level < belowLevel {
                let dummyID = -((dummies.len() + 1) as layoutgraph::EntityID);
                let dummy = a.newPlacementNode(level, Some(layoutgraph::NewNode(dummyID, 1.0, 1.0)));
                a.nodes[dummy].isChainningConnection = true;
                a.nodes[dummy].isDummy = true;
                let sl = match byLevel.get(&level) {
                    Some(&sl) => sl,
                    None => {
                        let sl = a.newSlice(Vec::new());
                        byLevel.insert(level, sl);
                        sl
                    }
                };
                a.nodes[dummy].rank = a.slices[sl].len() as i64;
                a.connect(dummy, above);
                dummies.push(dummy);
                a.slices[sl].push(dummy);
                above = dummy;
                level += 1;
            }
            a.connect(below, above);
        }
        let children = a.children(pn);
        let more = breakLongConnections(a, &children, byLevel);
        dummies.extend(more);
    }
    dummies
}

pub(crate) fn groupPlacementNodesByLevel(a: &mut placementArena, nodes: &[PN]) -> byLevelMap {
    let mut byLevel: byLevelMap = Map::default();
    for &pn in nodes {
        let level = a.nodes[pn].level;
        let sl = match byLevel.get(&level) {
            Some(&sl) => sl,
            None => {
                let sl = a.newSlice(Vec::new());
                byLevel.insert(level, sl);
                sl
            }
        };
        a.slices[sl].push(pn);
    }
    byLevel
}

/// removeTableColumnNodes removes nodes where `graphNode == nil` and restores edge references
pub(crate) fn removeTableColumnNodes(a: &mut placementArena, nodes: &[PN]) -> Vec<PN> {
    let mut newNodes = Vec::with_capacity(nodes.len());
    for &node in nodes {
        if a.nodes[node].isDummy {
            if a.nodes[node].graphNode.is_none() {
                continue;
            } else if a.nodes[node].isChainningConnection {
                let above = a.findNonDummyNode(node, true).expect("nil pointer dereference: above");
                let below = a.findNonDummyNode(node, false).expect("nil pointer dereference: below");
                if a.nodes[above].graphNode.is_none() || a.nodes[below].graphNode.is_none() {
                    continue;
                }
            }
        }
        newNodes.push(node);
        let children = a.children(node);
        let newChildren = removeTableColumnNodes(a, &children);
        let sl = a.newSlice(newChildren);
        a.nodes[node].children = sl;
    }
    newNodes
}

pub(crate) fn placeNodesByLevel(a: &mut placementArena, g: &GraphRef, byLevel: &byLevelMap, isHorizontal: bool) {
    let mut yOffset = 0.0;
    for i in 0..byLevel.len() as i64 {
        let mut nodes: Vec<NodeRef> = Vec::new();
        let mut levelHeight = 0.0;
        let mut xOffset = 0.0;
        let level = a.levelSlice(byLevel, i);
        for &pn in &level {
            let gn = a.gn(pn);
            nodes.push(gn.clone());
            gn.set_tl(Some(geo::NewPoint(xOffset, yOffset)));
            if !a.nodes[pn].isDummy {
                let container = a.nodes[pn].container.and_then(|c| a.nodes[c].graphNode.clone());
                let padding = g.ContainerPadding(container.as_ref(), true);
                let tl = gn.InsidePlacement(1.0, 1.0, padding);
                placeDescendants(a, g, pn, tl);
                levelHeight = math::Max(levelHeight, gn.h());
                xOffset += math::Ceil(gn.w()) + siblingSpacing;
            } else {
                xOffset += siblingSpacing;
            }
        }
        let distanceToNextLevel = minimumDistanceToNextLevel(a, &level, isHorizontal);
        yOffset += math::Ceil(levelHeight + distanceToNextLevel + 2.0 * layoutgraph::MinPortClearance);

        // move to center
        let center = nodes.Center();
        let cy = math::Ceil(center.b().Y);
        center.bm().Y = cy;
        for node in &nodes {
            let yDiff = math::Ceil(cy - node.Center().b().Y);
            node.Translate(0.0, yDiff);
        }
    }
}

/// recursively place all descendants of a given container
pub(crate) fn placeDescendants(a: &placementArena, g: &GraphRef, root: PN, tl: geo::Point) {
    if !a.nodes[root].isContainer {
        return;
    }
    let rootNode = a.gn(root);
    let padding = g.ContainerPadding(Some(&rootNode), true);
    let mut x = tl.X;
    for child in a.children(root) {
        let cg = a.gn(child);
        cg.set_tl(Some(geo::NewPoint(x, tl.Y)));
        let ctl = cg.InsidePlacement(1.0, 1.0, padding);
        placeDescendants(a, g, child, ctl);
        x += math::Ceil(cg.w()) + siblingSpacing;
    }
    let containerChildren = g.b().Containers.at(&Some(rootNode.clone()));
    let (childrenTL, childrenBR) = containerChildren.BoundingBox();
    rootNode.FitToBoundingBox(&childrenTL.unwrap(), &childrenBR.unwrap(), padding);
}

/// minimumDistanceToNextLevel computes the distance between two levels.
pub(crate) fn minimumDistanceToNextLevel(a: &placementArena, levelNodes: &[PN], isHorizontal: bool) -> f64 {
    let mut scratch = crossingScratch::default();
    let segments = scratch.crossLevelSegments(a, levelNodes, false, true);
    let crossings = countCrossings(segments);
    let crossingsDistance = crossings as f64 * crossingSpacing;

    let mut maxLabelSize = 0.0;
    for &node in levelNodes {
        if a.nodes[node].isDummy {
            continue;
        }
        let gn = a.gn(node);
        for e in gn.edges() {
            let adj = gn.Adjacent(&e);
            let mut from = gn.clone();
            let mut to = adj;
            if to.HierarchyLevel() < from.HierarchyLevel() {
                std::mem::swap(&mut from, &mut to);
            }
            if to.HierarchyLevel() - from.HierarchyLevel() != 1 {
                continue;
            }
            if let Some(l) = e.b().Label.clone() {
                let l = l.b();
                if isHorizontal {
                    maxLabelSize = math::Max(l.Width, maxLabelSize);
                } else {
                    maxLabelSize = math::Max(l.Height, maxLabelSize);
                }
            }
        }
    }
    let mut distance = math::Max(crossingsDistance, maxLabelSize + crossingSpacing);
    distance = math::Max(crossingSpacing, distance);
    distance = math::Min(distance, parentSpacing);
    math::Round(distance)
}

pub(crate) fn isSource(n: &NodeRef) -> bool {
    for e in n.edges() {
        if !isHierarchyStructuralEdge(&e) {
            continue;
        }
        if e.HasSourceArrow() == e.HasTargetArrow() || (e.HasSourceArrow() && e.from() == *n) || (e.HasTargetArrow() && e.to() == *n) {
            return false;
        }
    }
    true
}

pub(crate) fn isSink(n: &NodeRef) -> bool {
    for e in n.edges() {
        if !isHierarchyStructuralEdge(&e) {
            continue;
        }
        if e.HasSourceArrow() == e.HasTargetArrow() || (e.HasTargetArrow() && e.from() == *n) || (e.HasSourceArrow() && e.to() == *n) {
            return false;
        }
    }
    true
}

pub(crate) fn iterContainersBFS(g: &GraphRef, apply: &mut dyn FnMut(&NodeRef)) {
    let mut queue: std::collections::VecDeque<NodeRef> = g.b().Nodes.iter().cloned().collect();
    while let Some(curr) = queue.pop_front() {
        if curr.IsContainer() {
            queue.extend(g.b().Containers.at(&Some(curr.clone())));
        }
        apply(&curr);
    }
}

pub(crate) fn transpose(g: &GraphRef) {
    iterContainersBFS(g, &mut |n| n.Transpose());
}

pub(crate) fn mirrorX(g: &GraphRef) {
    iterContainersBFS(g, &mut |n| n.Mirror(true, false));
}

pub(crate) fn mirrorY(g: &GraphRef) {
    iterContainersBFS(g, &mut |n| n.Mirror(false, true));
}
