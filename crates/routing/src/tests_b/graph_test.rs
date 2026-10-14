// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/routing/graph_test.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of routing/graph_test.go.
use super::*;

#[test]
fn TestRouteEdges() {
    let graph = layoutgraph::NewGraph();
    let a = nodeAt(1, 8.0, 8.0, 4.0, 4.0);
    let b = nodeAt(2, 8.0, 8.0, 16.0, 16.0);
    let c = nodeAt(3, 8.0, 8.0, 2.0, 18.0);
    let d = nodeAt(4, 12.0, 8.0, 20.0, 6.0);
    let e = nodeAt(5, 4.0, 16.0, 26.0, 16.0);
    for node in [&a, &b, &c, &d, &e] {
        graph.AddNode(node);
    }

    graph.Connect(&a, &b);
    graph.Connect(&a, &c);
    graph.Connect(&a, &d);
    graph.Connect(&b, &c);
    graph.Connect(&b, &e);
    graph.Connect(&a, &e);

    RouteEdges(&bg(), &graph, &[]).unwrap();
}

#[test]
fn TestHasSharpAngleToBorder() {
    let p = |x: f64, y: f64| geo::Point { X: x, Y: y };
    assert!(!hasSharpAngleToBorder(&p(0.0, 0.0), &p(20.0, 20.0), geo::Right), "45-degree approach to right border reported sharp");
    assert!(
        hasSharpAngleToBorder(&p(0.0, 10.0), &p(20.0, 0.0), geo::Top),
        "26.6-degree left-to-right approach to top border was not reported sharp"
    );
    assert!(
        hasSharpAngleToBorder(&p(20.0, 10.0), &p(0.0, 0.0), geo::Top),
        "26.6-degree right-to-left approach to top border was not reported sharp"
    );
}
