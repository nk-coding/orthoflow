// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from the following files of terrastruct/d2, Copyright 2022 Terrastruct Inc.:
//   - d2layouts/d2talalayout/internal/routing/loop_router_correctness_test.go
//   - d2layouts/d2talalayout/internal/routing/loop_router_test.go
//   - d2layouts/d2talalayout/internal/routing/tree_routing_test.go
// Rust port: Copyright 2026 Niklas Krieger.

//! Ports of routing/tree_routing_test.go, loop_router_test.go and
//! loop_router_correctness_test.go.
use super::*;

fn buildOVG(g: &GraphRef) -> OVGRef {
    buildOVGFromGraphWithLimits(&bg(), g, &[], defaultOVGBuildLimits()).unwrap()
}

#[test]
fn TestTreePathAlignment() {
    let g = layoutgraph::NewGraph();
    let add = |id: i64, w: f64, h: f64, x: f64, y: f64| {
        let n = g.AddNode(&layoutgraph::NewNode(id, w, h));
        n.set_tl(Some(pt(x, y)));
        n
    };
    let n0 = add(3607948159, 191.0, 111.0, 1000.0, 1000.0);
    let n1 = add(873436672, 158.0, 126.0, 919.0, 1211.0);
    let n2 = add(2541194078, 130.0, 126.0, 1127.0, 1211.0);
    n2.SetShape(shape::STORED_DATA_TYPE);
    let n3 = add(4207512678, 158.0, 126.0, 919.0, 1437.0);
    let n4 = add(974102386, 100.0, 100.0, 1142.0, 1450.0);
    let tri = layoutgraph::TriangleArrowhead.to_string();
    for (a, b) in [(&n0, &n1), (&n0, &n2), (&n1, &n3), (&n2, &n4)] {
        g.Connect(a, b).bm().TargetArrowhead = tri.clone();
    }

    for node in g.nodes() {
        g.AddNodeToContainer(None, &node);
    }
    trees::Preprocess(&bg(), &g).unwrap();
    trees::Place(&bg(), &g, None).unwrap();

    let ovg = buildOVG(&g);
    let mut isTreeEdge = g.TreeEdgeMap();
    g.AddIsolatedTreeEdges(&mut isTreeEdge);

    let tree = g.b().NodeToTree.get(&n4).unwrap();
    let route = {
        let o = ovg.b();
        routeSentinelEdge(&tree, &o.Ports, &o.Centers, &gocompat::Map::default()).expect("Error routing tree edge")
    };
    assert_eq!(ovg.b().Ports.get(&n4).map_or(0, |v| v.len()), 13, "Expected 13 port nodes");
    let nodes = route.b().OVGNodes.clone();
    for i in 1..nodes.len() - 3 {
        assert_eq!(nodes[i].P().X, nodes[i + 1].P().X, "Expected tree route to be a vertical line, but there is a bend at {i}");
    }
}

#[test]
fn TestNoLoops() {
    let g = layoutgraph::NewGraph();
    let n1 = g.AddNode(&layoutgraph::NewNode(1, 100.0, 100.0));
    n1.set_tl(Some(pt(0.0, 0.0)));
    let n2 = g.AddNode(&layoutgraph::NewNode(2, 100.0, 100.0));
    n2.set_tl(Some(pt(200.0, 200.0)));
    g.Connect(&n1, &n2);
    assert_eq!(loops::Route(&n1).len(), 0);
}

fn portPoint(ovg: &OVGRef, n: &NodeRef, o: geo::Orientation, i: usize) -> geo::Point {
    let index = n.shape().PortIndices(o)[i] as usize;
    ovg.b().Ports.get(n).unwrap()[index].P()
}

#[test]
fn TestRouteLoop() {
    let g = layoutgraph::NewGraph();
    let n1 = g.AddNode(&layoutgraph::NewNode(1, 100.0, 100.0));
    n1.set_tl(Some(pt(0.0, 0.0)));
    g.Connect(&n1, &n1);

    let ovg = buildOVG(&g);
    let edges = loops::Route(&n1);
    assert_eq!(edges.len(), 1);

    let route = makeLoopRoute(&edges[0], &ovg.b());
    assert!(route.b().GEdge == g.edges()[0]);
    assert_eq!(route.b().OVGNodes.len(), 7);
    assert_eq!(portPoint(&ovg, &n1, geo::Left, 0), route.b().FromPort);
    assert_eq!(portPoint(&ovg, &n1, geo::Top, 0), route.b().ToPort);
}

