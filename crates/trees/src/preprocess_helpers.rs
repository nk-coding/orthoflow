// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/trees/preprocess_helpers.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

pub(crate) const treePreprocessLocation: &str = "PreprocessTrees";

pub(crate) fn treePreprocessBadState(msg: impl AsRef<str>) -> GoError {
    invariant::Errorf(msg)
}

/// treeContainerRDFSOrder is the bounded, iterative equivalent of
/// containerRDFSOrder.
pub(crate) fn treeContainerRDFSOrder(g: &GraphRef, root: Option<&NodeRef>, guard: &WorkGuardRef) -> GoResult<Vec<NodeRef>> {
    if let Some(r) = root {
        if !r.IsContainer() {
            return Ok(Vec::new());
        }
    }
    let containers = g.containers();
    let clusters = g.clusters();
    // (node, emit)
    let mut stack: Vec<(NodeRef, bool)> = Vec::new();
    for child in containers.at(&root.cloned()) {
        guard.Step()?;
        stack.push((child, false));
    }
    let mut order: Vec<NodeRef> = Vec::new();
    while let Some((node, emit)) = stack.pop() {
        guard.Step()?;
        if emit {
            order.push(node);
            continue;
        }
        if node.IsContainer() {
            stack.push((node.clone(), true));
            for child in containers.at(&Some(node.clone())) {
                guard.Step()?;
                stack.push((child, false));
            }
            continue;
        }
        if node.IsClusterVessel() {
            let Some(cluster) = clusters.get(&node) else {
                return Err(treePreprocessBadState(format!("cluster vessel {} has no cluster record", node.id())));
            };
            let nodes = cluster.b().Nodes.clone();
            for clusterNode in nodes {
                guard.Step()?;
                if clusterNode.IsContainer() {
                    stack.push((clusterNode, false));
                }
            }
        }
    }
    Ok(order)
}

pub(crate) struct treeAdjacency(Map<NodeRef, Map<NodeRef, EdgeRef>>);

impl treeAdjacency {
    pub(crate) fn edgeBetween(&self, a: &NodeRef, b: &NodeRef) -> Option<EdgeRef> {
        if let Some(e) = self.0.get(a).and_then(|m| m.get(b)) {
            return Some(e.clone());
        }
        self.0.get(b).and_then(|m| m.get(a)).cloned()
    }
}

pub(crate) fn buildTreeAdjacencyMatrix(g: &GraphRef, guard: &WorkGuardRef) -> GoResult<treeAdjacency> {
    let mut m: Map<NodeRef, Map<NodeRef, EdgeRef>> = Map::default();
    for edge in g.edges() {
        guard.Step()?;
        m.entry(edge.from()).or_default().insert(edge.to(), edge.clone());
    }
    Ok(treeAdjacency(m))
}

pub(crate) fn treeFringeNodes(nodes: &[NodeRef], guard: &WorkGuardRef) -> GoResult<Vec<NodeRef>> {
    let mut inSet: Set<NodeRef> = gocompat::NewSetCap(nodes.len());
    for node in nodes {
        guard.Step()?;
        inSet.insert(node.clone());
    }
    let mut fringeNodes: Vec<NodeRef> = Vec::new();
    for node in nodes {
        guard.Step()?;
        let edges = node.edges();
        if edges.len() != 1 {
            continue;
        }
        let edge = &edges[0];
        if edge.from() != *node && edge.to() != *node {
            return Err(treePreprocessBadState(format!("node {} contains a non-incident fringe edge", node.id())));
        }
        let adjacent = node.Adjacent(edge);
        if inSet.contains(&adjacent) {
            fringeNodes.push(node.clone());
        }
    }
    Ok(fringeNodes)
}

pub(crate) fn filterTerminalTrees(trees: &[TreeRef], terminals: &Map<NodeRef, bool>, guard: &WorkGuardRef) -> GoResult<Vec<TreeRef>> {
    let mut filtered: Vec<TreeRef> = Vec::new();
    for tree in trees {
        guard.Step()?;
        if !terminals.contains_key(&tree.b().Node) {
            filtered.push(tree.clone());
        }
    }
    Ok(filtered)
}

