// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/routing/edge_postprocess_test.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of routing/edge_postprocess_test.go.
use super::*;
use layoutgraph::{Cluster, ClusterRef, EdgeAbduction, EdgeAbductionRef};

#[test]
fn TestEvenlyDistribute() {
    let cases: Vec<((f64, f64, i64), Vec<f64>)> = vec![
        ((0.0, 10.0, 1), vec![5.0]),
        ((0.0, 10.0, 2), vec![3.0, 6.0]),
        ((0.0, 5.0, 3), vec![1.0, 2.0, 3.0]),
        ((20.0, 40.0, 5), vec![23.0, 26.0, 29.0, 32.0, 35.0]),
        ((0.0, 10.0, 0), vec![]),
        ((0.0, 10.0, -1), vec![]),
        // shouldn't infinite loop
        ((-235.0, -234.0, 1), vec![]),
    ];
    for ((floor, ceil, num), expected) in cases {
        assert_eq!(expected, evenlyDistribute(floor, ceil, num));
        // The guarded production kernel agrees.
        let guard = newRouteWorkGuard(&bg(), "test", maxRouteStageWorkUnits).unwrap();
        let got = evenlyDistributeGuarded(floor, ceil, num.max(0) as usize, &guard).unwrap();
        assert_eq!(expected, got);
    }
}

#[test]
fn TestFixClusterEdgeBranchingChoosesLowestCostPoint() {
    let g = layoutgraph::NewGraph();
    let external = nodeAt(1, 10.0, 10.0, 10_000.0, 10_000.0);
    let left = nodeAt(2, 10.0, 10.0, 11_000.0, 11_000.0);
    let right = nodeAt(3, 10.0, 10.0, 12_000.0, 12_000.0);
    for n in [&external, &left, &right] {
        g.AddNewNodeToContainer(None, n);
    }

    let cluster = ClusterRef::new(Cluster {
        Vessel: layoutgraph::NewNode(4, 10.0, 10.0),
        Nodes: vec![left.clone(), right.clone()],
        Arrangement: layoutgraph::Row,
        DesiredArrangement: layoutgraph::Row,
        Graph: Some(g.clone()),
        EdgeAbductions: vec![],
        Padding: 0.0,
        FixedSize: false,
        Container: None,
    });
    left.bm().Cluster = Some(cluster.clone());
    right.bm().Cluster = Some(cluster.clone());

    let leftEdge = g.Connect(&external, &left);
    setPoints(&leftEdge, &[(0.0, 0.0), (0.0, -10.0), (-100.0, -10.0), (-100.0, 100.0)]);
    let rightEdge = g.Connect(&external, &right);
    setPoints(&rightEdge, &[(0.0, 0.0), (0.0, 10.0), (100.0, 10.0), (100.0, -5.0)]);

    FixClusterEdgeBranching(&bg(), &g).unwrap();
    assert!(
        p(&leftEdge, 1).Y == 10.0 && p(&rightEdge, 1).Y == 10.0,
        "branch point y coordinates = ({}, {}), want (10, 10)",
        p(&leftEdge, 1).Y,
        p(&rightEdge, 1).Y
    );
}

