// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/routing/balance_order_test.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of routing/balance_order_test.go.
use super::*;
use gocompat::{Map, Set};
use layoutgraph::EdgeSegmentRef;

// This is the geometry entering balancing in the public one_container_loop
// example.
fn containerBalanceOrderGraph() -> (GraphRef, EdgeRef, EdgeRef) {
    let g = layoutgraph::NewGraph();
    let mut nodes: Map<&'static str, NodeRef> = Map::default();
    let specs: [(&str, f64, f64, f64, f64); 8] = [
        ("a", 47.0, 0.0, 393.0, 186.0),
        ("b", 327.0, 60.0, 53.0, 66.0),
        ("c", 194.0, 744.0, 53.0, 66.0),
        ("d", 193.0, 588.0, 54.0, 66.0),
        ("e", 0.0, 276.0, 53.0, 66.0),
        ("f", 195.0, 432.0, 51.0, 66.0),
        ("g", 193.0, 276.0, 54.0, 66.0),
        ("h", 107.0, 60.0, 53.0, 66.0),
    ];
    for (i, (name, x, y, w, h)) in specs.iter().enumerate() {
        let n = nodeAt(i as i64 + 1, *w, *h, *x, *y);
        nodes.insert(name, n.clone());
        let container = if *name == "b" || *name == "h" { nodes.get("a").cloned() } else { None };
        g.AddNewNodeToContainer(container.as_ref(), &n);
    }
    let connect = |from: &str, to: &str, coordinates: &[f64]| -> EdgeRef {
        let e = g.Connect(&nodes[from], &nodes[to]);
        let mut i = 0;
        while i < coordinates.len() {
            e.bm().Points.push(geo::NewPoint(coordinates[i], coordinates[i + 1]));
            i += 2;
        }
        e
    };
    connect("b", "c", &[340.0, 126.0, 340.0, 761.0, 247.0, 761.0]);
    connect("d", "c", &[221.0, 654.0, 221.0, 744.0]);
    connect("e", "c", &[40.0, 342.0, 40.0, 761.0, 194.0, 761.0]);
    connect("f", "d", &[221.0, 498.0, 221.0, 588.0]);
    let ae = connect("a", "e", &[145.0, 186.0, 145.0, 309.0, 53.0, 309.0]);
    connect("g", "f", &[221.0, 342.0, 221.0, 432.0]);
    let hg = connect("h", "g", &[147.0, 126.0, 147.0, 293.0, 193.0, 293.0]);
    (g, ae, hg)
}

fn balanceOrderCrossings(g: &GraphRef) -> i64 {
    let mut count = 0;
    let edges = g.edges();
    for (i, edge) in edges.iter().enumerate() {
        for other in &edges[i + 1..] {
            let (ep, op) = (pointValues(&edge.b().Points), pointValues(&other.b().Points));
            for a in 0..ep.len().saturating_sub(1) {
                for b in 0..op.len().saturating_sub(1) {
                    let (mut p, mut q) = (ep[a], ep[a + 1]);
                    let (mut r, mut s) = (op[b], op[b + 1]);
                    if p.Y == q.Y && r.X == s.X {
                        std::mem::swap(&mut p, &mut r);
                        std::mem::swap(&mut q, &mut s);
                    }
                    if p.X == q.X
                        && r.Y == s.Y
                        && p.X > r.X.min(s.X)
                        && p.X < r.X.max(s.X)
                        && r.Y > p.Y.min(q.Y)
                        && r.Y < p.Y.max(q.Y)
                    {
                        count += 1;
                    }
                }
            }
        }
    }
    count
}

