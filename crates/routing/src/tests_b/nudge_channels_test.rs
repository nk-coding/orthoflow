// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/routing/nudge_channels_test.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of routing/nudge_channels_test.go (custom-context probes and the
//! allocation benchmark are not ported).
use super::*;

fn group(lower: f64, upper: f64, nodeClearance: Option<Range>) -> channelGroupRef {
    channelGroupRef::new(channelGroup { segments: vec![], position: 0.0, lower, upper, fixed: false, nodeClearance })
}

fn guard(limit: u64) -> routeWorkGuardRef {
    newRouteWorkGuard(&bg(), "channel test", limit).unwrap()
}

#[test]
fn TestChannelGapSolvesCoupledDifferentIntervals() {
    let p = channelProblem {
        groups: vec![group(0.0, 120.0, None), group(20.0, 180.0, None)],
        arcs: vec![channelArc { from: 0, to: 1, separate: true }],
    };
    let g = guard(10000);
    let (lo, hi, feasible) = solveChannelGap(&p, 60.0, &g).unwrap();
    assert!(feasible);
    for (i, want) in [60.0, 120.0].iter().enumerate() {
        assert!(lo[i] == *want && hi[i] == *want, "variable{i} interval=[{},{}] want{want}", lo[i], hi[i]);
    }
    let (_, _, feasible) = solveChannelGap(&p, 61.0, &g).unwrap();
    assert!(!feasible);
    let (lo, hi, feasible) = solveChannelGap(&p, 20.0, &g).unwrap();
    assert!(feasible);
    let mid = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0];
    assert!(channelGap(&p, &mid) >= 20.0, "midpoint violates separation: {mid:?}");
}

#[test]
fn TestChannelGapAllowsSamePathLegsToMeet() {
    let p = channelProblem {
        groups: vec![group(0.0, 100.0, None), group(0.0, 100.0, None)],
        arcs: vec![channelArc { from: 0, to: 1, separate: false }],
    };
    let (lo, hi, feasible) = solveChannelGap(&p, 50.0, &guard(10000)).unwrap();
    assert!(feasible && lo[0] == 50.0 && lo[1] == 50.0 && hi[0] == 50.0 && hi[1] == 50.0, "same-path zero separation: {lo:?} {hi:?}");
}

#[test]
fn TestChannelGapAppliesNodeClearanceAsAnAbsoluteBound() {
    let p = channelProblem { groups: vec![group(0.0, 100.0, Some(Range { start: 10.0, end: 30.0 }))], arcs: vec![] };
    let g = newRouteWorkGuard(&bg(), "channel clearance test", 100).unwrap();
    let (lo, hi, feasible) = solveChannelGap(&p, 30.0, &g).unwrap();
    assert!(feasible && lo[0] == 30.0 && hi[0] == 30.0, "gap and absolute bound were not combined correctly");
    let (_, _, feasible) = solveChannelGap(&p, 31.0, &g).unwrap();
    assert!(!feasible, "gap escaped its node bound");
}

#[test]
fn TestNudgeChannelsDoesNotTradeNodeClearanceForShorterWire() {
    for initialGap in [21.0, 60.0] {
        let g = layoutgraph::NewGraph();
        let container = nodeAt(1, 200.0, 300.0, 0.0, 200.0);
        g.AddNewNodeToContainer(None, &container);
        let from = nodeAt(2, 50.0, 50.0, 50.0, 300.0);
        g.AddNewNodeToContainer(Some(&container), &from);
        let to = nodeAt(3, 50.0, 50.0, 400.0, 300.0);
        g.AddNewNodeToContainer(None, &to);
        let originalY = container.y() - initialGap;
        let ceiling = nodeAt(4, 500.0, originalY - 10.0, 0.0, 0.0);
        g.AddNewNodeToContainer(None, &ceiling);
        let e = g.Connect(&from, &to);
        setPoints(&e, &[(75.0, 300.0), (75.0, originalY), (425.0, originalY), (425.0, 300.0)]);
        NudgeEdgeChannels(&bg(), &g).unwrap();
        let gap = container.y() - p(&e, 1).Y;
        let minimum = initialGap.min(segmentSpacingBuffer);
        assert!(gap + channelEpsilon >= minimum, "wire shortening reduced node clearance from {initialGap} to {gap}; minimum {minimum}");
        if initialGap < segmentSpacingBuffer {
            assert_eq!(p(&e, 1).Y, originalY, "a narrow existing node gap changed");
        }
        if initialGap > segmentSpacingBuffer {
            assert!(p(&e, 1).Y > originalY, "comfortable clearance prevented useful wire shortening");
        }
    }
}