/// dominantTreeEdgeDirection preserves the recursive helper's preorder search.
pub(crate) fn dominantTreeEdgeDirection(root: &TreeRef, includeRoot: bool, guard: &WorkGuardRef) -> GoResult<TreeEdgeDirection> {
    let (rootNode, rootEdge, rootChildren) = {
        let r = root.b();
        (r.Node.clone(), r.SentinelEdge.clone(), r.Children.clone())
    };
    let Some(rootEdge) = rootEdge else {
        return Err(treePreprocessBadState(format!("tree node {} has no sentinel edge", rootNode.id())));
    };
    let mut stack: Vec<TreeRef> = Vec::new();
    if includeRoot {
        stack.push(root.clone());
    } else {
        for v in rootChildren.iter().rev() {
            guard.Step()?;
            stack.push(v.clone());
        }
    }
    let mut seen: Set<TreeRef> = Set::default();
    while let Some(current) = stack.pop() {
        guard.Step()?;
        let (node, edge, children) = {
            let c = current.b();
            (c.Node.clone(), c.SentinelEdge.clone(), c.Children.clone())
        };
        let Some(edge) = edge else {
            return Err(treePreprocessBadState("tree preprocessing encountered an incomplete tree"));
        };
        if seen.contains(&current) {
            return Err(treePreprocessBadState("tree preprocessing encountered a repeated tree node"));
        }
        seen.insert(current.clone());
        let direction = treeEdgeDirection(&node, &edge);
        if direction != Undirected {
            return Ok(direction);
        }
        for v in children.iter().rev() {
            guard.Step()?;
            stack.push(v.clone());
        }
    }
    Ok(treeEdgeDirection(&rootNode, &rootEdge))
}

pub(crate) fn countTreeDirections(trees: &[TreeRef], guard: &WorkGuardRef) -> GoResult<Map<TreeEdgeDirection, i64>> {
    let mut counts: Map<TreeEdgeDirection, i64> = Map::default();
    for tree in trees {
        guard.Step()?;
        let direction = dominantTreeEdgeDirection(tree, true, guard)?;
        *counts.entry(direction).or_insert(0) += 1;
    }
    Ok(counts)
}

pub(crate) fn candidateTreeRoots(roots: &[TreeRef], guard: &WorkGuardRef) -> GoResult<Vec<TreeRef>> {
    let byDir = rootsByTreeDirection(roots, guard)?;
    let get = |d: TreeEdgeDirection| byDir.get(&d).cloned().unwrap_or_default();
    let nOutwards = get(Outwards).len();
    let nInwards = get(Inwards).len();
    let nBidirectional = get(Bidirectional).len();
    let nUndirected = get(Undirected).len();
    let mut candidates: Vec<TreeRef> = Vec::with_capacity(roots.len());
    let mut appendRoots = |values: Vec<TreeRef>, candidates: &mut Vec<TreeRef>| -> GoResult<()> {
        for v in values {
            guard.Step()?;
            candidates.push(v);
        }
        Ok(())
    };
    let n = roots.len();
    if nOutwards == 1 && 1 + nInwards + nUndirected == n {
        appendRoots(get(Outwards), &mut candidates)?;
    }
    if nInwards == 1 && 1 + nOutwards + nUndirected == n {
        appendRoots(get(Inwards), &mut candidates)?;
    }
    if nUndirected > 0 && nOutwards > 1 && nUndirected + nOutwards == n {
        appendRoots(get(Undirected), &mut candidates)?;
    }
    if nUndirected > 0 && nInwards > 1 && nUndirected + nInwards == n {
        appendRoots(get(Undirected), &mut candidates)?;
    }
    if nBidirectional + nUndirected == n {
        appendRoots(get(Bidirectional), &mut candidates)?;
        appendRoots(get(Undirected), &mut candidates)?;
    }
    guard.Check()?;
    Ok(candidates)
}

