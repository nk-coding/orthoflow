// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/quality/inspection.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

/// Metrics describe a positioned drawing for bounded repair triggers and safety
/// checks. They are not weights or ordering components in the layout score.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Metrics {
    pub NodeOverlaps: i64,
    pub RouteObstructions: i64,
    pub TextOcclusions: i64,
    pub Crossings: f64,
    pub Detour: f64,
    pub RouteLength: f64,
}

/// Inspect reads diagnostics without mutating the graph or influencing Score.
pub fn Inspect(ctx: &Ctx, graph: Option<&GraphRef>) -> GoResult<Metrics> {
    inspectWithLimit(ctx, graph, maxEvaluationWorkUnits).map(|(m, _)| m).map_err(|(e, _)| e)
}

pub(crate) fn inspectWithLimit(ctx: &Ctx, graph: Option<&GraphRef>, workLimit: i64) -> Result<(Metrics, i64), (GoError, i64)> {
    let Some(graph) = graph else {
        return Err((errorf!("cannot evaluate a nil graph"), 0));
    };
    layoutgraph::ValidatePositionedGraph(ctx, "Inspect", graph).map_err(|e| (e, 0))?;
    let guard = limits::NewWorkGuard(ctx, "Inspect", workLimit).map_err(|e| (e, 0))?;
    let r = (|| -> GoResult<Metrics> {
        let mut score = Metrics::default();
        let (gnodes, gedges) = {
            let g = graph.b();
            (g.Nodes.clone(), g.Edges.clone())
        };
        let mut scale = 0.0;
        let mut nodeCount: i64 = 0;
        for node in &gnodes {
            guard.Step()?;
            if !node.IsContainer() && !node.b().IsInvisible {
                scale += (node.w() + node.h()) / 2.0;
                nodeCount += 1;
            }
        }
        scale = math::Max(1.0, scale / std::cmp::max(1, nodeCount) as f64);
        let mut visible: Vec<EdgeRef> = Vec::with_capacity(gedges.len());
        for edge in &gedges {
            guard.Step()?;
            if edge.b().IsInvisible {
                continue;
            }
            visible.push(edge.clone());
            let points: Vec<geo::Point> = edge.b().Points.iter().map(|p| *p.b()).collect();
            let mut length = 0.0;
            for i in 1..points.len() {
                guard.Step()?;
                let (a, b) = (points[i - 1], points[i]);
                length += math::Hypot(b.X - a.X, b.Y - a.Y);
            }
            score.RouteLength += length / scale;
            if points.len() < 2 || edge.IsLoop() {
                continue;
            }
            let (first, last) = (points[0], points[points.len() - 1]);
            let direct = math::Abs(last.X - first.X) + math::Abs(last.Y - first.Y);
            score.Detour += math::Max(0.0, length - direct) / math::Max(scale, direct);
        }
        let crossings = countNonSharedCrossings(&visible, &guard)?;
        score.Crossings = crossings as f64;
        measureGeometry(graph, &mut score, &guard)?;
        measureLabels(graph, &mut score, &guard)?;
        if !finite(score.RouteLength) || !finite(score.Detour) {
            return Err(errorf!("TALA Inspect produced non-finite geometry"));
        }
        guard.Finish()?;
        Ok(score)
    })();
    match r {
        Ok(m) => Ok((m, guard.Used())),
        Err(e) => Err((e, guard.Used())),
    }
}
