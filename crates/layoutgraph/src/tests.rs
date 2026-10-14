// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from the following files of terrastruct/d2, Copyright 2022 Terrastruct Inc.:
//   - d2layouts/d2talalayout/internal/layoutgraph/edge_test.go
//   - d2layouts/d2talalayout/internal/layoutgraph/geometry_policy_test.go
//   - d2layouts/d2talalayout/internal/layoutgraph/graph_test.go
//   - d2layouts/d2talalayout/internal/layoutgraph/node_test.go
//   - d2layouts/d2talalayout/internal/layoutgraph/segment_crossing_test.go
//   - d2layouts/d2talalayout/internal/layoutgraph/sequence_test.go
//   - d2layouts/d2talalayout/internal/layoutgraph/transaction_test.go
// Rust port: Copyright 2026 Niklas Krieger.

//! Ports of selected Go unit tests (node_test.go, transaction_test.go,
//! graph_test.go, edge_test.go, segment_crossing tests).
use crate::*;
use gocompat::context;

fn pt(x: f64, y: f64) -> geo::PointRef {
    geo::NewPoint(x, y)
}

fn placed(id: EntityID, x: f64, y: f64, w: f64, h: f64) -> NodeRef {
    let n = NewNode(id, w, h);
    n.set_tl(Some(pt(x, y)));
    n
}

impl GraphRef {
    fn connectByID(&self, a: EntityID, b: EntityID) -> EdgeRef {
        let nodes = self.nodes();
        let na = nodes.iter().find(|n| n.id() == a).unwrap().clone();
        let nb = nodes.iter().find(|n| n.id() == b).unwrap().clone();
        self.Connect(&na, &nb)
    }
}

impl NodeRef {
    fn allReachableNodes(&self, c: bool, n: bool, t: bool) -> Vec<NodeRef> {
        self.allReachableNodesGuarded(c, n, t, None, &unboundedWork).unwrap()
    }
}

fn createClusterVesselFixture(cluster: &ClusterRef, vesselID: EntityID) -> NodeRef {
    let (mut minX, mut minY) = (f64::INFINITY, f64::INFINITY);
    for node in cluster.b().Nodes.clone() {
        if node.has_tl() {
            minX = minX.min(node.x());
            minY = minY.min(node.y());
        }
    }
    let vessel = NewNode(vesselID, 0.0, 0.0);
    vessel.SetClusterVessel(true);
    cluster.bm().Vessel = vessel.clone();
    cluster.Resize(&vessel);
    if !minX.is_infinite() && !minY.is_infinite() {
        let arr = cluster.b().Arrangement.clone();
        let mut nodes = cluster.b().Nodes.clone();
        if arr == Row {
            gocompat::sort::Slice(&mut nodes, |a, b| a.x() < b.x());
        } else if arr == Column {
            gocompat::sort::Slice(&mut nodes, |a, b| a.y() < b.y());
        }
        cluster.bm().Nodes = nodes;
        vessel.set_tl(Some(pt(minX, minY)));
    }
    vessel
}

fn newCluster(nodes: Vec<NodeRef>, container: Option<NodeRef>, g: &GraphRef) -> ClusterRef {
    ClusterRef::new(Cluster {
        Vessel: NewNode(-1, 0.0, 0.0),
        Nodes: nodes,
        Arrangement: Row,
        DesiredArrangement: ClusterArrangement::default(),
        Graph: Some(g.clone()),
        EdgeAbductions: Vec::new(),
        Padding: 0.0,
        FixedSize: false,
        Container: container,
    })
}

#[test]
fn TestSortNodesByIDWithNil() {
    let higher = NewNode(2, 10.0, 10.0);
    let mut nodes = vec![Some(higher.clone()), None];
    sortOptNodesByID(&mut nodes);
    assert!(nodes[0].is_none());
    assert!(nodes[1].as_ref() == Some(&higher));
}

