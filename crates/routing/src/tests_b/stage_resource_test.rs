// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/routing/edge_routing_stage_resource_test.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of routing/edge_routing_stage_resource_test.go.
use super::*;
use std::rc::Rc;

#[test]
fn TestEdgeRoutingSplitConsumesStageAggregate() {
    let g = layoutgraph::NewGraph();
    let node = nodeAt(1, 10.0, 10.0, 0.0, 0.0);
    g.AddNewNodeToContainer(None, &node);
    let external = nodeAt(2, 10.0, 10.0, 100.0, 0.0);
    node.bm().Nears.insert(external.clone());

    let guard = newRouteWorkGuard(&bg(), "EdgeRouting", 1).unwrap();
    let (result, ownership) = g.SplitSubgraphsTracked(
        &bg(),
        layoutgraph::SplitOptions { IncludeContainers: true, IncludeNears: true, TraverseTrees: false },
        Some(&guard as &dyn layoutgraph::SharedWorkStepper),
    );
    let err = result.err().expect("split succeeded");
    assert!(err.Is(&errRouteStageWorkLimit), "SplitSubgraphs error = {err}");
    let _ = ownership;
    assert!(external.graph().is_none(), "failed split changed Near-only owner");
}

#[test]
fn TestEdgeRoutingSplitChargesEveryTopLevelNodeScan() {
    let g = layoutgraph::NewGraph();
    let node = nodeAt(1, 10.0, 10.0, 0.0, 0.0);
    g.AddNewNodeToContainer(None, &node);

    let guard = newRouteWorkGuard(&bg(), "EdgeRouting", u64::MAX).unwrap();
    let (result, ownership) = g.SplitSubgraphsTracked(
        &bg(),
        layoutgraph::SplitOptions { IncludeContainers: true, IncludeNears: true, TraverseTrees: false },
        Some(&guard as &dyn layoutgraph::SharedWorkStepper),
    );
    result.unwrap();
    ownership.Restore();
    assert_eq!(guard.used(), 6, "SplitSubgraphs charged {} units, want exact top-level scan accounting 6", guard.used());
}

#[test]
fn TestRouteStageGraphBoundingBoxMatchesLegacyAndConsumesAggregate() {
    let g = layoutgraph::NewGraph();
    for index in 0..8 {
        let node = nodeAt(index + 1, 20.0 + index as f64, 15.0 + index as f64, (index * 40) as f64, ((index % 3) * 30) as f64);
        node.bm().Label = Some(layoutgraph::LabelRef::new(layoutgraph::Label::new("", label::OutsideBottomRight, 60.0 + index as f64, 15.0)));
        g.AddNewNodeToContainer(None, &node);
    }
    let nodes = g.nodes();
    let edge = g.Connect(&nodes[0], &nodes[nodes.len() - 1]);
    setPoints(&edge, &[(0.0, 0.0), (25.0, -50.0), (100.0, -50.0), (300.0, 60.0)]);
    edge.bm().Label = Some(layoutgraph::LabelRef::new(layoutgraph::Label::new("", label::UnlockedMiddle, 35.0, 12.0)));
    edge.bm().SourceArrowheadLabel = Some(layoutgraph::LabelRef::new(layoutgraph::Label::new("", label::Unset, 20.0, 10.0)));
    edge.bm().TargetArrowheadLabel = Some(layoutgraph::LabelRef::new(layoutgraph::Label::new("", label::Unset, 22.0, 11.0)));

    let (wantTopLeft, wantBottomRight) = g.BoundingBox();
    let guard = newRouteWorkGuard(&bg(), "EdgeRouting", u64::MAX).unwrap();
    let (gotTopLeft, gotBottomRight) = routeStageGraphBoundingBox(&g, &guard).unwrap();
    assert_eq!(gotTopLeft.get(), wantTopLeft.unwrap().get());
    assert_eq!(gotBottomRight.get(), wantBottomRight.unwrap().get());
    let used = guard.used();
    assert!(used > (nodes.len() * nodes.len()) as u64, "guarded bounds charged {used} units");

    let limited = newRouteWorkGuard(&bg(), "EdgeRouting", used - 1).unwrap();
    let err = routeStageGraphBoundingBox(&g, &limited).err().unwrap();
    assert!(err.Is(&errRouteStageWorkLimit), "limited guarded bounds error = {err}");
}

