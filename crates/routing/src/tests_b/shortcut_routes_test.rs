// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/routing/shortcut_routes_test.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of routing/shortcut_routes_test.go (allocation counts and the nil
//! context/graph cases have no Rust counterpart).
use super::*;

fn shortcutTestPoints(xy: &[f64]) -> Vec<geo::PointRef> {
    xy.chunks(2).map(|c| geo::NewPoint(c[0], c[1])).collect()
}

fn vals(xy: &[f64]) -> Vec<geo::Point> {
    xy.chunks(2).map(|c| geo::Point { X: c[0], Y: c[1] }).collect()
}

fn shortcutTestGraph() -> (GraphRef, EdgeRef) {
    let g = layoutgraph::NewGraph();
    let from = g.AddNode(&layoutgraph::NewNode(1, 100.0, 100.0));
    from.set_tl(Some(pt(0.0, 200.0)));
    let to = g.AddNode(&layoutgraph::NewNode(2, 100.0, 100.0));
    to.set_tl(Some(pt(500.0, 0.0)));
    let e = g.Connect(&from, &to);
    e.bm().Points = shortcutTestPoints(&[100.0, 250.0, 200.0, 250.0, 200.0, 150.0, 400.0, 150.0, 400.0, 50.0, 500.0, 50.0]);
    (g, e)
}

fn guard(name: &str) -> routeWorkGuardRef {
    newRouteWorkGuard(&bg(), name, maxRouteStageWorkUnits).unwrap()
}

#[test]
fn TestShortcutRemovesMonotoneStaircaseBends() {
    for transpose in [false, true] {
        let (g, e) = shortcutTestGraph();
        if transpose {
            for n in g.nodes() {
                let t = n.tlv();
                n.set_tl(Some(pt(t.Y, t.X)));
            }
            for p in e.points() {
                let v = p.get();
                p.set(geo::Point { X: v.Y, Y: v.X });
            }
        }
        let before = e.points();
        let oldResult = simplifyPoints(&g, &e, &guard("old simplify")).unwrap();
        assert_eq!(oldResult.len(), before.len(), "test no longer distinguishes the new monotone case");
        ShortcutEdgeRoutes(&bg(), &g).unwrap();
        let now = e.points();
        assert!(shortcutBends(&now) == 2 && shortcutLength(&now) == shortcutLength(&before), "want four to two bends at unchanged length");
        let (b, n) = (pointValues(&before), pointValues(&now));
        assert!(
            now[0] == before[0]
                && now[now.len() - 1] == before[before.len() - 1]
                && sameRouteDirection(&b[0], &b[1], &n[0], &n[1])
                && sameRouteDirection(&b[4], &b[5], &n[2], &n[3]),
            "changed endpoint identity or signed port approach"
        );
        let after = captureExactRouteTest(&e);
        ShortcutEdgeRoutes(&bg(), &g).unwrap();
        after.assertRestored();
    }
}

#[test]
fn TestShortcutRejectsObstaclesLabelsAndBadPorts() {
    for kind in ["node", "label", "node wall", "reversed port", "short port"] {
        let (g, e) = shortcutTestGraph();
        let mut candidate = shortcutTestPoints(&[100.0, 250.0, 400.0, 250.0, 400.0, 50.0, 500.0, 50.0]);
        let mut boxes: Vec<geo::Box> = Vec::new();
        match kind {
            "node" => {
                let n = g.AddNode(&layoutgraph::NewNode(3, 40.0, 40.0));
                n.set_tl(Some(pt(280.0, 230.0)));
            }
            "label" => boxes.push(geo::Box { TopLeft: Some(pt(280.0, 230.0)), Width: 40.0, Height: 40.0 }),
            "node wall" => {
                let n = g.AddNode(&layoutgraph::NewNode(3, 40.0, 40.0));
                n.set_tl(Some(pt(280.0, 280.0)));
            }
            "reversed port" => candidate = shortcutTestPoints(&[100.0, 250.0, 60.0, 250.0, 60.0, 50.0, 500.0, 50.0]),
            "short port" => candidate = shortcutTestPoints(&[100.0, 250.0, 120.0, 250.0, 120.0, 50.0, 500.0, 50.0]),
            _ => {}
        }
        let safe = shortcutCandidateSafe(&g, &e, &candidate, &boxes, &guard(kind)).unwrap();
        assert!(!safe, "unsafe candidate accepted: {kind}");
    }
}

