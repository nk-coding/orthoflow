// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Copyright 2026 Niklas Krieger
//
// Rust twin of goref/d2/d2layouts/d2talalayout/internal/engine/zz_debug_export.go
// (port-harness hook, not part of terrastruct/d2); it drives the stages of
// d2layouts/d2talalayout/internal/engine/pipeline.go (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
//
// Port-harness hook (not upstream): runs every pipeline stage and reports the
// graph after each one so language ports can be diffed stage by stage.
use crate::*;

/// StageNames lists the default pipeline stage names in order.
pub fn StageNames() -> Vec<String> {
    defaultPipelineStages.iter().map(|s| s.name.to_string()).collect()
}

/// DebugRunStages clones graph and runs the default pipeline, invoking after
/// each stage with the stage index, name and the live workspace graph.
pub fn DebugRunStages(
    ctx: &Ctx,
    graph: &GraphRef,
    seed: i64,
    mut after: impl FnMut(usize, &str, &GraphRef) -> GoResult<()>,
) -> GoResult<GraphRef> {
    let workspace = layoutgraph::Clone(ctx, Some(graph))?;
    if workspace.b().Nodes.is_empty() {
        return Ok(workspace);
    }
    let mut p = newPipeline(&workspace, seed);
    let (ctx, _) = layoutgraph::EnsureTransactionWorkGuard(ctx, "AutolayoutTransactions")?;
    for (i, stage) in defaultPipelineStages.iter().enumerate() {
        if i == 0 {
            grouping::ResetClusters(&p.graph);
            p.resetFullLayoutRouteState();
        }
        if let Err(err) = (stage.run)(&mut p, &ctx) {
            return Err(GoError::wrap(format!("{}: {}", stage.name, err), err));
        }
        after(i, stage.name, &p.graph)?;
    }
    Ok(workspace)
}

/// DebugReplayStage runs only stage index on graph (in place) using a fresh
/// pipeline seeded with seed.
pub fn DebugReplayStage(ctx: &Ctx, graph: &GraphRef, seed: i64, index: i64) -> GoResult<()> {
    if index < 0 || index as usize >= defaultPipelineStages.len() {
        return Err(errorf!("stage index {} out of range", index));
    }
    let mut p = newPipeline(graph, seed);
    let (ctx, _) = layoutgraph::EnsureTransactionWorkGuard(ctx, "AutolayoutTransactions")?;
    let stage = &defaultPipelineStages[index as usize];
    if index == 0 {
        grouping::ResetClusters(&p.graph);
        p.resetFullLayoutRouteState();
    }
    if let Err(err) = (stage.run)(&mut p, &ctx) {
        return Err(GoError::wrap(format!("{}: {}", stage.name, err), err));
    }
    Ok(())
}
