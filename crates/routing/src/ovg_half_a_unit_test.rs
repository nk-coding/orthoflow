// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from the following files of terrastruct/d2, Copyright 2022 Terrastruct Inc.:
//   - d2layouts/d2talalayout/internal/routing/ovg_edge_set_test.go
//   - d2layouts/d2talalayout/internal/routing/ovg_edge_test.go
//   - d2layouts/d2talalayout/internal/routing/ovg_node_test.go
//   - d2layouts/d2talalayout/internal/routing/priority_queue_test.go
//   - d2layouts/d2talalayout/internal/routing/route_test.go
//   - d2layouts/d2talalayout/internal/routing/turns_test.go
// Rust port: Copyright 2026 Niklas Krieger.

//! Ported Go unit tests for routing half a (turns_test, route_test,
//! ovg_edge_test, ovg_node_test, priority_queue_test, ovg_edge_set_test).
use super::*;
use crate::*;
use gocompat::{GoError, GoResult, Sentinel};
use layoutgraph::NodeRef;

fn node(x: f64, y: f64) -> OVGNodeRef {
    NewOVGNode(geo::Point { X: x, Y: y })
}

fn gnode(id: i64, w: f64, h: f64, x: f64, y: f64) -> NodeRef {
    let n = layoutgraph::NewNode(id, w, h);
    n.set_tl(Some(geo::NewPoint(x, y)));
    n
}

#[test]
fn TestIdealTurnAxesOwnsRouteTurnPolicy() {
    let source = gnode(1, 20.0, 20.0, 100.0, 100.0);
    let target = gnode(2, 20.0, 20.0, 0.0, 0.0);
    let want = vec![idealTurnAxis { isX: true, val: 60.0 }, idealTurnAxis { isX: false, val: 60.0 }];
    assert_eq!(idealTurnAxes(&source, &target), want);
    target.set_tl(Some(geo::NewPoint(105.0, 105.0)));
    assert!(idealTurnAxes(&source, &target).is_empty());
}

#[test]
fn TestRouteOpposingColinear() {
    let a = gnode(1, 1.0, 1.0, 0.0, 0.0);
    let b = gnode(2, 1.0, 1.0, 0.0, 0.0);
    let e = layoutgraph::NewEdge(&a, &b);
    e.bm().TargetArrowhead = layoutgraph::TriangleArrowhead.to_string();
    let route = Route { GEdge: e, OVGNodes: vec![node(0.0, 0.0), node(100.0, 0.0), node(100.0, 100.0), node(200.0, 100.0)], FromPort: geo::Point::default(), ToPort: geo::Point::default() };
    let check = |f: (f64, f64), t: (f64, f64), fwd: bool, rev: bool| {
        let (from, to) = (node(f.0, f.1), node(t.0, t.1));
        assert_eq!(route.isOpposingColinear(&from, &to), fwd, "{:?}->{:?}", f, t);
        assert_eq!(route.isOpposingColinear(&to, &from), rev, "{:?}<-{:?}", f, t);
    };
    check((0.0, 0.0), (100.0, 0.0), false, true);
    check((100.0, 0.0), (100.0, 100.0), false, true);
    check((190.0, 0.0), (100.0, 0.0), false, false);
    check((90.0, 0.0), (120.0, 0.0), false, true);
    check((101.0, 0.0), (200.0, 0.0), false, false);
    check((0.0, 0.0), (100.0, 100.0), false, false);
    check((120.0, 100.0), (300.0, 100.0), false, false);
}

#[test]
fn TestIsVertical() {
    assert!(NewOVGEdge(&node(5.0, 5.0), &node(5.0, 15.0)).isVertical());
    assert!(!NewOVGEdge(&node(5.0, 15.0), &node(35.0, 15.0)).isVertical());
    assert!(!NewOVGEdge(&node(5.0, 15.0), &node(35.0, 55.0)).isVertical());
}

