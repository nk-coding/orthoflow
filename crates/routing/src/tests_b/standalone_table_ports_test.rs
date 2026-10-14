// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/routing/standalone_table_ports_test.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of routing/standalone_table_ports_test.go.
use super::*;

fn newStandaloneTableEdgeTestGraph() -> (GraphRef, EdgeRef) {
    let graph = layoutgraph::NewGraph();
    let from = graph.AddNode(&layoutgraph::NewNode(1, 100.0, 100.0));
    let to = graph.AddNode(&layoutgraph::NewNode(2, 100.0, 100.0));
    from.set_tl(Some(pt(0.0, 0.0)));
    to.set_tl(Some(pt(300.0, 0.0)));
    from.SetShape(shape::TABLE_TYPE);
    to.SetShape(shape::TABLE_TYPE);
    from.SetNumColumns(3);
    to.SetNumColumns(3);
    let edge = graph.Connect(&from, &to);
    edge.bm().FromTableColumnIndex = Some(0);
    edge.bm().ToTableColumnIndex = Some(2);
    (graph, edge)
}

#[test]
fn TestRouteEdgesKeepsSelectedTableEdgeOnRowPorts() {
    let (graph, edge) = newStandaloneTableEdgeTestGraph();
    let (wantFrom, wantTo, hasFrom, hasTo, _) = edge.FacingTablePortValues(None, None);
    assert!(hasFrom && hasTo, "table edge did not expose both row ports");

    let fixedTo = graph.AddNode(&layoutgraph::NewNode(3, 40.0, 40.0));
    fixedTo.set_tl(Some(pt(150.0, 200.0)));
    let fixed = graph.Connect(&edge.from(), &fixedTo);
    setPoints(&fixed, &[(100.0, 50.0), (120.0, 50.0), (120.0, 180.0), (170.0, 180.0), (170.0, 200.0)]);
    let fixedSnapshot = captureExactRouteTest(&fixed);

    let assertRoute = || {
        let points = pointValues(&edge.b().Points);
        assert!(points.len() >= 2, "selected table edge has {} route points", points.len());
        assert_eq!(points[0], wantFrom, "source port");
        assert_eq!(points[points.len() - 1], wantTo, "target port");
        fixedSnapshot.assertRestored();
    };

    RouteEdges(&bg(), &graph, &[edge.clone()]).unwrap();
    assertRoute();
    let firstRoute = values(&edge);
    RouteEdges(&bg(), &graph, &[edge.clone()]).unwrap();
    assertRoute();
    assert_eq!(values(&edge), firstRoute, "repeated table routing changed the route");
}

#[test]
fn TestRouteEdgesCancellationRestoresTentativeTableRoute() {
    let (graph, edge) = newStandaloneTableEdgeTestGraph();
    let snapshot = captureExactRouteTest(&edge);
    let err = RouteEdges(&canceledContext(), &graph, &[edge]).err().unwrap();
    requireCanceled(&err);
    snapshot.assertRestored();
}

#[test]
fn TestRouteEdgesTableWorkLimitIsExactAndAtomic() {
    let run = |limit: u64| {
        let (graph, edge) = newStandaloneTableEdgeTestGraph();
        let result = routeEdgesWithBudgets(&bg(), &graph, &[edge.clone()], defaultOVGBuildLimits(), limit);
        (graph, edge, result)
    };
    let mut minimum = 1u64;
    loop {
        let (_, _, result) = run(minimum);
        match result {
            Ok(()) => break,
            Err(err) => assert!(err.Is(&errRouteStageWorkLimit), "work limit {minimum} returned {err}"),
        }
        minimum *= 2;
        assert!(minimum <= 1 << 20, "could not find a successful standalone table-edge work limit");
    }
    let (mut low, mut high) = (minimum / 2, minimum);
    while low + 1 < high {
        let mid = low + (high - low) / 2;
        let (_, _, result) = run(mid);
        match result {
            Ok(()) => high = mid,
            Err(err) => {
                assert!(err.Is(&errRouteStageWorkLimit), "work limit {mid} returned {err}");
                low = mid;
            }
        }
    }

    let (_, edge, result) = run(high);
    result.unwrap();
    let (wantFrom, wantTo, hasFrom, hasTo, _) = edge.FacingTablePortValues(None, None);
    assert!(hasFrom && hasTo);
    let points = pointValues(&edge.b().Points);
    assert_eq!(points[0], wantFrom);
    assert_eq!(points[points.len() - 1], wantTo);

    let (graph, edge) = newStandaloneTableEdgeTestGraph();
    let snapshot = captureExactRouteTest(&edge);
    let cellSize = graph.b().CellSize;
    let err = routeEdgesWithBudgets(&bg(), &graph, &[edge.clone()], defaultOVGBuildLimits(), high - 1).err().unwrap();
    assert!(err.Is(&errRouteStageWorkLimit), "one-unit-short table-edge error = {err}");
    snapshot.assertRestored();
    let costs = graph.RoutingCosts();
    assert!(graph.b().CellSize == cellSize && costs.Crossing == 0.0 && costs.Turn == 0.0 && costs.NonCenterPort == 0.0);
}
