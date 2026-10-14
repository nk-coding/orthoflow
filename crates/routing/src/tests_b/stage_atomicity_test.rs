// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from the following files of terrastruct/d2, Copyright 2022 Terrastruct Inc.:
//   - d2layouts/d2talalayout/internal/routing/post_route_atomicity_test.go
//   - d2layouts/d2talalayout/internal/routing/routing_cancellation_test.go
//   - d2layouts/d2talalayout/internal/routing/stage_context_atomicity_test.go
//   - d2layouts/d2talalayout/internal/routing/straight_fallback_atomicity_test.go
// Rust port: Copyright 2026 Niklas Krieger.

//! Ports of routing/post_route_atomicity_test.go, stage_context_atomicity_test.go
//! and straight_fallback_atomicity_test.go. Go's custom-context probes
//! (cancel/panic when a route mutates) cannot be expressed with gocompat
//! contexts; the work-limit variants cover the same rollback boundary.
use super::*;
use layoutgraph::{Cluster, ClusterRef, EdgeAbduction, EdgeAbductionRef};

pub(crate) fn simplificationMutationGraph() -> (GraphRef, EdgeRef) {
    let g = layoutgraph::NewGraph();
    let from = nodeAt(1, 10.0, 10.0, 1_000.0, 1_000.0);
    let to = nodeAt(2, 10.0, 10.0, 2_000.0, 2_000.0);
    g.AddNewNodeToContainer(None, &from);
    g.AddNewNodeToContainer(None, &to);
    let edge = g.Connect(&from, &to);
    setPoints(&edge, &[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (20.0, 10.0), (20.0, 0.0)]);
    (g, edge)
}

fn balanceMutationGraph() -> (GraphRef, EdgeRef) {
    let g = layoutgraph::NewGraph();
    let top = nodeAt(1, 1_000.0, 1_000.0, 0.0, 0.0);
    let bottom = nodeAt(2, 1_000.0, 1_000.0, 0.0, 2_000.0);
    g.AddNewNodeToContainer(None, &top);
    g.AddNewNodeToContainer(None, &bottom);
    let edge = g.Connect(&top, &bottom);
    setPoints(&edge, &[(1.0, 1_000.0), (1.0, 2_000.0)]);
    (g, edge)
}