fn channelZGraph() -> (GraphRef, EdgeRef) {
    let g = layoutgraph::NewGraph();
    let from = g.AddNode(&layoutgraph::NewNode(1, 100.0, 100.0));
    from.set_tl(Some(pt(0.0, 0.0)));
    let to = g.AddNode(&layoutgraph::NewNode(2, 100.0, 100.0));
    to.set_tl(Some(pt(300.0, 200.0)));
    let e = g.Connect(&from, &to);
    setPoints(&e, &[(100.0, 50.0), (120.0, 50.0), (120.0, 250.0), (300.0, 250.0)]);
    (g, e)
}

#[test]
fn TestNudgeChannelsBalancesFourPointRoutesAndPreservesPorts() {
    for kind in [shape::SQUARE_TYPE, shape::DIAMOND_TYPE, shape::TABLE_TYPE] {
        let (g, e) = channelZGraph();
        let (from, to) = (e.from(), e.to());
        from.SetShape(kind);
        to.SetShape(kind);
        if kind == shape::TABLE_TYPE {
            from.SetNumColumns(1);
            to.SetNumColumns(1);
            e.bm().FromTableColumnIndex = Some(0);
            e.bm().ToTableColumnIndex = Some(0);
        }
        let (first, last) = (p(&e, 0), p(&e, 3));
        let (ft, tt) = (from.tlv(), to.tlv());
        NudgeEdgeChannels(&bg(), &g).unwrap();
        assert!(p(&e, 0) == first && p(&e, 3) == last && from.tlv() == ft && to.tlv() == tt, "moved a fixed port or node ({kind})");
        assert!(
            (p(&e, 1).X - 200.0).abs() <= channelEpsilon && (p(&e, 2).X - 200.0).abs() <= channelEpsilon,
            "interior corridor was not centered: {:?} ({kind})",
            values(&e)
        );
        assert!(p(&e, 1).Y == first.Y && p(&e, 2).Y == last.Y, "changed port approach direction");
        let before = captureExactRouteTest(&e);
        NudgeEdgeChannels(&bg(), &g).unwrap();
        before.assertRestored();
    }
}

#[test]
fn TestNudgeChannelsKeepsSharedTrunksAndUnrelatedLockedRoutes() {
    let (g, e) = channelZGraph();
    let duplicate = g.Connect(&e.from(), &e.to());
    for point in e.points() {
        duplicate.bm().Points.push(point.Copy());
    }
    let loopEdge = g.Connect(&e.from(), &e.from());
    let shared = pt(50.0, 50.0);
    loopEdge.bm().Points = vec![shared.clone(), shared];
    let locked = captureExactRouteTest(&loopEdge);
    NudgeEdgeChannels(&bg(), &g).unwrap();
    locked.assertRestored();
    for i in 0..e.b().Points.len() {
        assert_eq!(p(&e, i), p(&duplicate, i), "split a shared trunk");
    }
    assert!((p(&e, 1).X - 200.0).abs() <= channelEpsilon, "shared corridor did not move: {:?}", p(&e, 1));
}

#[test]
fn TestNudgeChannelsLeavesUnboundedOuterRoutesAlone() {
    let (g, e) = channelZGraph();
    e.to().set_x(0.0);
    setPoints(&e, &[(100.0, 50.0), (160.0, 50.0), (160.0, 250.0), (100.0, 250.0)]);
    let snapshot = captureExactRouteTest(&e);
    NudgeEdgeChannels(&bg(), &g).unwrap();
    snapshot.assertRestored();
}

#[test]
fn TestChannelProposalRejectsShortLegAndNewPointContact() {
    for contact in [false, true] {
        let (g, e) = channelZGraph();
        let mut x = 105.0;
        if contact {
            x = 200.0;
            let other = g.Connect(&e.from(), &e.to());
            setPoints(&other, &[(200.0, 150.0), (250.0, 150.0)]);
        }
        let points = e.points();
        let mut proposal: channelProposal = gocompat::Map::default();
        proposal.insert(points[1].clone(), geo::Point { X: x, Y: 50.0 });
        proposal.insert(points[2].clone(), geo::Point { X: x, Y: 250.0 });
        let safe = channelProposalSafe(&g, &proposal, &guard(10000)).unwrap();
        assert!(!safe, "unsafe proposal accepted (contact={contact})");
    }
}