pub(crate) fn treeEndOfLine(root: &TreeRef, guard: &WorkGuardRef) -> GoResult<Option<TreeRef>> {
    let mut seen: Set<TreeRef> = Set::default();
    let mut current = root.clone();
    loop {
        guard.Step()?;
        if seen.contains(&current) {
            return Err(treePreprocessBadState("tree preprocessing encountered a tree cycle"));
        }
        seen.insert(current.clone());
        let children = current.b().Children.clone();
        match children.len() {
            0 => return Ok(Some(current)),
            1 => current = children[0].clone(),
            _ => return Ok(None),
        }
    }
}

pub(crate) fn removeTreeChild(parent: &TreeRef, child: &TreeRef, guard: &WorkGuardRef) -> GoResult<()> {
    let children = parent.b().Children.clone();
    let mut filtered: Vec<TreeRef> = Vec::with_capacity(children.len());
    for current in children {
        guard.Step()?;
        if current != *child {
            filtered.push(current);
        }
    }
    parent.bm().Children = filtered;
    Ok(())
}

pub(crate) fn addTreeChild(parent: &TreeRef, child: &TreeRef, guard: &WorkGuardRef) -> GoResult<()> {
    let oldParent = child.b().Parent.clone();
    if let Some(p) = oldParent {
        removeTreeChild(&p, child, guard)?;
    }
    guard.Step()?;
    child.bm().Parent = Some(parent.clone());
    parent.bm().Children.push(child.clone());
    Ok(())
}

pub(crate) fn reverseTreeChain(root: &TreeRef, guard: &WorkGuardRef) -> GoResult<()> {
    let mut chain: Vec<TreeRef> = vec![root.clone()];
    let mut seen: Set<TreeRef> = Set::default();
    let mut current = root.clone();
    loop {
        let children = current.b().Children.clone();
        if children.is_empty() {
            break;
        }
        guard.Step()?;
        if children.len() != 1 {
            return Err(treePreprocessBadState("tree preprocessing attempted to reverse a branching tree"));
        }
        if seen.contains(&current) {
            return Err(treePreprocessBadState("tree preprocessing encountered a tree cycle"));
        }
        seen.insert(current.clone());
        let next = children[0].clone();
        let nextEdge = next.b().SentinelEdge.clone();
        current.bm().SentinelEdge = nextEdge;
        next.bm().SentinelEdge = None;
        removeTreeChild(&current, &next, guard)?;
        chain.push(next.clone());
        current = next;
    }
    let mut i = chain.len() - 1;
    while i > 0 {
        addTreeChild(&chain[i], &chain[i - 1], guard)?;
        i -= 1;
    }
    Ok(())
}

pub(crate) fn treeBranches(root: &TreeRef, guard: &WorkGuardRef) -> GoResult<bool> {
    let mut seen: Set<TreeRef> = Set::default();
    let mut current = root.clone();
    loop {
        guard.Step()?;
        if seen.contains(&current) {
            return Err(treePreprocessBadState("tree preprocessing encountered a tree cycle"));
        }
        seen.insert(current.clone());
        let children = current.b().Children.clone();
        match children.len() {
            0 => return Ok(false),
            1 => current = children[0].clone(),
            _ => return Ok(true),
        }
    }
}

pub(crate) fn treeSize(root: &TreeRef, guard: &WorkGuardRef) -> GoResult<i64> {
    let mut stack: Vec<TreeRef> = vec![root.clone()];
    let mut seen: Set<TreeRef> = Set::default();
    let mut size = 0i64;
    while let Some(current) = stack.pop() {
        guard.Step()?;
        if seen.contains(&current) {
            return Err(treePreprocessBadState("tree preprocessing encountered a repeated tree node"));
        }
        seen.insert(current.clone());
        size += 1;
        let children = current.b().Children.clone();
        for v in children.iter().rev() {
            guard.Step()?;
            stack.push(v.clone());
        }
    }
    Ok(size)
}