#[test]
fn TestReachableNodes() {
    let graph = NewGraph();
    graph.AddNode(&NewNode(1, 5.0, 5.0));
    graph.AddNode(&NewNode(2, 2.0, 5.0));
    graph.connectByID(1, 2);
    let n1 = graph.nodes()[0].clone();
    assert_eq!(n1.allReachableNodes(false, true, false).len(), 2);
    graph.AddNode(&NewNode(3, 5.0, 5.0));
    assert_eq!(graph.nodes()[2].allReachableNodes(false, true, false).len(), 1);
    for i in 4..=6 {
        graph.AddNode(&NewNode(i, 5.0, 5.0));
    }
    graph.connectByID(4, 5);
    graph.connectByID(5, 6);
    graph.connectByID(4, 6);
    assert_eq!(graph.nodes()[3].allReachableNodes(false, true, false).len(), 3);
}

#[test]
fn TestMoveNodeWithChildrenSupportsFractionalOffsets() {
    let g = NewGraph();
    let parent = placed(1, 10.0, 10.0, 20.0, 20.0);
    let child = placed(2, 15.0, 15.0, 10.0, 10.0);
    g.AddNewNodeToContainer(None, &parent);
    g.AddNewNodeToContainer(Some(&parent), &child);
    parent.moveNodeWithChildren(0.5, 1.25);
    assert_eq!((parent.x(), parent.y()), (10.5, 11.25));
    assert_eq!((child.x(), child.y()), (15.5, 16.25));
}

#[test]
fn TestReachableNodesWithContainers() {
    let graph = NewGraph();
    let a = NewNode(1, 5.0, 5.0);
    let b = NewNode(2, 5.0, 5.0);
    let c = NewNode(3, 5.0, 5.0);
    let d = NewNode(4, 5.0, 5.0);
    for n in [&a, &b, &c, &d] {
        graph.AddNode(n);
    }
    graph.connectByID(1, 2);
    graph.connectByID(3, 4);
    graph.containers().set(Some(b.clone()), vec![c.clone()]);
    let e = NewNode(5, 5.0, 5.0);
    let f = NewNode(6, 2.0, 5.0);
    graph.AddNode(&e);
    graph.AddNode(&f);
    graph.connectByID(5, 6);
    graph.containers().set(None, vec![a.clone(), b.clone(), d.clone(), e.clone(), f.clone()]);
    for n in [&a, &b, &c, &d] {
        assert_eq!(n.allReachableNodes(false, true, false).len(), 2);
    }
}

#[test]
fn TestReachableNodesNearsDeterminism() {
    let g = NewGraph();
    let ns: Vec<NodeRef> = (1..=4).map(|i| g.AddNode(&NewNode(i, 10.0, 10.0))).collect();
    ns[0].AddNear(Some(&ns[1]));
    ns[0].AddNear(Some(&ns[2]));
    ns[0].AddNear(Some(&ns[3]));
    for _ in 0..10 {
        let r = ns[0].allReachableNodes(true, true, false);
        let got: Vec<EntityID> = r.iter().map(|n| n.id()).collect();
        assert_eq!(got, vec![1, 2, 3, 4]);
    }
    let r = ns[0].allReachableNodes(true, false, false);
    assert_eq!(r.len(), 1);
}

#[test]
fn TestAddNearIsSymmetric() {
    let g = NewGraph();
    let a = NewNode(1, 10.0, 10.0);
    let b = NewNode(2, 10.0, 10.0);
    let c = NewNode(3, 10.0, 10.0);
    for n in [&a, &b, &c] {
        g.AddNewNodeToContainer(None, n);
    }
    a.AddNear(Some(&b));
    b.AddNear(Some(&c));
    c.AddNear(Some(&a));
    a.AddNear(Some(&b));
    a.AddNear(Some(&a));
    a.AddNear(None);
    let zeroA = NodeRef::new(Node::default());
    let zeroB = NodeRef::new(Node::default());
    zeroA.AddNear(Some(&zeroB));
    assert!(zeroA.b().Nears.contains(&zeroB) && zeroB.b().Nears.contains(&zeroA));
    for (x, y) in [(&a, &b), (&b, &c), (&c, &a)] {
        assert!(x.b().Nears.contains(y) && y.b().Nears.contains(x));
    }
    assert!(!a.b().Nears.contains(&a));
}

