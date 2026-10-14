// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/layoutgraph/positioned_validation.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

pub(crate) const maxPositionedRoutePoints: i64 = 1_000_000;

/// ValidatePositionedGraphSelection checks the topology and positioned route
/// geometry consumed by label placement and completed-layout evaluation.
/// extraEdges may repeat graph edges once (the normal route-only selection),
/// but neither input list may contain aliases within itself.
pub fn ValidatePositionedGraphSelection(ctx: &Ctx, operation: &str, graph: Option<&GraphRef>, extraEdges: &[EdgeRef]) -> GoResult<()> {
    // Keep the central topology preflight first.
    validateEngineGraph(ctx, operation, graph)?;
    if let Some(err) = ctx.Err() {
        return Err(wrapf!(err, "{}: {}", operation, err));
    }
    let graph = graph.unwrap();
    let (gnodes, gedges) = {
        let g = graph.b();
        (g.Nodes.clone(), g.Edges.clone())
    };
    if gnodes.len() as i64 > limits::MaxEngineNodes {
        return Err(errorf!("TALA {} node count exceeds limit {}", operation, limits::MaxEngineNodes));
    }
    if gedges.len() as i64 > limits::MaxEngineEdges {
        return Err(errorf!("TALA {} edge count exceeds limit {}", operation, limits::MaxEngineEdges));
    }
    if extraEdges.len() as i64 > limits::MaxEngineEdges {
        return Err(errorf!("TALA {} requested edge count exceeds limit {}", operation, limits::MaxEngineEdges));
    }

    let checks = std::cell::Cell::new(0i64);
    let checkContext = || -> GoResult<()> {
        checks.set(checks.get() + 1);
        if checks.get() % 64 != 0 {
            return Ok(());
        }
        if let Some(err) = ctx.Err() {
            return Err(wrapf!(err, "{}: {}", operation, err));
        }
        Ok(())
    };
    for node in &gnodes {
        checkContext()?;
        if !node.has_tl() {
            return Err(errorf!("TALA {} graph node {} has no position", operation, node.id()));
        }
    }

    let mut seenGraphEdges: Set<EdgeRef> = Set::default();
    let mut seenRequestedEdges: Set<EdgeRef> = Set::default();
    let mut validatedEdges: Set<EdgeRef> = Set::default();
    let mut routePoints: i64 = 0;
    let mut validateEdge = |edge: &EdgeRef, index: usize, kind: &str, seenInList: &mut Set<EdgeRef>| -> GoResult<()> {
        checkContext()?;
        if seenInList.contains(edge) {
            return Err(errorf!("TALA {} {} at index {} repeats edge {}", operation, kind, index, edge.entityID()));
        }
        seenInList.insert(edge.clone());
        if validatedEdges.contains(edge) {
            return Ok(());
        }
        validatedEdges.insert(edge.clone());
        let (npoints, hasLabel) = {
            let e = edge.b();
            (e.Points.len() as i64, e.Label.is_some() || e.SourceArrowheadLabel.is_some() || e.TargetArrowheadLabel.is_some())
        };
        if npoints > maxPositionedRoutePoints - routePoints {
            return Err(errorf!("TALA {} route point count exceeds limit {}", operation, maxPositionedRoutePoints));
        }
        routePoints += npoints;
        for _ in 0..npoints {
            checkContext()?;
        }
        if hasLabel && npoints < 2 {
            return Err(errorf!("TALA {} labeled edge {} requires at least two route points", operation, edge.entityID()));
        }
        Ok(())
    };
    for (index, edge) in gedges.iter().enumerate() {
        validateEdge(edge, index, "graph edge", &mut seenGraphEdges)?;
    }
    for (index, edge) in extraEdges.iter().enumerate() {
        validateEdge(edge, index, "requested edge", &mut seenRequestedEdges)?;
    }
    if let Some(err) = ctx.Err() {
        return Err(wrapf!(err, "{}: {}", operation, err));
    }
    Ok(())
}
