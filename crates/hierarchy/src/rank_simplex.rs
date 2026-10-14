// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/hierarchy/rank_simplex.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

/// rankSimplex is the graphical network-simplex formulation from Gansner et al.
pub(crate) struct rankSimplex<'a> {
    pub(crate) problem: &'a rankProblem,
    pub(crate) incidentStart: Vec<usize>,
    pub(crate) incidentEdges: Vec<usize>,
    pub(crate) balance: Vec<i64>,
    pub(crate) scratch: rankSimplexScratch,
}

#[derive(Default)]
pub(crate) struct rankSimplexScratch {
    pub(crate) parent: Vec<i64>,
    pub(crate) parentEdge: Vec<i64>,
    pub(crate) order: Vec<usize>,
    pub(crate) stack: Vec<usize>,
    pub(crate) subtreeBalance: Vec<i64>,
    pub(crate) cutValue: Vec<i64>,
    pub(crate) head: Vec<bool>,
    pub(crate) queue: Vec<usize>,
    pub(crate) computedBalance: Vec<i64>,
}

impl rankProblem {
    pub(crate) fn solve(&self, initialLevels: &[i64], guard: &OptimizationWorkGuardRef) -> GoResult<(Vec<i64>, i64)> {
        let mut solver = newRankSimplex(self, guard)?;
        let mut levels = initialLevels.to_vec();
        let mut tree = solver.feasibleTree(&mut levels, guard)?;
        solver.optimize(&mut levels, &mut tree, guard)?;
        let dualValue = solver.certify(&levels, &tree, guard)?;
        Ok((levels, dualValue))
    }
}

pub(crate) fn newRankSimplex<'a>(problem: &'a rankProblem, guard: &OptimizationWorkGuardRef) -> GoResult<rankSimplex<'a>> {
    let nodeCount = problem.nodes.len();
    let mut degree = vec![0usize; nodeCount];
    let mut balance = vec![0i64; nodeCount];
    for edge in &problem.edges {
        guard.Step()?;
        degree[edge.from] += 1;
        degree[edge.to] += 1;
        let (v, ok) = checkedAddInt64(balance[edge.from], edge.weight);
        balance[edge.from] = v;
        if !ok {
            return Err(errorf!("supply overflow"));
        }
        let (v, ok) = checkedSubInt64(balance[edge.to], edge.weight);
        balance[edge.to] = v;
        if !ok {
            return Err(errorf!("demand overflow"));
        }
    }

    let mut incidentStart = vec![0usize; nodeCount + 1];
    for (node, &count) in degree.iter().enumerate() {
        guard.Step()?;
        incidentStart[node + 1] = incidentStart[node] + count;
    }
    let mut incidentEdges = vec![0usize; problem.edges.len() * 2];
    let mut next: Vec<usize> = incidentStart[..nodeCount].to_vec();
    for &edgeIndex in &problem.pivotOrder {
        guard.Step()?;
        let edge = problem.edges[edgeIndex];
        incidentEdges[next[edge.from]] = edgeIndex;
        next[edge.from] += 1;
        incidentEdges[next[edge.to]] = edgeIndex;
        next[edge.to] += 1;
    }

    let mut total: i64 = 0;
    for &amount in &balance {
        guard.Step()?;
        let (t, ok) = checkedAddInt64(total, amount);
        total = t;
        if !ok {
            return Err(errorf!("balance overflow"));
        }
    }
    if total != 0 {
        return Err(invariant::errorf!("unbalanced supplies: {}", total));
    }
    Ok(rankSimplex {
        problem,
        incidentStart,
        incidentEdges,
        balance,
        scratch: rankSimplexScratch {
            parent: vec![0; nodeCount],
            parentEdge: vec![0; nodeCount],
            order: Vec::with_capacity(nodeCount),
            stack: Vec::with_capacity(nodeCount),
            subtreeBalance: vec![0; nodeCount],
            cutValue: vec![0; problem.edges.len()],
            head: vec![false; nodeCount],
            queue: Vec::with_capacity(nodeCount),
            computedBalance: vec![0; nodeCount],
        },
    })
}