#[test]
fn TestBoxesOverlapWithPadding() {
    let bx = |x: f64, y: f64, w: f64, h: f64| geo::Box { TopLeft: Some(pt(x, y)), Width: w, Height: h };
    let cases = [
        (bx(0., 0., 10., 10.), bx(5., 5., 10., 10.), 0.0, true),
        (bx(0., 0., 10., 10.), bx(2., 2., 3., 3.), 0.0, true),
        (bx(0., 0., 10., 10.), bx(10., 2., 4., 4.), 0.0, false),
        (bx(0., 0., 10., 10.), bx(10., 10., 4., 4.), 0.0, false),
        (bx(0., 0., 10., 10.), bx(15., 3., 4., 4.), 6.0, true),
        (bx(0., 0., 10., 10.), bx(15., 3., 4., 4.), 5.0, false),
        (bx(-10., -10., 5., 5.), bx(-4., -20., 5., 5.), 0.0, false),
    ];
    for (b1, b2, pad, want) in cases.iter() {
        assert_eq!(boxesOverlapWithPadding(b1, b2, *pad), *want);
        assert_eq!(boxesOverlapWithPadding(b2, b1, *pad), *want);
        if *pad == 0.0 {
            assert_eq!(b1.Overlaps(b2), *want);
        }
    }
}

#[test]
fn TestNodeDistance() {
    let a = placed(1, 0., 0., 10., 10.);
    for (b, want) in [
        (placed(2, 5., 5., 10., 10.), 0.0),
        (placed(2, 10., 3., 4., 4.), 0.0),
        (placed(2, 15., 2., 4., 4.), 5.0),
        (placed(2, 2., 16., 4., 4.), 6.0),
        (placed(2, 13., 14., 4., 4.), 5.0),
    ] {
        assert!((a.distanceTo(&b, true) - want).abs() <= 1e-9);
        assert!((b.distanceTo(&a, true) - want).abs() <= 1e-9);
    }
    let b = placed(2, 3., 4., 100., 100.);
    assert_eq!(a.distanceTo(&b, false), 5.0);
}

#[test]
fn TestOrientationAndIntersects() {
    let p = geo::Point::new(0., 0.);
    let q = geo::Point::new(2., 0.);
    assert!(orientation(&p, &q, &geo::Point::new(1., -1.)) > 0.0);
    assert!(orientation(&p, &q, &geo::Point::new(1., 1.)) < 0.0);
    assert_eq!(orientation(&p, &q, &geo::Point::new(5., 0.)), 0.0);
    let P = |x: f64, y: f64| geo::Point::new(x, y);
    let cases = [
        (P(0., 0.), P(0., 100.), P(40., 0.), P(40., 100.), false),
        (P(0., 0.), P(0., 100.), P(0., 110.), P(0., 140.), false),
        (P(0., 0.), P(0., 100.), P(0., 90.), P(0., 140.), true),
        (P(0., 0.), P(10., 10.), P(5., 5.), P(15., 15.), true),
        (P(0., 0.), P(50., 50.), P(30., 0.), P(0., 40.), true),
        (P(0., 0.), P(50., 50.), P(50., 50.), P(70., 70.), true),
        (P(5., 5.), P(5., 5.), P(0., 0.), P(10., 10.), true),
        (P(5., 5.), P(5., 5.), P(6., 6.), P(6., 6.), false),
    ];
    for (p1, q1, p2, q2, want) in cases.iter() {
        assert_eq!(intersects(p1, q1, p2, q2), *want);
        assert_eq!(intersects(p2, q2, p1, q1), *want);
    }
}

