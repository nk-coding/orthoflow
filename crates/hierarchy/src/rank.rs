// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/hierarchy/rank.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

/// rankResult is the complete output of hierarchical rank assignment.
#[derive(Clone, Default)]
pub(crate) struct rankResult {
    pub(crate) nodeToLevel: Map<NodeRef, i64>,
    pub(crate) levelCount: i64,
}

/// rankDAG minimizes the weighted sum of edge spans subject to every edge
/// spanning at least one level (network simplex, Gansner et al. 1993).
pub(crate) fn rankDAG(ctx: &Ctx, g: Option<&GraphRef>) -> GoResult<rankResult> {
    match rankDAGWithLimit(ctx, g, limits::MaxOptimizationWorkUnits) {
        Ok(r) => Ok(r),
        Err(err) => Err(wrapf!(err, "TALA RankDAG failed: {}", err)),
    }
}

pub(crate) fn rankDAGWithLimit(ctx: &Ctx, g: Option<&GraphRef>, workLimit: u64) -> GoResult<rankResult> {
    let guard = limits::NewOptimizationWorkGuard(ctx, "RankDAG", workLimit)?;
    let problem = newRankProblem(g, &guard)?;
    if problem.nodes.is_empty() {
        guard.Finish()?;
        return Ok(rankResult { nodeToLevel: Map::default(), levelCount: 0 });
    }

    let initialLevels = problem.longestPathLevels(&guard)?;
    if problem.edges.is_empty() {
        guard.Finish()?;
        let mut m = Map::default();
        m.insert(problem.nodes[0].clone(), 0);
        return Ok(rankResult { nodeToLevel: m, levelCount: 1 });
    }

    let (levels, dualValue) = problem.solve(&initialLevels, &guard)?;
    let (result, primalValue) = problem.makeResult(&levels, &guard)?;
    if primalValue != dualValue {
        return Err(invariant::errorf!("optimality certificate failed: primal cost {}, dual value {}", primalValue, dualValue));
    }
    guard.Finish()?;
    Ok(result)
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct rankEdge {
    pub(crate) from: usize,
    pub(crate) to: usize,
    pub(crate) weight: i64,
    pub(crate) id: layoutgraph::EntityID,
}

pub(crate) const rankMinSpan: i64 = 1;

pub(crate) struct rankProblem {
    pub(crate) nodes: Vec<NodeRef>,
    pub(crate) edges: Vec<rankEdge>,
    pub(crate) outgoingStart: Vec<usize>,
    pub(crate) pivotOrder: Vec<usize>,
}

impl rankProblem {
    pub(crate) fn makeResult(&self, levels: &[i64], guard: &OptimizationWorkGuardRef) -> GoResult<(rankResult, i64)> {
        let mut minLevel = i64::MAX;
        let mut maxLevel = i64::MIN;
        for &level in levels {
            guard.Step()?;
            minLevel = std::cmp::min(minLevel, level);
            maxLevel = std::cmp::max(maxLevel, level);
        }

        let mut primalValue: i64 = 0;
        for edge in &self.edges {
            guard.Step()?;
            let (span, ok) = checkedSubInt64(levels[edge.to], levels[edge.from]);
            if !ok {
                return Err(errorf!("edge span overflow"));
            }
            if span < rankMinSpan {
                return Err(invariant::errorf!("infeasible span {}", span));
            }
            let (weightedSpan, ok) = checkedMulInt64(edge.weight, span);
            if !ok {
                return Err(errorf!("objective overflow"));
            }
            let (pv, ok) = checkedAddInt64(primalValue, weightedSpan);
            primalValue = pv;
            if !ok {
                return Err(errorf!("objective overflow"));
            }
        }

        let (levelRange, ok) = checkedSubInt64(maxLevel, minLevel);
        if !ok || levelRange > i64::MAX - 1 {
            return Err(errorf!("level count overflow"));
        }
        let mut result = rankResult { nodeToLevel: Map::default(), levelCount: levelRange + 1 };
        for (node, &level) in levels.iter().enumerate() {
            guard.Step()?;
            let (normalized, ok) = checkedSubInt64(level, minLevel);
            if !ok {
                return Err(errorf!("normalized level overflow"));
            }
            result.nodeToLevel.insert(self.nodes[node].clone(), normalized);
        }
        Ok((result, primalValue))
    }
}

pub(crate) fn checkedSubInt64(a: i64, b: i64) -> (i64, bool) {
    if b > 0 && a < i64::MIN + b || b < 0 && a > i64::MAX + b {
        return (0, false);
    }
    (a - b, true)
}

pub(crate) fn checkedAddInt64(a: i64, b: i64) -> (i64, bool) {
    if b > 0 && a > i64::MAX - b || b < 0 && a < i64::MIN - b {
        return (0, false);
    }
    (a + b, true)
}

pub(crate) fn checkedMulInt64(a: i64, b: i64) -> (i64, bool) {
    if a == 0 || b == 0 {
        return (0, true);
    }
    if a == -1 && b == i64::MIN || b == -1 && a == i64::MIN {
        return (0, false);
    }
    let product = a.wrapping_mul(b);
    (product, product.wrapping_div(b) == a)
}