#[test]
fn TestBounds() {
    let seg = |a: (f64, f64), b: (f64, f64)| geo::Segment { Start: pt(a.0, a.1), End: pt(b.0, b.1) };
    struct testCase {
        line: geo::Segment,
        otherLines: Vec<geo::Segment>,
        buffer: f64,
        expectedFloor: f64,
        expectedCeil: f64,
    }
    let inf = f64::INFINITY;
    let cases = vec![
        // Test basic horizontal direction
        testCase {
            line: seg((10.0, 0.0), (10.0, 10.0)),
            otherLines: vec![seg((0.0, 0.0), (0.0, 10.0)), seg((20.0, 0.0), (20.0, 10.0))],
            buffer: 2.0,
            expectedFloor: 0.0,
            expectedCeil: 20.0,
        },
        // Test basic vertical direction
        testCase {
            line: seg((0.0, 10.0), (10.0, 10.0)),
            otherLines: vec![seg((0.0, 0.0), (10.0, 0.0)), seg((0.0, 20.0), (10.0, 20.0))],
            buffer: 2.0,
            expectedFloor: 0.0,
            expectedCeil: 20.0,
        },
        // Test non-overlaps not considered
        testCase {
            line: seg((10.0, 0.0), (10.0, 10.0)),
            otherLines: vec![
                seg((0.0, 0.0), (0.0, 10.0)),
                seg((20.0, 0.0), (20.0, 10.0)),
                seg((8.0, 100.0), (8.0, 110.0)),
                seg((18.0, -100.0), (18.0, -110.0)),
                seg((15.0, -2.0), (15.0, -12.0)),
            ],
            buffer: 2.0,
            expectedFloor: 0.0,
            expectedCeil: 20.0,
        },
        // Test no lower bound
        testCase {
            line: seg((10.0, 0.0), (10.0, 10.0)),
            otherLines: vec![seg((21.0, 0.0), (21.0, 10.0)), seg((20.0, 0.0), (20.0, 10.0))],
            buffer: 2.0,
            expectedFloor: -inf,
            expectedCeil: 20.0,
        },
        // Test point (start = end)
        testCase {
            line: seg((10.0, 0.0), (10.0, 0.0)),
            otherLines: vec![seg((21.0, 0.0), (21.0, 10.0)), seg((20.0, 0.0), (20.0, 10.0))],
            buffer: 2.0,
            expectedFloor: -inf,
            expectedCeil: inf,
        },
    ];
    for tc in cases {
        let guard = newRouteWorkGuard(&bg(), "test", maxRouteStageWorkUnits).unwrap();
        let (actualFloor, actualCeil) = routeSegmentBounds(&tc.line, &tc.otherLines, tc.buffer, &guard).unwrap();
        assert_eq!(tc.expectedFloor, actualFloor);
        assert_eq!(tc.expectedCeil, actualCeil);
        let refs: Vec<geo::SegmentRef> = tc.otherLines.iter().map(|s| geo::SegmentRef::new(s.clone())).collect();
        let (legacyFloor, legacyCeil) = tc.line.GetBounds(&refs, tc.buffer);
        assert_eq!(tc.expectedFloor, legacyFloor);
        assert_eq!(tc.expectedCeil, legacyCeil);
    }
}

fn twoStacked() -> (GraphRef, NodeRef, NodeRef) {
    let g = layoutgraph::NewGraph();
    let a = nodeAt(0, 1000.0, 1000.0, 0.0, 0.0);
    let b = nodeAt(0, 1000.0, 1000.0, 0.0, 2000.0);
    g.AddNodeUnchecked(&a);
    g.AddNodeUnchecked(&b);
    (g, a, b)
}

#[test]
fn TestBalanceEdgeSegmentsSimpleSingle() {
    let (g, a, b) = twoStacked();
    let e1 = g.Connect(&a, &b);
    setPoints(&e1, &[(1.0, 1000.0), (1.0, 2000.0)]);

    BalanceEdgeSegments(&bg(), &g).unwrap();

    assert_eq!(p(&e1, 0).X, 500.0);
    assert_eq!(p(&e1, 1).X, 500.0);
}

#[test]
fn TestBalanceEdgeSegmentsSimpleDouble() {
    let (g, a, b) = twoStacked();
    let e1 = g.Connect(&a, &b);
    setPoints(&e1, &[(1.0, 1000.0), (1.0, 2000.0)]);
    let e2 = g.Connect(&a, &b);
    setPoints(&e2, &[(3.0, 1000.0), (3.0, 2000.0)]);

    BalanceEdgeSegments(&bg(), &g).unwrap();

    assert_eq!((1000.0f64 / 3.0).floor(), p(&e1, 0).X);
    assert_eq!((1000.0f64 / 3.0).floor(), p(&e1, 1).X);
    assert_eq!((1000.0f64 * 2.0 / 3.0).floor(), p(&e2, 0).X);
    assert_eq!((1000.0f64 * 2.0 / 3.0).floor(), p(&e2, 1).X);
}

#[test]
fn TestBalanceEdgeSegmentsSimpleDoubleOppositeDirections() {
    let (g, a, b) = twoStacked();
    let e1 = g.Connect(&a, &b);
    setPoints(&e1, &[(1.0, 1000.0), (1.0, 2000.0)]);
    let e2 = g.Connect(&b, &a);
    setPoints(&e2, &[(3.0, 2000.0), (3.0, 1000.0)]);

    BalanceEdgeSegments(&bg(), &g).unwrap();

    assert_eq!((1000.0f64 / 3.0).floor(), p(&e1, 0).X);
    assert_eq!((1000.0f64 / 3.0).floor(), p(&e1, 1).X);
    assert_eq!((1000.0f64 * 2.0 / 3.0).floor(), p(&e2, 0).X);
    assert_eq!((1000.0f64 * 2.0 / 3.0).floor(), p(&e2, 1).X);
}