#[test]
fn TestShortcutKeepsShortUnchangedLegsButRejectsNewTinyJogs() {
    let (g, e) = shortcutTestGraph();
    e.bm().Points =
        shortcutTestPoints(&[100.0, 250.0, 200.0, 250.0, 200.0, 150.0, 300.0, 150.0, 300.0, 60.0, 400.0, 60.0, 400.0, 50.0, 500.0, 50.0]);
    for x in [400.0, 350.0] {
        let candidate = shortcutTestPoints(&[100.0, 250.0, 300.0, 250.0, 300.0, 60.0, x, 60.0, x, 50.0, 500.0, 50.0]);
        let safe = shortcutCandidateSafe(&g, &e, &candidate, &[], &guard("short leg")).unwrap();
        assert_eq!(safe, x == 400.0, "x={x}");
    }
}

#[test]
fn TestShortcutComparesContactsGeometrically() {
    let before = vals(&[100.0, 250.0, 200.0, 250.0, 200.0, 150.0, 400.0, 150.0, 400.0, 50.0, 500.0, 50.0]);
    let after = vals(&[100.0, 250.0, 400.0, 250.0, 400.0, 50.0, 500.0, 50.0]);
    for (name, points, want) in [
        ("new crossing", vals(&[300.0, 220.0, 300.0, 280.0]), false),
        ("new point contact", vals(&[300.0, 250.0, 300.0, 300.0]), false),
        ("new shared run", vals(&[300.0, 250.0, 350.0, 250.0]), false),
        ("extended shared run", vals(&[150.0, 250.0, 350.0, 250.0]), false),
        ("existing shared run", vals(&[150.0, 250.0, 180.0, 250.0]), true),
        ("existing crossing in longer segment", vals(&[150.0, 220.0, 150.0, 280.0]), true),
        ("disjoint", vals(&[250.0, 300.0, 350.0, 300.0]), true),
    ] {
        let safe = shortcutContactsPreserved(&before, &after, &points, &guard(name)).unwrap();
        assert_eq!(safe, want, "{name}");
    }
}

#[test]
fn TestShortcutDoesNotTurnSharedCornerIntoThroughCrossing() {
    let before = vals(&[0.0, 0.0, 100.0, 0.0, 100.0, 100.0, 200.0, 100.0, 200.0, 200.0, 300.0, 200.0, 300.0, 300.0]);
    let after = vals(&[0.0, 0.0, 300.0, 0.0, 300.0, 300.0]);
    for other in [vals(&[100.0, -100.0, 100.0, 300.0]), vals(&[100.0, -100.0, 100.0, 0.0, 100.0, 300.0])] {
        let safe = shortcutContactsPreserved(&before, &after, &other, &guard("crossing kind")).unwrap();
        assert!(!safe, "old shared corner became a new crossing");
    }
}

#[test]
fn TestShortcutPreservesParallelRouteClearance() {
    for x in [401.0, 440.0] {
        let (g, e) = shortcutTestGraph();
        let from = g.AddNode(&layoutgraph::NewNode(3, 20.0, 20.0));
        from.set_tl(Some(pt(600.0, 170.0)));
        let to = g.AddNode(&layoutgraph::NewNode(4, 20.0, 20.0));
        to.set_tl(Some(pt(600.0, 220.0)));
        let other = g.Connect(&from, &to);
        other.bm().Points = shortcutTestPoints(&[600.0, 180.0, x, 180.0, x, 230.0, 600.0, 230.0]);
        let candidate = shortcutTestPoints(&[100.0, 250.0, 400.0, 250.0, 400.0, 50.0, 500.0, 50.0]);
        let safe = shortcutCandidateSafe(&g, &e, &candidate, &[], &guard("parallel clearance")).unwrap();
        assert_eq!(safe, x == 440.0, "parallel gap {}", x - 400.0);
    }
}

#[test]
fn TestShortcutParallelClearanceKeepsExistingSharedPortGeometry() {
    let (g, e) = shortcutTestGraph();
    let other = g.Connect(&e.from(), &e.to());
    other.bm().Points = shortcutTestPoints(&[100.0, 250.0, 200.0, 250.0, 200.0, 350.0, 500.0, 350.0, 500.0, 50.0]);
    let ev = pointValues(&e.b().Points);
    let P = |x: f64, y: f64| geo::Point { X: x, Y: y };
    let safe = shortcutParallelClearance(&g, &e, &ev, &P(100.0, 250.0), &P(400.0, 250.0), &guard("shared port")).unwrap();
    assert!(safe, "existing source trunk rejected");
    let safe = shortcutParallelClearance(&g, &e, &ev, &P(100.0, 251.0), &P(400.0, 251.0), &guard("shared trunk peeling")).unwrap();
    assert!(!safe, "shared trunk justified a new 1px corridor");
}