#[test]
fn TestSegmentIntersectsBoxIsSymmetric() {
    let bx = |x: f64, y: f64, w: f64, h: f64| geo::Box { TopLeft: Some(pt(x, y)), Width: w, Height: h };
    let cases = [
        (pt(-20., -20.), pt(0.5, 10.5), bx(0., 0., 10., 10.), true),
        (pt(-20., -20.), pt(-1., -1.), bx(0., 0., 10., 10.), false),
        (pt(-10., 10.), pt(10., -10.), bx(0., 0., 10., 10.), false),
        (pt(-10., -10.), pt(0., 0.), bx(0., 0., 10., 10.), true),
        (pt(-10., 0.), pt(20., 0.), bx(0., 0., 10., 10.), true),
        (pt(-1., 5.), pt(11., 5.), bx(10., 10., -10., -10.), true),
    ];
    for (p1, p2, b, want) in cases.iter() {
        assert_eq!(segmentIntersectsBox(Some(p1), Some(p2), Some(b)), *want);
        assert_eq!(segmentIntersectsBox(Some(p2), Some(p1), Some(b)), *want);
    }
}

#[test]
fn TestPassesThrough() {
    let a = placed(1, 10., 10., 5., 5.);
    assert!(a.passesThrough(Some(&pt(12., 12.)), Some(&pt(19., 19.))));
    assert!(a.passesThrough(Some(&pt(9., 9.)), Some(&pt(19., 19.))));
}

#[test]
fn TestIsMajorityTarget() {
    let g = NewGraph();
    let a = NewNode(1, 10., 10.);
    let b = NewNode(2, 10., 10.);
    g.AddNode(&a);
    g.AddNode(&b);
    g.Connect(&a, &b);
    g.edges()[0].bm().TargetArrowhead = TriangleArrowhead.into();
    assert!(!a.isMajorityTarget());
    assert!(b.isMajorityTarget());
    let c = NewNode(3, 10., 10.);
    g.AddNode(&c);
    g.Connect(&b, &c);
    assert!(b.isMajorityTarget());
    assert!(!c.isMajorityTarget());
    let d = NewNode(4, 10., 10.);
    let e = NewNode(5, 10., 10.);
    g.AddNode(&d);
    g.AddNode(&e);
    g.Connect(&d, &c);
    g.edges()[2].bm().SourceArrowhead = TriangleArrowhead.into();
    g.edges()[2].bm().TargetArrowhead = TriangleArrowhead.into();
    g.Connect(&e, &c);
    g.edges()[3].bm().TargetArrowhead = TriangleArrowhead.into();
    assert!(c.isMajorityTarget());
    assert!(!d.isMajorityTarget());
}

#[test]
fn TestMoveNodeAbs() {
    let g = NewGraph();
    let a = placed(1, 0., 0., 10., 10.);
    g.AddNode(&a);
    let b = placed(2, 20., 0., 1000., 1000.);
    g.AddNode(&b);
    let c = placed(3, 200., 200., 10., 10.);
    let d = placed(4, 220., 200., 10., 10.);
    let cluster = newCluster(vec![c.clone(), d.clone()], Some(b.clone()), &g);
    let vessel = createClusterVesselFixture(&cluster, 5);
    g.AddNode(&vessel);
    g.bm().Containers = GoMap::new();
    g.containers().set(None, vec![a.clone(), b.clone()]);
    g.containers().set(Some(b.clone()), vec![vessel.clone()]);
    g.bm().Clusters = GoMap::new();
    g.clusters().set(vessel.clone(), cluster.clone());
    let all = [&a, &b, &c, &d, &vessel];
    let before: Vec<geo::Point> = all.iter().map(|n| n.tlv()).collect();
    let check = || {
        for (i, n) in all.iter().enumerate() {
            assert_eq!(n.tlv(), before[i]);
        }
    };
    b.moveNodeAbsWithChildren(1000., 1000.);
    b.moveNodeAbsWithChildren(20., 0.);
    check();
    b.moveNodeWithChildren(1000., 1000.);
    b.moveNodeWithChildren(-1000., -1000.);
    check();
}