#[test]
fn TestBalanceEdgeSegmentsMultipleNodes() {
    let g = layoutgraph::NewGraph();
    let a = nodeAt(0, 200.0, 1000.0, 0.0, 0.0);
    let b = nodeAt(0, 200.0, 1000.0, 500.0, 0.0);
    let c = nodeAt(0, 1000.0, 1000.0, 0.0, 2000.0);
    g.AddNodeUnchecked(&a);
    g.AddNodeUnchecked(&b);
    g.AddNodeUnchecked(&c);

    let e1 = g.Connect(&a, &c);
    setPoints(&e1, &[(1.0, 1000.0), (1.0, 2000.0)]);
    let e2 = g.Connect(&b, &c);
    setPoints(&e2, &[(501.0, 1000.0), (501.0, 2000.0)]);

    BalanceEdgeSegments(&bg(), &g).unwrap();

    assert_eq!(100.0, p(&e1, 0).X);
    assert_eq!(100.0, p(&e1, 1).X);
    assert_eq!(600.0, p(&e2, 0).X);
    assert_eq!(600.0, p(&e2, 1).X);
}

#[test]
fn TestBalanceEdgeSegmentsSingleBendy() {
    let (g, a, b) = twoStacked();
    let e = g.Connect(&a, &b);
    setPoints(&e, &[(1.0, 1000.0), (1.0, 1300.0), (300.0, 1300.0), (300.0, 2000.0)]);

    BalanceEdgeSegments(&bg(), &g).unwrap();

    assert_eq!(500.0, p(&e, 0).X);
    assert_eq!(500.0, p(&e, 1).X);
    assert_eq!(500.0, p(&e, 2).X);
}

#[test]
fn TestBalanceEdgeSegmentsBackToBack() {
    let (g, a, b) = twoStacked();
    let e1 = g.Connect(&a, &b);
    setPoints(&e1, &[(1000.0, 500.0), (1450.0, 500.0), (1450.0, 2400.0), (1000.0, 2400.0)]);

    let c = nodeAt(0, 1000.0, 1000.0, 2000.0, 0.0);
    let d = nodeAt(0, 1000.0, 1000.0, 2000.0, 2000.0);
    g.AddNodeUnchecked(&c);
    g.AddNodeUnchecked(&d);

    let e2 = g.Connect(&c, &d);
    setPoints(&e2, &[(2000.0, 500.0), (1500.0, 500.0), (1500.0, 2500.0), (2000.0, 2500.0)]);

    BalanceEdgeSegments(&bg(), &g).unwrap();

    let third = (1000.0f64 / 3.0).floor();
    assert_eq!(geo::Point { X: 1000.0, Y: 500.0 }, p(&e1, 0));
    assert_eq!(geo::Point { X: 1000.0 + third, Y: 500.0 }, p(&e1, 1));
    assert_eq!(geo::Point { X: 1000.0 + third, Y: 2500.0 }, p(&e1, 2));
    assert_eq!(geo::Point { X: 1000.0, Y: 2500.0 }, p(&e1, 3));

    assert_eq!(geo::Point { X: 2000.0, Y: 500.0 }, p(&e2, 0));
    assert_eq!(geo::Point { X: 1000.0 + 2.0 * third, Y: 500.0 }, p(&e2, 1));
    assert_eq!(geo::Point { X: 1000.0 + 2.0 * third, Y: 2500.0 }, p(&e2, 2));
    assert_eq!(geo::Point { X: 2000.0, Y: 2500.0 }, p(&e2, 3));
}