pub(crate) fn treeDescendants(root: &TreeRef, guard: &WorkGuardRef) -> GoResult<Vec<TreeRef>> {
    let mut descendants: Vec<TreeRef> = Vec::new();
    let mut queue: Vec<TreeRef> = vec![root.clone()];
    let mut seen: Set<TreeRef> = Set::default();
    let mut index = 0;
    while index < queue.len() {
        guard.Step()?;
        let current = queue[index].clone();
        if seen.contains(&current) {
            return Err(treePreprocessBadState("tree preprocessing encountered a repeated tree node"));
        }
        seen.insert(current.clone());
        let children = current.b().Children.clone();
        for child in children {
            guard.Step()?;
            descendants.push(child.clone());
            queue.push(child);
        }
        index += 1;
    }
    Ok(descendants)
}

pub(crate) fn rootsByTreeDirection(treeRoots: &[TreeRef], guard: &WorkGuardRef) -> GoResult<Map<TreeEdgeDirection, Vec<TreeRef>>> {
    let mut byDir: Map<TreeEdgeDirection, Vec<TreeRef>> = Map::default();
    for root in treeRoots {
        guard.Step()?;
        let direction = dominantTreeEdgeDirection(root, false, guard)?;
        byDir.entry(direction).or_default().push(root.clone());
    }
    Ok(byDir)
}

pub(crate) fn removeNodeFromGraph(node: &NodeRef, graph: &GraphRef, guard: &WorkGuardRef) -> GoResult<()> {
    let nodes = graph.nodes();
    if nodes.is_empty() {
        return Err(treePreprocessBadState(format!("node {} is missing from the graph", node.id())));
    }
    let mut newNodes: Vec<NodeRef> = Vec::with_capacity(nodes.len() - 1);
    let mut found = false;
    for current in nodes {
        guard.Step()?;
        if current == *node {
            found = true;
            continue;
        }
        newNodes.push(current);
    }
    if !found {
        return Err(treePreprocessBadState(format!("node {} is missing from the graph", node.id())));
    }
    graph.bm().Nodes = newNodes;
    Ok(())
}

pub(crate) fn removeEdgeFromNode(node: &NodeRef, edge: &EdgeRef, guard: &WorkGuardRef) -> GoResult<()> {
    let edges = node.edges();
    for (i, current) in edges.iter().enumerate() {
        guard.Step()?;
        if current == edge {
            node.bm().Edges.remove(i);
            return Ok(());
        }
    }
    Err(treePreprocessBadState(format!("edge {} is missing from node {}", edge.id(), node.id())))
}

pub(crate) fn disconnectTreeEdge(graph: &GraphRef, edge: Option<&EdgeRef>, guard: &WorkGuardRef) -> GoResult<()> {
    let Some(edge) = edge else {
        return Err(treePreprocessBadState("tree preprocessing cannot disconnect an incomplete edge"));
    };
    let (from, to) = (edge.from(), edge.to());
    removeEdgeFromNode(&from, edge, guard)?;
    if to != from {
        removeEdgeFromNode(&to, edge, guard)?;
    } else {
        for current in to.edges() {
            guard.Step()?;
            if current == *edge {
                removeEdgeFromNode(&to, edge, guard)?;
                break;
            }
        }
    }
    let edges = graph.edges();
    let mut newEdges: Vec<EdgeRef> = Vec::with_capacity(edges.len());
    let mut found = false;
    for current in edges {
        guard.Step()?;
        if current == *edge {
            found = true;
            continue;
        }
        newEdges.push(current);
    }
    if !found {
        return Err(treePreprocessBadState(format!("edge {} is missing from the graph", edge.id())));
    }
    graph.bm().Edges = newEdges;
    Ok(())
}

pub(crate) fn removeNodeFromContainer(graph: &GraphRef, container: Option<&NodeRef>, node: &NodeRef, guard: &WorkGuardRef) -> GoResult<()> {
    let mut filtered: Vec<NodeRef> = Vec::new();
    let mut found = false;
    let containers = graph.containers();
    for current in containers.at(&container.cloned()) {
        guard.Step()?;
        if current == *node {
            found = true;
            continue;
        }
        filtered.push(current);
    }
    if !found {
        return Err(treePreprocessBadState(format!("node {} is missing from its tree container", node.id())));
    }
    containers.set(container.cloned(), filtered);
    Ok(())
}