#[test]
fn TestRouteLoopsWithDifferentArrowheads() {
    let g = layoutgraph::NewGraph();
    let n1 = g.AddNode(&layoutgraph::NewNode(1, 100.0, 100.0));
    n1.set_tl(Some(pt(0.0, 0.0)));
    let (none, tri) = (layoutgraph::NoArrowhead.to_string(), layoutgraph::TriangleArrowhead.to_string());
    for (s, t) in [(&none, &none), (&tri, &tri), (&tri, &none)] {
        let e = g.Connect(&n1, &n1);
        e.bm().SourceArrowhead = s.clone();
        e.bm().TargetArrowhead = t.clone();
    }

    let ovg = buildOVG(&g);
    let edges = loops::Route(&n1);

    let route = makeLoopRoute(&edges[0], &ovg.b());
    assert_eq!(portPoint(&ovg, &n1, geo::Left, 0), route.b().FromPort);
    assert_eq!(portPoint(&ovg, &n1, geo::Top, 0), route.b().ToPort);

    let route = makeLoopRoute(&edges[1], &ovg.b());
    assert_eq!(portPoint(&ovg, &n1, geo::Top, 2), route.b().FromPort);
    assert_eq!(portPoint(&ovg, &n1, geo::Right, 0), route.b().ToPort);

    let route = makeLoopRoute(&edges[2], &ovg.b());
    assert_eq!(portPoint(&ovg, &n1, geo::Bottom, 0), route.b().FromPort);
    assert_eq!(portPoint(&ovg, &n1, geo::Left, 2), route.b().ToPort);
}

#[test]
fn TestRouteLoopsSortedByArea() {
    let g = layoutgraph::NewGraph();
    let n1 = g.AddNode(&layoutgraph::NewNode(1, 100.0, 100.0));
    n1.set_tl(Some(pt(0.0, 0.0)));
    let (none, tri) = (layoutgraph::NoArrowhead.to_string(), layoutgraph::TriangleArrowhead.to_string());
    let mut es = Vec::new();
    for (s, t) in [(&none, &none), (&tri, &tri), (&tri, &none), (&tri, &none), (&tri, &tri)] {
        let e = g.Connect(&n1, &n1);
        e.bm().SourceArrowhead = s.clone();
        e.bm().TargetArrowhead = t.clone();
        es.push(e);
    }
    es[4].bm().Label = Some(layoutgraph::LabelRef::new(layoutgraph::Label::new("", label::OutsideBottomRight, 100.0, 100.0)));

    loops::Route(&n1);

    let area = |e: &EdgeRef| {
        let (tl, br) = e.BoundingBoxValues();
        (br.X - tl.X) * (br.Y - tl.Y)
    };
    assert!(area(&es[3]) > area(&es[1]));
}

#[test]
fn TestRouteLoopsSupportsAllFourArrowCategories() {
    let g = layoutgraph::NewGraph();
    let node = g.AddNode(&layoutgraph::NewNode(1, 100.0, 100.0));
    node.set_tl(Some(pt(0.0, 0.0)));
    for (source, target) in [(true, false), (false, true), (true, true), (false, false)] {
        let edge = g.Connect(&node, &node);
        if source {
            edge.bm().SourceArrowhead = layoutgraph::TriangleArrowhead.to_string();
        }
        if target {
            edge.bm().TargetArrowhead = layoutgraph::TriangleArrowhead.to_string();
        }
    }

    let routed = loops::Route(&node);
    assert_eq!(routed.len(), 4, "expected four routed loops");
    let mut portPairs: gocompat::Set<[u64; 4]> = gocompat::Set::default();
    for edge in &routed {
        let points = pointValues(&edge.b().Points);
        assert_eq!(points.len(), 5, "loop {} has {} route points", edge.IDValue(), points.len());
        let last = points[points.len() - 1];
        portPairs.insert([points[0].X.to_bits(), points[0].Y.to_bits(), last.X.to_bits(), last.Y.to_bits()]);
    }
    assert_eq!(portPairs.len(), 4, "expected four distinct loop port pairs");
}