#[test]
fn TestNudgeChannelsRestoresOnCancellationAndWorkLimit() {
    let (g, e) = channelZGraph();
    let snapshot = captureExactRouteTest(&e);
    let err = NudgeEdgeChannels(&canceledContext(), &g).unwrap_err();
    requireCanceled(&err);
    snapshot.assertRestored();
    let err = nudgeChannelsWithLimit(&bg(), &g, 1).unwrap_err();
    assert!(err.Is(&errRouteStageWorkLimit), "work limit error={err}");
    snapshot.assertRestored();
    // Every failing injected limit restores the route exactly.
    for limit in 1..=5000u64 {
        let (g, e) = channelZGraph();
        let snapshot = captureExactRouteTest(&e);
        match nudgeChannelsWithLimit(&bg(), &g, limit) {
            Ok(()) => {
                assert!(snapshot.changed(), "nudging did not move the channel");
                return;
            }
            Err(err) => {
                assert!(err.Is(&errRouteStageWorkLimit));
                snapshot.assertRestored();
            }
        }
    }
    panic!("no successful nudge limit");
}

fn oversizedChannelGraph(kind: &str) -> (GraphRef, EdgeRef) {
    let (g, e) = channelZGraph();
    match kind {
        "nodes" => {
            while g.b().Nodes.len() <= maxChannelSegments {
                let count = g.b().Nodes.len();
                let n = g.AddNode(&layoutgraph::NewNode(count as i64 + 1, 100.0, 100.0));
                let count = g.b().Nodes.len();
                n.set_tl(Some(pt((count * 200) as f64, 500.0)));
            }
        }
        "edges" => {
            while g.b().Edges.len() <= maxChannelSegments {
                let other = g.Connect(&e.from(), &e.to());
                for point in e.points() {
                    other.bm().Points.push(point.Copy());
                }
            }
        }
        "points" => {
            while e.b().Points.len() <= maxChannelSegments {
                let last = e.b().Points[e.b().Points.len() - 1].Copy();
                e.bm().Points.push(last);
            }
        }
        _ => panic!("unknown oversized channel graph kind"),
    }
    (g, e)
}

#[test]
fn TestNudgeChannelsSkipsOversizedInputWithoutSnapshot() {
    for kind in ["nodes", "edges", "points"] {
        let (g, e) = oversizedChannelGraph(kind);
        let snapshot = captureExactRouteTest(&e);
        // A one-unit budget cannot validate or snapshot this graph.
        nudgeChannelsWithLimit(&bg(), &g, 1).unwrap();
        NudgeEdgeChannels(&bg(), &g).unwrap();
        snapshot.assertRestored();
    }
}

#[test]
fn TestChannelAdmissionBoundary() {
    for count in [maxChannelSegments, maxChannelSegments + 1] {
        let (g, _) = channelZGraph();
        while g.b().Nodes.len() < count {
            g.AddNodeUnchecked(&nodeAt(9, 1.0, 1.0, 0.0, 0.0));
        }
        assert_eq!(channelInputTooLarge(&g), count > maxChannelSegments, "nodes {count}");
        let g = layoutgraph::NewGraph();
        let (a, b) = (nodeAt(1, 1.0, 1.0, 0.0, 0.0), nodeAt(2, 1.0, 1.0, 5.0, 0.0));
        while g.b().Edges.len() < count {
            g.Connect(&a, &b);
        }
        assert_eq!(channelInputTooLarge(&g), count > maxChannelSegments, "edges {count}");
        let g = layoutgraph::NewGraph();
        let e = g.Connect(&a, &b);
        e.bm().Points = (0..count).map(|i| pt(i as f64, 0.0)).collect();
        assert_eq!(channelInputTooLarge(&g), count > maxChannelSegments, "points {count}");
    }
}

#[test]
fn TestNudgeChannelsOversizedSkipPreservesCancellation() {
    let (g, e) = oversizedChannelGraph("points");
    let snapshot = captureExactRouteTest(&e);
    let err = NudgeEdgeChannels(&canceledContext(), &g).unwrap_err();
    requireCanceled(&err);
    snapshot.assertRestored();
}

#[test]
fn TestNudgeChannelsSkipsUninspectedExternalPointAliases() {
    let (g, e) = channelZGraph();
    let external = layoutgraph::NewEdge(&e.from(), &e.to());
    external.bm().Points = vec![e.b().Points[1].clone(), pt(120.0, 150.0)];
    e.from().bm().Edges.push(external.clone());
    let (original, outside) = (captureExactRouteTest(&e), captureExactRouteTest(&external));
    NudgeEdgeChannels(&bg(), &g).unwrap();
    original.assertRestored();
    outside.assertRestored();
}
