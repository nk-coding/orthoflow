// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/placement/compaction.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.
//
// Port of placement/compaction.go
#[allow(unused_imports)]
use crate::*;
use gocompat::math;
use gocompat::{Ctx, GoError, GoResult, Map};
use layoutgraph::{EdgeAbductionRef, EdgeRef, GraphRef, NodeRef, NodesMethods};

// compaction serves two functions:
// 1. Inflate nodes to create room for more optimal node placements (escaping local maximums)
// 2. Reduce distance between nodes too far apart
//
// We use compaction to inflate when transitioning from sizeless phase to sized phase, which the transition flag is for
#[derive(Clone)]
pub(crate) struct compactionOptions {
    pub(crate) edgeAbductions: Option<Vec<EdgeAbductionRef>>,
    pub(crate) axis: layoutAxis,
    pub(crate) includeSizes: bool,
    pub(crate) factor: f64,
    pub(crate) transition: bool,
    pub(crate) moveWorkLimit: u64,
}

impl Default for compactionOptions {
    fn default() -> Self {
        compactionOptions {
            edgeAbductions: None,
            axis: invalidAxis,
            includeSizes: false,
            factor: 0.0,
            transition: false,
            moveWorkLimit: 0,
        }
    }
}

pub(crate) fn compaction(ctx: &Ctx, g: &GraphRef, options: compactionOptions) -> GoResult<()> {
    if !options.axis.valid() {
        return Err(GoError::new("TALA Compaction requires an axis"));
    }
    if options.factor <= 0.0 || options.factor.is_nan() || options.factor.is_infinite() {
        return Err(GoError::new("TALA Compaction requires a finite positive factor"));
    }
    let edgeAbductions = options.edgeAbductions.clone();
    let isHorizontal = options.axis.isHorizontal();
    let includeSizes = options.includeSizes;
    let factor = options.factor;
    let transition = options.transition;
    layoutgraph::Validate(ctx, "Compaction", g)?;
    let originalCosts = g.RoutingCosts();
    let originalPositions = snapshotNodePositionsContext(ctx, "Compaction", &g.nodes())?;
    let result = (|| -> GoResult<()> {
        let visibilityEdges = visibilityEdges(ctx, g, isHorizontal, includeSizes)?;

        inflateAlongAxis(g, isHorizontal, includeSizes, factor, &visibilityEdges, transition);
        if let Some(err) = ctx.Err() {
            return Err(GoError::wrap(format!("Compaction: {}", err), err));
        }

        // We're returning early during transition because the below calls assume no overlap
        // And during transition, there can be temporary overlaps along a dimension
        // The main point of the transition is just to inflate anyway, there shouldn't be any compacting needed because
        // they inflate to the minimum possible value
        if transition {
            return Ok(());
        }

        for _ in 0..20 {
            let changed = shiftSubgraphs(ctx, g, isHorizontal, includeSizes, factor, edgeAbductions.as_deref(), &visibilityEdges)?;
            if !changed {
                break;
            }
        }
        let mut moveWorkLimit = options.moveWorkLimit;
        if moveWorkLimit == 0 || moveWorkLimit > limits::MaxOptimizationWorkUnits {
            moveWorkLimit = limits::MaxOptimizationWorkUnits;
        }
        let movesGuard = limits::NewOptimizationWorkGuard(ctx, "CompactionMoves", moveWorkLimit)?;

        for _ in 0..20 {
            let changed = compactAlongAxis(ctx, g, isHorizontal, includeSizes, factor, edgeAbductions.as_deref(), &visibilityEdges, &movesGuard)?;
            if !changed {
                break;
            }
        }
        if let Some(err) = ctx.Err() {
            return Err(GoError::wrap(format!("Compaction: {}", err), err));
        }
        Ok(())
    })();
    if result.is_err() {
        restoreNodePositions(&originalPositions);
        g.RestoreRoutingCosts(originalCosts);
    }
    result
}

fn sortAlongAxis(nodes: &mut Vec<NodeRef>, isHorizontal: bool) {
    let mut keyed: Vec<(f64, layoutgraph::EntityID, NodeRef)> = nodes
        .iter()
        .map(|n| (if isHorizontal { n.x() } else { n.y() }, n.id(), n.clone()))
        .collect();
    gocompat::sort::Slice(&mut keyed, |a, b| {
        if a.0 == b.0 {
            return a.1 < b.1;
        }
        a.0 < b.0
    });
    *nodes = keyed.into_iter().map(|k| k.2).collect();
}