#[test]
fn TestDistanceToBoundary() {
    let a = node(0.0, 0.0);
    let b = gnode(1, 1.0, 1.0, 6.0, 0.0);
    assert_eq!(a.distanceToBoundary(&b), 6.0);
    a.bm().Point = geo::Point { X: 10.0, Y: 10.0 };
    b.set_tl(Some(geo::NewPoint(5.0, 6.0)));
    assert_eq!(a.distanceToBoundary(&b), 5.0);
}

#[test]
fn TestAddEdgeAndAdjacent() {
    let n1 = node(0.0, 0.0);
    let n2 = node(1.0, 1.0);
    let e = NewOVGEdge(&n1, &n2);
    n1.addEdge(e.clone());
    assert_eq!(n1.b().Edges.len(), 1);
    assert_eq!(n2.b().Edges.len(), 0);
    assert!(n1.adjacent(&e) == n2);
    assert!(n2.adjacent(&e) == n1);
}

#[test]
fn TestPortDirectionsForObstacleWithoutOwnerAreUnrestricted() {
    let n = node(0.0, 0.0);
    let owner = layoutgraph::NewNode(1, 10.0, 10.0);
    let directions = n.portDirectionsForObstacle(&owner);
    assert_eq!(directions.0, 0);
    assert!(directions.any(|d| d == geo::NONE));
}

static errRejected: Sentinel = Sentinel::new("reject priority queue work");
struct rejecting;
impl workBudget for rejecting {
    fn step(&self) -> GoResult<()> {
        Err(errRejected.err())
    }
    fn add(&self, _: u64) -> GoResult<()> {
        Err(errRejected.err())
    }
    fn check(&self) -> GoResult<()> {
        Err(errRejected.err())
    }
}

#[test]
fn TestPriorityQueue() {
    let n = node(0.0, 0.0);
    let mut q = priorityQueue::default();
    let large = q.push(3.0, &n, false, None).unwrap();
    let medium = q.push(2.0, &n, false, None).unwrap();
    let small = q.push(1.0, &n, false, None).unwrap();
    for want in [small, medium, large] {
        assert_eq!(q.pop(None).unwrap(), want);
    }
    assert!(q.empty());

    // decrease
    let mut q = priorityQueue::default();
    let entry = q.push(3.0, &n, false, None).unwrap();
    q.push(2.0, &n, false, None).unwrap();
    q.decrease(Some(entry), 1.0, None).unwrap();
    assert_eq!(q.pop(None).unwrap(), entry);
    assert!(q.decrease(Some(entry), 0.0, None).is_err());

    // decrease preserves order
    let mut q = priorityQueue::default();
    let first = q.push(1.0, &n, false, None).unwrap();
    let second = q.push(2.0, &n, false, None).unwrap();
    q.decrease(Some(second), 1.0, None).unwrap();
    assert_eq!(q.pop(None).unwrap(), first);
    assert_eq!(q.pop(None).unwrap(), second);

    // guard failures do not mutate
    let mut q = priorityQueue::default();
    let e: GoError = q.push(1.0, &n, false, Some(&rejecting)).unwrap_err();
    assert!(e.Is(&errRejected));
    assert!(q.empty());
    let first = q.push(1.0, &n, false, None).unwrap();
    let _second = q.push(2.0, &n, false, None).unwrap();
    assert!(q.pop(Some(&rejecting)).unwrap_err().Is(&errRejected));
    assert_eq!(q.entry(first).index, 0);
    assert!(q.decrease(Some(first), 0.5, Some(&rejecting)).unwrap_err().Is(&errRejected));
    assert_eq!(q.entry(first).priority, 1.0);

    // equal priority order
    let mut q = priorityQueue::default();
    let hs: Vec<_> = (0..4).map(|i| q.push(1.0, &n, i % 2 == 0, None).unwrap()).collect();
    for want in hs {
        assert_eq!(q.pop(None).unwrap(), want);
    }

    // reset reuses entries
    let mut q = priorityQueue::default();
    let first = q.push(1.0, &n, false, None).unwrap();
    q.reset();
    assert_eq!(q.push(2.0, &n, false, None).unwrap(), first);
}

