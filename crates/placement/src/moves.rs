// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/placement/moves.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of placement/moves.go
use crate::*;
use gocompat::Ctx;
use gocompat::{errorf, math, wrapf, GoResult};
use layoutgraph::{EdgeAbductionRef, GraphRef, NodeRef};
use limits::OptimizationWorkGuardRef;

// moveNodeToBest chooses the candidate with the lowest placement cost using
// the compaction stage's shared work guard.
pub fn moveNodeToBest(
    ctx: &Ctx,
    graph: &GraphRef,
    node: &NodeRef,
    points: &[geo::PointRef],
    edgeAbductions: Option<&[EdgeAbductionRef]>,
    includeSizes: bool,
    guard: Option<&OptimizationWorkGuardRef>,
) -> GoResult<bool> {
    let Some(guard) = guard else {
        return Err(errorf!("TALA moveNodeToBest requires a work guard"));
    };
    guard.Finish()?;

    let symmetryCost = graph.b().CellSize * node.b().Edges.len() as f64;
    let mut leastDistance = math::Inf(1);
    let mut leastDistancePoint = node.tlv();
    let current = node.tlv();
    let (currentX, currentY) = (current.X, current.Y);

    let mut fixedOrigin = optimizerFixedOrigin(Some(graph), node.EffectiveContainer().as_ref(), guard)?;
    if let Some(fo) = fixedOrigin.as_mut() {
        if !includeSizes {
            let cellSize = graph.b().CellSize;
            fo.X = math::Round(fo.X / cellSize);
            fo.Y = math::Round(fo.Y / cellSize);
        }
    }
    let abductionsLen = edgeAbductions.map_or(0, |a| a.len());
    let opts = edgeLengthOptions(edgeAbductions, includeSizes);

    for point in points {
        guard.Step()?;
        let p = *point.b();
        let (x, y) = (p.X, p.Y);
        if let Some(fo) = &fixedOrigin {
            if x < fo.X || y < fo.Y {
                continue;
            }
        }
        let mut canMove = x == currentX && y == currentY;
        if !canMove {
            canMove = optimizerCanMove(node, point, includeSizes, guard)?;
        }
        if !canMove {
            continue;
        }
        optimizerMoveNodeAbs(node, x, y, guard)?;
        chargeOptimizerScoring(node, abductionsLen, includeSizes, guard)?;
        let mut edgeLength = placementcost::NodeEdgeLength(ctx, node, &opts)?;
        if includeSizes {
            if !math::IsInf(leastDistance, 1) && geo::PrecisionCompare(edgeLength - symmetryCost, leastDistance, geo::PRECISION) == 1 {
                continue;
            }
            let crossingCost = placementcost::ColumnCrossingCost(ctx, node, edgeAbductions)?;
            let symmetry = placementcost::NodeSymmetry(ctx, node, edgeAbductions)?;
            edgeLength += crossingCost;
            edgeLength -= symmetry * symmetryCost;
        }
        match geo::PrecisionCompare(edgeLength, leastDistance, geo::PRECISION) {
            -1 => {
                leastDistance = edgeLength;
                leastDistancePoint = p;
            }
            0 => {
                if p.X == currentX && p.Y == currentY {
                    leastDistance = edgeLength;
                    leastDistancePoint = p;
                }
            }
            _ => {}
        }
    }

    if let Some(err) = ctx.Err() {
        return Err(wrapf!(err, "EdgeLength: {}", err));
    }
    if math::IsInf(leastDistance, 1) {
        return Err(errorf!("sizedOptimizer.moveNodeToBest: could not find any placement"));
    }
    optimizerMoveNodeAbs(node, leastDistancePoint.X, leastDistancePoint.Y, guard)?;
    guard.Finish()?;
    let after = node.tlv();
    Ok(after.X != currentX || after.Y != currentY)
}