pub(crate) fn orderedAlongAxis(g: &GraphRef, isHorizontal: bool) -> Vec<NodeRef> {
    let mut orderedNodes = g.nodes();
    sortAlongAxis(&mut orderedNodes, isHorizontal);
    orderedNodes
}

// shiftSubgraphs tries to move entire subgraphs of the visibility graph, for a more global attempt to close gaps
// The primary purpose is to escape local maximas, where no individual nodes would move closer to each other
pub(crate) fn shiftSubgraphs(
    ctx: &Ctx,
    g: &GraphRef,
    isHorizontal: bool,
    includeSizes: bool,
    factor: f64,
    edgeAbductions: Option<&[EdgeAbductionRef]>,
    vEdges: &[EdgeRef],
) -> GoResult<bool> {
    let mut changed = false;
    let mut subgraphSet: Map<NodeRef, Vec<NodeRef>> = Map::default();
    let mut orderedSubgraphRoots: Vec<NodeRef> = Vec::new();

    let orderedNodes = orderedAlongAxis(g, isHorizontal);

    for node in &orderedNodes {
        let nearestFrom = nearestFrom(vEdges, node, isHorizontal, includeSizes);
        match nearestFrom {
            None => {
                subgraphSet.insert(node.clone(), vec![node.clone()]);
                orderedSubgraphRoots.push(node.clone());
            }
            Some(nf) => {
                subgraphSet.entry(nf).or_default().push(node.clone());
            }
        }
    }

    let mut filtered: Vec<NodeRef> = Vec::new();
    for root in &orderedSubgraphRoots {
        let mut sgHasFixedNode = false;
        for n in subgraphSet.get(root).map(|v| v.as_slice()).unwrap_or(&[]) {
            if n.b().FixedTopLeft.is_some() {
                sgHasFixedNode = true;
                break;
            }
        }
        if sgHasFixedNode {
            subgraphSet.remove(root);
        } else {
            filtered.push(root.clone());
        }
    }
    let mut orderedSubgraphRoots = filtered;
    let fixedNodes = g.FixedNodes();

    sortAlongAxis(&mut orderedSubgraphRoots, isHorizontal);

    let globallyFurthestBehind = globallyFurthestBehind(g, isHorizontal);
    for furthestBehind in &orderedSubgraphRoots {
        let subgraph: Vec<NodeRef> = subgraphSet.get(furthestBehind).cloned().unwrap_or_default();
        let mut possibleMoves: Vec<geo::PointRef>;
        if isHorizontal {
            if furthestBehind.x() == globallyFurthestBehind.x() {
                possibleMoves = candidateMoves(ctx, g, furthestBehind, factor, isHorizontal, includeSizes, 2, vEdges)?;
            } else {
                possibleMoves = candidateMoves(ctx, g, furthestBehind, factor, isHorizontal, includeSizes, 0, vEdges)?;
            }
        } else if furthestBehind.y() == globallyFurthestBehind.y() {
            possibleMoves = candidateMoves(ctx, g, furthestBehind, factor, isHorizontal, includeSizes, 2, vEdges)?;
        } else {
            possibleMoves = candidateMoves(ctx, g, furthestBehind, factor, isHorizontal, includeSizes, 0, vEdges)?;
        }
        possibleMoves.push(furthestBehind.tl().expect("nil pointer dereference: TopLeft").Copy());

        // shifting subgraphs shouldn't introduce any overlaps between subgraph nodes, but we still need to check for overlaps with nodes not in this subgraph
        let mut nonSubgraphNodes: Vec<NodeRef> = Vec::new();
        nonSubgraphNodes.extend(fixedNodes.iter().cloned());
        for otherN in g.nodes() {
            let isIn = subgraph.contains(&otherN);
            if !isIn {
                nonSubgraphNodes.push(otherN);
            }
        }

        let mut symmetryCost = 1.0;
        if includeSizes {
            symmetryCost = g.b().CellSize;
        }
        symmetryCost *= subgraph.NumAdjacent() as f64;

        let elo = || placementcost::EdgeLengthOptions {
            EdgeAbductions: edgeAbductions.map(|a| a.to_vec()),
            IncludeNodeSizes: includeSizes,
            EnforceMinimumGap: false,
            PenalizeDirection: true,
        };
        let mut bestEdgeDistance = placementcost::NodesEdgeLength(ctx, &subgraph, &elo())?;
        let mut bestDelta = 0.0;

        let originalPositions = snapshotNodePositionsContext(ctx, "Compaction", &subgraph)?;
        for mv in &possibleMoves {
            let mv = *mv.b();
            let delta = if isHorizontal { furthestBehind.x() - mv.X } else { furthestBehind.y() - mv.Y };

            let mut overlaps = false;
            for n in &subgraph {
                let p = n.tl().expect("nil pointer dereference: TopLeft").Copy();
                if isHorizontal {
                    p.bm().X -= delta;
                } else {
                    p.bm().Y -= delta;
                }
                if !fixedNodes.is_empty() {
                    let pv = *p.b();
                    if n.PointPastFixedOrigin(pv.X, pv.Y, includeSizes) {
                        overlaps = true;
                        break;
                    }
                }
                for otherN in &nonSubgraphNodes {
                    if includeSizes {
                        if n.DoesOverlapAt(otherN, &p) {
                            overlaps = true;
                            break;
                        }
                    } else if nonNilEquals(&otherN.tlv(), &p.b()) {
                        overlaps = true;
                        break;
                    }
                }
                if overlaps {
                    break;
                }
            }
            if overlaps {
                continue;
            }

            for node in &subgraph {
                if isHorizontal {
                    node.MoveWithChildren(-delta, 0.0);
                } else {
                    node.MoveWithChildren(0.0, -delta);
                }
            }

            let mut edgeDistanceAfter = match placementcost::NodesEdgeLength(ctx, &subgraph, &elo()) {
                Ok(v) => v,
                Err(err) => {
                    restoreNodePositions(&originalPositions);
                    return Err(err);
                }
            };
            if includeSizes {
                let symmetry = match placementcost::NodesSymmetry(ctx, &subgraph, edgeAbductions) {
                    Ok(v) => v,
                    Err(err) => {
                        restoreNodePositions(&originalPositions);
                        return Err(err);
                    }
                };
                edgeDistanceAfter -= symmetry * symmetryCost;
            }

            restoreNodePositions(&originalPositions);

            if edgeDistanceAfter < bestEdgeDistance {
                bestEdgeDistance = edgeDistanceAfter;
                bestDelta = delta;
            }
        }
        if bestDelta != 0.0 {
            changed = true;
            for node in &subgraph {
                if isHorizontal {
                    node.MoveWithChildren(-bestDelta, 0.0);
                } else {
                    node.MoveWithChildren(0.0, -bestDelta);
                }
            }
        }
    }
    Ok(changed)
}

