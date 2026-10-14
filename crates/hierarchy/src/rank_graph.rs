// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/hierarchy/rank_graph.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

pub(crate) fn newRankProblem(g: Option<&GraphRef>, guard: &OptimizationWorkGuardRef) -> GoResult<rankProblem> {
    let Some(g) = g else {
        return Err(errorf!("graph is required"));
    };
    let (gnodes, gedges) = {
        let gb = g.b();
        (gb.Nodes.clone(), gb.Edges.clone())
    };
    if gnodes.len() as i64 > limits::MaxEngineNodes {
        return Err(errorf!("node count exceeds limit {}", limits::MaxEngineNodes));
    }
    if gedges.len() as i64 > limits::MaxEngineEdges {
        return Err(errorf!("edge count exceeds limit {}", limits::MaxEngineEdges));
    }

    let mut nodes = gnodes.clone();
    guard.AddSort(nodes.len() as i64)?;
    gocompat::slices::SortFunc(&mut nodes, |a, b| gocompat::slices::Compare(a.id(), b.id()));

    let mut index: Map<NodeRef, usize> = Map::default();
    for (i, node) in nodes.iter().enumerate() {
        guard.Step()?;
        if i > 0 && nodes[i - 1].id() == node.id() {
            return Err(errorf!("duplicate node ID {}", node.id()));
        }
        if index.contains_key(node) {
            return Err(errorf!("duplicate node {}", node.DebugID()));
        }
        index.insert(node.clone(), i);
    }

    struct indexedEdge {
        edge: EdgeRef,
        from: usize,
        to: usize,
    }
    let mut edges: Vec<indexedEdge> = Vec::with_capacity(gedges.len());
    let mut seenEdges: Set<EdgeRef> = Set::default();
    let mut seenEdgeIDs: Set<layoutgraph::EntityID> = Set::default();
    let mut seenEndpoints: Set<(usize, usize)> = Set::default();
    for edge in &gedges {
        guard.Step()?;
        let (from, to) = (index.get(&edge.from()).copied(), index.get(&edge.to()).copied());
        let (Some(from), Some(to)) = (from, to) else {
            return Err(errorf!("edge {} references a node outside the graph", edge.id()));
        };
        if from == to {
            return Err(errorf!("self edge on {}", edge.from().DebugID()));
        }
        if edge.HasSourceArrow() || !edge.HasTargetArrow() {
            return Err(errorf!("canonical source-to-target directed edges required"));
        }
        let weight = edge.HierarchyRankWeight();
        if weight <= 0 || weight > limits::MaxEngineEdges {
            return Err(errorf!("edge {} has invalid rank weight {}", edge.id(), weight));
        }
        if seenEdges.contains(edge) {
            return Err(errorf!("duplicate edge reference {}", edge.id()));
        }
        if seenEdgeIDs.contains(&edge.id()) {
            return Err(errorf!("duplicate edge ID {}", edge.id()));
        }
        if seenEndpoints.contains(&(from, to)) {
            return Err(errorf!("duplicate directed edge {} -> {}", edge.from().DebugID(), edge.to().DebugID()));
        }
        seenEdges.insert(edge.clone());
        seenEdgeIDs.insert(edge.id());
        seenEndpoints.insert((from, to));
        edges.push(indexedEdge { edge: edge.clone(), from, to });
    }

    guard.AddSort(edges.len() as i64)?;
    gocompat::slices::SortFunc(&mut edges, |a, b| {
        let order = gocompat::slices::Compare(a.from as i64, b.from as i64);
        if order != 0 {
            return order;
        }
        let order = gocompat::slices::Compare(a.to as i64, b.to as i64);
        if order != 0 {
            return order;
        }
        gocompat::slices::Compare(a.edge.id(), b.edge.id())
    });

    let mut incidentCounts: Map<EdgeRef, (i64, i64)> = Map::default();
    let mut incidentReferences: i64 = 0;
    for node in &nodes {
        for edge in node.edges() {
            guard.Step()?;
            incidentReferences += 1;
            if incidentReferences > limits::MaxEngineEdges * 2 {
                return Err(errorf!("incident edge references exceed limit {}", limits::MaxEngineEdges * 2));
            }
            if edge.from() != *node && edge.to() != *node {
                return Err(errorf!("malformed incident edge on {}", node.DebugID()));
            }
            if !seenEdges.contains(&edge) {
                return Err(errorf!("node {} references an edge outside the graph", node.DebugID()));
            }
            let counts = incidentCounts.entry(edge.clone()).or_insert((0, 0));
            if edge.from() == *node {
                counts.0 += 1;
            } else {
                counts.1 += 1;
            }
        }
    }
    for edge in &edges {
        guard.Step()?;
        let counts = incidentCounts.get(&edge.edge).copied().unwrap_or((0, 0));
        if counts != (1, 1) {
            return Err(errorf!(
                "edge {} must appear once on each endpoint; got source={} target={}",
                edge.edge.id(),
                counts.0,
                counts.1
            ));
        }
    }

    if nodes.len() > 1 {
        let mut seen = vec![false; nodes.len()];
        seen[0] = true;
        let mut queue = vec![0usize];
        let mut head = 0;
        while head < queue.len() {
            let u = queue[head];
            for edge in nodes[u].edges() {
                guard.Step()?;
                let v = index[&nodes[u].Adjacent(&edge)];
                if !seen[v] {
                    seen[v] = true;
                    queue.push(v);
                }
            }
            head += 1;
        }
        if queue.len() != nodes.len() {
            return Err(errorf!("input graph is disconnected"));
        }
    }

    let mut problemEdges: Vec<rankEdge> = Vec::with_capacity(edges.len());
    let mut outgoingStart = vec![0usize; nodes.len() + 1];
    for edge in &edges {
        guard.Step()?;
        problemEdges.push(rankEdge { from: edge.from, to: edge.to, weight: edge.edge.HierarchyRankWeight(), id: edge.edge.id() });
        outgoingStart[edge.from + 1] += 1;
    }
    for node in 0..nodes.len() {
        guard.Step()?;
        outgoingStart[node + 1] += outgoingStart[node];
    }
    let mut pivotOrder: Vec<usize> = Vec::with_capacity(problemEdges.len());
    for edgeIndex in 0..problemEdges.len() {
        guard.Step()?;
        pivotOrder.push(edgeIndex);
    }
    guard.AddSort(pivotOrder.len() as i64)?;
    gocompat::slices::SortFunc(&mut pivotOrder, |&a, &b| gocompat::slices::Compare(problemEdges[a].id, problemEdges[b].id));
    Ok(rankProblem { nodes, edges: problemEdges, outgoingStart, pivotOrder })
}