#[test]
fn TestPriorityQueueAcrossEntryChunks() {
    let n = node(0.0, 0.0);
    let size = priorityQueueEntryChunkSize * 2 + 1;
    let mut q = priorityQueue::default();
    let mut model: Vec<(usize, f64, usize)> = Vec::new();
    for i in 0..size {
        let priority = ((i * 37) % 100) as f64;
        let e = q.push(priority, &n, i % 2 == 0, None).unwrap();
        model.push((e, priority, i));
    }
    let mut i = 0;
    while i < model.len() {
        let priority = -((i + 1) as f64);
        q.decrease(Some(model[i].0), priority, None).unwrap();
        model[i].1 = priority;
        i += 17;
    }
    model.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap().then(a.2.cmp(&b.2)));
    for want in model {
        assert_eq!(q.pop(None).unwrap(), want.0);
    }
}

fn intersectsWith(set: &ovgEdgeSet, e: &OVGEdgeRef) -> bool {
    set.intersectsWithChecked(e, None).unwrap()
}

fn edge(a: (f64, f64), b: (f64, f64)) -> OVGEdgeRef {
    NewOVGEdge(&node(a.0, a.1), &node(b.0, b.1))
}

#[test]
fn TestOvgEdgeSet() {
    let mut set = newOvgEdgeSet();
    let vertical = edge((5.0, 5.0), (5.0, 15.0));
    set.add(&vertical);
    assert_eq!(set.verticalEdges.len(), 1);
    assert_eq!(set.horizontalEdges.len(), 0);
    let horizontal = edge((5.0, 5.0), (15.0, 5.0));
    set.add(&horizontal);
    assert_eq!(set.horizontalEdges.len(), 1);
    set.add(&edge((5.0, 10.0), (15.0, 5.0)));
    assert_eq!(set.horizontalEdges.len(), 1);
    assert_eq!(set.verticalEdges.len(), 1);

    let mut set = newOvgEdgeSet();
    let v1 = edge((5.0, 5.0), (5.0, 15.0));
    let v2 = edge((5.0, 25.0), (5.0, 39.0));
    let v3 = edge((7.0, 5.0), (7.0, 15.0));
    let h1 = edge((5.0, 5.0), (7.0, 5.0));
    let h2 = edge((20.0, 25.0), (30.0, 25.0));
    let h3 = edge((0.0, 40.0), (50.0, 40.0));
    for e in [&v1, &v2, &v3, &h1, &h2, &h3] {
        set.add(e);
    }
    assert!(!intersectsWith(&set, &v1));
    assert!(!intersectsWith(&set, &h3));
    assert!(!intersectsWith(&set, &h1));
    assert!(!intersectsWith(&set, &edge((50.0, 40.0), (0.0, 40.0))));
    assert!(intersectsWith(&set, &edge((6.0, 4.0), (6.0, 10.0))));
    assert!(intersectsWith(&set, &edge((5.0, 19.0), (5.0, 50.0))));
    assert!(intersectsWith(&set, &edge((1.0, 9.0), (19.0, 9.0))));
    assert!(!intersectsWith(&set, &edge((6.0, 0.0), (6.0, 4.0))));
    assert!(!intersectsWith(&set, &edge((6.0, 39.0), (30.0, 39.0))));
    assert!(!intersectsWith(&set, &edge((0.0, 0.0), (50.0, 50.0))));

    let ov = set.overlappingEdgesChecked(&h1, None).unwrap();
    assert_eq!(ov.len(), 1);
    assert!(ov[0] == h1);
    let mut ov = set.overlappingEdgesChecked(&edge((5.0, 14.0), (5.0, 25.0)), None).unwrap();
    ov.sort_by(|a, b| a.From.Y().partial_cmp(&b.From.Y()).unwrap());
    assert_eq!(ov.len(), 2);
    assert!(ov[0] == v1 && ov[1] == v2);
    assert_eq!(set.overlappingEdgesChecked(&edge((6.0, 0.0), (6.0, 80.0)), None).unwrap().len(), 0);
    assert_eq!(set.overlappingEdgesChecked(&edge((0.0, 0.0), (50.0, 50.0)), None).unwrap().len(), 0);
}
