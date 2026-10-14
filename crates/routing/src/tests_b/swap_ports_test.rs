// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/routing/swap_ports_test.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of routing/swap_ports_test.go.
use super::stage_atomicity_test::swapEdgePorts;
use super::*;

#[track_caller]
fn assertPoints(e: &EdgeRef, expected: &[geo::Point]) {
    for (i, want) in expected.iter().enumerate() {
        assert!(want.Equals(&p(e, i)), "expected {:?} at Points[{i}], got {:?}", want, p(e, i));
    }
}

fn P(x: f64, y: f64) -> geo::Point {
    geo::Point { X: x, Y: y }
}

#[test]
fn TestSwapEdgePorts() {
    let g = layoutgraph::NewGraph();
    let n1 = g.AddNode(&layoutgraph::NewNode(1, 200.0, 100.0));
    n1.set_tl(Some(pt(0.0, 150.0)));
    let n2 = g.AddNode(&layoutgraph::NewNode(2, 50.0, 50.0));
    n2.set_tl(Some(pt(200.0, 0.0)));
    let n3 = g.AddNode(&layoutgraph::NewNode(3, 50.0, 50.0));
    n3.set_tl(Some(pt(100.0, 175.0)));
    let (t2, t3) = (n2.tlv(), n3.tlv());
    let c2 = n2.Center().get();

    let e1 = g.Connect(&n3, &n2);
    setPoints(&e1, &[((t3.X + 0.75 * n3.w()).round(), t3.Y), ((t3.X + 0.75 * n3.w()).round(), c2.Y), (t2.X, c2.Y)]);
    let e2 = g.Connect(&n2, &n3);
    setPoints(
        &e2,
        &[
            (c2.X, t2.Y + n2.h()),
            (c2.X, t2.Y + n2.h() + 25.0),
            ((t3.X + 0.25 * n3.w()).round(), t2.Y + n2.h() + 25.0),
            ((t3.X + 0.25 * n3.w()).round(), t3.Y),
        ],
    );

    g.AddNodeToContainer(None, &n1);
    g.AddNodeToContainer(None, &n2);
    g.AddNodeToContainer(Some(&n1), &n3);

    swapEdgePorts(&bg(), &n3).unwrap();

    assertPoints(&e1, &[P((t3.X + 0.25 * n3.w()).round(), t3.Y), P((t3.X + 0.25 * n3.w()).round(), c2.Y), P(t2.X, c2.Y)]);
    assertPoints(
        &e2,
        &[
            P(c2.X, t2.Y + n2.h()),
            P(c2.X, t2.Y + n2.h() + 25.0),
            P((t3.X + 0.75 * n3.w()).round(), t2.Y + n2.h() + 25.0),
            P((t3.X + 0.75 * n3.w()).round(), t3.Y),
        ],
    );
}

#[test]
fn TestInvalidSwapNodeOverlap() {
    let g = layoutgraph::NewGraph();
    let n1 = g.AddNode(&layoutgraph::NewNode(1, 100.0, 50.0));
    n1.set_tl(Some(pt(50.0, 0.0)));
    let n2 = g.AddNode(&layoutgraph::NewNode(2, 50.0, 50.0));
    n2.set_tl(Some(pt(200.0, 0.0)));
    let n3 = g.AddNode(&layoutgraph::NewNode(2, 200.0, 100.0));
    n3.set_tl(Some(pt(0.0, 150.0)));
    let (t1, t2, t3) = (n1.tlv(), n2.tlv(), n3.tlv());
    let c2 = n2.Center().get();

    let e1 = g.Connect(&n3, &n2);
    setPoints(&e1, &[(175.0, t3.Y), (175.0, c2.Y), (t2.X, c2.Y)]);
    let e2 = g.Connect(&n2, &n3);
    setPoints(&e2, &[(225.0, t2.Y + n2.h()), (225.0, t2.Y + n2.h() + 25.0), (75.0, t2.Y + n2.h() + 25.0), (75.0, t3.Y)]);
    let e3 = g.Connect(&n3, &n1);
    setPoints(&e3, &[(125.0, t3.Y), (125.0, t1.Y + n1.h())]);

    let (x1, x2, x3) = (pointValues(&e1.b().Points), pointValues(&e2.b().Points), pointValues(&e3.b().Points));
    swapEdgePorts(&bg(), &n3).unwrap();
    assertPoints(&e1, &x1);
    assertPoints(&e2, &x2);
    assertPoints(&e3, &x3);
}

#[test]
fn TestInvalidSwapArrowhead() {
    let g = layoutgraph::NewGraph();
    let n1 = g.AddNode(&layoutgraph::NewNode(1, 50.0, 50.0));
    n1.set_tl(Some(pt(10.0, 0.0)));
    let n2 = g.AddNode(&layoutgraph::NewNode(2, 50.0, 50.0));
    n2.set_tl(Some(pt(200.0, 0.0)));
    let n3 = g.AddNode(&layoutgraph::NewNode(2, 200.0, 100.0));
    n3.set_tl(Some(pt(0.0, 150.0)));
    let (t1, t2, t3) = (n1.tlv(), n2.tlv(), n3.tlv());
    let (c1, c2) = (n1.Center().get(), n2.Center().get());
    let (none, tri) = (layoutgraph::NoArrowhead.to_string(), layoutgraph::TriangleArrowhead.to_string());

    let e1 = g.Connect(&n3, &n2);
    e1.bm().SourceArrowhead = none.clone();
    e1.bm().TargetArrowhead = tri.clone();
    setPoints(&e1, &[(175.0, t3.Y), (175.0, c2.Y), (t2.X, c2.Y)]);
    let e2 = g.Connect(&n2, &n3);
    e2.bm().SourceArrowhead = none.clone();
    e2.bm().TargetArrowhead = tri.clone();
    setPoints(&e2, &[(225.0, t2.Y + n2.h()), (225.0, t2.Y + n2.h() + 25.0), (125.0, t2.Y + n2.h() + 25.0), (125.0, t3.Y)]);
    let e3 = g.Connect(&n3, &n1);
    e3.bm().SourceArrowhead = tri.clone();
    e3.bm().TargetArrowhead = tri.clone();
    setPoints(&e3, &[(125.0, t3.Y), (125.0, c1.Y), (t1.X + n1.w(), c1.Y)]);

    let (x1, x2, x3) = (pointValues(&e1.b().Points), pointValues(&e2.b().Points), pointValues(&e3.b().Points));
    swapEdgePorts(&bg(), &n3).unwrap();
    assertPoints(&e1, &x1);
    assertPoints(&e2, &x2);
    assertPoints(&e3, &x3);
}