impl<'a> rankSimplex<'a> {
    /// feasibleTree implements figure 2-2 of Gansner et al.
    pub(crate) fn feasibleTree(&mut self, levels: &mut [i64], guard: &OptimizationWorkGuardRef) -> GoResult<Vec<bool>> {
        let problem = self.problem;
        let nodeCount = problem.nodes.len();
        let mut tree = vec![false; problem.edges.len()];
        let mut inTree = vec![false; nodeCount];
        inTree[0] = true;
        let mut treeNodeCount = 1usize;
        let mut queue: Vec<usize> = vec![0];

        let incidentStart = &self.incidentStart;
        let incidentEdges = &self.incidentEdges;
        let growTight = |queue: &mut Vec<usize>,
                         tree: &mut Vec<bool>,
                         inTree: &mut Vec<bool>,
                         treeNodeCount: &mut usize,
                         levels: &[i64]|
         -> GoResult<()> {
            let mut head = 0;
            while head < queue.len() {
                guard.Step()?;
                let node = queue[head];
                for i in incidentStart[node]..incidentStart[node + 1] {
                    guard.Step()?;
                    let edgeIndex = incidentEdges[i];
                    let edge = problem.edges[edgeIndex];
                    let other = if edge.from == node { edge.to } else { edge.from };
                    if inTree[other] {
                        continue;
                    }
                    let slack = rankSlack(edge, levels)?;
                    if slack != 0 {
                        continue;
                    }
                    tree[edgeIndex] = true;
                    inTree[other] = true;
                    *treeNodeCount += 1;
                    queue.push(other);
                }
                head += 1;
            }
            queue.clear();
            Ok(())
        };
        growTight(&mut queue, &mut tree, &mut inTree, &mut treeNodeCount, levels)?;

        let mut boundary: Vec<usize> = Vec::with_capacity(problem.edges.len());
        while treeNodeCount < nodeCount {
            let mut bestSlack = i64::MAX;
            boundary.clear();
            for &edgeIndex in &problem.pivotOrder {
                guard.Step()?;
                let edge = problem.edges[edgeIndex];
                if inTree[edge.from] == inTree[edge.to] {
                    continue;
                }
                let slack = rankSlack(edge, levels)?;
                if slack < bestSlack {
                    bestSlack = slack;
                    boundary.clear();
                    boundary.push(edgeIndex);
                } else if slack == bestSlack {
                    boundary.push(edgeIndex);
                }
            }
            if boundary.is_empty() {
                return Err(invariant::New("could not construct a spanning feasible tree"));
            }

            let selected = problem.edges[boundary[0]];
            let selectedTailInTree = inTree[selected.from];
            let mut delta = bestSlack;
            if !selectedTailInTree {
                let (d, ok) = checkedSubInt64(0, bestSlack);
                delta = d;
                if !ok {
                    return Err(errorf!("feasible-tree shift overflow"));
                }
            }
            for node in 0..inTree.len() {
                guard.Step()?;
                if !inTree[node] {
                    continue;
                }
                let (v, ok) = checkedAddInt64(levels[node], delta);
                levels[node] = v;
                if !ok {
                    return Err(errorf!("feasible-tree level overflow"));
                }
            }

            for &edgeIndex in &boundary {
                guard.Step()?;
                let edge = problem.edges[edgeIndex];
                if inTree[edge.from] == inTree[edge.to] {
                    continue;
                }
                let tailInTree = inTree[edge.from];
                if bestSlack != 0 && tailInTree != selectedTailInTree {
                    continue;
                }
                let slack = rankSlack(edge, levels)?;
                if slack != 0 {
                    continue;
                }
                let outside = if tailInTree { edge.to } else { edge.from };
                tree[edgeIndex] = true;
                inTree[outside] = true;
                treeNodeCount += 1;
                queue.push(outside);
            }
            if queue.is_empty() {
                return Err(invariant::New("feasible-tree shift did not add a node"));
            }
            growTight(&mut queue, &mut tree, &mut inTree, &mut treeNodeCount, levels)?;
        }
        Ok(tree)
    }