// inflateAlongAxis makes sure that the appropriate amount of distance (based on factor) between connected (in vGraph) nodes
pub(crate) fn inflateAlongAxis(g: &GraphRef, isHorizontal: bool, includeSizes: bool, factor: f64, vEdges: &[EdgeRef], transition: bool) {
    let orderedNodes = orderedAlongAxis(g, isHorizontal);
    let hasFixedNode = g.HasFixedNode();

    for node in &orderedNodes {
        let nearestFrom = nearestFrom(vEdges, node, isHorizontal, includeSizes);
        let Some(nearestFrom) = nearestFrom else {
            // TODO reconsider this
            if hasFixedNode && transition && node.b().FixedTopLeft.is_none() {
                // during transition we want all nodes to move to a cellSize even if they don't have a nearestFrom
                let cellSize = g.b().CellSize;
                if isHorizontal {
                    node.MoveAbsWithChildren(roundToPreviousCellSize(node.x(), cellSize), node.y());
                } else {
                    node.MoveAbsWithChildren(node.x(), roundToPreviousCellSize(node.y(), cellSize));
                }
            }
            continue;
        };
        // we don't want to move fixed nodes but we transitioning from sizeless to sized is an exception
        if node.b().FixedTopLeft.is_some() && !transition {
            continue;
        }

        // During transition, because there are temporary overlaps, we want to maintain those overlaps for visibility edges in
        // the subsequent iteration of compaction. If one is connected and another is not, then that overlap can be lost.
        // So we force it to be maintained.
        let delta: i64 = if transition {
            layoutgraph::ConnectedNodeGap
        } else {
            node.DeltaTo(&nearestFrom, node.tl().as_ref())
        };
        let floor = compactionFloor(g, &nearestFrom, factor, isHorizontal, includeSizes, delta);
        let cellSize = g.b().CellSize;
        if isHorizontal {
            if includeSizes {
                if floor * cellSize > node.x() {
                    node.MoveAbsWithChildren(floor * cellSize, node.y());
                }
            } else if floor > node.x() {
                node.MoveAbsWithChildren(floor, node.y());
            }
        } else if includeSizes {
            if floor * cellSize > node.y() {
                node.MoveAbsWithChildren(node.x(), floor * cellSize);
            }
        } else if floor > node.y() {
            node.MoveAbsWithChildren(node.x(), floor);
        }
    }
}