fn branchingMutationGraph() -> (GraphRef, EdgeRef) {
    let g = layoutgraph::NewGraph();
    let external = nodeAt(1, 10.0, 10.0, 10_000.0, 10_000.0);
    let left = nodeAt(2, 10.0, 10.0, 11_000.0, 11_000.0);
    let right = nodeAt(3, 10.0, 10.0, 12_000.0, 12_000.0);
    for node in [&external, &left, &right] {
        g.AddNewNodeToContainer(None, node);
    }
    let vessel = nodeAt(4, 10.0, 10.0, 11_000.0, 11_000.0);
    vessel.bm().Graph = Some(g.clone());
    let cluster = ClusterRef::new(Cluster {
        Vessel: vessel.clone(),
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
    g.b().Clusters.set(vessel, cluster);

    let leftEdge = g.Connect(&external, &left);
    setPoints(&leftEdge, &[(0.0, 0.0), (0.0, -10.0), (-100.0, -10.0), (-100.0, 100.0)]);
    let rightEdge = g.Connect(&external, &right);
    setPoints(&rightEdge, &[(0.0, 0.0), (0.0, 10.0), (100.0, 10.0), (100.0, -5.0)]);
    (g, leftEdge)
}

pub(crate) fn swapMutationGraph() -> (GraphRef, EdgeRef) {
    let g = layoutgraph::NewGraph();
    let container = g.AddNode(&layoutgraph::NewNode(1, 200.0, 100.0));
    container.set_tl(Some(pt(0.0, 150.0)));
    let remote = g.AddNode(&layoutgraph::NewNode(2, 50.0, 50.0));
    remote.set_tl(Some(pt(200.0, 0.0)));
    let node = g.AddNode(&layoutgraph::NewNode(3, 50.0, 50.0));
    node.set_tl(Some(pt(100.0, 175.0)));

    let first = g.Connect(&node, &remote);
    setPoints(&first, &[(138.0, 175.0), (138.0, 25.0), (200.0, 25.0)]);
    let second = g.Connect(&remote, &node);
    setPoints(&second, &[(225.0, 50.0), (225.0, 75.0), (112.0, 75.0), (112.0, 175.0)]);
    g.AddNodeToContainer(None, &container);
    g.AddNodeToContainer(None, &remote);
    g.AddNodeToContainer(Some(&container), &node);
    (g, first)
}

fn hostileSegmentGraph(vertical: bool) -> (GraphRef, EdgeRef) {
    let g = layoutgraph::NewGraph();
    let from = nodeAt(1, 10.0, 10.0, 10_000.0, 10_000.0);
    let to = nodeAt(2, 10.0, 10.0, 20_000.0, 20_000.0);
    g.AddNewNodeToContainer(None, &from);
    g.AddNewNodeToContainer(None, &to);
    let edge = g.Connect(&from, &to);
    let mut points = Vec::new();
    for index in 0..129 {
        if vertical {
            points.push((0.0, index as f64));
        } else {
            points.push((index as f64, 0.0));
        }
    }
    setPoints(&edge, &points);
    (g, edge)
}

/// swap_ports_test.go's swapEdgePorts helper.
pub(crate) fn swapEdgePorts(ctx: &Ctx, n: &NodeRef) -> gocompat::GoResult<bool> {
    let mut swapped = false;
    let g = n.graph().expect("TALA SwapEdgePorts requires a graph node");
    runAtomicRouteStage(ctx, "SwapEdgePorts", &g, &n.edges(), maxRouteStageWorkUnits, |guard| {
        swapped = swapEdgePortsGuarded(n, guard)?;
        Ok(())
    })?;
    Ok(swapped)
}

#[test]
fn TestPostRouteStagesCanceledBeforeWork() {
    type mk = fn() -> (GraphRef, EdgeRef);
    let tests: Vec<(&str, mk, fn(&Ctx, &GraphRef) -> gocompat::GoResult<()>)> = vec![
        ("SimplifyEdgeRoutes", simplificationMutationGraph, |c, g| SimplifyEdgeRoutes(c, g)),
        ("BalanceEdgeSegments", balanceMutationGraph, |c, g| BalanceEdgeSegments(c, g)),
        ("FixClusterEdgeBranching", branchingMutationGraph, |c, g| FixClusterEdgeBranching(c, g)),
        ("SwapEdgePorts", swapMutationGraph, |c, g| SwapAllEdgePorts(c, g)),
    ];
    for (location, make, run) in tests {
        let (graph, edge) = make();
        let snapshot = captureExactRouteTest(&edge);
        let err = run(&canceledContext(), &graph).unwrap_err();
        requireCanceled(&err);
        let _ = location;
        snapshot.assertRestored();
    }
}

#[test]
fn TestPostRouteWorkLimitAfterMutationRestoresExactRoute() {
    let (graph, edge) = simplificationMutationGraph();
    let snapshot = captureExactRouteTest(&edge);
    // Geometry validation consumes seven units and the first simplification
    // commits tentatively at unit twelve. The next guarded operation crosses
    // this injected limit, proving resource failures roll the mutation back.
    let err = simplifyEdgeRoutesWithLimit(&bg(), &graph, 12).unwrap_err();
    assert!(err.Error().contains("work exceeds limit 12"), "stage error = {err}; want post-mutation work-limit error");
    snapshot.assertRestored();
}

#[test]
fn TestPostRouteStagesEnforceInjectedWorkLimits() {
    type mk = Box<dyn Fn() -> (GraphRef, EdgeRef)>;
    type run = fn(&GraphRef, &routeWorkGuardRef) -> gocompat::GoResult<()>;
    let tests: Vec<(&str, mk, run)> = vec![
        ("simplify hostile segments", Box::new(|| hostileSegmentGraph(false)), |g, guard| {
            simplifyPoints(g, &g.edges()[0], guard).map(|_| ())
        }),
        ("balance hostile segments", Box::new(|| hostileSegmentGraph(true)), |g, guard| balanceEdgeSegmentsGuarded(g, guard)),
        ("cluster hostile candidates", Box::new(branchingMutationGraph), |g, guard| fixClusterEdgeBranchingGuarded(g, guard)),
        ("swap hostile pairs", Box::new(swapMutationGraph), |g, guard| {
            for node in g.nodes() {
                swapEdgePortsGuarded(&node, guard)?;
            }
            Ok(())
        }),
    ];
    for (name, make, run) in tests {
        let (graph, _) = make();
        let guard = newRouteWorkGuard(&bg(), name, 64).unwrap();
        let err = run(&graph, &guard).unwrap_err();
        assert!(err.Error().contains("work exceeds limit"), "{name}: kernel error = {err}; want work-limit error");
    }
}

/// Go stops at the first limit whose failure follows an observed tentative
/// mutation; the port checks exact restoration for every failing limit.
#[test]
fn TestPostRouteStageWorkLimitsRollBackAfterMutation() {
    type mk = fn() -> (GraphRef, EdgeRef);
    let tests: Vec<(&str, mk, fn(&Ctx, &GraphRef, u64) -> gocompat::GoResult<()>)> = vec![
        ("simplify", simplificationMutationGraph, |c, g, l| simplifyEdgeRoutesWithLimit(c, g, l)),
        ("balance", balanceMutationGraph, |c, g, l| balanceEdgeSegmentsWithLimit(c, g, l)),
        ("cluster branching", branchingMutationGraph, |c, g, l| fixClusterEdgeBranchingWithLimit(c, g, l)),
        ("swap ports", swapMutationGraph, |c, g, l| swapAllEdgePortsWithWorkLimit(c, g, l)),
    ];
    for (name, make, run) in tests {
        let mut succeeded = false;
        let mut failures = 0;
        for limit in 1..=10_000u64 {
            let (graph, edge) = make();
            let snapshot = captureExactRouteTest(&edge);
            match run(&bg(), &graph, limit) {
                Ok(()) => {
                    succeeded = true;
                    break;
                }
                Err(err) => {
                    assert!(err.Is(&errRouteStageWorkLimit), "{name}: unexpected error {err}");
                    snapshot.assertRestored();
                    failures += 1;
                }
            }
        }
        assert!(succeeded && failures > 0, "{name}: no successful limit found");
    }
}

#[test]
fn TestRouteStagePreflightCapsExtraEdgesBeforeSnapshot() {
    let graph = layoutgraph::NewGraph();
    let from = nodeAt(1, 10.0, 10.0, 0.0, 0.0);
    let to = nodeAt(2, 10.0, 10.0, 100.0, 0.0);
    let edge = layoutgraph::NewEdge(&from, &to);
    let tooManyReferences = vec![edge.clone(); layoutgraph::MaxTopologyReferences as usize + 1];
    let err = runAtomicRouteStage(&bg(), "extra references", &graph, &tooManyReferences, maxRouteStageWorkUnits, |_| {
        panic!("oversized extra-edge list reached the mutation callback")
    })
    .unwrap_err();
    assert!(err.Error().contains("extra edge references exceed limit"), "reference error = {err}");

    edge.bm().Points = (0..=layoutgraph::MaxRoutePoints).map(|i| pt(i as f64, 0.0)).collect();
    let err = runAtomicRouteStage(&bg(), "extra capacity", &graph, &[edge.clone()], maxRouteStageWorkUnits, |_| {
        panic!("oversized extra route reached the mutation callback")
    })
    .unwrap_err();
    assert!(err.Error().contains("route point capacity exceeds limit"), "capacity error = {err}");

    let repeated = vec![edge.clone(); 20];
    setPoints(&edge, &[(0.0, 0.0), (100.0, 0.0)]);
    let err = runAtomicRouteStage(&bg(), "extra duplicates", &graph, &repeated, 10, |_| {
        panic!("repeated extra edges reached the mutation callback")
    })
    .unwrap_err();
    assert!(err.Is(&errRouteStageWorkLimit), "duplicate-reference error = {err}; want work limit");
}

#[test]
fn TestRouteStageErrorRestoresGraphCachesAndNodePositions() {
    let (graph, _) = simplificationMutationGraph();
    let from = graph.nodes()[0].clone();
    let originalPosition = from.tl().unwrap();
    let originalValue = originalPosition.get();
    graph.bm().CellSize = 17.0;

    let err = runAtomicRouteStage(&bg(), "cache rollback", &graph, &[], maxRouteStageWorkUnits, |guard| {
        graph.bm().CellSize = 99.0;
        graph.RestoreRoutingCosts(layoutgraph::RoutingCostState { Crossing: 101.0, Turn: 102.0, NonCenterPort: 103.0 });
        originalPosition.set(geo::Point { X: 1.0, Y: 2.0 });
        from.set_tl(Some(pt(123.0, 456.0)));
        guard.step()?;
        Err(gocompat::GoError::new("route graph mutation probe"))
    })
    .unwrap_err();
    assert_eq!(err.Error(), "route graph mutation probe");
    let costs = graph.RoutingCosts();
    assert!(
        graph.b().CellSize == 17.0 && costs.Crossing == 0.0 && costs.Turn == 0.0 && costs.NonCenterPort == 0.0,
        "graph caches were not restored"
    );
    assert!(from.tl().unwrap() == originalPosition && originalPosition.get() == originalValue, "node position not restored");
}

#[test]
fn TestRouteWorkGuardAdditionCannotOverflow() {
    let guard = newRouteWorkGuard(&bg(), "test", u64::MAX).unwrap();
    guard.bm().used = u64::MAX - 1;
    guard.add(1).unwrap();
    let err = guard.add(1).unwrap_err();
    assert!(err.Error().contains("work exceeds limit"), "overflow-boundary error = {err}");
}

fn crosshatchMutationGraph() -> (GraphRef, EdgeRef, EdgeRef) {
    let g = layoutgraph::NewGraph();
    let left = nodeAt(1, 100.0, 100.0, 0.0, 0.0);
    let right = nodeAt(2, 100.0, 100.0, 0.0, 200.0);
    let externalLeft = nodeAt(3, 100.0, 100.0, 400.0, 0.0);
    let externalRight = nodeAt(4, 100.0, 100.0, 400.0, 200.0);
    for node in [&left, &right, &externalLeft, &externalRight] {
        g.AddNewNodeToContainer(None, node);
    }
    let vessel = nodeAt(5, 250.0, 400.0, -50.0, -50.0);
    vessel.bm().Graph = Some(g.clone());
    let cluster = ClusterRef::new(Cluster {
        Vessel: vessel.clone(),
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
    g.b().Clusters.set(vessel, cluster.clone());

    let leftEdge = g.Connect(&left, &externalLeft);
    let rightEdge = g.Connect(&right, &externalRight);
    setPoints(&leftEdge, &[(100.0, 100.0), (400.0, 50.0)]);
    setPoints(&rightEdge, &[(100.0, 100.0), (400.0, 250.0)]);
    let ab = |edge: &EdgeRef, from: &NodeRef, to: &NodeRef| {
        EdgeAbductionRef::new(EdgeAbduction {
            Edge: edge.clone(),
            OriginallyFrom: Some(from.clone()),
            OriginallyTo: None,
            CurrentFrom: Some(from.clone()),
            CurrentTo: Some(to.clone()),
        })
    };
    cluster.bm().EdgeAbductions = vec![ab(&leftEdge, &left, &externalLeft), ab(&rightEdge, &right, &externalRight)];
    (g, leftEdge, rightEdge)
}

#[test]
fn TestCrosshatchConvertsSharedPortRoutes() {
    let (g, first, second) = crosshatchMutationGraph();
    let (a, b) = (captureExactRouteTest(&first), captureExactRouteTest(&second));
    Crosshatch(&bg(), &g).unwrap();
    assert!(a.changed() && b.changed(), "crosshatch did not convert the shared-port routes");
}

#[test]
fn TestCrosshatchWorkLimitIsDeterministicAndAtomic() {
    let mut minimum = 1u64;
    loop {
        let (g, _, _) = crosshatchMutationGraph();
        if crosshatchWithWorkLimit(&bg(), &g, minimum).is_ok() {
            break;
        }
        minimum *= 2;
        assert!(minimum <= 1 << 20, "could not find a successful crosshatch work limit");
    }
    let (mut low, mut high) = (minimum / 2, minimum);
    while low + 1 < high {
        let mid = low + (high - low) / 2;
        let (g, _, _) = crosshatchMutationGraph();
        if crosshatchWithWorkLimit(&bg(), &g, mid).is_ok() {
            high = mid;
        } else {
            low = mid;
        }
    }
    assert!(high > 1, "minimum successful limit = {high}, want > 1");

    let (g, first, second) = crosshatchMutationGraph();
    let firstSnapshot = captureExactRouteTest(&first);
    let secondSnapshot = captureExactRouteTest(&second);
    let err = crosshatchWithWorkLimit(&bg(), &g, high - 1).unwrap_err();
    assert!(err.Is(&errRouteStageWorkLimit), "crosshatchWithWorkLimit error = {err}");
    firstSnapshot.assertRestored();
    secondSnapshot.assertRestored();
}

fn traceMutationGraph() -> (GraphRef, EdgeRef) {
    let g = layoutgraph::NewGraph();
    let from = nodeAt(1, 100.0, 100.0, 0.0, 0.0);
    let to = nodeAt(2, 100.0, 100.0, 300.0, 0.0);
    from.SetShape(shape::CIRCLE_TYPE);
    to.SetShape(shape::CIRCLE_TYPE);
    g.AddNewNodeToContainer(None, &from);
    g.AddNewNodeToContainer(None, &to);
    let edge = g.Connect(&from, &to);
    setPoints(&edge, &[(100.0, 0.0), (300.0, 100.0)]);
    (g, edge)
}

#[test]
fn TestTraceEdgesWorkLimitAfterMutationRollsBack() {
    for limit in 1..=200u64 {
        let (g, edge) = traceMutationGraph();
        let snapshot = captureExactRouteTest(&edge);
        match traceEdgesToShapeBorderWithWorkLimit(&bg(), &g, limit) {
            Ok(()) => {
                assert!(snapshot.changed(), "trace did not move the circle endpoints");
                return;
            }
            Err(err) => {
                assert!(err.Is(&errRouteStageWorkLimit));
                snapshot.assertRestored();
            }
        }
    }
    panic!("no successful trace limit");
}

fn straightFallbackMutationGraph(edgeCount: usize) -> (GraphRef, Vec<EdgeRef>) {
    let graph = layoutgraph::NewGraph();
    let mut edges = Vec::new();
    for index in 0..edgeCount {
        let y = (index * 300) as f64;
        let from = graph.AddNode(&layoutgraph::NewNode((index * 2 + 1) as i64, 40.0, 40.0));
        let to = graph.AddNode(&layoutgraph::NewNode((index * 2 + 2) as i64, 40.0, 40.0));
        from.set_tl(Some(pt(0.0, y)));
        to.set_tl(Some(pt(200.0, y)));
        let edge = graph.Connect(&from, &to);
        setPoints(&edge, &[(40.0, y + 20.0), (40.0, y + 100.0), (200.0, y + 100.0), (200.0, y + 20.0)]);
        edges.push(edge);
    }
    (graph, edges)
}

#[test]
fn TestStraightEdgesFallbackCommitsSuccessfulReplacement() {
    let (graph, edges) = straightFallbackMutationGraph(1);
    let original = captureExactRouteTest(&edges[0]);
    StraightEdgesFallback(&bg(), &graph).unwrap();
    assert!(original.changed(), "straight-edge fallback did not replace the route");
    assert_eq!(edges[0].b().Points.len(), 2);
    assert!(graph.RoutingCosts().NonCenterPort != 0.0, "successful fallback did not retain the initialized routing-cost cache");
}

#[test]
fn TestStraightEdgesFallbackCanceledBeforeWork() {
    let (graph, edges) = straightFallbackMutationGraph(2);
    let snapshots: Vec<_> = edges.iter().map(captureExactRouteTest).collect();
    let err = StraightEdgesFallback(&canceledContext(), &graph).unwrap_err();
    requireCanceledAt(&err, "EdgeRouting");
    for s in &snapshots {
        s.assertRestored();
    }
    let costs = graph.RoutingCosts();
    assert!(costs.Crossing == 0.0 && costs.Turn == 0.0 && costs.NonCenterPort == 0.0);
}

/// tryStraightEdgeFallbackGuarded (RouteEdges) agrees with the stage helper.
#[test]
fn TestGuardedStraightEdgeFallbackMatchesOrdinarySuccess() {
    let (g1, e1) = straightFallbackMutationGraph(1);
    let (g2, e2) = straightFallbackMutationGraph(1);
    StraightEdgesFallback(&bg(), &g1).unwrap();
    let guard = newRouteWorkGuard(&bg(), "test", maxRouteStageWorkUnits).unwrap();
    tryStraightEdgeFallbackGuarded(&g2, &e2[0], &guard).unwrap();
    assert_eq!(values(&e1[0]), values(&e2[0]));
}