    /// optimize performs the feasible-tree exchanges in figure 2-1 (Bland's rule).
    pub(crate) fn optimize(&mut self, levels: &mut [i64], tree: &mut [bool], guard: &OptimizationWorkGuardRef) -> GoResult<()> {
        let problem = self.problem;
        loop {
            self.computeCutValues(tree, guard)?;
            let mut leaving: i64 = -1;
            for &edgeIndex in &problem.pivotOrder {
                guard.Step()?;
                if tree[edgeIndex] && self.scratch.cutValue[edgeIndex] < 0 {
                    leaving = edgeIndex as i64;
                    break;
                }
            }
            if leaving < 0 {
                return Ok(());
            }
            let leaving = leaving as usize;

            self.computeHeadComponent(tree, leaving, guard)?;
            let mut entering: i64 = -1;
            let mut minimumSlack = i64::MAX;
            for &edgeIndex in &problem.pivotOrder {
                guard.Step()?;
                let edge = problem.edges[edgeIndex];
                let head = &self.scratch.head;
                if tree[edgeIndex] || !head[edge.from] || head[edge.to] {
                    continue;
                }
                let slack = rankSlack(edge, levels)?;
                if slack < minimumSlack {
                    minimumSlack = slack;
                    entering = edgeIndex as i64;
                }
            }
            if entering < 0 {
                return Err(invariant::errorf!("negative cut on edge {} has no entering edge", problem.edges[leaving].id));
            }
            let entering = entering as usize;

            for node in 0..self.scratch.head.len() {
                guard.Step()?;
                if !self.scratch.head[node] {
                    continue;
                }
                let (v, ok) = checkedAddInt64(levels[node], minimumSlack);
                levels[node] = v;
                if !ok {
                    return Err(errorf!("simplex level overflow"));
                }
            }
            let slack = rankSlack(problem.edges[entering], levels)?;
            if slack != 0 {
                return Err(invariant::errorf!("entering edge {} has slack {} after exchange", problem.edges[entering].id, slack));
            }
            tree[leaving] = false;
            tree[entering] = true;
        }
    }

    /// computeCutValues roots the undirected feasible tree at node zero.
    pub(crate) fn computeCutValues(&mut self, tree: &[bool], guard: &OptimizationWorkGuardRef) -> GoResult<()> {
        let problem = self.problem;
        let nodeCount = problem.nodes.len();
        let mut treeEdgeCount = 0usize;
        for &present in tree {
            guard.Step()?;
            if present {
                treeEdgeCount += 1;
            }
        }
        if treeEdgeCount as i64 != nodeCount as i64 - 1 {
            return Err(invariant::errorf!("basis has {} tree edges, want {}", treeEdgeCount, nodeCount as i64 - 1));
        }

        const unvisited: i64 = -2;
        let s = &mut self.scratch;
        for node in 0..s.parent.len() {
            guard.Step()?;
            s.parent[node] = unvisited;
            s.parentEdge[node] = -1;
        }
        s.parent[0] = -1;
        s.order.clear();
        s.stack.clear();
        s.stack.push(0);
        while let Some(node) = s.stack.pop() {
            guard.Step()?;
            s.order.push(node);
            for i in self.incidentStart[node]..self.incidentStart[node + 1] {
                guard.Step()?;
                let edgeIndex = self.incidentEdges[i];
                if !tree[edgeIndex] || edgeIndex as i64 == s.parentEdge[node] {
                    continue;
                }
                let edge = problem.edges[edgeIndex];
                let other = if edge.from == node { edge.to } else { edge.from };
                if s.parent[other] != unvisited {
                    return Err(invariant::errorf!("basis contains a cycle through edge {}", edge.id));
                }
                s.parent[other] = node as i64;
                s.parentEdge[other] = edgeIndex as i64;
                s.stack.push(other);
            }
        }
        if s.order.len() != nodeCount {
            return Err(invariant::New("basis is disconnected"));
        }

        for (node, &amount) in self.balance.iter().enumerate() {
            guard.Step()?;
            s.subtreeBalance[node] = amount;
        }
        for v in s.cutValue.iter_mut() {
            *v = 0;
        }
        let mut i = s.order.len() as i64 - 1;
        while i > 0 {
            guard.Step()?;
            let node = s.order[i as usize];
            let edgeIndex = s.parentEdge[node] as usize;
            let edge = problem.edges[edgeIndex];
            let amount = s.subtreeBalance[node];
            if edge.from == node {
                s.cutValue[edgeIndex] = amount;
            } else if edge.to == node {
                let (v, ok) = checkedSubInt64(0, amount);
                s.cutValue[edgeIndex] = v;
                if !ok {
                    return Err(errorf!("cut-value overflow"));
                }
            } else {
                return Err(invariant::errorf!("malformed tree edge {}", edge.id));
            }
            let p = s.parent[node] as usize;
            let (v, ok) = checkedAddInt64(s.subtreeBalance[p], amount);
            s.subtreeBalance[p] = v;
            if !ok {
                return Err(errorf!("subtree-balance overflow"));
            }
            i -= 1;
        }
        if s.subtreeBalance[0] != 0 {
            return Err(invariant::errorf!("tree balance is {}, want zero", s.subtreeBalance[0]));
        }
        Ok(())
    }