// Each node moves as far back as possible while making progress towards an adjacent node
// (this is a visibility edge)
pub(crate) fn compactAlongAxis(
    ctx: &Ctx,
    g: &GraphRef,
    isHorizontal: bool,
    includeSizes: bool,
    factor: f64,
    edgeAbductions: Option<&[EdgeAbductionRef]>,
    vEdges: &[EdgeRef],
    guard: &limits::OptimizationWorkGuardRef,
) -> GoResult<bool> {
    let mut changed = false;

    let orderedNodes = orderedAlongAxis(g, isHorizontal);

    for node in &orderedNodes {
        if node.b().FixedTopLeft.is_some() {
            continue;
        }
        let mut possibleMoves = candidateMoves(ctx, g, node, factor, isHorizontal, includeSizes, 0, vEdges)?;
        possibleMoves.push(node.tl().expect("nil pointer dereference: TopLeft").Copy());

        let moved = moveNodeToBest(ctx, g, node, &possibleMoves, edgeAbductions, includeSizes, Some(guard))?;
        if moved {
            changed = true;
        }
    }

    Ok(changed)
}

pub(crate) fn visibilityEdges(ctx: &Ctx, g: &GraphRef, isHorizontal: bool, includeSizes: bool) -> GoResult<Vec<EdgeRef>> {
    let guard = limits::NewWorkGuard(ctx, "CompactionVisibility", limits::MaxEngineWorkUnits)?;
    let mut edges: Vec<EdgeRef> = Vec::new();
    let gnodes = g.nodes();
    for node in &gnodes {
        guard.Step()?;
        for otherNode in &gnodes {
            guard.Step()?;
            if node.id() == otherNode.id() {
                continue;
            }

            let delta = node.DeltaTo(otherNode, node.tl().as_ref()) as f64;
            if !node.VisibilityGraphCandidate(isHorizontal, true, includeSizes, otherNode, delta) {
                continue;
            }

            let blocked = isVisibilityGraphEdgeBlocked(g, isHorizontal, includeSizes, node, otherNode, &guard)?;
            if !blocked {
                edges.push(layoutgraph::NewEdge(node, otherNode));
            }
        }
    }

    guard.Finish()?;
    Ok(edges)
}

pub(crate) fn isVisibilityGraphEdgeBlocked(
    g: &GraphRef,
    isHorizontal: bool,
    includeSizes: bool,
    nodeA: &NodeRef,
    nodeB: &NodeRef,
    guard: &limits::WorkGuardRef,
) -> GoResult<bool> {
    let (aID, bID) = (nodeA.id(), nodeB.id());
    for node in g.nodes() {
        guard.Step()?;
        if (node.id() == aID) || (node.id() == bID) {
            continue;
        }

        if node.IsBlocked(nodeA, nodeB, includeSizes, isHorizontal) {
            return Ok(true);
        }
    }

    Ok(false)
}

pub(crate) fn nearestFrom(edges: &[EdgeRef], node: &NodeRef, isHorizontal: bool, includeSizes: bool) -> Option<NodeRef> {
    let mut nearestBehind: Option<NodeRef> = None;

    let mut largestBehindDistance = math::Inf(-1);

    for edge in edges {
        let (from, to) = {
            let e = edge.b();
            (e.From.clone(), e.To.clone())
        };
        if &to == node {
            let v = if isHorizontal {
                if includeSizes {
                    from.x() + from.w()
                } else {
                    from.x()
                }
            } else if includeSizes {
                from.y() + from.h()
            } else {
                from.y()
            };
            if v > largestBehindDistance {
                largestBehindDistance = v;
                nearestBehind = Some(from);
            }
        }
    }

    nearestBehind
}

// compactionFloor returns the floor value along an axis that a node must satisfy, given the anchor.
// if include sizes, it returns it as a number of cell size
pub(crate) fn compactionFloor(g: &GraphRef, anchor: &NodeRef, factor: f64, isHorizontal: bool, includeSizes: bool, padding: i64) -> f64 {
    let mut floor;
    let cellSize = g.b().CellSize;

    if isHorizontal {
        if includeSizes {
            floor = ((anchor.x() + (factor * anchor.w())) / cellSize).ceil();
        } else {
            floor = anchor.x() + factor.floor();
        }
    } else if includeSizes {
        floor = ((anchor.y() + (factor * anchor.h())) / cellSize).ceil();
    } else {
        floor = anchor.y() + factor.floor();
    }

    if includeSizes {
        let anchorVal = if isHorizontal { anchor.x() + anchor.w() } else { anchor.y() + anchor.h() };
        while (floor * cellSize - anchorVal) <= padding as f64 {
            floor += 1.0;
        }
    }

    floor
}