#[test]
fn TestRouteStageGraphBoundingBoxChargesLabelRouteScans() {
    let g = layoutgraph::NewGraph();
    let from = nodeAt(1, 10.0, 10.0, 0.0, 0.0);
    let to = nodeAt(2, 10.0, 10.0, 100.0, 0.0);
    g.AddNewNodeToContainer(None, &from);
    g.AddNewNodeToContainer(None, &to);
    let edge = g.Connect(&from, &to);
    setPoints(&edge, &[(10.0, 5.0), (25.0, -20.0), (50.0, -20.0), (75.0, 5.0), (100.0, 5.0)]);

    let used = || {
        let guard = newRouteWorkGuard(&bg(), "EdgeRouting", u64::MAX).unwrap();
        routeStageGraphBoundingBox(&g, &guard).unwrap();
        guard.used()
    };
    let baseline = used();
    let pointCount = edge.b().Points.len() as u64;
    let lbl = |w: f64, h: f64, pos: label::Position| Some(layoutgraph::LabelRef::new(layoutgraph::Label::new("", pos, w, h)));
    edge.bm().Label = lbl(30.0, 12.0, label::UnlockedMiddle);
    assert_eq!(used() - baseline, 2 * pointCount, "positioned edge label");
    edge.bm().Label = None;
    edge.bm().SourceArrowheadLabel = lbl(20.0, 10.0, label::Unset);
    assert_eq!(used() - baseline, 3 * pointCount, "source arrowhead label");
    edge.bm().SourceArrowheadLabel = None;
    edge.bm().TargetArrowheadLabel = lbl(20.0, 10.0, label::Unset);
    assert_eq!(used() - baseline, 3 * pointCount, "target arrowhead label");
}

#[test]
fn TestEdgeRoutingNestedWorkBudgetIsSharedAcrossSubgraphs() {
    let newComponent = |id: i64| {
        let g = layoutgraph::NewGraph();
        let from = nodeAt(id, 10.0, 10.0, 0.0, 0.0);
        let to = nodeAt(id + 1, 10.0, 10.0, 100.0, 0.0);
        g.AddNewNodeToContainer(None, &from);
        g.AddNewNodeToContainer(None, &to);
        g.Connect(&from, &to);
        g.ComputeCellSize();
        g
    };

    let stageGuard = newRouteWorkGuard(&bg(), "EdgeRouting", u64::MAX).unwrap();
    let aggregate: workBudgetRef = Rc::new(stageGuard.clone());
    let routingCtx = contextWithRouteAggregateWork(&bg(), Some(aggregate));
    let ovgGuard = newOVGBuildGuard(Some(&routingCtx), defaultOVGBuildLimits()).unwrap();
    routeEdgesWithResourceGuards(&routingCtx, &newComponent(1), &[], &ovgGuard, maxRouteSearchWorkUnits).unwrap();
    assert!(stageGuard.used() != 0, "nested OVG and route search consumed no aggregate work");
    let used = stageGuard.used();
    stageGuard.bm().limit = used + 1;

    let err = routeEdgesWithResourceGuards(&routingCtx, &newComponent(100), &[], &ovgGuard, maxRouteSearchWorkUnits).err().unwrap();
    assert!(err.Is(&errRouteStageWorkLimit), "route second subgraph error = {err}");
}

#[test]
fn TestRouteFlavorsShareAggregateBudget() {
    let stepsPerFlavor = 1_000u64;
    let stageGuard = newRouteWorkGuard(&bg(), "EdgeRouting", 2 * stepsPerFlavor).unwrap();
    let aggregate: workBudgetRef = Rc::new(stageGuard.clone());
    let ctx = contextWithRouteAggregateWork(&bg(), Some(aggregate));
    let first = newRouteSearchWorkGuard(&ctx, Default, maxRouteSearchWorkUnits).unwrap();
    let second = newRouteSearchWorkGuard(&ctx, ShortestToLongest, maxRouteSearchWorkUnits).unwrap();
    for guard in [&first, &second] {
        for _ in 0..stepsPerFlavor {
            guard.step().unwrap();
        }
    }
    let err = first.step().err().unwrap();
    assert!(err.Is(&errRouteStageWorkLimit), "post-flavor work error = {err}");
}