    /// computeHeadComponent stores the head component formed by removing leaving.
    pub(crate) fn computeHeadComponent(&mut self, tree: &[bool], leaving: usize, guard: &OptimizationWorkGuardRef) -> GoResult<()> {
        let problem = self.problem;
        let s = &mut self.scratch;
        for h in s.head.iter_mut() {
            *h = false;
        }
        let start = problem.edges[leaving].to;
        s.head[start] = true;
        s.queue.clear();
        s.queue.push(start);
        let mut cursor = 0;
        while cursor < s.queue.len() {
            guard.Step()?;
            let node = s.queue[cursor];
            for i in self.incidentStart[node]..self.incidentStart[node + 1] {
                guard.Step()?;
                let edgeIndex = self.incidentEdges[i];
                if edgeIndex == leaving || !tree[edgeIndex] {
                    continue;
                }
                let edge = problem.edges[edgeIndex];
                let other = if edge.from == node { edge.to } else { edge.from };
                if !s.head[other] {
                    s.head[other] = true;
                    s.queue.push(other);
                }
            }
            cursor += 1;
        }
        if s.queue.len() == problem.nodes.len() {
            return Err(invariant::errorf!("leaving edge {} does not cut the tree", problem.edges[leaving].id));
        }
        Ok(())
    }

    /// certify constructs the ranking LP's dual flow using only the final tight spanning tree.
    pub(crate) fn certify(&mut self, levels: &[i64], tree: &[bool], guard: &OptimizationWorkGuardRef) -> GoResult<i64> {
        self.computeCutValues(tree, guard)?;
        let problem = self.problem;
        let s = &mut self.scratch;
        for v in s.computedBalance.iter_mut() {
            *v = 0;
        }
        let mut dualValue: i64 = 0;
        for (edgeIndex, edge) in problem.edges.iter().enumerate() {
            guard.Step()?;
            let mut flow: i64 = 0;
            if tree[edgeIndex] {
                flow = s.cutValue[edgeIndex];
                if flow < 0 {
                    return Err(invariant::errorf!("final tree edge {} has negative cut value {}", edge.id, flow));
                }
                let span = rankSpan(*edge, levels)?;
                if span != rankMinSpan {
                    return Err(invariant::errorf!("final tree edge {} is not tight", edge.id));
                }
            }
            let (v, mut ok) = checkedAddInt64(s.computedBalance[edge.from], flow);
            s.computedBalance[edge.from] = v;
            if ok {
                let (v, ok2) = checkedSubInt64(s.computedBalance[edge.to], flow);
                s.computedBalance[edge.to] = v;
                ok = ok2;
            }
            if !ok {
                return Err(errorf!("certificate balance overflow"));
            }
            let (contribution, mut ok) = checkedMulInt64(flow, rankMinSpan);
            if ok {
                let (d, ok2) = checkedAddInt64(dualValue, contribution);
                dualValue = d;
                ok = ok2;
            }
            if !ok {
                return Err(errorf!("dual value overflow"));
            }
        }
        for (node, &want) in self.balance.iter().enumerate() {
            guard.Step()?;
            if s.computedBalance[node] != want {
                return Err(invariant::errorf!(
                    "certificate balance mismatch on node {}: got {}, want {}",
                    problem.nodes[node].DebugID(),
                    s.computedBalance[node],
                    want
                ));
            }
        }
        Ok(dualValue)
    }
}

pub(crate) fn rankSlack(edge: rankEdge, levels: &[i64]) -> GoResult<i64> {
    let span = rankSpan(edge, levels)?;
    let (slack, ok) = checkedSubInt64(span, rankMinSpan);
    if !ok {
        return Err(errorf!("edge slack overflow"));
    }
    if slack < 0 {
        return Err(invariant::errorf!("infeasible span {}", span));
    }
    Ok(slack)
}

pub(crate) fn rankSpan(edge: rankEdge, levels: &[i64]) -> GoResult<i64> {
    let (span, ok) = checkedSubInt64(levels[edge.to], levels[edge.from]);
    if !ok {
        return Err(errorf!("edge span overflow"));
    }
    Ok(span)
}