impl rankProblem {
    pub(crate) fn longestPathLevels(&self, guard: &OptimizationWorkGuardRef) -> GoResult<Vec<i64>> {
        let mut indegree = vec![0i64; self.nodes.len()];
        for edge in &self.edges {
            guard.Step()?;
            indegree[edge.to] += 1;
        }

        let mut ready = nodeMinHeap(Vec::with_capacity(self.nodes.len()));
        for (node, &degree) in indegree.iter().enumerate() {
            guard.Step()?;
            if degree == 0 {
                ready.push(node);
                guard.Step()?;
            }
        }

        let mut levels = vec![0i64; self.nodes.len()];
        let mut visited = 0;
        while ready.len() > 0 {
            guard.Step()?;
            let u = ready.pop();
            visited += 1;
            for edgeIndex in self.outgoingStart[u]..self.outgoingStart[u + 1] {
                guard.Step()?;
                let edge = self.edges[edgeIndex];
                let (candidate, ok) = checkedAddInt64(levels[u], rankMinSpan);
                if !ok {
                    return Err(errorf!("initial level overflow"));
                }
                levels[edge.to] = std::cmp::max(levels[edge.to], candidate);
                indegree[edge.to] -= 1;
                if indegree[edge.to] == 0 {
                    ready.push(edge.to);
                    guard.Step()?;
                }
            }
        }
        if visited != self.nodes.len() {
            return Err(errorf!("input graph contains a directed cycle"));
        }
        Ok(levels)
    }
}
