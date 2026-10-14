// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/hierarchy/sifting.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

pub(crate) fn globalSifting(a: &mut placementArena, byLevel: &byLevelMap) -> GoResult<()> {
    let queue = nodesInDescendingDegreeOrder(a, byLevel);
    let nodeToSiblings = buildNodeToSiblings(a, &queue, byLevel);
    let mut scratch = crossingScratch::default();
    let mut improveIfEqualCrossings = false;
    for _iter in 0..10 {
        let mut improved = false;
        for &node in &queue {
            let oldRank = a.nodes[node].rank;
            sifting(a, node, nodeToSiblings.get(&node).copied(), byLevel, improveIfEqualCrossings, &mut scratch)?;
            if a.nodes[node].rank != oldRank {
                improved = true;
            }
        }
        if !improved {
            if improveIfEqualCrossings {
                break;
            }
            improveIfEqualCrossings = true;
        } else {
            improveIfEqualCrossings = false;
        }
    }
    Ok(())
}

pub(crate) fn nodesInDescendingDegreeOrder(a: &placementArena, byLevel: &byLevelMap) -> Vec<PN> {
    let mut nodes: Vec<PN> = Vec::new();
    for level in 0..byLevel.len() as i64 {
        let lvl = a.levelSlice(byLevel, level);
        nodes.extend(allDescendants(a, &lvl, true));
    }
    let degrees: Map<PN, i64> = nodes.iter().map(|&n| (n, a.degree(n))).collect();
    gocompat::slices::SortStableFunc(&mut nodes, |&x, &y| -> i64 {
        let order = gocompat::slices::Compare(degrees[&y], degrees[&x]);
        if order != 0 {
            return order;
        }
        let order = gocompat::slices::Compare(a.nodes[x].level, a.nodes[y].level);
        if order != 0 {
            return order;
        }
        gocompat::slices::Compare(a.nodes[x].rank, a.nodes[y].rank)
    });
    nodes
}

/// Values are shared slice ids (GO-ALIAS: Go stores the slice headers of
/// `container.children` / `byLevel[level]`, and sifting permutes them in place).
pub(crate) fn buildNodeToSiblings(a: &placementArena, nodes: &[PN], byLevel: &byLevelMap) -> Map<PN, SL> {
    let mut nodeToSiblings: Map<PN, SL> = Map::default();
    for &node in nodes {
        if let Some(c) = a.nodes[node].container {
            nodeToSiblings.insert(node, a.nodes[c].children);
        } else if let Some(&sl) = byLevel.get(&a.nodes[node].level) {
            nodeToSiblings.insert(node, sl);
        }
    }
    nodeToSiblings
}

pub(crate) fn sifting(
    a: &mut placementArena,
    node: PN,
    siblingsSL: Option<SL>,
    byLevel: &byLevelMap,
    improveIfEqualCrossings: bool,
    scratch: &mut crossingScratch,
) -> GoResult<()> {
    let Some(sl) = siblingsSL else {
        // nil siblings: zero segments, zero crossings
        return Ok(());
    };
    let nodeLevel = a.nodes[node].level;
    let siblings = a.slices[sl].clone();
    let segments = scratch.crossLevelSegments(a, &siblings, true, true);
    let mut bestCrossings = countCrossings(segments);
    let mut bestLength = addLengths(segments);
    let mut bestI: i64 = -1;
    let n = a.slices[sl].len();

    if bestCrossings == 0 || n == 1 {
        return Ok(());
    } else if node == a.slices[sl][n - 1] {
        bestI = n as i64 - 1;
    } else {
        for i in 0..n - 1 {
            if bestI == -1 {
                if a.slices[sl][i] == node {
                    bestI = i as i64;
                } else {
                    continue;
                }
            }

            a.slices[sl].swap(i + 1, i);
            let lvl = a.levelSlice(byLevel, nodeLevel);
            computeLevelRanks(a, &lvl);
            let siblings = a.slices[sl].clone();
            let segments = scratch.crossLevelSegments(a, &siblings, true, true);
            let crossings = countCrossings(segments);
            let length = addLengths(segments);
            if improved(crossings, bestCrossings, length, bestLength, improveIfEqualCrossings) {
                bestCrossings = crossings;
                bestI = i as i64 + 1;
                bestLength = length;
            }
        }
        if a.slices[sl][n - 1] != node {
            return Err(errorf!(
                "sifting: expected node {} to have moved to the right of siblings slice",
                layoutgraph::DebugIDOf(a.nodes[node].graphNode.as_ref())
            ));
        }
        if bestI == -1 {
            return Err(errorf!("sifting: node {} not found", layoutgraph::DebugIDOf(a.nodes[node].graphNode.as_ref())));
        }
    }

    let mut i = n - 1;
    while i > 0 {
        a.slices[sl].swap(i - 1, i);
        let lvl = a.levelSlice(byLevel, nodeLevel);
        computeLevelRanks(a, &lvl);
        let siblings = a.slices[sl].clone();
        let segments = scratch.crossLevelSegments(a, &siblings, true, true);
        let crossings = countCrossings(segments);
        let length = addLengths(segments);
        if improved(crossings, bestCrossings, length, bestLength, improveIfEqualCrossings) {
            bestCrossings = crossings;
            bestI = i as i64 - 1;
            bestLength = length;
        }
        i -= 1;
    }
    if a.slices[sl][0] != node {
        return Err(errorf!(
            "sifting: expected node {} to have moved to the left of siblings slice",
            layoutgraph::DebugIDOf(a.nodes[node].graphNode.as_ref())
        ));
    }

    for i in 0..n {
        if i as i64 == bestI {
            break;
        }
        a.slices[sl].swap(i + 1, i);
    }
    let lvl = a.levelSlice(byLevel, nodeLevel);
    computeLevelRanks(a, &lvl);
    Ok(())
}

pub(crate) fn improved(crossings: i64, bestCrossings: i64, length: f64, bestLength: f64, improveIfEqualCrossings: bool) -> bool {
    if !improveIfEqualCrossings {
        return crossings < bestCrossings || (crossings == bestCrossings && geo::PrecisionCompare(length, bestLength, geo::PRECISION) == -1);
    }
    crossings <= bestCrossings || (crossings == bestCrossings && geo::PrecisionCompare(length, bestLength, geo::PRECISION) < 1)
}