#[test]
fn TestSetNumColumnsAndUnknownShape() {
    let n = NewNode(0, 100., 100.);
    n.SetNumColumns(5);
    assert_eq!(n.NumColumns(), 0);
    n.SetShape(shape::TABLE_TYPE);
    n.SetNumColumns(5);
    assert_eq!(n.NumColumns(), 5);

    n.SetShape(shape::STEP_TYPE);
    let before = n.shape();
    let beforeType = n.b().shapeType;
    n.SetShape("unsupported");
    assert!(std::rc::Rc::ptr_eq(&before, &n.shape()));
    assert_eq!(beforeType, n.b().shapeType);
}

#[test]
fn TestMirrorWithLoops() {
    let cases: Vec<(f64, f64, f64, f64, Vec<(geo::Orientation, f64)>)> = vec![
        (40., 40., 20., 20., vec![(geo::Top, 10.), (geo::Left, 10.)]),
        (40., 40., 10., 10., vec![(geo::Bottom, 10.), (geo::Right, 10.)]),
        (25., 25., 20., 25., vec![(geo::Top, 15.), (geo::Bottom, 10.), (geo::Right, 15.), (geo::Left, 10.)]),
    ];
    for (w, h, x, y, loops) in cases {
        let n1 = placed(1, 10., 10., 50., 50.);
        let n2 = placed(1, x, y, w, h);
        for (o, v) in loops {
            n2.bm().LoopOffsets.insert(o, v);
        }
        let (a1, b1) = n1.bounds(None);
        let (a2, b2) = n2.bounds(None);
        assert_eq!((*a1.b(), *b1.b()), (*a2.b(), *b2.b()));
        n1.mirror(true, true);
        n2.mirror(true, true);
        let (a1, b1) = n1.bounds(None);
        let (a2, b2) = n2.bounds(None);
        assert_eq!((*a1.b(), *b1.b()), (*a2.b(), *b2.b()));
    }
}

#[test]
fn TestHasLeakyEdge() {
    let graph = NewGraph();
    let n1 = NewNode(1, 1000., 1000.);
    let n2 = NewNode(2, 100., 100.);
    let n3 = NewNode(3, 10., 10.);
    let n4 = NewNode(4, 1., 1.);
    graph.AddNewNodeToContainer(None, &n1);
    graph.AddNewNodeToContainer(Some(&n1), &n2);
    graph.AddNewNodeToContainer(Some(&n2), &n3);
    graph.AddNewNodeToContainer(Some(&n3), &n4);
    graph.connectByID(1, 4);
    assert!(!n1.HasLeakyEdge());
    assert!(!n4.HasLeakyEdge());
    assert!(n3.HasLeakyEdge());
    assert!(n2.HasLeakyEdge());
    let n5 = NewNode(5, 10., 10.);
    let n6 = NewNode(6, 1., 1.);
    graph.AddNewNodeToContainer(Some(&n2), &n5);
    graph.AddNewNodeToContainer(Some(&n5), &n6);
    graph.Disconnect(&graph.edges()[0]);
    graph.connectByID(4, 6);
    assert!(n3.HasLeakyEdge());
    assert!(n5.HasLeakyEdge());
    assert!(!n2.HasLeakyEdge());
}

// ---------------------------------------------------------------- transactions

fn mustNewTransaction(g: &GraphRef, options: TransactionOptions) -> TransactionRef {
    g.newTransactionWithOptionsContext(&context::Background(), options, None).unwrap()
}

#[test]
fn TestIsCandidateRejection() {
    assert!(!IsCandidateRejection(None));
    assert!(IsCandidateRejection(Some(&ErrInvalidCandidate.err())));
    assert!(IsCandidateRejection(Some(&GoError::wrap("candidate: x", ErrInvalidCandidate.err()))));
    assert!(IsCandidateRejection(Some(&ErrNonImprovingCandidate.err())));
    assert!(!IsCandidateRejection(Some(&context::Canceled.err())));
    assert!(!IsCandidateRejection(Some(&GoError::new("arbitrary"))));
}