pub(crate) fn globallyFurthestBehind(g: &GraphRef, isHorizontal: bool) -> NodeRef {
    let gnodes = g.nodes();
    let mut globallyFurthestBehind = gnodes[0].clone();
    for n in &gnodes {
        if isHorizontal {
            if n.x() < globallyFurthestBehind.x() {
                globallyFurthestBehind = n.clone();
            }
        } else if n.y() < globallyFurthestBehind.y() {
            globallyFurthestBehind = n.clone();
        }
    }
    globallyFurthestBehind
}

// candidateMoves returns the points a node may move to while compacting toward the nearest node behind it.
// floorDecrease is only safe to add when called with the node being the globally most behind
pub(crate) fn candidateMoves(
    ctx: &Ctx,
    g: &GraphRef,
    node: &NodeRef,
    factor: f64,
    isHorizontal: bool,
    includeSizes: bool,
    floorDecrease: i64,
    vEdges: &[EdgeRef],
) -> GoResult<Vec<geo::PointRef>> {
    let guard = limits::NewWorkGuard(ctx, "CompactionCandidates", limits::MaxEngineWorkUnits)?;
    let nearestBehind = match nearestFrom(vEdges, node, isHorizontal, includeSizes) {
        Some(n) => n,
        // Node is at the start of visibility graph chain
        None => globallyFurthestBehind(g, isHorizontal),
    };

    let mut floor = compactionFloor(g, &nearestBehind, factor, isHorizontal, includeSizes, node.DeltaTo(&nearestBehind, node.tl().as_ref()));

    let hasFixed = g.HasFixedNode();
    let cellSize = g.b().CellSize;

    if isHorizontal {
        // If it's same node or a node on another axis, no need add size buffer
        let nearestBehindAnotherAxis = if includeSizes {
            (nearestBehind.y() > node.y() + node.h()) || (nearestBehind.y() + nearestBehind.h() < node.y())
        } else {
            nearestBehind.y() != node.y()
        };
        if (nearestBehind.x() == node.x()) || nearestBehindAnotherAxis {
            if includeSizes {
                floor = nearestBehind.x() / cellSize;
                // with fixed positions floor may not be exactly on a cellSize
                if hasFixed {
                    floor = floor.round();
                }
            } else {
                floor = nearestBehind.x();
            }
        }
    } else {
        let nearestBehindAnotherAxis = if includeSizes {
            (nearestBehind.x() > node.x() + node.w()) || (nearestBehind.x() + nearestBehind.w() < node.x())
        } else {
            nearestBehind.x() != node.x()
        };
        if (nearestBehind.y() == node.y()) || nearestBehindAnotherAxis {
            if includeSizes {
                floor = nearestBehind.y() / cellSize;
                // with fixed positions floor may not be exactly on a cellSize
                if hasFixed {
                    floor = floor.round();
                }
            } else {
                floor = nearestBehind.y();
            }
        }
    }

    // Every node needs to include at least itself
    let ceil = if isHorizontal {
        if includeSizes {
            node.x() / cellSize
        } else {
            node.x()
        }
    } else if includeSizes {
        node.y() / cellSize
    } else {
        node.y()
    };
    let start = floor - floorDecrease as f64;
    if start.is_nan() || start.is_infinite() || ceil.is_nan() || ceil.is_infinite() {
        return Err(invariant::New("compaction candidate range is not finite"));
    }
    let mut candidateCount = 0usize;
    if start <= ceil {
        let count = (ceil - start).floor() + 1.0;
        if count > maxCompactionCandidateCount as f64 {
            return Err(GoError::new(format!(
                "TALA compaction candidate count {:.0} exceeds limit {}",
                count, maxCompactionCandidateCount
            )));
        }
        candidateCount = count as usize;
    }
    let mut positions: Vec<geo::PointRef> = Vec::with_capacity(candidateCount);
    let (nx, ny) = (node.x(), node.y());
    let mut i = start;
    while i <= ceil {
        guard.Step()?;
        let p = if isHorizontal {
            if includeSizes {
                geo::NewPoint(i * cellSize, ny)
            } else {
                geo::NewPoint(i, ny)
            }
        } else if includeSizes {
            geo::NewPoint(nx, i * cellSize)
        } else {
            geo::NewPoint(nx, i)
        };

        positions.push(p);
        i += 1.0;
    }
    guard.Finish()?;
    Ok(positions)
}