#[test]
fn TestBalanceEdgeSegmentsPreservesContainerRouteOrder() {
    for orientation in ["down", "up", "right", "left"] {
        for reverse in [false, true] {
            let (g, ae, _) = containerBalanceOrderGraph();
            let transform = |p: &geo::PointRef| {
                let v = p.get();
                let mut m = p.bm();
                match orientation {
                    "up" => m.Y = 810.0 - v.Y,
                    "right" => {
                        m.X = v.Y;
                        m.Y = v.X;
                    }
                    "left" => {
                        m.X = 810.0 - v.Y;
                        m.Y = v.X;
                    }
                    _ => {}
                }
            };
            let mut boxes: Vec<(NodeRef, geo::Point, f64, f64)> = Vec::new();
            for n in g.nodes() {
                let tl = n.tl().unwrap();
                let br = geo::NewPoint(tl.X() + n.w(), tl.Y() + n.h());
                transform(&tl);
                transform(&br);
                n.set_w((br.X() - tl.X()).abs());
                n.set_h((br.Y() - tl.Y()).abs());
                let (x, y) = (tl.X().min(br.X()), tl.Y().min(br.Y()));
                tl.set(geo::Point { X: x, Y: y });
                boxes.push((n.clone(), tl.get(), n.w(), n.h()));
            }
            for e in g.edges() {
                for p in e.points() {
                    transform(&p);
                }
                if reverse {
                    let mut eb = e.bm();
                    let (f, t) = (eb.From.clone(), eb.To.clone());
                    eb.From = t;
                    eb.To = f;
                    eb.Points.reverse();
                }
            }
            assert_eq!(balanceOrderCrossings(&g), 0, "fixture has crossings before balancing ({orientation} {reverse})");
            BalanceEdgeSegments(&bg(), &g).unwrap();
            assert_eq!(balanceOrderCrossings(&g), 0, "balancing introduced crossings ({orientation} {reverse})");
            for e in g.edges() {
                let points = pointValues(&e.b().Points);
                assert!(
                    e.from().ContainsPointOnBox(&points[0]) && e.to().ContainsPointOnBox(&points[points.len() - 1]),
                    "balancing detached an endpoint from its node"
                );
            }
            for (n, tl, w, h) in &boxes {
                assert!(n.tlv() == *tl && n.w() == *w && n.h() == *h, "balancing moved a node");
            }
            let ap = pointValues(&ae.b().Points);
            let mut length = 0.0;
            for i in 1..ap.len() {
                length += (ap[i].X - ap[i - 1].X).abs() + (ap[i].Y - ap[i - 1].Y).abs();
            }
            assert!(length < 215.0, "useful balancing was lost: a→e length {length}, originally 215 ({orientation} {reverse})");
        }
    }
}

fn makeSegment(x: f64, start: f64, end: f64) -> EdgeSegmentRef {
    let e = layoutgraph::NewEdge(&layoutgraph::NewNode(1, 10.0, 10.0), &layoutgraph::NewNode(2, 10.0, 10.0));
    layoutgraph::NewEdgeSegment(&pt(x, start), &pt(x, end), &e)
}

fn set(segments: &[&EdgeSegmentRef]) -> Set<EdgeSegmentRef> {
    segments.iter().map(|s| (*s).clone()).collect()
}

#[test]
fn TestBalanceOrderChecksClosedIntervalsAndEverySharedMember() {
    for (name, start, end, target, want) in [
        ("overtakes overlapping route", 5.0, 15.0, 30.0, false),
        ("touches at dragged elbow", 10.0, 20.0, 30.0, false),
        ("disjoint routes may reorder", 11.0, 20.0, 30.0, true),
        ("safe movement remains available", 5.0, 15.0, 19.0, true),
        ("new route contact is rejected", 5.0, 15.0, 20.0, false),
    ] {
        let (moving, outside) = (makeSegment(10.0, 0.0, 10.0), makeSegment(20.0, start, end));
        let guard = newRouteWorkGuard(&bg(), "order test", 100).unwrap();
        let got = checkBalanceOrder(&[moving.clone()], &set(&[&moving]), &[moving.clone(), outside], &[target], true, &guard).unwrap();
        assert_eq!(got == balanceOrderPreserved, want, "{name}");
    }
    // Only the second member of a shared trunk reaches the foreign route.
    let (first, second, outside) = (makeSegment(10.0, 0.0, 10.0), makeSegment(10.0, 5.0, 30.0), makeSegment(20.0, 20.0, 40.0));
    let batch = vec![first.clone(), second.clone()];
    let s = set(&[&first, &second]);
    let guard = newRouteWorkGuard(&bg(), "shared order test", 100).unwrap();
    let ok = checkBalanceOrder(&batch, &s, &[first.clone(), second.clone(), outside.clone()], &[30.0, 30.0], true, &guard).unwrap();
    assert!(ok != balanceOrderPreserved, "partial shared trunk ignored its second member's constraint");
    assert!(first.b().Segment.Start.X() == 10.0 && second.b().Segment.Start.X() == 10.0, "a rejected proposal mutated the shared trunk");

    let guard = newRouteWorkGuard(&bg(), "limited order test", 1).unwrap();
    let err = checkBalanceOrder(&batch, &s, &[outside], &[30.0, 30.0], true, &guard).unwrap_err();
    assert!(err.Is(&errRouteStageWorkLimit), "work limit error = {err}");
    assert!(first.b().Segment.Start.X() == 10.0 && second.b().Segment.Start.X() == 10.0, "exhausted order check mutated routes");
}

