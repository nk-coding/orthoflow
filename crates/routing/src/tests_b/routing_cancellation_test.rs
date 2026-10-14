// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/routing/routing_cancellation_test.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of routing/routing_cancellation_test.go (route-line and fallback cases;
//! Err-counting context probes are not ported, quickRoute belongs to half a).
use super::*;

pub(crate) fn newRoutingTestGraph(targetX: f64, targetY: f64) -> (GraphRef, EdgeRef) {
    let graph = layoutgraph::NewGraph();
    let source = nodeAt(1, 40.0, 40.0, 0.0, 0.0);
    let target = nodeAt(2, 40.0, 40.0, targetX, targetY);
    graph.AddNode(&source);
    graph.AddNode(&target);
    let e = graph.Connect(&source, &target);
    (graph, e)
}

#[test]
fn TestRouteLineReturnsPorts() {
    let (graph, edge) = newRoutingTestGraph(200.0, 0.0);
    let (_, _, cost) = routeLine(&bg(), &graph, &edge, &graph.edges(), &[]).unwrap();
    assert!(cost > 0.0, "route cost = {cost}; want positive cost");
}

#[test]
fn TestRouteLineIgnoresNonOverlappingEdgesWhenCheckingSharedRoutes() {
    let makeGraph = || {
        let graph = layoutgraph::NewGraph();
        let from = graph.AddNode(&nodeAt(1, 100.0, 100.0, 0.0, 0.0));
        let to = graph.AddNode(&nodeAt(2, 100.0, 100.0, 300.0, 0.0));
        let candidate = graph.Connect(&from, &to);
        candidate.bm().TargetArrowhead = layoutgraph::TriangleArrowhead.to_string();

        let sharedTarget = graph.AddNode(&nodeAt(3, 100.0, 100.0, 200.0, 300.0));
        let shareable = graph.Connect(&from, &sharedTarget);
        shareable.bm().TargetArrowhead = layoutgraph::TriangleArrowhead.to_string();
        setPoints(&shareable, &[(100.0, 50.0), (250.0, 50.0), (250.0, 300.0)]);

        let unrelatedFrom = graph.AddNode(&nodeAt(4, 100.0, 100.0, 0.0, 500.0));
        let unrelatedTo = graph.AddNode(&nodeAt(5, 100.0, 100.0, 300.0, 500.0));
        let unrelated = graph.Connect(&unrelatedFrom, &unrelatedTo);
        setPoints(&unrelated, &[(100.0, 550.0), (300.0, 550.0)]);
        (graph, candidate)
    };

    for guarded in [false, true] {
        let (graph, candidate) = makeGraph();
        let (from, to, _) = if guarded {
            let guard = newRouteWorkGuard(&bg(), "route-line overlap test", maxRouteStageWorkUnits).unwrap();
            routeLineGuarded(&graph, &candidate, &graph.edges(), &[], &guard).unwrap()
        } else {
            routeLine(&bg(), &graph, &candidate, &graph.edges(), &[]).unwrap()
        };
        assert_eq!((from.X(), from.Y(), to.X(), to.Y()), (100.0, 50.0, 300.0, 50.0), "guarded={guarded}");
    }
}

#[test]
fn TestStraightEdgeFallbackCanceledBeforeWork() {
    let (graph, edge) = newRoutingTestGraph(200.0, 0.0);
    setPoints(&edge, &[(40.0, 20.0), (200.0, 20.0)]);
    let want = captureExactRouteTest(&edge);
    let (changed, err) = tryStraightEdgeFallback(&canceledContext(), &graph, &edge);
    requireCanceledAt(&err.unwrap(), "EdgeRouting");
    assert!(!changed, "canceled fallback reported a route replacement");
    want.assertRestored();
}

#[test]
fn TestStraightEdgeFallbackPreservesCandidateRejection() {
    let (graph, edge) = newRoutingTestGraph(0.0, 0.0);
    setPoints(&edge, &[(0.0, 20.0), (40.0, 20.0)]);
    let want = captureExactRouteTest(&edge);
    let routeErr = routeLine(&bg(), &graph, &edge, &graph.edges(), &[]).err().unwrap();
    assert!(routeErr.Is(&layoutgraph::ErrInvalidCandidate), "routeLine error = {routeErr}");
    let (changed, err) = tryStraightEdgeFallback(&bg(), &graph, &edge);
    assert!(err.is_none(), "ordinary straight-edge miss returned an error");
    assert!(!changed, "ordinary straight-edge miss reported a route replacement");
    want.assertRestored();
}

#[test]
fn TestGuardedStraightEdgeFallbackChargesAndMatchesOrdinary() {
    let makeGraph = || {
        let (graph, edge) = newRoutingTestGraph(200.0, 0.0);
        setPoints(&edge, &[(40.0, 20.0), (40.0, 100.0), (200.0, 100.0), (200.0, 20.0)]);
        (graph, edge)
    };
    let (ordinaryGraph, ordinaryEdge) = makeGraph();
    let (changed, err) = tryStraightEdgeFallback(&bg(), &ordinaryGraph, &ordinaryEdge);
    assert!(err.is_none() && changed, "ordinary fallback did not report its route replacement");
    let (guardedGraph, guardedEdge) = makeGraph();
    let guard = newRouteWorkGuard(&bg(), "fallback parity", maxRouteStageWorkUnits).unwrap();
    tryStraightEdgeFallbackGuarded(&guardedGraph, &guardedEdge, &guard).unwrap();
    assert!(guard.used() != 0, "guarded fallback did not charge any work");
    assert_eq!(ordinaryEdge.b().Points.len(), 2);
    assert_eq!(values(&ordinaryEdge), values(&guardedEdge));
}

#[test]
fn TestPipelineStraightEdgeFallbackPreservesCancellation() {
    let (graph, edge) = newRoutingTestGraph(200.0, 0.0);
    setPoints(&edge, &[(40.0, 20.0), (200.0, 20.0)]);
    let want = captureExactRouteTest(&edge);
    let err = StraightEdgesFallback(&canceledContext(), &graph).err().unwrap();
    requireCanceledAt(&err, "EdgeRouting");
    want.assertRestored();
}

#[test]
fn TestRouteLineCanceledBeforeWork() {
    let (graph, edge) = newRoutingTestGraph(200.0, 0.0);
    let err = routeLine(&canceledContext(), &graph, &edge, &graph.edges(), &[]).err().unwrap();
    requireCanceledAt(&err, "EdgeRouting");
}

#[test]
fn TestSlingshotCanceledBeforeWork() {
    let (graph, edge) = newRoutingTestGraph(100.0, 100.0);
    let ovg = buildOVGFromGraphWithLimits(&bg(), &graph, &[], defaultOVGBuildLimits()).unwrap();
    let mut router = newOVGEdgeRouterWithWorkLimit(&bg(), Default, &ovg, &graph, &[], &graph.edges(), maxRouteSearchWorkUnits).unwrap();
    let err = router.slingshot(&canceledContext(), &edge).err().unwrap();
    requireCanceledAt(&err, "EdgeRouting");
}
