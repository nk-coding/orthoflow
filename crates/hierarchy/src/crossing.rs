// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/hierarchy/crossing.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;
use layoutgraph::CrossingSegment;

pub(crate) const crossingComparisonPrecision: f64 = 0.0001;

#[derive(Default)]
pub(crate) struct crossingScratch {
    pub(crate) segments: Vec<CrossingSegment>,
}

pub(crate) fn initializeRanks(a: &mut placementArena, byLevel: &byLevelMap) {
    for i in 0..byLevel.len() as i64 {
        let nodes = a.levelSlice(byLevel, i);
        computeLevelRanks(a, &nodes);
    }
}

/// minimizes crossings iteratively.
pub(crate) fn minimizeHierarchyCrossings(a: &mut placementArena, byLevel: &byLevelMap) {
    let mut widestLevel = 0usize;
    for (_, &sl) in byLevel.iter() {
        widestLevel = std::cmp::max(widestLevel, a.slices[sl].len());
    }
    let mut scratch = crossingScratch::default();
    let iterCount = widestLevel;
    for _n in 0..iterCount {
        for level in 0..byLevel.len() as i64 {
            let sl = *byLevel.get(&level).expect("nil level slice");
            minimizeCrossings(a, sl, byLevel, level > 0, &mut scratch);
        }
    }
}

/// The main routine to find the optimal node rank compared to its siblings.
pub(crate) fn minimizeCrossings(a: &mut placementArena, nodesSL: SL, byLevel: &byLevelMap, isTopDown: bool, scratch: &mut crossingScratch) {
    sortLevelNodesByAdacencyPosition(a, nodesSL, isTopDown);
    let level0 = a.nodes[a.slices[nodesSL][0]].level;
    let lvl = a.levelSlice(byLevel, level0);
    computeLevelRanks(a, &lvl);
    let nodes = a.slices[nodesSL].clone();
    let segments = scratch.crossLevelSegments(a, &nodes, true, true);
    let mut crossings = countCrossings(segments);
    let mut length = addLengths(segments);
    let n = a.slices[nodesSL].len();
    for j in 0..n {
        let (newCrossings, newLength, swapIndex) = bestIndexBySwappingNeighbors(a, nodesSL, j, byLevel, scratch);
        if newCrossings < crossings {
            crossings = newCrossings;
            length = newLength;
            a.slices[nodesSL].swap(j, swapIndex);
        } else if newCrossings == crossings && geo::PrecisionCompare(newLength, length, crossingComparisonPrecision) == -1 {
            crossings = newCrossings;
            length = newLength;
            a.slices[nodesSL].swap(j, swapIndex);
        }
        let level0 = a.nodes[a.slices[nodesSL][0]].level;
        let lvl = a.levelSlice(byLevel, level0);
        computeLevelRanks(a, &lvl);
    }
    let nodes = a.slices[nodesSL].clone();
    for node in nodes {
        let csl = a.nodes[node].children;
        if !a.slices[csl].is_empty() && a.nodes[node].optimizeChildrenCrossings {
            minimizeCrossings(a, csl, byLevel, isTopDown, scratch);
        }
    }
}

/// Swap a given node with its 3 neighbors to the right and return the best crossing and index of this swap.
pub(crate) fn bestIndexBySwappingNeighbors(
    a: &mut placementArena,
    levelSL: SL,
    nodeIndex: usize,
    byLevel: &byLevelMap,
    scratch: &mut crossingScratch,
) -> (i64, f64, usize) {
    let mut bestCrossing = i64::MAX;
    let mut bestLength = math::Inf(1);
    let mut bestIndex = 0usize;
    let len = a.slices[levelSL].len();
    for i in nodeIndex + 1..nodeIndex + 4 {
        let j = i % len;
        a.slices[levelSL].swap(nodeIndex, j);
        let lvlNum = a.nodes[a.slices[levelSL][nodeIndex]].level;
        let lvl = a.levelSlice(byLevel, lvlNum);
        computeLevelRanks(a, &lvl);
        let levelNodes = a.slices[levelSL].clone();
        let segments = scratch.crossLevelSegments(a, &levelNodes, true, true);
        let newCrossings = countCrossings(segments);
        let newLength = addLengths(segments);
        if newCrossings < bestCrossing {
            bestCrossing = newCrossings;
            bestIndex = j;
            bestLength = newLength;
        } else if newCrossings == bestCrossing && geo::PrecisionCompare(newLength, bestLength, crossingComparisonPrecision) == -1 {
            bestCrossing = newCrossings;
            bestIndex = j;
            bestLength = newLength;
        }
        a.slices[levelSL].swap(j, nodeIndex);
    }
    (bestCrossing, bestLength, bestIndex)
}

