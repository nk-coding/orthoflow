// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/routing/standalone_router_test.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of routing/standalone_router_test.go (custom-context and custom-shape
//! panic probes are not ported).
use super::*;

#[test]
fn TestRouteEdgesRejectsUnroutedFixedEdges() {
    let graph = layoutgraph::NewGraph();
    let a = graph.AddNode(&nodeAt(1, 10.0, 10.0, 0.0, 0.0));
    let b = graph.AddNode(&nodeAt(2, 10.0, 10.0, 100.0, 0.0));
    let c = graph.AddNode(&nodeAt(3, 10.0, 10.0, 200.0, 0.0));
    let selected = graph.Connect(&a, &b);
    let unroutedFixed = graph.Connect(&b, &c);

    let err = RouteEdges(&bg(), &graph, &[selected]).err().unwrap();
    assert!(err.Error().contains("unselected edge"), "error = {err}; want unrouted fixed-edge validation");
    assert!(unroutedFixed.b().Points.is_empty(), "validation mutated the unselected edge");
}

#[test]
fn TestRouteEdgesPreservesUnselectedRoutes() {
    let graph = layoutgraph::NewGraph();
    let a = graph.AddNode(&nodeAt(1, 40.0, 40.0, 0.0, 0.0));
    let b = graph.AddNode(&nodeAt(2, 40.0, 40.0, 200.0, 0.0));
    let c = graph.AddNode(&nodeAt(3, 40.0, 40.0, 100.0, 120.0));

    let selected = graph.Connect(&a, &b);
    let unselected = graph.Connect(&a, &c);
    setPoints(&unselected, &[(40.0, 20.0), (70.0, 20.0), (70.0, 140.0), (100.0, 140.0)]);
    let snapshot = captureExactRouteTest(&unselected);

    RouteEdges(&bg(), &graph, &[selected.clone()]).unwrap();
    snapshot.assertRestored();
    assert!(selected.b().Points.len() >= 2);
}

fn newInjectedRouteNodeIndexGraph() -> (GraphRef, EdgeRef, NodeRef) {
    let graph = layoutgraph::NewGraph();
    let addNode = |id: i64, x: f64, y: f64, width: f64, height: f64, shapeType: &str| {
        let node = graph.AddNode(&layoutgraph::NewNode(id, width, height));
        node.set_tl(Some(pt(x, y)));
        node.SetShape(shapeType);
        node
    };
    let n4 = addNode(4, 782.0, 21.0, 62.0, 61.0, shape::REAL_SQUARE_TYPE);
    addNode(5, 0.0, 287.0, 96.0, 110.0, shape::PACKAGE_TYPE);
    let n6 = addNode(6, 265.0, 261.0, 88.0, 87.0, shape::CYLINDER_TYPE);
    let n7 = addNode(7, 550.0, 269.0, 96.0, 82.0, shape::PACKAGE_TYPE);
    let n8 = addNode(8, 808.0, 264.0, 60.0, 64.0, shape::PERSON_TYPE);

    let fixedReverse = graph.Connect(&n8, &n6);
    setPoints(&fixedReverse, &[(838.0, 328.0), (838.0, 414.0), (309.0, 414.0), (309.0, 348.0)]);
    let fixedOther = graph.Connect(&n7, &n4);
    setPoints(&fixedOther, &[(598.0, 269.0), (598.0, 116.0), (813.0, 116.0), (813.0, 82.0)]);
    let selected = graph.Connect(&n6, &n8);
    (graph, selected, n7)
}

#[track_caller]
fn requireRouteAvoidsNode(edge: &EdgeRef, node: &NodeRef) {
    let points = edge.points();
    for index in 1..points.len() {
        assert!(
            !node.PassesThrough(Some(&points[index - 1]), Some(&points[index])),
            "route segment {:?} -> {:?} passes through unrelated node {}",
            points[index - 1].get(),
            points[index].get(),
            node.DebugID()
        );
    }
}

#[test]
fn TestRouteAdditionalEdgesIndexesInjectedRouteNodes() {
    let (graph, selected, unrelated) = newInjectedRouteNodeIndexGraph();
    graph.ComputeCellSize();
    let ovg = routeAdditionalEdgesWithLimits(&bg(), &graph, &[selected.clone()], defaultOVGBuildLimits()).unwrap();
    for (index, node) in ovg.b().Nodes.iter().enumerate() {
        assert_eq!(node.b().Index, index, "OVG node {index} at {:?} has aliased index", node.P());
    }
    requireRouteAvoidsNode(&selected, &unrelated);
}