#[test]
fn TestTransactionCommitObservesCanceledContext() {
    let g = NewGraph();
    let n = placed(1, 1., 2., 10., 10.);
    g.AddNewNodeToContainer(None, &n);
    let (ctx, cancel) = context::WithCancel(&context::Background());
    cancel();
    let called = std::rc::Rc::new(std::cell::Cell::new(false));
    let txn = mustNewTransaction(&g, TransactionOptions::default());
    let (c2, n2) = (called.clone(), n.clone());
    txn.AddOp(move || {
        c2.set(true);
        n2.set_x(100.);
        Ok(())
    });
    let err = txn.Commit(&ctx).unwrap_err();
    assert!(err.Is(&context::Canceled));
    assert!(!called.get());
    assert_eq!(n.x(), 1.0);
}

#[test]
fn TestTransactionCommitRollsBackWhenContextCanceledByOperation() {
    let g = NewGraph();
    let n = placed(1, 1., 2., 10., 10.);
    g.AddNewNodeToContainer(None, &n);
    let (ctx, cancel) = context::WithCancel(&context::Background());
    let second = std::rc::Rc::new(std::cell::Cell::new(false));
    let txn = mustNewTransaction(&g, TransactionOptions::default());
    let n2 = n.clone();
    txn.AddOp(move || {
        n2.set_x(100.);
        cancel();
        Ok(())
    });
    let s2 = second.clone();
    txn.AddOp(move || {
        s2.set(true);
        Ok(())
    });
    let err = txn.Commit(&ctx).unwrap_err();
    assert!(err.Is(&context::Canceled));
    assert!(!second.get());
    assert_eq!(n.x(), 1.0);
}

#[test]
fn TestTransactionUpdateStateAdvancesRollbackPoint() {
    let g = NewGraph();
    let n = placed(1, 1., 2., 10., 10.);
    g.AddNewNodeToContainer(None, &n);
    let txn = mustNewTransaction(&g, TransactionOptions::default());
    let n2 = n.clone();
    txn.AddOp(move || {
        n2.set_x(10.);
        Ok(())
    });
    txn.Commit(&context::Background()).unwrap();
    txn.Clear();
    txn.UpdateState().unwrap();
    let n3 = n.clone();
    txn.AddOp(move || {
        n3.set_x(100.);
        Err(GoError::new("reject"))
    });
    assert!(txn.Commit(&context::Background()).is_err());
    assert_eq!(n.x(), 10.0);
}

#[test]
fn TestTransactionRollbackRestoresTopLeftPointerIdentity() {
    let g = NewGraph();
    let with = NewNode(1, 10., 10.);
    let original = pt(1., 2.);
    with.set_tl(Some(original.clone()));
    let without = NewNode(2, 10., 10.);
    g.AddNewNodeToContainer(None, &with);
    g.AddNewNodeToContainer(None, &without);
    let txn = mustNewTransaction(&g, TransactionOptions::default());
    let (w2, wo2) = (with.clone(), without.clone());
    txn.AddOp(move || {
        w2.set_tl(Some(pt(100., 200.)));
        wo2.set_tl(Some(pt(300., 400.)));
        Err(GoError::new("reject"))
    });
    assert!(txn.Commit(&context::Background()).is_err());
    assert!(with.tl().unwrap().ptr_eq(&original));
    assert_eq!(with.tlv(), geo::Point::new(1., 2.));
    assert!(without.tl().is_none());
}

