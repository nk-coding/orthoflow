// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/quality/scoring.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

/// EvaluateWithArea returns the human-facing layout score and the graph area
/// separately. Go `(score, area, err)`.
pub fn EvaluateWithArea(ctx: &Ctx, graph: Option<&GraphRef>) -> GoResult<(f64, f64)> {
    let (score, area, _) = evaluateWithAreaLimit(ctx, graph, maxEvaluationWorkUnits).map_err(|(e, _)| e)?;
    Ok((score, area))
}

/// Returns (score, area, used) or (err, used).
pub(crate) fn evaluateWithAreaLimit(ctx: &Ctx, graph: Option<&GraphRef>, workLimit: i64) -> Result<(f64, f64, i64), (GoError, i64)> {
    let Some(graph) = graph else {
        return Err((errorf!("cannot evaluate a nil graph"), 0));
    };
    layoutgraph::ValidatePositionedGraph(ctx, "Evaluate", graph).map_err(|e| (e, 0))?;
    let guard = newEvaluationWorkGuard(ctx, workLimit).map_err(|e| (e, 0))?;
    let r = (|| -> GoResult<(f64, f64)> {
        let (gedges,) = {
            let g = graph.b();
            (g.Edges.clone(),)
        };
        let mut score = 0.0;
        for e in &gedges {
            guard.Step()?;
            if e.from().clusterField().is_some() || e.to().clusterField().is_some() {
                continue;
            }
            let points = e.points();
            let turns = std::cmp::max(0, points.len() as i64 - 2) as f64;
            score += turns * 0.5;

            let n = points.len() as i64;
            let mut i = 0i64;
            while i < n - 1 {
                guard.Step()?;
                let curr = *points[i as usize].b();
                let next = *points[i as usize + 1].b();
                if curr.X != next.X && curr.Y != next.Y {
                    score += 3.0;
                }
                i += 1;
            }
        }
        let crossings = countNonSharedCrossings(&gedges, &guard)?;
        score += crossings as f64;
        let labelScore = scoreExistingLabelPlacements(graph, &guard)?;
        score += 1.0 - labelScore;
        chargeEvaluationAreaWork(graph, &guard)?;
        let area = graph.Area();
        guard.Finish()?;
        Ok((score, area))
    })();
    match r {
        Ok((s, a)) => Ok((s, a, guard.Used())),
        Err(e) => Err((e, guard.Used())),
    }
}
