// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from the following files of terrastruct/d2, Copyright 2022 Terrastruct Inc.:
//   - d2layouts/d2talalayout/internal/placement/extracted_kernels_test.go
//   - d2layouts/d2talalayout/internal/placement/layout_invariants_test.go
//   - d2layouts/d2talalayout/internal/placement/sized_optimizer_test.go
//   - d2layouts/d2talalayout/internal/placement/sizeless_optimizer_test.go
//   - d2layouts/d2talalayout/internal/placement/stress_initialize_test.go
// Rust port: Copyright 2026 Niklas Krieger.

//! Ported Go unit tests for placement half a (kernels that do not need
//! placementcost scoring).
#![allow(non_snake_case)]
extern crate orthoflow_placement as placement;
use gocompat::context;
use gocompat::rand;
use gocompat::Set;
use layoutgraph::{GraphRef, NodeRef};
use placement::*;

fn node(g: &GraphRef, id: i64, w: f64, h: f64, x: f64, y: f64) -> NodeRef {
    let n = g.AddNode(&layoutgraph::NewNode(id, w, h));
    n.set_tl(Some(geo::NewPoint(x, y)));
    n
}

fn pt(x: f64, y: f64) -> geo::Point {
    geo::Point { X: x, Y: y }
}

fn guard() -> limits::OptimizationWorkGuardRef {
    limits::NewOptimizationWorkGuard(&context::Background(), "LocalOptimize", limits::MaxOptimizationWorkUnits).unwrap()
}

fn starGraph(id5: i64) -> (GraphRef, Vec<NodeRef>) {
    let g = layoutgraph::NewGraph();
    let n1 = node(&g, 1, 10.0, 10.0, 1.0, 0.0);
    let n2 = node(&g, 2, 10.0, 10.0, 0.0, 1.0);
    let n3 = node(&g, 3, 10.0, 10.0, 1.0, 2.0);
    let n4 = node(&g, 4, 10.0, 10.0, 2.0, 1.0);
    let n5 = node(&g, id5, 10.0, 10.0, 0.0, 0.0);
    (g, vec![n1, n2, n3, n4, n5])
}

#[test]
fn TestPlacementPoints() {
    let (g, n) = starGraph(0);
    for i in 0..4 {
        g.Connect(&n[i], &n[4]);
    }
    let r = rand::New(rand::NewSource(1));
    let optim = newSizelessOptimizer(&context::Background(), &g, Some(&r)).unwrap();
    let ctx = context::Background();
    let median = pt(2.0, 2.0);
    let distance = optim.FindClosestUnoccupiedDistance(&ctx, &n[4], &median).unwrap();
    let points = optim.placementPointsGuarded(&n[4], &median, distance, &guard()).unwrap();
    assert_eq!(points.len(), 3);
    let mut expected: Set<geo::Point> = [pt(2.0, 2.0), pt(2.0, 3.0), pt(3.0, 2.0)].into_iter().collect();
    for p in &points {
        expected.remove(p);
    }
    assert!(expected.is_empty());
}

#[test]
fn TestFindClosestUnoccupiedDistance() {
    let (g, n) = starGraph(0);
    for i in 0..4 {
        g.Connect(&n[i], &n[4]);
    }
    let r = rand::New(rand::NewSource(1));
    let mut optim = newSizelessOptimizer(&context::Background(), &g, Some(&r)).unwrap();
    for x in &n {
        assert!(optim.isOccupied(&x.tlv()));
    }
    assert!(!optim.isOccupied(&pt(1.0, 1.0)));
    assert!(!optim.isOccupied(&pt(2.0, 2.0)));
    let ctx = context::Background();
    assert_eq!(optim.FindClosestUnoccupiedDistance(&ctx, &n[4], &pt(2.0, 2.0)).unwrap(), 0.0);
    assert_eq!(optim.FindClosestUnoccupiedDistance(&ctx, &n[4], &pt(1.0, 2.0)).unwrap(), 1.0);
    n[4].set_x(1.0);
    n[4].set_y(1.0);
    optim.resetOccupied();
    assert_eq!(optim.FindClosestUnoccupiedDistance(&ctx, &n[4], &pt(1.0, 1.0)).unwrap(), 2.0);
}