#[test]
fn TestBalanceEdgeSegmentsMultipleRanges() {
    let g = layoutgraph::NewGraph();
    let a = nodeAt(0, 1000.0, 1000.0, 0.0, 0.0);
    let b = nodeAt(0, 1000.0, 1000.0, 0.0, 6000.0);
    g.AddNodeUnchecked(&a);
    g.AddNodeUnchecked(&b);

    let e1 = g.Connect(&a, &b);
    setPoints(&e1, &[(1000.0, 500.0), (1600.0, 500.0), (1600.0, 6200.0), (1000.0, 6200.0)]);

    let c = nodeAt(0, 1000.0, 1000.0, 0.0, 3000.0);
    let d = nodeAt(0, 1000.0, 1000.0, 0.0, 10000.0);
    g.AddNodeUnchecked(&c);
    g.AddNodeUnchecked(&d);

    let e2 = g.Connect(&c, &d);
    setPoints(&e2, &[(1000.0, 3500.0), (2640.0, 3500.0), (2640.0, 10500.0), (1000.0, 10500.0)]);

    let e = nodeAt(0, 1000.0, 1000.0, 1000.0, 7500.0);
    g.AddNodeUnchecked(&e);
    let f = nodeAt(0, 1000.0, 10000.0, 5000.0, 0.0);
    g.AddNodeUnchecked(&f);

    BalanceEdgeSegments(&bg(), &g).unwrap();

    let (at, bt, ct, dt, et, ft) = (a.tlv(), b.tlv(), c.tlv(), d.tlv(), e.tlv(), f.tlv());
    let mid2 = ((et.X + e.w() + ft.X) / 2.0).floor();
    assert_eq!(geo::Point { X: ct.X + c.w(), Y: (ct.Y + c.h() / 2.0).floor() }, p(&e2, 0));
    assert_eq!(geo::Point { X: mid2, Y: (ct.Y + c.h() / 2.0).floor() }, p(&e2, 1));
    assert_eq!(geo::Point { X: mid2, Y: (dt.Y + d.h() / 2.0).floor() }, p(&e2, 2));
    assert_eq!(geo::Point { X: dt.X + d.w(), Y: (dt.Y + d.h() / 2.0).floor() }, p(&e2, 3));

    let mid1 = ((at.X + a.w() + p(&e2, 1).X) / 2.0).floor();
    assert_eq!(geo::Point { X: at.X + c.w(), Y: (at.Y + a.h() / 2.0).floor() }, p(&e1, 0));
    assert_eq!(geo::Point { X: mid1, Y: (at.Y + a.h() / 2.0).floor() }, p(&e1, 1));
    assert_eq!(geo::Point { X: mid1, Y: (bt.Y + b.h() / 2.0).floor() }, p(&e1, 2));
    assert_eq!(geo::Point { X: bt.X + d.w(), Y: (bt.Y + b.h() / 2.0).floor() }, p(&e1, 3));
}

// 2 going down, 1 going up
#[test]
fn TestBalanceEdgeSegmentsSharedRoutes() {
    let (g, a, b) = twoStacked();
    let e1 = g.Connect(&a, &b);
    setPoints(&e1, &[(1.0, 1000.0), (1.0, 2000.0)]);
    let e2 = g.Connect(&a, &b);
    setPoints(&e2, &[(1.0, 1000.0), (1.0, 2000.0)]);
    let e3 = g.Connect(&b, &a);
    setPoints(&e3, &[(3.0, 2000.0), (3.0, 1000.0)]);

    BalanceEdgeSegments(&bg(), &g).unwrap();

    let third = (1000.0f64 / 3.0).floor();
    let twoThirds = (1000.0f64 * 2.0 / 3.0).floor();
    assert_eq!(third, p(&e1, 0).X);
    assert_eq!(third, p(&e1, 1).X);
    assert_eq!(third, p(&e2, 0).X);
    assert_eq!(third, p(&e2, 1).X);
    assert_eq!(twoThirds, p(&e3, 0).X);
    assert_eq!(twoThirds, p(&e3, 1).X);
}

// a-b has smaller range of motion, but when it moves, it should also move its shared route, a-c
#[test]
fn TestBalanceEdgeSegmentsSharedRoutesDifferentRanges() {
    let g = layoutgraph::NewGraph();
    let a = nodeAt(0, 3000.0, 1000.0, 4000.0, 0.0);
    let b = nodeAt(0, 1000.0, 1000.0, 4500.0, 4000.0);
    let c = nodeAt(0, 1000.0, 1000.0, 0.0, 2000.0);
    g.AddNodeUnchecked(&a);
    g.AddNodeUnchecked(&b);
    g.AddNodeUnchecked(&c);
    let (at, bt, ct) = (a.tlv(), b.tlv(), c.tlv());

    let ab = g.Connect(&a, &b);
    setPoints(&ab, &[(bt.X + 10.0, at.Y + a.h()), (bt.X + 10.0, bt.Y)]);

    let ac = g.Connect(&a, &c);
    setPoints(
        &ac,
        &[(bt.X + 10.0, at.Y + a.h()), (bt.X + 10.0, ct.Y + (c.h() / 2.0).round()), (ct.X + c.w(), ct.Y + (c.h() / 2.0).round())],
    );

    BalanceEdgeSegments(&bg(), &g).unwrap();

    let x = bt.X + (b.w() / 2.0).round();
    assert_eq!(geo::Point { X: x, Y: at.Y + a.h() }, p(&ab, 0));
    assert_eq!(geo::Point { X: x, Y: bt.Y }, p(&ab, 1));

    assert_eq!(geo::Point { X: x, Y: at.Y + a.h() }, p(&ac, 0));
    assert_eq!(geo::Point { X: x, Y: ct.Y + (c.h() / 2.0).round() }, p(&ac, 1));
    assert_eq!(geo::Point { X: ct.X + c.w(), Y: ct.Y + (c.h() / 2.0).round() }, p(&ac, 2));
}

