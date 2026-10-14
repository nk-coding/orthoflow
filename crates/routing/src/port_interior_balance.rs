// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/routing/port_interior_balance.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of routing/port_interior_balance.go.
#[allow(unused_imports)]
use crate::*;
use gocompat::{GoResult, Map};
use layoutgraph::{EdgeRef, GraphRef};

pub(crate) fn hasFixedBalancingPorts(edge: &EdgeRef) -> bool {
    edge.HasTableColumn()
        || edge.from().shape().GetType() == shape::DIAMOND_TYPE
        || edge.to().shape().GetType() == shape::DIAMOND_TYPE
}

// balancePortInteriorsGuarded gives fixed-port routes a separate conservative
// balancing pass. Other routes, endpoints, and their approach segments act as
// immutable corridor boundaries. Reject the whole tentative batch if moving
// connected bends creates an obstruction, crossing, or substantial detour.
pub(crate) fn balancePortInteriorsGuarded(g: &GraphRef, guard: &routeWorkGuardRef) -> GoResult<()> {
    let mut selected: Vec<EdgeRef> = Vec::new();
    let mut locked: Vec<EdgeRef> = Vec::new();
    let nodeToTree = g.b().NodeToTree.clone();
    for e in g.edges() {
        guard.step()?;
        let fromTree = nodeToTree.has(&e.from());
        let toTree = nodeToTree.has(&e.to());
        if hasFixedBalancingPorts(&e) && !e.b().IsCurve && !e.IsLoop() && !fromTree && !toTree && e.b().Points.len() >= 6 {
            selected.push(e);
        } else {
            locked.push(e);
        }
    }
    if selected.is_empty() {
        return Ok(());
    }
    let ctx = guard.ctx();
    let beforeMetrics = quality::Inspect(&ctx, Some(g))?;
    let snapshot = captureRouteMutations(Some(g), &[], guard)?;
    let before = copyRoutePoints(&selected, guard)?;
    balanceRegularEdgesGuarded(g, &locked, &selected, guard)?;
    for e in &selected {
        let valid = changedRouteIsClear(g, e, &pointValues(&e.b().Points), &before[e], true, guard)?;
        if !valid {
            snapshot.restore();
            return Ok(());
        }
    }
    let afterMetrics = quality::Inspect(&ctx, Some(g))?;
    if afterMetrics.RouteObstructions > beforeMetrics.RouteObstructions
        || afterMetrics.Crossings > beforeMetrics.Crossings
        || afterMetrics.TextOcclusions > beforeMetrics.TextOcclusions
        || afterMetrics.RouteLength > beforeMetrics.RouteLength * 1.1
    {
        snapshot.restore();
    }
    Ok(())
}

/// Go returns fresh `*geo.Point` copies; they are only ever read, so the port
/// keeps the values.
pub(crate) fn copyRoutePoints(edges: &[EdgeRef], guard: &routeWorkGuardRef) -> GoResult<Map<EdgeRef, Vec<geo::Point>>> {
    let mut result: Map<EdgeRef, Vec<geo::Point>> = Map::default();
    for e in edges {
        let points = pointValues(&e.b().Points);
        guard.add(points.len() as u64 + 1)?;
        result.insert(e.clone(), points);
    }
    Ok(result)
}

/// `e` supplies the endpoints (Go passes either the edge or a value copy of it
/// with replaced points); `points` are the route's current point values.
pub(crate) fn changedRouteIsClear(
    g: &GraphRef,
    e: &EdgeRef,
    points: &[geo::Point],
    before: &[geo::Point],
    fixedPorts: bool,
    guard: &routeWorkGuardRef,
) -> GoResult<bool> {
    if points.len() < 2 || before.len() < 2 {
        return Ok(false);
    }
    let (n, m) = (points.len(), before.len());
    if fixedPorts
        && (points[0] != before[0]
            || points[n - 1] != before[m - 1]
            || !sameRouteDirection(&before[0], &before[1], &points[0], &points[1])
            || !sameRouteDirection(&before[m - 2], &before[m - 1], &points[n - 2], &points[n - 1]))
    {
        return Ok(false);
    }
    for i in 1..n {
        let (a, b) = (points[i - 1], points[i]);
        let mut unchanged = false;
        for j in 1..m {
            guard.step()?;
            if a == before[j - 1] && b == before[j] {
                unchanged = true;
                break;
            }
        }
        if unchanged {
            continue;
        }
        if a.X != b.X && a.Y != b.Y {
            return Ok(false);
        }
        let blocked = lineIntersectsUnrelatedNode(g, e, &a, &b, guard)?;
        if blocked {
            return Ok(false);
        }
    }
    Ok(true)
}