#[test]
fn TestCanOptimizeNode() {
    let g = layoutgraph::NewGraph();
    let n = layoutgraph::NewNode(1, 10.0, 10.0);
    assert!(!canOptimizeNodeGuarded(&n, &g, &guard()).unwrap());
    n.bm().FixedTopLeft = Some(geo::NewPoint(10.0, 10.0));
    assert!(!canOptimizeNodeGuarded(&n, &g, &guard()).unwrap());

    let n = layoutgraph::NewNode(3, 10.0, 10.0);
    g.Connect(&n, &layoutgraph::NewNode(2, 10.0, 10.0));
    g.b().NodeToTree.set(n.clone(), layoutgraph::NewTree(&n));
    assert!(!canOptimizeNodeGuarded(&n, &g, &guard()).unwrap());

    let n = layoutgraph::NewNode(4, 10.0, 10.0);
    g.Connect(&n, &layoutgraph::NewNode(2, 10.0, 10.0));
    assert!(canOptimizeNodeGuarded(&n, &g, &guard()).unwrap());

    let n = layoutgraph::NewNode(4, 10.0, 10.0);
    n.bm().Nears.insert(layoutgraph::NewNode(5, 10.0, 10.0));
    assert!(canOptimizeNodeGuarded(&n, &g, &guard()).unwrap());
}

#[test]
fn TestSwapCandidates() {
    let (g, n) = starGraph(5);
    g.Connect(&n[0], &n[1]);
    g.Connect(&n[2], &n[3]);
    g.Connect(&n[2], &n[4]);
    let r = rand::New(rand::NewSource(1));
    g.ComputeCellSize();
    let optim = newSizelessOptimizer(&context::Background(), &g, Some(&r)).unwrap();
    let a = optim.swapCandidatesGuarded(&n[0], &guard()).unwrap();
    assert_eq!(a, vec![n[4].clone()]);
    let a = optim.swapCandidatesGuarded(&n[1], &guard()).unwrap();
    assert_eq!(a, vec![n[4].clone()]);
    assert!(optim.swapCandidatesGuarded(&n[3], &guard()).unwrap().is_empty());
    assert!(optim.swapCandidatesGuarded(&n[2], &guard()).unwrap().is_empty());
    let a = optim.swapCandidatesGuarded(&n[4], &guard()).unwrap();
    assert_eq!(a, vec![n[1].clone(), n[0].clone()]);
    n[0].bm().FixedTopLeft = Some(geo::NewPoint(1.0, 0.0));
    let a = optim.swapCandidatesGuarded(&n[4], &guard()).unwrap();
    assert_eq!(a, vec![n[1].clone()]);
}

#[test]
fn TestIterPlacementsAroundPoint() {
    let graph = layoutgraph::NewGraph();
    let n = layoutgraph::NewNode(1, 10.0, 10.0);
    graph.AddNodeUnchecked(&n);
    graph.bm().CellSize = 2.0;
    let mut points: Vec<geo::Point> = Vec::new();
    iterPlacementsAroundPoint(&n, 5.0, 5.0, true, &mut |x, y| {
        points.push(pt(x, y));
        false
    });
    let expected = [
        (0, 0), (0, 10), (10, 0), (10, 10), (2, 0), (4, 0), (6, 0), (8, 0), (0, 2), (0, 4), (0, 6), (0, 8), (10, 2),
        (10, 4), (10, 6), (10, 8), (2, 10), (4, 10), (6, 10), (8, 10),
    ];
    for (x, y) in expected {
        assert!(points.contains(&pt(x as f64, y as f64)), "point {x},{y} not found");
    }
}

fn sizedStar(connected: bool, withN6: bool) -> (GraphRef, Vec<NodeRef>) {
    let g = layoutgraph::NewGraph();
    let n1 = node(&g, 1, 50.0, 50.0, 100.0, 0.0);
    let n2 = node(&g, 2, 50.0, 50.0, 0.0, 100.0);
    let n3 = node(&g, 3, 50.0, 50.0, 200.0, 100.0);
    let n4 = node(&g, 4, 50.0, 50.0, 100.0, 200.0);
    let n5 = node(&g, 5, 50.0, 50.0, 0.0, 0.0);
    if connected {
        for x in [&n1, &n2, &n3, &n4] {
            g.Connect(x, &n5);
        }
    }
    let mut v = vec![n1, n2, n3, n4, n5];
    if withN6 {
        v.push(node(&g, 6, 50.0, 50.0, 100.0, 100.0));
    }
    g.ComputeCellSize();
    (g, v)
}

fn closest(g: &GraphRef, n: &NodeRef, p: geo::Point) -> f64 {
    let r = rand::New(rand::NewSource(1));
    let mut optim = newSizedOptimizer(&context::Background(), g, None, None, Some(&r), &[]).unwrap();
    let gd = guard();
    optim.rebuildSpatialIndex(&gd).unwrap();
    optim.findClosestUnoccupiedDistanceGuarded(n, &p, true, None, &gd).unwrap()
}

