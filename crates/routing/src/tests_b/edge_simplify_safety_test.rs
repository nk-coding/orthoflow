// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/routing/edge_simplify_safety_test.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of routing/edge_simplify_safety_test.go.
use super::*;

#[test]
fn TestSimplifyChecksBothReplacementLegs() {
    for transpose in [false, true] {
        for obstruction in ["node", "edge", "target approach", "clear"] {
            let g = layoutgraph::NewGraph();
            let from = g.AddNode(&layoutgraph::NewNode(1, 40.0, 40.0));
            from.set_tl(Some(pt(0.0, 0.0)));
            let to = g.AddNode(&layoutgraph::NewNode(2, 40.0, 40.0));
            to.set_tl(Some(pt(240.0, 50.0)));
            let e = g.Connect(&from, &to);
            setPoints(&e, &[(40.0, 20.0), (80.0, 20.0), (80.0, 120.0), (200.0, 120.0), (200.0, 70.0), (240.0, 70.0)]);
            match obstruction {
                "node" => {
                    let n = g.AddNode(&layoutgraph::NewNode(3, 20.0, 20.0));
                    n.set_tl(Some(pt(190.0, 35.0)));
                }
                "edge" => {
                    let a = g.AddNode(&layoutgraph::NewNode(3, 10.0, 10.0));
                    a.set_tl(Some(pt(140.0, 40.0)));
                    let b = g.AddNode(&layoutgraph::NewNode(4, 10.0, 10.0));
                    b.set_tl(Some(pt(210.0, 40.0)));
                    let other = g.Connect(&a, &b);
                    setPoints(&other, &[(150.0, 45.0), (210.0, 45.0)]);
                }
                "target approach" => {
                    to.set_tl(Some(pt(180.0, 30.0)));
                    e.bm().Points.truncate(5);
                }
                _ => {}
            }
            if transpose {
                for n in g.nodes() {
                    let t = n.tlv();
                    n.set_tl(Some(pt(t.Y, t.X)));
                    let (w, h) = (n.w(), n.h());
                    n.set_w(h);
                    n.set_h(w);
                }
                for edge in g.edges() {
                    for p in edge.points() {
                        let v = p.get();
                        p.set(geo::Point { X: v.Y, Y: v.X });
                    }
                }
            }
            let before = captureExactRouteTest(&e);
            SimplifyEdgeRoutes(&bg(), &g).unwrap();
            if obstruction == "clear" {
                assert_eq!(e.b().Points.len(), 4, "clear route retained unnecessary bends: {:?}", values(&e));
            } else {
                before.assertRestored();
            }
        }
    }
}

#[test]
fn TestSimplifyChecksObstacleContainingNewBend() {
    let (g, e) = super::stage_atomicity_test::simplificationMutationGraph();
    let obstacle = g.AddNode(&layoutgraph::NewNode(3, 8.0, 8.0));
    obstacle.set_tl(Some(pt(16.0, -4.0)));
    let before = captureExactRouteTest(&e);
    SimplifyEdgeRoutes(&bg(), &g).unwrap();
    before.assertRestored();
}