#[test]
fn TestClusterBalancing() {
    let a = nodeAt(1, 215.0, 126.0, 1349.0, 1232.0);
    let b = nodeAt(2, 162.0, 126.0, 1376.0, 1421.0);
    let c = nodeAt(3, 139.0, 126.0, 1387.0, 1610.0);
    let d = nodeAt(4, 160.0, 126.0, 1000.0, 1275.0);
    let e = nodeAt(5, 160.0, 126.0, 1000.0, 1421.0);
    let f = nodeAt(6, 160.0, 126.0, 1000.0, 1567.0);

    let graph = layoutgraph::NewGraph();
    for n in [&a, &b, &c, &d, &e, &f] {
        graph.AddNewNodeToContainer(None, n);
    }
    let tri = layoutgraph::TriangleArrowhead.to_string();

    let ab = graph.Connect(&a, &b);
    setPoints(&ab, &[(1457.0, 1358.0), (1457.0, 1421.0)]);
    ab.bm().TargetArrowhead = tri.clone();

    let cb = graph.Connect(&c, &b);
    setPoints(&cb, &[(1457.0, 1610.0), (1457.0, 1547.0)]);
    cb.bm().SourceArrowhead = tri.clone();

    let bd = graph.Connect(&b, &d);
    setPoints(&bd, &[(1376.0, 1484.0), (1268.0, 1484.0), (1268.0, 1338.0), (1160.0, 1338.0)]);
    bd.bm().SourceArrowhead = tri.clone();
    bd.bm().TargetArrowhead = tri.clone();

    let be = graph.Connect(&b, &e);
    setPoints(&be, &[(1376.0, 1484.0), (1160.0, 1484.0)]);
    be.bm().SourceArrowhead = tri.clone();
    be.bm().TargetArrowhead = tri.clone();

    let bf = graph.Connect(&b, &f);
    setPoints(&bf, &[(1376.0, 1484.0), (1268.0, 1484.0), (1268.0, 1630.0), (1160.0, 1630.0)]);
    bf.bm().SourceArrowhead = tri.clone();
    bf.bm().TargetArrowhead = tri.clone();

    graph.bm().CellSize = 215.0;

    let vessel = nodeAt(8, 160.0, 418.0, 0.0, 275.0);
    let abduction = |edge: &EdgeRef, to: &NodeRef| {
        EdgeAbductionRef::new(EdgeAbduction {
            Edge: edge.clone(),
            OriginallyFrom: None,
            OriginallyTo: Some(to.clone()),
            CurrentFrom: Some(b.clone()),
            CurrentTo: Some(vessel.clone()),
        })
    };
    let cluster = ClusterRef::new(Cluster {
        Vessel: vessel.clone(),
        Nodes: vec![d.clone(), e.clone(), f.clone()],
        Arrangement: layoutgraph::Column,
        DesiredArrangement: layoutgraph::Column,
        Graph: None,
        EdgeAbductions: vec![abduction(&bd, &d), abduction(&be, &e), abduction(&bf, &f)],
        Padding: 20.0,
        FixedSize: false,
        Container: None,
    });
    graph.bm().Clusters = layoutgraph::GoMap::new();
    graph.b().Clusters.set(vessel.clone(), cluster.clone());
    for n in [&d, &e, &f] {
        n.bm().Cluster = Some(cluster.clone());
    }

    BalanceEdgeSegments(&bg(), &graph).unwrap();

    let distributionVals = evenlyDistribute(f.tlv().X + f.w(), a.tlv().X, 1);
    assert_eq!(distributionVals.len(), 1);
    assert_eq!(distributionVals[0], p(&bf, 1).X);
    assert_eq!(distributionVals[0], p(&bf, 2).X);
}
