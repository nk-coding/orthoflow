// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/routing/slingshot_test.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of routing/slingshot_test.go.
use super::*;

#[test]
fn TestPreferLaunchingVertically() {
    let graph = layoutgraph::NewGraph();
    let a = nodeAt(1, 5.0, 5.0, 10.0, 10.0);
    let b = nodeAt(2, 5.0, 5.0, 20.0, 20.0);
    graph.AddNode(&a);
    graph.AddNode(&b);

    let (_, strong) = preferLaunchingVertically(&a, &b, geo::TopLeft);
    assert!(!strong);

    graph.Connect(&a, &b);

    let c = nodeAt(3, 5.0, 5.0, 10.0, 20.0);
    graph.AddNode(&c);
    graph.Connect(&a, &c);

    let (pref, strong) = preferLaunchingVertically(&a, &b, geo::TopLeft);
    assert!(!pref);
    assert!(strong);

    c.set_tl(Some(pt(20.0, 10.0)));
    let (pref, strong) = preferLaunchingVertically(&a, &b, geo::TopLeft);
    assert!(pref);
    assert!(strong);
}

fn newRouter(graph: &GraphRef) -> ovgEdgeRouter {
    let ovg = buildOVGFromGraphWithLimits(&bg(), graph, &[], defaultOVGBuildLimits()).unwrap();
    newOVGEdgeRouterWithWorkLimit(&bg(), Default, &ovg, graph, &[], &graph.edges(), maxRouteSearchWorkUnits).unwrap()
}

#[test]
fn TestFillPath() {
    let graph = layoutgraph::NewGraph();
    let a = nodeAt(1, 4.0, 4.0, 10.0, 10.0);
    let b = nodeAt(2, 4.0, 4.0, 20.0, 20.0);
    graph.AddNode(&a);
    graph.AddNode(&b);

    let mut ovg = NewOVG(graph.nodes());
    let mut add = |x: f64, y: f64| ovg.AddNode(&NewOVGNode(geo::Point { X: x, Y: y }));
    let aCenter = add(12.0, 12.0);
    let aPort = add(12.0, 14.0);
    let inPathNode = add(12.0, 16.0);
    let notInPathNode = add(10.0, 16.0);
    let bPort = add(20.0, 22.0);
    let bCenter = add(22.0, 22.0);
    let anchor = add(12.0, 22.0);

    ovg.Connect(&aCenter, &aPort);
    ovg.Connect(&aPort, &inPathNode);
    ovg.Connect(&aPort, &notInPathNode);
    ovg.Connect(&inPathNode, &anchor);
    ovg.Connect(&anchor, &bPort);
    ovg.Connect(&bPort, &bCenter);

    // Go builds `&ovgEdgeRouter{work: guard}`; fillPathGuarded only uses the guard.
    graph.Connect(&a, &b);
    let mut router = newRouter(&graph);
    let (got, ok) = router.fillPathGuarded(&aPort, &anchor).unwrap();
    assert!(ok);
    let exp = [aPort.clone(), inPathNode.clone()];
    assert_eq!(exp.len(), got.len());
    for i in 0..exp.len() {
        assert_eq!(exp[i].P(), got[i].P());
    }
}

#[test]
fn TestSlingshot() {
    let graph = layoutgraph::NewGraph();
    let a = nodeAt(1, 40.0, 40.0, 0.0, 0.0);
    let b = nodeAt(2, 40.0, 40.0, 100.0, 100.0);
    graph.AddNode(&a);
    graph.AddNode(&b);
    graph.Connect(&a, &b);
    let edge = graph.edges()[0].clone();

    // Works in isolation
    let (_, distance) = newRouter(&graph).slingshot(&bg(), &edge).unwrap();
    assert!(distance > 0.0);

    // Works when a node blocks one side
    let c = nodeAt(3, 10.0, 40.0, 80.0, 0.0);
    graph.AddNode(&c);
    let (_, distance2) = newRouter(&graph).slingshot(&bg(), &edge).unwrap();
    assert_eq!(distance, distance2);

    // Goes to s-shape when both sides are blocked
    let d = nodeAt(4, 10.0, 40.0, 0.0, 80.0);
    graph.AddNode(&d);
    let (_, distance3) = newRouter(&graph).slingshot(&bg(), &edge).unwrap();
    // s-shapes cost more
    assert!(distance3 > distance);
    assert!(distance3 > distance2);

    // block all slingshot routes
    let e = nodeAt(5, 1000.0, 10.0, 0.0, 20.0);
    graph.AddNode(&e);
    let (_, distance4) = newRouter(&graph).slingshot(&bg(), &edge).unwrap();
    assert_eq!(f64::INFINITY, distance4);
}