impl crossingScratch {
    pub(crate) fn crossLevelSegments(&mut self, a: &placementArena, nodes: &[PN], aboves: bool, belows: bool) -> &[CrossingSegment] {
        self.segments.clear();
        let segs = &mut self.segments;
        iterAllDescendants(a, nodes, &mut |pn| {
            let n = &a.nodes[pn];
            if aboves {
                for &connected in &n.aboves {
                    let c = &a.nodes[connected];
                    segs.push(CrossingSegment {
                        Start: geo::Point { X: n.rank as f64, Y: n.level as f64 },
                        End: geo::Point { X: c.rank as f64, Y: c.level as f64 },
                    });
                }
            }
            if belows {
                for &connected in &n.belows {
                    let c = &a.nodes[connected];
                    segs.push(CrossingSegment {
                        Start: geo::Point { X: n.rank as f64, Y: n.level as f64 },
                        End: geo::Point { X: c.rank as f64, Y: c.level as f64 },
                    });
                }
            }
        });
        gocompat::slices::SortStableFunc(&mut self.segments, |a: &CrossingSegment, b: &CrossingSegment| -> i64 {
            if a.Start.X == b.Start.X {
                return if a.End.X < b.End.X {
                    -1
                } else if b.End.X < a.End.X {
                    1
                } else {
                    0
                };
            }
            if a.Start.X < b.Start.X {
                -1
            } else if b.Start.X < a.Start.X {
                1
            } else {
                0
            }
        });
        &self.segments
    }
}

pub(crate) fn iterAllDescendants(a: &placementArena, nodes: &[PN], f: &mut dyn FnMut(PN)) {
    for &pn in nodes {
        f(pn);
    }
    for &pn in nodes {
        let children = &a.slices[a.nodes[pn].children];
        iterAllDescendants(a, children, f);
    }
}

pub(crate) fn allDescendants(a: &placementArena, nodes: &[PN], forOptimization: bool) -> Vec<PN> {
    let mut allNodes: Vec<PN> = nodes.to_vec();
    for &pn in nodes {
        if !forOptimization || a.nodes[pn].optimizeChildrenCrossings {
            let children = &a.slices[a.nodes[pn].children];
            allNodes.extend(allDescendants(a, children, forOptimization));
        }
    }
    allNodes
}

pub(crate) fn addLengths(segments: &[CrossingSegment]) -> f64 {
    let mut length = 0.0;
    for s in segments {
        length += geo::EuclideanDistance(s.Start.X, s.Start.Y, s.End.X, s.End.Y);
    }
    length
}

pub(crate) fn countCrossings(segments: &[CrossingSegment]) -> i64 {
    layoutgraph::CountSegmentCrossings(segments)
}

pub(crate) fn sortLevelNodesByAdacencyPosition(a: &mut placementArena, nodesSL: SL, useConnectionsAbove: bool) {
    let adjacentLevelAverage = |a: &placementArena, pn: PN| -> f64 {
        let n = &a.nodes[pn];
        let nodes = if useConnectionsAbove { &n.aboves } else { &n.belows };
        if nodes.is_empty() {
            return 0.0;
        }
        let mut sum: i64 = 0;
        for &connected in nodes {
            sum += a.nodes[connected].rank;
        }
        sum as f64 / nodes.len() as f64
    };
    let mut nodes = std::mem::take(&mut a.slices[nodesSL]);
    {
        let ar: &placementArena = a;
        gocompat::slices::SortStableFunc(&mut nodes, |&x, &y| -> i64 {
            let xAverage = adjacentLevelAverage(ar, x);
            let yAverage = adjacentLevelAverage(ar, y);
            if xAverage < yAverage {
                -1
            } else if yAverage < xAverage {
                1
            } else {
                0
            }
        });
    }
    a.slices[nodesSL] = nodes;
}

/// Computes the nodes rank (index) from left to right (DFS).
pub(crate) fn computeLevelRanks(a: &mut placementArena, nodes: &[PN]) {
    let mut stack: Vec<PN> = nodes.iter().rev().copied().collect();
    let mut rank: i64 = 0;
    while let Some(current) = stack.pop() {
        a.nodes[current].rank = rank;
        let children = &a.slices[a.nodes[current].children];
        for i in (0..children.len()).rev() {
            stack.push(children[i]);
        }
        rank += 1;
    }
}