#[test]
fn TestShortcutSkipsLabeledTargetsAndOpenInventories() {
    for kind in ["main label", "source label", "target label", "external edge"] {
        let (g, e) = shortcutTestGraph();
        let l = layoutgraph::LabelRef::new(layoutgraph::Label::new("keep position", label::InsideMiddleCenter, 40.0, 20.0));
        match kind {
            "main label" => e.bm().Label = Some(l),
            "source label" => e.bm().SourceArrowheadLabel = Some(l),
            "target label" => e.bm().TargetArrowheadLabel = Some(l),
            _ => {
                let other = layoutgraph::NewEdge(&e.from(), &e.to());
                other.bm().Points = shortcutTestPoints(&[100.0, 250.0, 100.0, 400.0, 500.0, 400.0, 500.0, 50.0]);
                e.from().bm().Edges.push(other);
            }
        }
        let before = captureExactRouteTest(&e);
        ShortcutEdgeRoutes(&bg(), &g).unwrap();
        before.assertRestored();
    }
}

#[test]
fn TestShortcutAtomicRollbackAfterAUsefulMutation() {
    for kind in ["cancel", "budget"] {
        let (g, e) = shortcutTestGraph();
        let before = captureExactRouteTest(&e);
        let (ctx, cancel) = gocompat::context::WithCancel(&bg());
        let err = runAtomicRouteStage(&ctx, "shortcut injected failure", &g, &[], maxRouteStageWorkUnits, |guard| {
            shortcutRoutesGuarded(&g, guard)?;
            assert_eq!(shortcutBends(&e.points()), 2, "did not reach useful mutation");
            if kind == "cancel" {
                cancel();
                return Ok(());
            }
            let limit = guard.b().limit;
            guard.add(limit)
        })
        .unwrap_err();
        if kind == "cancel" {
            requireCanceled(&err);
        } else {
            assert!(err.Is(&errRouteStageWorkLimit), "got {err}");
        }
        before.assertRestored();
    }
}

#[test]
fn TestShortcutAdmissionAndCancellation() {
    let g = layoutgraph::NewGraph();
    for _ in 0..=maxShortcutInput {
        g.AddNodeUnchecked(&nodeAt(1, 1.0, 1.0, 0.0, 0.0));
    }
    ShortcutEdgeRoutes(&bg(), &g).unwrap();
    let err = ShortcutEdgeRoutes(&canceledContext(), &g).unwrap_err();
    requireCanceled(&err);
    let (g, e) = shortcutTestGraph();
    let before = captureExactRouteTest(&e);
    let err = shortcutRoutesWithLimit(&bg(), &g, 1).unwrap_err();
    assert!(err.Is(&errRouteStageWorkLimit), "{err}");
    before.assertRestored();
}

// Eight 31-leg staircases reach the admitted 256-point envelope.
fn shortcutEnvelopeGraph() -> GraphRef {
    let g = layoutgraph::NewGraph();
    for i in 0..8 {
        let mut y = (i * 2000) as f64;
        let from = g.AddNode(&layoutgraph::NewNode(2 * i + 1, 100.0, 100.0));
        from.set_tl(Some(pt(0.0, y)));
        let to = g.AddNode(&layoutgraph::NewNode(2 * i + 2, 100.0, 100.0));
        to.set_tl(Some(pt(1700.0, y + 1500.0)));
        let e = g.Connect(&from, &to);
        let mut x = 100.0;
        y += 50.0;
        e.bm().Points.push(pt(x, y));
        for j in 0..31 {
            if j % 2 == 0 {
                x += 100.0;
            } else {
                y += 100.0;
            }
            e.bm().Points.push(pt(x, y));
        }
    }
    g
}

#[test]
fn TestShortcutAdmittedEnvelopeHonorsBudget() {
    let g = shortcutEnvelopeGraph();
    assert!(!shortcutInputTooLarge(&g), "envelope unexpectedly skipped");
    let before: Vec<_> = g.edges().iter().map(captureExactRouteTest).collect();
    let err = shortcutRoutesWithLimit(&bg(), &g, 10_000).unwrap_err();
    assert!(err.Is(&errRouteStageWorkLimit), "{err}");
    for snapshot in &before {
        snapshot.assertRestored();
    }
}
