// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from the following files of terrastruct/d2, Copyright 2022 Terrastruct Inc.:
//   - d2layouts/d2talalayout/internal/labeling/arrowhead_test.go
//   - d2layouts/d2talalayout/internal/labeling/refine_test.go
// Rust port: Copyright 2026 Niklas Krieger.

//! Ports of arrowhead_test.go and refine_test.go.
use crate::*;
use layoutgraph::{Label, LabelRef, NewEdge, NewNode};

fn dummyEdge() -> EdgeRef {
    let n = NewNode(1, 10.0, 10.0);
    NewEdge(&n, &n)
}

#[test]
fn TestPositionArrowheadLabelOwnsRenderedRecord() {
    let edge = dummyEdge();
    {
        let mut e = edge.bm();
        e.Points = vec![geo::NewPoint(0.0, 0.0), geo::NewPoint(100.0, 0.0)];
        e.SourceArrowhead = layoutgraph::TriangleArrowhead.into();
        e.TargetArrowhead = layoutgraph::TriangleArrowhead.into();
        e.SourceArrowheadLabel = Some(LabelRef::new(Label::new("source", label::Unset, 30.0, 12.0)));
        e.TargetArrowheadLabel = Some(LabelRef::new(Label::new("target", label::Unset, 40.0, 14.0)));
    }
    for (isTarget, text, width, height, want) in
        [(false, "source", 30.0, 12.0, (5.0, -20.0)), (true, "target", 40.0, 14.0, (55.0, -22.0))]
    {
        let positioned = PositionArrowheadLabel(&edge, isTarget, &edge.points()).expect("positioned label is nil");
        assert!(positioned.Edge == edge && positioned.IsTarget == isTarget && positioned.Text == text);
        assert_eq!((positioned.Box.Width, positioned.Box.Height), (width, height));
        let tl = *positioned.Box.TopLeft.as_ref().unwrap().b();
        assert_eq!((tl.X, tl.Y), want);
    }
    assert!(PositionArrowheadLabel(&dummyEdge(), false, &edge.points()).is_none());
}

#[test]
fn TestSharedSegmentsPreserveExclusiveGaps() {
    for vertical in [false, true] {
        for reverse in [false, true] {
            let mut edges = vec![];
            for interval in [(0.0, 100.0), (10.0, 20.0), (30.0, 40.0), (50.0, 50.0)] {
                let (mut a, mut b) = ((interval.0, 0.0), (interval.1, 0.0));
                if vertical {
                    a = (a.1, a.0);
                    b = (b.1, b.0);
                }
                if reverse {
                    std::mem::swap(&mut a, &mut b);
                }
                let edge = dummyEdge();
                edge.bm().Points = vec![geo::NewPoint(a.0, a.1), geo::NewPoint(b.0, b.1)];
                edges.push(edge);
            }
            let segments = findSharedSegmentsChecked(&edges, None).unwrap();
            assert_eq!(segments.len(), 2, "vertical={vertical} reverse={reverse}");
            let c = |p: &geo::Point| if vertical { p.Y } else { p.X };
            assert!(c(&segments[0].0) == 10.0 && c(&segments[0].1) == 20.0 && c(&segments[1].0) == 30.0 && c(&segments[1].1) == 40.0);
        }
    }
}

#[test]
fn TestLabelPlacementPreservesFixedEdgeLabelsAndReservesTheirSpace() {
    for selected in [false, true] {
        for position in [label::InsideMiddleCenter, label::OutsideTopCenter] {
            let g = layoutgraph::NewGraph();
            let a = g.AddNode(&NewNode(1, 40.0, 40.0));
            a.set_tl(Some(geo::NewPoint(0.0, 0.0)));
            let b = g.AddNode(&NewNode(2, 40.0, 40.0));
            b.set_tl(Some(geo::NewPoint(300.0, 0.0)));
            let fixed = g.Connect(&a, &b);
            fixed.bm().Points = vec![geo::NewPoint(40.0, 20.0), geo::NewPoint(300.0, 20.0)];
            let fl = LabelRef::new(Label::new("fixed", position, 80.0, 20.0));
            fl.FixPosition();
            fixed.bm().Label = Some(fl.clone());
            fixed.bm().LabelPercentage = 0.25;
            let movable = g.Connect(&a, &b);
            movable.bm().Points = fixed.points();
            movable.bm().Label = Some(LabelRef::new(Label::new("automatic", label::Unset, 80.0, 20.0)));
            let ctx = gocompat::context::Background();
            let edges = g.b().Edges.clone();
            if selected {
                PlaceNewEdges(&ctx, &g, &edges).unwrap();
            } else {
                Place(&ctx, &g).unwrap();
            }
            let cur = fixed.b().Label.clone().unwrap();
            assert!(cur == fl && cur.b().Position == position && fixed.b().LabelPercentage == 0.25);
            let boxOf = |e: &EdgeRef| {
                let l = e.b().Label.clone().unwrap();
                let l = l.b();
                let tl = *e.LabelTopLeft(l.Position, l.Width, l.Height).unwrap().b();
                (tl.X, tl.Y, l.Width, l.Height)
            };
            let (ax, ay, aw, ah) = boxOf(&fixed);
            let (bx, by, bw, bh) = boxOf(&movable);
            assert!(!(ax < bx + bw && bx < ax + aw && ay < by + bh && by < ay + ah), "selected={selected} overlap");
        }
    }
}