#[test]
fn TestTransactionRollbackRestoresClusterPolicy() {
    let g = NewGraph();
    let vessel = placed(1, 0., 0., 10., 10.);
    g.AddNewNodeToContainer(None, &vessel);
    let cluster = newCluster(vec![], None, &g);
    {
        let mut c = cluster.bm();
        c.Vessel = vessel.clone();
        c.DesiredArrangement = Column;
        c.Padding = 12.;
    }
    g.clusters().set(vessel.clone(), cluster.clone());
    let txn = mustNewTransaction(&g, TransactionOptions::default());
    let c2 = cluster.clone();
    txn.AddOp(move || {
        let mut c = c2.bm();
        c.Arrangement = Column;
        c.DesiredArrangement = Row;
        c.Padding = 99.;
        Err(GoError::new("reject"))
    });
    assert!(txn.Commit(&context::Background()).is_err());
    let c = cluster.b();
    assert_eq!(c.Arrangement, Row);
    assert_eq!(c.DesiredArrangement, Column);
    assert_eq!(c.Padding, 12.);
}

#[test]
fn TestTransactionRollbackRestoresEdgeGeometryAndTreeOrientation() {
    let g = NewGraph();
    let a = placed(1, 0., 0., 10., 10.);
    let b = placed(2, 20., 0., 10., 10.);
    g.AddNewNodeToContainer(None, &a);
    g.AddNewNodeToContainer(None, &b);
    let edge = g.Connect(&a, &b);
    let first = pt(10., 5.);
    edge.bm().Points = vec![first.clone(), pt(20., 5.)];
    let tree = NewTree(&a);
    tree.bm().Orientation = geo::Right;
    g.trees().set(Some(a.clone()), vec![tree.clone()]);
    g.nodeToTree().set(a.clone(), tree.clone());
    let txn = mustNewTransaction(&g, TransactionOptions { AffectEdgeRoutes: true, ..Default::default() });
    let (e2, f2, t2) = (edge.clone(), first.clone(), tree.clone());
    txn.AddOp(move || {
        f2.bm().X = 999.;
        e2.bm().Points[0] = pt(777., 888.);
        e2.bm().Points.push(pt(500., 500.));
        t2.bm().Orientation = geo::Left;
        Err(GoError::new("reject"))
    });
    assert!(txn.Commit(&context::Background()).is_err());
    assert_eq!(edge.points().len(), 2);
    assert!(edge.points()[0].ptr_eq(&first));
    assert_eq!(edge.points()[0].b().X, 10.);
    assert_eq!(tree.b().Orientation, geo::Right);
}

#[test]
fn TestTransactionRejectsNewOverlap() {
    let g = NewGraph();
    let a = placed(1, 0., 0., 10., 10.);
    let b = placed(2, 100., 0., 10., 10.);
    g.AddNewNodeToContainer(None, &a);
    g.AddNewNodeToContainer(None, &b);
    let txn = mustNewTransaction(&g, TransactionOptions::default());
    let b2 = b.clone();
    txn.AddOp(move || {
        b2.set_x(15.);
        Ok(())
    });
    let err = txn.Commit(&context::Background()).unwrap_err();
    assert!(err.Is(&ErrInvalidCandidate));
    assert_eq!(b.x(), 100.);
}

// ---------------------------------------------------------------- graph / edges

#[test]
fn TestGraphSplitting() {
    let ctx = context::Background();
    let g = NewGraph();
    for i in 1..=6 {
        g.AddNewNodeToContainer(None, &NewNode(i, 10., 10.));
    }
    g.connectByID(1, 2);
    g.connectByID(2, 3);
    g.connectByID(4, 5);
    let subs = g.SplitSubgraphs(&ctx, SplitOptions::default()).unwrap();
    let got: Vec<Vec<EntityID>> = subs.iter().map(|s| s.nodes().iter().map(|n| n.id()).collect()).collect();
    assert_eq!(got, vec![vec![1, 2, 3], vec![4, 5], vec![6]]);
    for s in &subs {
        assert!(s.b().Containers.ptr_eq(&g.b().Containers));
        for n in s.nodes() {
            assert!(n.graph().as_ref() == Some(s));
        }
    }
    assert_eq!(subs[0].edges().len(), 2);
}