#[test]
fn TestRouteEdgesDoesNotAliasInjectedOVGNodes() {
    let (graph, selected, unrelated) = newInjectedRouteNodeIndexGraph();
    RouteEdges(&bg(), &graph, &[selected.clone()]).unwrap();
    requireRouteAvoidsNode(&selected, &unrelated);
}

#[test]
fn TestRouteEdgesValidatesGraphAndSelection() {
    RouteEdges(&bg(), &layoutgraph::NewGraph(), &[]).unwrap();
    let graph = layoutgraph::NewGraph();
    let a = graph.AddNode(&layoutgraph::NewNode(1, 10.0, 10.0));
    let b = graph.AddNode(&layoutgraph::NewNode(2, 10.0, 10.0));
    let edge = graph.Connect(&a, &b);
    RouteEdges(&bg(), &graph, &[]).unwrap();
    assert!(edge.b().Points.is_empty(), "empty selection mutated an edge");
}

#[test]
fn TestRouteEdgesCapsSelectionAtGraphInventory() {
    let graph = layoutgraph::NewGraph();
    let from = graph.AddNode(&nodeAt(1, 10.0, 10.0, 0.0, 0.0));
    let to = graph.AddNode(&nodeAt(2, 10.0, 10.0, 100.0, 0.0));
    let edge = graph.Connect(&from, &to);
    let err = RouteEdges(&bg(), &graph, &[edge.clone(), edge]).err().unwrap();
    assert!(err.Error().contains("exceeds graph edge count"), "selection error = {err}");
}

#[test]
fn TestRouteEdgesCancellationRestoresTentativeRoute() {
    let graph = layoutgraph::NewGraph();
    let a = graph.AddNode(&nodeAt(1, 10.0, 10.0, 0.0, 0.0));
    let b = graph.AddNode(&nodeAt(2, 10.0, 10.0, 100.0, 0.0));
    let edge = graph.Connect(&a, &b);
    let snapshot = captureExactRouteTest(&edge);
    let err = RouteEdges(&canceledContext(), &graph, &[edge]).err().unwrap();
    requireCanceled(&err);
    snapshot.assertRestored();
}

/// Go stops at the first failing limit after an observed mutation; the port
/// checks exact restoration for every failing limit up to success.
#[test]
fn TestRouteEdgesWorkLimitRestoresTentativeRoute() {
    for limit in 1..=10_000u64 {
        let graph = layoutgraph::NewGraph();
        let from = graph.AddNode(&nodeAt(1, 10.0, 10.0, 0.0, 0.0));
        let to = graph.AddNode(&nodeAt(2, 10.0, 10.0, 100.0, 0.0));
        let edge = graph.Connect(&from, &to);
        let snapshot = captureExactRouteTest(&edge);
        let cellSize = graph.b().CellSize;
        match routeEdgesWithBudgets(&bg(), &graph, &[edge.clone()], defaultOVGBuildLimits(), limit) {
            Ok(()) => {
                assert!(edge.b().Points.len() >= 2);
                return;
            }
            Err(err) => {
                assert!(err.Is(&errRouteStageWorkLimit), "limit {limit}: {err}");
                snapshot.assertRestored();
                let costs = graph.RoutingCosts();
                assert!(
                    graph.b().CellSize == cellSize && costs.Crossing == 0.0 && costs.Turn == 0.0 && costs.NonCenterPort == 0.0,
                    "standalone graph state was not restored"
                );
            }
        }
    }
    panic!("no successful standalone work limit");
}

#[test]
fn TestRouteEdgesRejectsMalformedGraphWithoutPanicking() {
    let placed = |id: i64, x: f64| nodeAt(id, 10.0, 10.0, x, 0.0);
    // empty graph
    let err = RouteEdges(&bg(), &layoutgraph::NewGraph(), &[layoutgraph::NewEdge(&placed(1, 0.0), &placed(2, 100.0))]).err().unwrap();
    assert!(err.Error().contains("empty graph"), "{err}");
    // source endpoint not in graph
    let g = layoutgraph::NewGraph();
    let outside = placed(1, 0.0);
    let inside = g.AddNode(&placed(2, 100.0));
    let edge = layoutgraph::NewEdge(&outside, &inside);
    g.bm().Edges.push(edge.clone());
    let err = RouteEdges(&bg(), &g, &[edge]).err().unwrap();
    assert!(err.Error().contains("source node does not belong"), "{err}");
    // target endpoint not in graph
    let g = layoutgraph::NewGraph();
    let inside = g.AddNode(&placed(1, 0.0));
    let outside = placed(2, 100.0);
    let edge = layoutgraph::NewEdge(&inside, &outside);
    g.bm().Edges.push(edge.clone());
    let err = RouteEdges(&bg(), &g, &[edge]).err().unwrap();
    assert!(err.Error().contains("target node does not belong"), "{err}");
}