// Two short source stubs are crossed by a longer return route.
#[test]
fn TestBalanceEdgeSegmentsStillUncrossesDifferentRanges() {
    let g = layoutgraph::NewGraph();
    let source = nodeAt(1, 80.0, 40.0, 80.0, 80.0);
    let sink = nodeAt(2, 60.0, 40.0, 0.0, 200.0);
    let remote = nodeAt(3, 80.0, 40.0, 180.0, 300.0);
    for n in [&source, &sink, &remote] {
        g.AddNewNodeToContainer(None, n);
    }
    for x in [100.0, 140.0] {
        let e = g.Connect(&source, &sink);
        setPoints(&e, &[(x, 80.0), (x, 40.0), (30.0, 40.0), (30.0, 200.0)]);
    }
    let returnEdge = g.Connect(&remote, &sink);
    setPoints(&returnEdge, &[(260.0, 320.0), (300.0, 320.0), (300.0, 60.0), (30.0, 60.0), (30.0, 200.0)]);
    assert_eq!(balanceOrderCrossings(&g), 2, "fixture crossings");
    BalanceEdgeSegments(&bg(), &g).unwrap();
    assert_eq!(balanceOrderCrossings(&g), 0, "balancing left avoidable crossings");
}

#[test]
fn TestBalanceOrderReversalDoesNotHideLaterContact() {
    let (moving, reversed, contact) = (makeSegment(10.0, 0.0, 10.0), makeSegment(20.0, 0.0, 10.0), makeSegment(30.0, 0.0, 10.0));
    let guard = newRouteWorkGuard(&bg(), "mixed order test", 100).unwrap();
    let status = checkBalanceOrder(&[moving.clone()], &set(&[&moving]), &[reversed, contact], &[30.0], true, &guard).unwrap();
    assert_eq!(status, balanceOrderContactChanged, "a later contact must prohibit fallback");
}

#[test]
fn TestBalanceReversalDoesNotTradeCrossingsOrAddSharedRuns() {
    for sharedRun in [false, true] {
        let g = layoutgraph::NewGraph();
        let addRoute = |coordinates: &[f64]| -> EdgeRef {
            let count = g.b().Nodes.len();
            let n = nodeAt(count as i64 + 1, 10.0, 10.0, 1000.0, (count * 20) as f64);
            g.AddNewNodeToContainer(None, &n);
            let m = nodeAt(g.b().Nodes.len() as i64 + 1, 10.0, 10.0, 1100.0, n.y());
            g.AddNewNodeToContainer(None, &m);
            let e = g.Connect(&n, &m);
            let mut i = 0;
            while i < coordinates.len() {
                e.bm().Points.push(geo::NewPoint(coordinates[i], coordinates[i + 1]));
                i += 2;
            }
            e
        };
        let (moving, segment, target);
        if sharedRun {
            moving = addRoute(&[-20.0, 0.0, 10.0, 0.0, 10.0, -10.0]);
            let p = moving.points();
            segment = layoutgraph::NewEdgeSegment(&p[1], &p[2], &moving);
            target = 50.0;
            addRoute(&[-30.0, -5.0, 30.0, -5.0]); // This pair loses one crossing.
            addRoute(&[20.0, 30.0, 20.0, 0.0, 40.0, 0.0, 40.0, -30.0]); // This pair would gain a shared run.
        } else {
            moving = addRoute(&[10.0, 0.0, 10.0, 20.0]);
            let p = moving.points();
            segment = layoutgraph::NewEdgeSegment(&p[0], &p[1], &moving);
            target = 30.0;
            addRoute(&[0.0, 5.0, 20.0, 5.0]); // This pair loses one crossing.
            addRoute(&[20.0, 15.0, 40.0, 15.0]); // This pair would gain one crossing.
        }
        let snapshot = captureExactRouteTest(&moving);
        let guard = newRouteWorkGuard(&bg(), "fallback test", 10000).unwrap();
        let accepted = balanceReversalRemovesCrossings(&g, &[segment], &[target], true, &guard).unwrap();
        assert!(!accepted, "unsafe reversal accepted (sharedRun={sharedRun})");
        snapshot.assertRestored();
    }
}