#[test]
fn TestSizedFindClosestUnoccupiedDistance() {
    let (g, n) = sizedStar(true, false);
    assert_eq!(closest(&g, &n[4], n[4].tlv()), 0.0);
    assert_eq!(closest(&g, &n[4], pt(100.0, 100.0)), 4.0);
    assert_eq!(closest(&g, &n[4], n[3].tlv()), 3.0);
    let n6 = node(&g, 6, 50.0, 50.0, 100.0, 100.0);
    g.ComputeCellSize();
    assert_eq!(closest(&g, &n[4], n6.tlv()), 4.0);
}

#[test]
fn TestFindClosestUnoccupiedDistanceNoConnections() {
    let (g, n) = sizedStar(false, false);
    assert_eq!(closest(&g, &n[4], n[4].tlv()), 0.0);
    assert_eq!(closest(&g, &n[4], pt(100.0, 100.0)), 0.0);
    assert_eq!(closest(&g, &n[4], n[3].tlv()), 1.0);
    let n6 = node(&g, 6, 50.0, 50.0, 100.0, 100.0);
    g.ComputeCellSize();
    assert_eq!(closest(&g, &n[4], n6.tlv()), 3.0);
}

#[test]
fn TestSizedOptimizerGetPlacementPoints() {
    let (g, n) = sizedStar(true, false);
    let r = rand::New(rand::NewSource(1));
    let optim = newSizedOptimizer(&context::Background(), &g, None, None, Some(&r), &[]).unwrap();
    let mut scratch = placementPointsScratch::default();
    optim.fillPlacementPointsGuarded(&n[4], &pt(100.0, 100.0), 1.0, true, &mut scratch, &guard()).unwrap();
    assert_eq!(scratch.points.len(), 24);
    for p in &scratch.points {
        assert_eq!(p.X % optim.cellSize, 0.0);
        assert_eq!(p.Y % optim.cellSize, 0.0);
    }
}

fn stressTestGraph(n: i64, connected: bool) -> GraphRef {
    let g = layoutgraph::NewGraph();
    for i in 0..n {
        g.AddNode(&layoutgraph::NewNode(i, 100.0, 50.0));
    }
    if connected {
        let nodes = g.b().Nodes.clone();
        for i in 1..nodes.len() {
            g.Connect(&nodes[i - 1], &nodes[i]);
        }
    }
    g
}

#[test]
fn TestGraphDistanceInitialization() {
    let g = stressTestGraph(12, true);
    assert!(initializeByGraphDistance(&context::Background(), &g).unwrap());
    let nodes = g.b().Nodes.clone();
    let mut seen: Set<geo::Point> = Set::default();
    let mut prior = Vec::new();
    for n in &nodes {
        let p = n.tlv();
        assert!(p.X % 1.0 == 0.0 && p.Y % 1.0 == 0.0 && !seen.contains(&p));
        seen.insert(p);
        prior.push(p);
    }
    let d = |a: &NodeRef, b: &NodeRef| ((a.x() - b.x()).powi(2) + (a.y() - b.y()).powi(2)).sqrt();
    assert!(d(&nodes[0], &nodes[11]) >= 2.0 * d(&nodes[0], &nodes[1]));
    assert!(initializeByGraphDistance(&context::Background(), &g).unwrap());
    for (i, n) in nodes.iter().enumerate() {
        assert_eq!(n.tlv(), prior[i]);
    }
}

#[test]
fn TestDirectionTransforms() {
    let c = |left, right, top, bottom| directionCounts { left, right, top, bottom };
    let tests = [
        (c(0, 0, 0, 0), geo::Right, false, false),
        (c(0, 1, 0, 0), geo::Right, false, false),
        (c(0, 1, 0, 0), geo::Left, true, false),
        (c(0, 0, 0, 1), geo::Top, false, true),
        (c(0, 0, 1, 0), geo::Bottom, false, true),
        (c(2, 0, 1, 0), geo::Right, true, true),
        (c(1, 1, 1, 0), geo::Right, false, true),
        (c(1, 0, 0, 1), geo::Bottom, true, false),
        (c(1, 0, 0, 1), geo::NONE, true, false),
    ];
    for (counts, direction, mx, my) in tests {
        let got = counts.transformsTo(direction);
        assert_eq!((got.mirrorX, got.mirrorY), (mx, my), "{counts:?} {direction:?}");
    }
    let a = directionCount { direction: geo::Right, count: 2 };
    assert_eq!(compareDirectionCounts(&a, &a, geo::Right), 0);
}

#[test]
fn TestValidateGridAlignment() {
    let g = layoutgraph::NewGraph();
    g.bm().CellSize = 10.0;
    let n = node(&g, 1, 10.0, 10.0, 15.0, 0.0);
    assert!(validateGridAlignment(&g).is_err());
    n.set_x(20.0);
    assert!(validateGridAlignment(&g).is_ok());
    g.bm().CellSize = 0.5;
    assert!(validateCellSize(&g).is_err());
}
