// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/routing/reorder_duplicates_test.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of routing/reorder_duplicates_test.go (context-probe variants need
//! custom Go contexts and are not ported).
use super::*;

fn newDuplicateEdgeTestGraph() -> (GraphRef, Vec<EdgeRef>) {
    let g = layoutgraph::NewGraph();
    let from = g.AddNode(&layoutgraph::NewNode(1, 40.0, 40.0));
    from.set_tl(Some(pt(0.0, 0.0)));
    let to = g.AddNode(&layoutgraph::NewNode(2, 40.0, 40.0));
    to.set_tl(Some(pt(0.0, 200.0)));
    let mut edges = Vec::new();
    for i in 0..3 {
        let e = g.Connect(&from, &to);
        let x = (10 * (i + 1)) as f64;
        setPoints(&e, &[(x, 40.0), (x, 200.0)]);
        edges.push(e);
    }
    edges[1].bm().Label = Some(layoutgraph::LabelRef::new(layoutgraph::Label::new("labeled", label::Unset, 0.0, 0.0)));
    (g, edges)
}

fn newFinalDuplicateEdgeTestGraph(reverseUnlabeled: bool) -> (GraphRef, Vec<EdgeRef>) {
    let g = layoutgraph::NewGraph();
    let from = g.AddNode(&layoutgraph::NewNode(1, 40.0, 40.0));
    from.set_tl(Some(pt(0.0, 0.0)));
    let to = g.AddNode(&layoutgraph::NewNode(2, 40.0, 40.0));
    to.set_tl(Some(pt(0.0, 200.0)));

    let first = g.Connect(&from, &to);
    setPoints(&first, &[(10.0, 40.0), (10.0, 200.0)]);
    let last;
    if reverseUnlabeled {
        last = g.Connect(&to, &from);
        setPoints(&last, &[(30.0, 200.0), (30.0, 40.0)]);
    } else {
        last = g.Connect(&from, &to);
        setPoints(&last, &[(30.0, 40.0), (30.0, 200.0)]);
    }
    let labeled = g.Connect(&from, &to);
    setPoints(&labeled, &[(20.0, 40.0), (20.0, 200.0)]);
    labeled.bm().Label = Some(layoutgraph::LabelRef::new(layoutgraph::Label::new("labeled", label::Unset, 0.0, 0.0)));
    (g, vec![first, last, labeled])
}

fn copyEdgePointSlices(edges: &[EdgeRef]) -> Vec<Vec<geo::PointRef>> {
    edges.iter().map(|e| e.points()).collect()
}

#[track_caller]
fn requireSameEdgePointSlices(edges: &[EdgeRef], want: &[Vec<geo::PointRef>]) {
    for (i, edge) in edges.iter().enumerate() {
        let points = edge.points();
        assert_eq!(points.len(), want[i].len(), "edge {i} route length");
        for j in 0..want[i].len() {
            assert!(points[j] == want[i][j], "edge {i} point {j} identity differs");
        }
    }
}

#[test]
fn TestReorderDuplicatesCanceledBeforeWork() {
    let (g, edges) = newDuplicateEdgeTestGraph();
    let want = copyEdgePointSlices(&edges);
    let err = ReorderDuplicates(&canceledContext(), &g).unwrap_err();
    requireCanceledAt(&err, "ReorderDuplicates");
    requireSameEdgePointSlices(&edges, &want);
}

#[test]
fn TestReorderDuplicatesCommitsSuccessfulSwap() {
    for reverseUnlabeled in [false, true] {
        let (g, edges) = newFinalDuplicateEdgeTestGraph(reverseUnlabeled);
        let mut want = copyEdgePointSlices(&edges);
        ReorderDuplicates(&bg(), &g).unwrap();

        if reverseUnlabeled {
            want[1].reverse();
            want[2].reverse();
        }
        want.swap(1, 2);
        requireSameEdgePointSlices(&edges, &want);
    }
}

/// The guarded standalone variant makes the same decision.
#[test]
fn TestReorderDuplicatesGuardedMatchesOrdinary() {
    for reverseUnlabeled in [false, true] {
        let (g1, edges1) = newFinalDuplicateEdgeTestGraph(reverseUnlabeled);
        let (_g2, edges2) = newFinalDuplicateEdgeTestGraph(reverseUnlabeled);
        ReorderDuplicates(&bg(), &g1).unwrap();
        let guard = newRouteWorkGuard(&bg(), "test", maxRouteStageWorkUnits).unwrap();
        reorderDuplicatesInEdgesGuarded(&edges2, &guard).unwrap();
        for (a, b) in edges1.iter().zip(edges2.iter()) {
            assert_eq!(values(a), values(b));
        }
    }
}
