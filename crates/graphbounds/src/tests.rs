// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/graphbounds/bounds_test.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of graphbounds/bounds_test.go (guarded vs legacy layoutgraph geometry).
use crate::*;
use layoutgraph::{IconRef, LabelRef, NewNode};

fn guard() -> limits::WorkGuardRef {
    limits::NewWorkGuard(&gocompat::context::Background(), "GraphBoundsParity", 1_000_000_000).unwrap()
}

fn same(a: &geo::PointRef, b: &geo::PointRef) {
    let (a, b) = (*a.b(), *b.b());
    assert!(a.X == b.X && a.Y == b.Y, "{:?} != {:?}", a, b);
}

fn assertGeometryParity(nodes: &[NodeRef]) {
    let g = guard();
    for node in nodes {
        let (wtl, wbr) = node.BoundingBox(Some(nodes));
        let (gtl, gbr) = NodeBoundingBox(Some(node), Some(nodes), &g).unwrap();
        same(&gtl, &wtl);
        same(&gbr, &wbr);
    }
    let (wtl, wbr) = nodes.BoundingBox();
    let (gtl, gbr) = BoundingBox(nodes, &g).unwrap();
    same(&gtl, &wtl.unwrap());
    same(&gbr, &wbr.unwrap());
    let (wtl, wbr) = nodes.FixedBoundingBox();
    let (gtl, gbr) = FixedBoundingBox(nodes, &g).unwrap();
    same(&gtl, &wtl.unwrap());
    same(&gbr, &wbr.unwrap());
}

#[test]
fn TestGuardedGeometryMatchesLegacyBranches() {
    let n1 = NewNode(1, 31.75, 42.25);
    n1.set_tl(Some(geo::NewPoint(17.5, -11.25)));
    for (o, v) in [(geo::Top, 3.5), (geo::Bottom, 4.25), (geo::Left, 5.75), (geo::Right, 6.5)] {
        n1.bm().LoopOffsets.insert(o, v);
    }
    n1.bm().IsMultiple = true;
    let n2 = NewNode(2, 52.5, 27.75);
    n2.set_tl(Some(geo::NewPoint(89.25, 63.5)));
    n2.SetShape(shape::HEXAGON_TYPE);
    n2.bm().Is3D = true;
    assertGeometryParity(&[n1, n2]);

    for p in 1..=12i8 {
        let position = label::Position(p);
        let target = NewNode(1, 40., 30.);
        target.set_tl(Some(geo::NewPoint(100., 100.)));
        target.bm().Label = Some(LabelRef::new(layoutgraph::Label::new("", position, 83., 19.)));
        target.bm().Icon = Some(IconRef::new(layoutgraph::Icon::new(position)));
        let more = NewNode(2, 20., 20.);
        more.set_tl(Some(geo::NewPoint(20., 20.)));
        assertGeometryParity(&[target.clone(), more]);
        assertGeometryParity(&[target]);
    }

    let container = NewNode(10, 500., 400.);
    container.set_tl(Some(geo::NewPoint(0., 0.)));
    let a = NewNode(1, 40., 30.);
    a.set_tl(Some(geo::NewPoint(80., 30.)));
    a.bm().FixedTopLeft = Some(geo::NewPoint(70., 20.));
    a.bm().Container = Some(container.clone());
    let b = NewNode(2, 35., 50.);
    b.set_tl(Some(geo::NewPoint(150., 90.)));
    b.bm().Container = Some(container.clone());
    assertGeometryParity(&[a, b]);
}

#[test]
fn TestGuardedGeometryRandomizedLegacyParity() {
    let rr = gocompat::rand::New(gocompat::rand::NewSource(7));
    let mut r = rr.bm();
    for _ in 0..200 {
        let count = 1 + r.Intn(5);
        let mut nodes = Vec::new();
        for i in 0..count {
            let n = NewNode(i + 1, 1. + r.Float64() * 80., 1. + r.Float64() * 80.);
            n.set_tl(Some(geo::NewPoint(r.Float64() * 400. - 200., r.Float64() * 400. - 200.)));
            if r.Intn(2) == 0 {
                let p = label::Position(1 + r.Intn(21) as i8);
                n.bm().Label = Some(LabelRef::new(layoutgraph::Label::new("x", p, r.Float64() * 100., r.Float64() * 30.)));
            }
            if r.Intn(3) == 0 {
                n.bm().Icon = Some(IconRef::new(layoutgraph::Icon::new(label::Position(1 + r.Intn(21) as i8))));
            }
            n.bm().Is3D = r.Intn(4) == 0;
            nodes.push(n);
        }
        assertGeometryParity(&nodes);
    }
}