#[test]
fn TestEmptyGraphCostsAreFinite() {
    let g = NewGraph();
    assert_eq!(g.CrossingCost(), 0.);
    assert_eq!(g.TurnCost(), 0.);
    assert_eq!(g.NonCenterPortCost(), 0.);
}

#[test]
fn TestCostWeightBits() {
    // Go folds 0.48*0.48*0.48 exactly before rounding.
    assert_eq!(CrossingCostWeight, 0.110592);
}

#[test]
fn TestEdgeBoundingBoxAndLength() {
    let a = placed(1, 0., 0., 10., 10.);
    let b = placed(2, 100., 0., 10., 10.);
    let e = NewEdge(&a, &b);
    e.bm().Points = vec![pt(10., 5.), pt(50.4, 5.), pt(50.4, 40.6), pt(100., 40.6)];
    let (tl, br) = e.boundingBoxValues();
    assert_eq!((tl.X, tl.Y, br.X, br.Y), (10., 5., 100., 41.));
    assert!((e.Length() - (40.4 + 35.6 + 49.6)).abs() < 1e-9);
    assert!(!e.isDirected());
    e.bm().SourceArrowhead = TriangleArrowhead.into();
    assert!(e.isDirected());
    assert!(e.isTargetedTo(&a));
    let (from, to, ok) = e.directedEndpoints();
    assert!(ok && from.unwrap() == b && to.unwrap() == a);
}

#[test]
fn TestSequenceAdvance() {
    assert_eq!(SequenceAdvance(0.), 0.);
    assert_eq!(SequenceAdvance(-5.), 0.);
    let w = shape::STEP_WEDGE_WIDTH;
    assert_eq!(SequenceAdvance(w), w / 2.);
    assert_eq!(SequenceAdvance(w * 4.), w * 3.);
}

#[test]
fn TestCountSegmentCrossingsEndpointContract() {
    let s = |a: (f64, f64), b: (f64, f64)| CrossingSegment { Start: geo::Point::new(a.0, a.1), End: geo::Point::new(b.0, b.1) };
    assert_eq!(CountSegmentCrossings(&[s((0., 0.), (10., 10.)), s((0., 10.), (10., 0.))]), 1);
    // equal starts are ignored
    assert_eq!(CountSegmentCrossings(&[s((0., 0.), (10., 10.)), s((0., 0.), (10., 0.))]), 0);
    // parallel never crosses
    assert_eq!(CountSegmentCrossings(&[s((0., 0.), (10., 0.)), s((0., 1.), (10., 1.))]), 0);
    let (ctx, cancel) = context::WithCancel(&context::Background());
    cancel();
    let err = CountSegmentCrossingsContext(&ctx, &[s((0., 0.), (1., 1.))]).unwrap_err();
    assert!(err.Is(&context::Canceled));
}

#[test]
fn TestPlacementCostSnapshotRestores() {
    let g = NewGraph();
    g.StoreEdgeLengthCost(1, 2.5);
    let snap = g.SnapshotPlacementCosts();
    g.StoreEdgeLengthCost(2, 3.5);
    g.ResetPlacementCosts();
    assert_eq!(g.EdgeLengthCacheEntries(), 0);
    snap.Restore();
    assert_eq!(g.LookupEdgeLengthCost(1), (2.5, true));
    assert_eq!(g.LookupEdgeLengthCost(2), (0., false));
    let z = NewZeroGraph();
    z.StoreEdgeLengthCost(1, 1.);
    assert_eq!(z.EdgeLengthCacheEntries(), 0);
}

#[test]
fn TestEnginePreflightRejectsCycles() {
    let ctx = context::Background();
    let g = NewGraph();
    let a = NewNode(1, 1., 1.);
    let b = NewNode(2, 1., 1.);
    g.AddNodeUnchecked(&a);
    g.AddNodeUnchecked(&b);
    a.bm().Container = Some(b.clone());
    b.bm().Container = Some(a.clone());
    assert!(Validate(&ctx, "test", &g).is_err());
    b.bm().Container = None;
    assert!(Validate(&ctx, "test", &g).is_ok());
}