pub(crate) fn reconnectTreeGuarded(g: &GraphRef, tree: &TreeRef, container: Option<&NodeRef>, guard: &WorkGuardRef) -> GoResult<()> {
    let mut stack: Vec<TreeRef> = vec![tree.clone()];
    let mut seen: Set<TreeRef> = Set::default();
    while let Some(current) = stack.pop() {
        guard.Step()?;
        if seen.contains(&current) {
            return Err(treePreprocessBadState("tree preprocessing encountered a repeated tree node"));
        }
        seen.insert(current.clone());
        let (node, parent, edge, children) = {
            let c = current.b();
            (c.Node.clone(), c.Parent.clone(), c.SentinelEdge.clone(), c.Children.clone())
        };
        if parent.is_some() {
            let Some(edge) = edge else {
                return Err(treePreprocessBadState(format!("tree node {} has no complete sentinel edge", node.id())));
            };
            g.AddNodeUnchecked(&node);
            g.AddNodeToContainer(container, &node);
            edge.from().AddIncidentEdgeUnchecked(&edge);
            edge.to().AddIncidentEdgeUnchecked(&edge);
            g.bm().Edges.push(edge.clone());
            guard.Step()?;
        }
        for v in children.iter().rev() {
            guard.Step()?;
            stack.push(v.clone());
        }
    }
    Ok(())
}

pub(crate) fn isIsolatedTreeGuarded(g: &GraphRef, node: &NodeRef, guard: &WorkGuardRef) -> GoResult<bool> {
    if node.IsContainer() {
        return Ok(false);
    }
    let roots = g.trees().at(&Some(node.clone()));
    for edge in node.edges() {
        guard.Step()?;
        let mut isToTree = false;
        for root in &roots {
            guard.Step()?;
            if root.b().SentinelEdge.as_ref() == Some(&edge) {
                isToTree = true;
                break;
            }
        }
        if !isToTree {
            return Ok(false);
        }
    }
    Ok(true)
}

fn sortOptNodesByID(nodes: &mut Vec<Option<NodeRef>>) {
    if nodes.len() < 2 {
        return;
    }
    let mut pairs: Vec<(i64, Option<NodeRef>)> =
        nodes.iter().map(|n| (layoutgraph::entityIDOf(n.as_ref()), n.clone())).collect();
    gocompat::sort::SortSlice(&mut pairs, |a, b| a.0 < b.0);
    for (i, (_, n)) in pairs.into_iter().enumerate() {
        nodes[i] = n;
    }
}

pub(crate) fn buildNodeToTreeGuarded(g: &GraphRef, guard: &WorkGuardRef) -> GoResult<()> {
    let mut nodeToTree: Map<NodeRef, TreeRef> = Map::default();
    let trees = g.trees();
    let mut order: Vec<Option<NodeRef>> = Vec::with_capacity(trees.len());
    for rootSentinel in trees.keys() {
        guard.Step()?;
        order.push(rootSentinel);
    }
    sortOptNodesByID(&mut order);
    guard.Check()?;
    for rootSentinel in &order {
        guard.Step()?;
        for root in trees.at(rootSentinel) {
            let mut queue: Vec<TreeRef> = vec![root.clone()];
            let mut seen: Set<TreeRef> = Set::default();
            let mut index = 0;
            while index < queue.len() {
                guard.Step()?;
                let current = queue[index].clone();
                if seen.contains(&current) {
                    return Err(treePreprocessBadState("tree preprocessing encountered a repeated tree node"));
                }
                seen.insert(current.clone());
                let (node, children) = {
                    let c = current.b();
                    (c.Node.clone(), c.Children.clone())
                };
                nodeToTree.insert(node, current.clone());
                for child in children {
                    guard.Step()?;
                    queue.push(child);
                }
                index += 1;
            }
        }
    }
    g.bm().NodeToTree = layoutgraph::GoMap::from_map(nodeToTree);
    guard.Check()
}
