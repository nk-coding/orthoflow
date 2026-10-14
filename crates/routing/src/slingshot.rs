// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/routing/slingshot.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of routing/slingshot.go.
#[allow(unused_imports)]
use crate::*;
use gocompat::math;
use gocompat::{GoResult, Map};
use labeling::PositionedArrowheadLabel;
use layoutgraph::{EdgeRef, NodeRef};

pub(crate) fn positionedArrowheadLabelCost(
    pal: &PositionedArrowheadLabel,
    nodes: &[NodeRef],
    labels: &[PositionedArrowheadLabel],
    routes: &[RouteRef],
    edges: &[EdgeRef],
) -> f64 {
    let mut penalty = 0.0;
    for other in labels {
        if pal.Edge == other.Edge && pal.IsTarget == other.IsTarget {
            continue;
        }
        if pal.Box.Overlaps(&other.Box) {
            if pal.Text == other.Text {
                continue;
            }
            return math::Inf(1);
        }
    }

    let graph = pal.Edge.from().g();
    let fakeLabelNode = fakeBoxNode(&pal.Box, Some(graph.clone()));
    penalty += 4.0 * graph.TurnCost() * fakeLabelNode.NodeOverlapCount(nodes, label::PADDING as f64) as f64;

    let mut overlappingEdgeCount: i64 = 0;
    // during edge routing we work with routes, afterwards we use edges
    for route in routes {
        let r = route.b();
        if r.GEdge == pal.Edge {
            continue;
        }
        for i in 1..r.OVGNodes.len() {
            if fakeLabelNode.OverlapsLine(&r.OVGNodes[i - 1].P(), &r.OVGNodes[i].P(), 0.0) {
                overlappingEdgeCount += 1;
                break;
            }
        }
    }
    for edge in edges {
        if *edge == pal.Edge {
            continue;
        }
        let points = pointValues(&edge.b().Points);
        for i in 1..points.len() {
            if fakeLabelNode.OverlapsLine(&points[i - 1], &points[i], 0.0) {
                overlappingEdgeCount += 1;
                break;
            }
        }
    }
    penalty += graph.TurnCost() * overlappingEdgeCount as f64;
    penalty
}

// launch simulates the route blasting off from source, going around the anchor, and landing on target
// It returns the distance if successful, with 0.0 meaning it crashed into a node along the way
impl ovgEdgeRouter {
    pub(crate) fn launch(&mut self, source: &OVGNodeRef, anchor: &OVGNodeRef, target: &OVGNodeRef, gEdge: &EdgeRef) -> GoResult<f64> {
        let nodes = self.ovg.b().NodesInsideBoundingBox.clone();
        let (s, a, t) = (source.P(), anchor.P(), target.P());

        gEdge.bm().Points = vec![geo::PointRef::new(s), geo::PointRef::new(a)];
        let intersects = routeIntersectsNodeGuarded(&nodes, gEdge, self.w())?;
        if intersects {
            return Ok(0.0);
        }

        gEdge.bm().Points = vec![geo::PointRef::new(t), geo::PointRef::new(a)];
        let intersects = routeIntersectsNodeGuarded(&nodes, gEdge, self.w())?;
        if intersects {
            return Ok(0.0);
        }

        let d = geo::EuclideanDistance(s.X, s.Y, a.X, a.Y) + geo::EuclideanDistance(t.X, t.Y, a.X, a.Y);
        Ok(d)
    }

    pub(crate) fn findLaunchings(&self, isVerticalLaunch: bool, orientation: geo::Orientation, gSource: &NodeRef) -> Vec<OVGNodeRef> {
        let direction = if orientation == geo::TopLeft {
            if isVerticalLaunch { geo::Bottom } else { geo::Right }
        } else if orientation == geo::TopRight {
            if isVerticalLaunch { geo::Bottom } else { geo::Left }
        } else if orientation == geo::BottomLeft {
            if isVerticalLaunch { geo::Top } else { geo::Right }
        } else if orientation == geo::BottomRight {
            if isVerticalLaunch { geo::Top } else { geo::Left }
        } else {
            geo::NONE
        };
        if direction == geo::NONE {
            return Vec::new();
        }
        self.portsWithDirection(gSource, direction)
    }

    pub(crate) fn findLandings(&self, isVerticalLaunch: bool, orientation: geo::Orientation, gTarget: &NodeRef) -> Vec<OVGNodeRef> {
        let direction = if orientation == geo::TopLeft {
            if isVerticalLaunch { geo::Left } else { geo::Top }
        } else if orientation == geo::TopRight {
            if isVerticalLaunch { geo::Right } else { geo::Top }
        } else if orientation == geo::BottomLeft {
            if isVerticalLaunch { geo::Left } else { geo::Bottom }
        } else if orientation == geo::BottomRight {
            if isVerticalLaunch { geo::Right } else { geo::Bottom }
        } else {
            geo::NONE
        };
        if direction == geo::NONE {
            return Vec::new();
        }
        self.portsWithDirection(gTarget, direction)
    }

    /// The shared loop of findLaunchings/findLandings: distinct ports of `owner`
    /// with the given port direction, in port order.
    fn portsWithDirection(&self, owner: &NodeRef, direction: geo::Orientation) -> Vec<OVGNodeRef> {
        let ports = self.ovg.b().Ports.get(owner).cloned().unwrap_or_default();
        let mut out: Vec<OVGNodeRef> = Vec::new();
        let mut seen: gocompat::Set<OVGNodeRef> = gocompat::Set::default();
        for port in &ports {
            if seen.contains(port) {
                continue;
            }
            seen.insert(port.clone());
            if port.hasPortDirection(owner, direction) {
                out.push(port.clone());
            }
        }
        out
    }
}

pub(crate) fn undershot(n: &OVGNodeRef, target: &NodeRef, orientation: geo::Orientation, isVerticalLaunch: bool) -> bool {
    let p = n.P();
    let tl = target.tlv();
    let (w, h) = (target.w(), target.h());
    if orientation == geo::TopLeft {
        if isVerticalLaunch && p.Y < tl.Y {
            return true;
        } else if !isVerticalLaunch && p.X < tl.X {
            return true;
        }
    } else if orientation == geo::TopRight {
        if isVerticalLaunch && p.Y < tl.Y {
            return true;
        } else if !isVerticalLaunch && p.X > tl.X + w {
            return true;
        }
    } else if orientation == geo::BottomLeft {
        if isVerticalLaunch && p.Y > tl.Y + h {
            return true;
        } else if !isVerticalLaunch && p.X < tl.X {
            return true;
        }
    } else if orientation == geo::BottomRight {
        if isVerticalLaunch && p.Y > tl.Y + h {
            return true;
        } else if !isVerticalLaunch && p.X > tl.X + w {
            return true;
        }
    }
    false
}

// overshot returns true if n is past the target, given the orientation of the source.
// L routes can keep going until n is completely past t
// S routes are overshot when n reaches any part of t
pub(crate) fn overshot(n: &OVGNodeRef, target: &NodeRef, orientation: geo::Orientation, isVerticalLaunch: bool, forLRoute: bool) -> bool {
    let p = n.P();
    let tl = target.tlv();
    let (w, h) = (target.w(), target.h());
    if orientation == geo::TopLeft {
        if isVerticalLaunch {
            if forLRoute && p.Y > tl.Y + h {
                return true;
            }
            if !forLRoute && p.Y > tl.Y {
                return true;
            }
        } else {
            if forLRoute && p.X > tl.X + w {
                return true;
            }
            if !forLRoute && p.X > tl.X {
                return true;
            }
        }
    } else if orientation == geo::TopRight {
        if isVerticalLaunch {
            if forLRoute && p.Y > tl.Y + h {
                return true;
            }
            if !forLRoute && p.Y > tl.Y {
                return true;
            }
        } else {
            if forLRoute && p.X < tl.X {
                return true;
            }
            if !forLRoute && p.X < tl.X + w {
                return true;
            }
        }
    } else if orientation == geo::BottomLeft {
        if isVerticalLaunch {
            if forLRoute && p.Y < tl.Y {
                return true;
            }
            if !forLRoute && p.Y < tl.Y + h {
                return true;
            }
        } else {
            if forLRoute && p.X > tl.X + w {
                return true;
            }
            if !forLRoute && p.X > tl.X {
                return true;
            }
        }
    } else if orientation == geo::BottomRight {
        if isVerticalLaunch {
            if forLRoute && p.Y < tl.Y {
                return true;
            }
            if !forLRoute && p.Y < tl.Y + h {
                return true;
            }
        } else {
            if forLRoute && p.X < tl.X {
                return true;
            }
            if !forLRoute && p.X < tl.X + w {
                return true;
            }
        }
    }
    false
}

pub(crate) fn isOnFlightPlan(prev: &OVGNodeRef, next: &OVGNodeRef, orientation: geo::Orientation, isVerticalLaunch: bool) -> bool {
    let (prev, next) = (prev.P(), next.P());
    if orientation == geo::TopLeft {
        if isVerticalLaunch && next.X == prev.X && next.Y > prev.Y {
            return true;
        } else if !isVerticalLaunch && next.Y == prev.Y && next.X > prev.X {
            return true;
        }
    } else if orientation == geo::TopRight {
        if isVerticalLaunch && next.X == prev.X && next.Y > prev.Y {
            return true;
        } else if !isVerticalLaunch && next.Y == prev.Y && next.X < prev.X {
            return true;
        }
    } else if orientation == geo::BottomLeft {
        if isVerticalLaunch && next.X == prev.X && next.Y < prev.Y {
            return true;
        } else if !isVerticalLaunch && next.Y == prev.Y && next.X > prev.X {
            return true;
        }
    } else if orientation == geo::BottomRight {
        if isVerticalLaunch && next.X == prev.X && next.Y < prev.Y {
            return true;
        } else if !isVerticalLaunch && next.Y == prev.Y && next.X < prev.X {
            return true;
        }
    }
    false
}

// So routes which are less likely to take up the routing space of other nodes are preferred
pub(crate) fn preferLaunchingVertically(gSource: &NodeRef, gTarget: &NodeRef, orientation: geo::Orientation) -> (bool, bool) {
    let mut numInVerticalLaunchSpace = 0;
    let mut numInHorizontalLaunchSpace = 0;

    let (verticalSpaceSourceOrientations, verticalSpaceTargetOrientations, horizontalSpaceSourceOrientations, horizontalSpaceTargetOrientations): (
        Vec<geo::Orientation>,
        Vec<geo::Orientation>,
        Vec<geo::Orientation>,
        Vec<geo::Orientation>,
    ) = if orientation == geo::TopLeft {
        (vec![geo::BottomLeft, geo::Bottom], vec![geo::Left, geo::BottomLeft], vec![geo::TopRight, geo::Right], vec![geo::Top, geo::TopRight])
    } else if orientation == geo::TopRight {
        (vec![geo::BottomRight, geo::Bottom], vec![geo::Right, geo::BottomRight], vec![geo::TopLeft, geo::Left], vec![geo::Top, geo::TopLeft])
    } else if orientation == geo::BottomLeft {
        (vec![geo::TopLeft, geo::Top], vec![geo::Left, geo::TopLeft], vec![geo::BottomRight, geo::Right], vec![geo::Bottom, geo::BottomRight])
    } else if orientation == geo::BottomRight {
        (vec![geo::TopRight, geo::Top], vec![geo::Right, geo::TopRight], vec![geo::BottomLeft, geo::Left], vec![geo::Bottom, geo::BottomLeft])
    } else {
        (Vec::new(), Vec::new(), Vec::new(), Vec::new())
    };

    for e in gSource.edges() {
        let adj = gSource.Adjacent(&e);
        if adj == *gTarget {
            continue;
        }
        let o = adj.Orientation(gSource);
        if o == geo::NONE {
            continue;
        }
        if verticalSpaceSourceOrientations.contains(&o) {
            numInVerticalLaunchSpace += 1;
        }
        if horizontalSpaceSourceOrientations.contains(&o) {
            numInHorizontalLaunchSpace += 1;
        }
    }

    for e in gTarget.edges() {
        let adj = gTarget.Adjacent(&e);
        if adj == *gSource {
            continue;
        }
        let o = adj.Orientation(gTarget);
        if o == geo::NONE {
            continue;
        }
        if verticalSpaceTargetOrientations.contains(&o) {
            numInVerticalLaunchSpace += 1;
        } else if horizontalSpaceTargetOrientations.contains(&o) {
            numInHorizontalLaunchSpace += 1;
        }
    }

    // Even if there is no distinction, we don't just want to always prefer one or the other, as that can lead to weird looking graphs
    // So we use node ID as a pseudo-random tiebreak
    if numInVerticalLaunchSpace == numInHorizontalLaunchSpace {
        return (gSource.id() < gTarget.id(), false);
    }
    (numInVerticalLaunchSpace < numInHorizontalLaunchSpace, true)
}

impl ovgEdgeRouter {
    pub(crate) fn fillPathGuarded(&mut self, from: &OVGNodeRef, to: &OVGNodeRef) -> GoResult<(Vec<OVGNodeRef>, bool)> {
        let mut out: Vec<OVGNodeRef> = vec![from.clone()];
        let mut curr = from.clone();
        let (f, t) = (from.P(), to.P());
        loop {
            self.w().step()?;
            let prev = curr.clone();
            let edges = curr.b().Edges.clone();
            for edge in &edges {
                self.w().step()?;
                let adjacent = curr.Adjacent(edge);
                let (a, c) = (adjacent.P(), curr.P());
                let mut onPath = false;
                if f.X == t.X && a.X == c.X {
                    onPath = f.Y < t.Y && c.Y < a.Y || f.Y > t.Y && c.Y > a.Y;
                } else if f.Y == t.Y && a.Y == c.Y {
                    onPath = f.X < t.X && c.X < a.X || f.X > t.X && c.X > a.X;
                }
                if onPath {
                    curr = adjacent;
                    break;
                }
            }
            if curr == prev {
                return Ok((Vec::new(), false));
            }
            if curr == *to {
                break;
            }
            out.push(curr.clone());
        }
        Ok((out, true))
    }
}

/// Which memoized check a [`RouteChecker`] computes (Go stores a closure).
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum routeCheckKind {
    /// occupiedRouteChecker: the segment would share a route it may not share.
    occupied,
    /// crossedRouteChecker: the segment crosses an existing route.
    crossed,
}

pub struct RouteChecker {
    pub(crate) mem: Map<OVGNodeRef, Map<OVGNodeRef, bool>>,
    pub(crate) kind: routeCheckKind,
    pub(crate) gEdge: EdgeRef,
}

pub(crate) fn newFallibleRouteChecker(kind: routeCheckKind, gEdge: &EdgeRef) -> RouteChecker {
    RouteChecker { mem: Map::default(), kind, gEdge: gEdge.clone() }
}

impl RouteChecker {
    pub(crate) fn cacheCheck(&self, from: &OVGNodeRef, to: &OVGNodeRef) -> (bool, bool) {
        if let Some(m) = self.mem.get(from) {
            if let Some(&is) = m.get(to) {
                return (is, true);
            }
        }
        (false, false)
    }

    pub(crate) fn check(&mut self, router: &mut ovgEdgeRouter, from: &OVGNodeRef, to: &OVGNodeRef) -> GoResult<bool> {
        let (is, ok) = self.cacheCheck(from, to);
        if ok {
            return Ok(is);
        }
        self.mem.entry(from.clone()).or_default();
        // Go: checker.mem[from][to], err = checker.compute(from, to) stores the
        // (zero on error) result before returning the error.
        let result = match self.kind {
            routeCheckKind::occupied => router.occupiedRouteCheck(&self.gEdge, from, to),
            routeCheckKind::crossed => router.crossedRouteCheck(from, to),
        };
        let value = *result.as_ref().unwrap_or(&false);
        self.mem.get_mut(from).unwrap().insert(to.clone(), value);
        result
    }
}

impl ovgEdgeRouter {
    fn occupiedRouteCheck(&mut self, gEdge: &EdgeRef, from: &OVGNodeRef, to: &OVGNodeRef) -> GoResult<bool> {
        let mut overlappingEdges: Vec<EdgeRef> = Vec::new();
        let overlappingRoutes = self.findOverlappingRoutes(from, to)?;
        for route in &overlappingRoutes {
            let n = route.b().OVGNodes.len() as u64;
            self.w().reserveProduct(n + 1, 2)?;
            overlappingEdges.push(route.b().GEdge.clone());
            if gEdge.IsDirected() && route.b().isOpposingColinear(from, to) {
                return Ok(true);
            }
        }
        let canOverlap = edgeCanOverlapEdgesGuarded(gEdge, &overlappingEdges, None, None, self.w())?;
        Ok(!overlappingEdges.is_empty() && !canOverlap)
    }

    fn crossedRouteCheck(&mut self, from: &OVGNodeRef, to: &OVGNodeRef) -> GoResult<bool> {
        let w = self.w().clone();
        self.edgeSet.intersectsWithGuarded(&NewOVGEdge(from, to), &w)
    }

    pub(crate) fn arrowheadLabelOverlapPenalty(&mut self, gEdge: &EdgeRef, route: &[OVGNodeRef]) -> GoResult<f64> {
        let (hasSource, hasTarget) = {
            let e = gEdge.b();
            (e.SourceArrowheadLabel.is_some(), e.TargetArrowheadLabel.is_some())
        };
        if !hasSource && !hasTarget {
            return Ok(0.0);
        }

        let mut penalty = 0.0;
        let mut routePoints: Vec<geo::PointRef> = Vec::with_capacity(route.len().saturating_sub(2));
        for i in 1..route.len().saturating_sub(1) {
            self.w().step()?;
            routePoints.push(geo::PointRef::new(route[i].P()));
        }

        let graphNodes = self.graph.nodes();
        let nonAncestors = filterEdgeAncestorsGuarded(gEdge, &graphNodes, self.w())?;

        if hasSource {
            let pal = labeling::PositionArrowheadLabel(gEdge, false, &routePoints).expect("nil pointer dereference");
            let cost = positionedArrowheadLabelCostGuarded(&pal, &nonAncestors, &self.positionedLabels, &self.routes, &[], self.w())?;
            penalty += cost;
        }
        if hasTarget {
            let pal = labeling::PositionArrowheadLabel(gEdge, true, &routePoints).expect("nil pointer dereference");
            let cost = positionedArrowheadLabelCostGuarded(&pal, &nonAncestors, &self.positionedLabels, &self.routes, &[], self.w())?;
            penalty += cost;
        }
        Ok(penalty)
    }

    pub(crate) fn slingshot(&mut self, ctx: &gocompat::context::Ctx, gEdge: &EdgeRef) -> GoResult<(Vec<OVGNodeRef>, f64)> {
        self.bindWork(ctx)?;
        self.w().step()?;
        let (gSource, gTarget) = (gEdge.from(), gEdge.to());

        if gSource.clusterField().is_some() || gTarget.clusterField().is_some() {
            return Ok((Vec::new(), 0.0));
        }

        let orientation = gSource.Orientation(&gTarget);
        if orientation == geo::NONE {
            return Ok((Vec::new(), 0.0));
        }
        if gEdge.HasTableColumn() {
            return Ok((Vec::new(), 0.0));
        }
        if !orientation.IsDiagonal() {
            return Ok((Vec::new(), 0.0));
        }

        let mut occupiedRouteChecker = newFallibleRouteChecker(routeCheckKind::occupied, gEdge);
        let mut crossedRouteChecker = newFallibleRouteChecker(routeCheckKind::crossed, gEdge);

        let mut order = vec![false, true];
        self.w().reserveSum(gSource.b().Edges.len() as u64, gTarget.b().Edges.len() as u64)?;
        let (verticalPref, strongLaunchPref) = preferLaunchingVertically(&gSource, &gTarget, orientation);
        if verticalPref {
            order = vec![true, false];
        }

        if !gEdge.IsBetweenTableColumns() {
            // as we route column to column, there's no way to get L shaped routes between tables
            let routes = self.routes.clone();
            let (shortestPath, mut shortestFlight) = self.findLShapedRoute(
                ctx,
                &routes,
                &order,
                verticalPref,
                strongLaunchPref,
                gEdge,
                orientation,
                &mut occupiedRouteChecker,
                &mut crossedRouteChecker,
            )?;
            if shortestFlight != math::Inf(1) {
                shortestFlight += self.turnCost;
                return Ok((shortestPath, shortestFlight));
            }
        }

        self.findSShapedRoute(
            ctx,
            &order,
            verticalPref,
            strongLaunchPref,
            gEdge,
            orientation,
            &mut occupiedRouteChecker,
            &mut crossedRouteChecker,
        )
    }

    fn tableColumnPorts(&mut self, gEdge: &EdgeRef, gSource: &NodeRef, gTarget: &NodeRef, sourcePorts: &mut Vec<OVGNodeRef>, targetPorts: &mut Vec<OVGNodeRef>) -> GoResult<()> {
        let (sPorts, tPorts) = self.tablePorts(gEdge, gSource, gTarget, true)?;
        // Go ranges over the port sets; with facing ports each set holds at most
        // one port, so the order is irrelevant.
        if let Some(s) = sPorts {
            sourcePorts.extend(s.iter().cloned());
        }
        if let Some(t) = tPorts {
            targetPorts.extend(t.iter().cloned());
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn findLShapedRoute(
        &mut self,
        ctx: &gocompat::context::Ctx,
        routes: &[RouteRef],
        order: &[bool],
        verticalPref: bool,
        strongLaunchPref: bool,
        gEdge: &EdgeRef,
        orientation: geo::Orientation,
        occupiedRouteChecker: &mut RouteChecker,
        crossedRouteChecker: &mut RouteChecker,
    ) -> GoResult<(Vec<OVGNodeRef>, f64)> {
        self.bindWork(ctx)?;
        self.w().step()?;
        let (gSource, gTarget) = (gEdge.from(), gEdge.to());

        let (source, target) = {
            let ovg = self.ovg.b();
            (ovg.Centers.get(&gSource).cloned(), ovg.Centers.get(&gTarget).cloned())
        };

        let mut shortestPath: Vec<OVGNodeRef> = Vec::new();
        let mut shortestFlight = math::Inf(1);

        let gEdgeCopy = layoutgraph::NewEdge(&gSource, &gTarget);
        let (fromCol, toCol) = {
            let e = gEdge.b();
            (e.FromTableColumnIndex, e.ToTableColumnIndex)
        };

        for &isVerticalLaunch in order {
            self.w().step()?;
            if isVerticalLaunch && fromCol.is_some() {
                // this can't happen
                continue;
            }
            let mut sourcePorts: Vec<OVGNodeRef> = Vec::new();
            let mut targetPorts: Vec<OVGNodeRef> = Vec::new();
            if fromCol.is_none() {
                let n = self.ovg.b().Ports.get(&gSource).map_or(0, |v| v.len());
                self.w().add(n as u64)?;
                sourcePorts = self.findLaunchings(isVerticalLaunch, orientation, &gSource);
            }
            if toCol.is_none() {
                let n = self.ovg.b().Ports.get(&gTarget).map_or(0, |v| v.len());
                self.w().add(n as u64)?;
                targetPorts = self.findLandings(isVerticalLaunch, orientation, &gTarget);
            }
            if gEdge.HasTableColumn() {
                self.tableColumnPorts(gEdge, &gSource, &gTarget, &mut sourcePorts, &mut targetPorts)?;
            }

            for sPort in &sourcePorts {
                self.w().step()?;
                let mut curr = sPort.clone();
                loop {
                    self.w().step()?;
                    let mut next: Option<OVGNodeRef> = None;
                    let edges = curr.b().Edges.clone();
                    for e in &edges {
                        self.w().step()?;
                        let adj = curr.Adjacent(e);

                        if isOnFlightPlan(&curr, &adj, orientation, isVerticalLaunch) {
                            next = Some(adj);
                            break;
                        }
                    }
                    let Some(next) = next else { break };
                    if overshot(&next, &gTarget, orientation, isVerticalLaunch, true) {
                        break;
                    }
                    let occupied = occupiedRouteChecker.check(self, &curr, &next)?;
                    if occupied {
                        break;
                    }
                    curr = next.clone();
                    if undershot(&next, &gTarget, orientation, isVerticalLaunch) {
                        continue;
                    }
                    for tPort in &targetPorts {
                        self.w().step()?;
                        let (np, tp) = (next.P(), tPort.P());
                        if isVerticalLaunch && np.Y != tp.Y {
                            continue;
                        }
                        if !isVerticalLaunch && np.X != tp.X {
                            continue;
                        }

                        let mut d = self.launch(sPort, &next, tPort, &gEdgeCopy)?;
                        if d == 0.0 {
                            continue;
                        }

                        let crossed = crossedRouteChecker.check(self, sPort, &next)?;
                        if crossed {
                            d += self.crossingCost;
                        }
                        let crossed = crossedRouteChecker.check(self, &next, tPort)?;
                        if crossed {
                            d += self.crossingCost;
                        }

                        if strongLaunchPref {
                            if verticalPref && !isVerticalLaunch {
                                d += self.crossingCost / 2.0;
                            }
                            if !verticalPref && isVerticalLaunch {
                                d += self.crossingCost / 2.0;
                            }
                        }
                        if d < shortestFlight {
                            let mut path: Vec<OVGNodeRef> = vec![source.clone().expect("nil OVG center")];
                            let (p, ok) = self.fillPathGuarded(sPort, &next)?;
                            if !ok {
                                continue;
                            }
                            path.extend(p);
                            let (mut p, ok) = self.fillPathGuarded(&next, tPort)?;
                            if !ok {
                                continue;
                            }
                            p.push(tPort.clone());
                            // We already checked share for sPort to anchor
                            let mut badShare = false;
                            let mut i = 0usize;
                            while i + 2 < p.len() {
                                self.w().step()?;
                                let (c, n) = (p[i].clone(), p[i + 1].clone());
                                let occupied = occupiedRouteChecker.check(self, &c, &n)?;
                                if occupied {
                                    badShare = true;
                                    break;
                                }
                                i += 1;
                            }
                            if badShare {
                                continue;
                            }
                            path.extend(p);
                            path.push(target.clone().expect("nil OVG center"));

                            // even if it is ok to share with a route, we don't want the exact same route
                            let mut duplicateRoute = false;
                            let (sp, tp) = (sPort.P(), tPort.P());
                            for r in routes {
                                self.w().step()?;
                                let rb = r.b();
                                if nonNilEquals(&rb.FromPort, &sp) && nonNilEquals(&rb.ToPort, &tp) && rb.OVGNodes.len() == path.len() {
                                    let mut isDuplicate = true;
                                    // check if points between ports are equal
                                    let mut i = 2usize;
                                    while i + 2 < path.len() {
                                        self.w().step()?;
                                        if !nonNilEquals(&rb.OVGNodes[i].P(), &path[i].P()) {
                                            isDuplicate = false;
                                            break;
                                        }
                                        i += 1;
                                    }
                                    if isDuplicate {
                                        duplicateRoute = true;
                                        break;
                                    }
                                }
                            }
                            if duplicateRoute {
                                continue;
                            }

                            let labelPenalty = self.arrowheadLabelOverlapPenalty(gEdge, &path)?;
                            d += labelPenalty;
                            if d < shortestFlight {
                                shortestFlight = d;
                                shortestPath = path;
                            }
                        }
                    }
                }
            }
        }

        Ok((shortestPath, shortestFlight))
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn findSShapedRoute(
        &mut self,
        ctx: &gocompat::context::Ctx,
        order: &[bool],
        verticalPref: bool,
        strongLaunchPref: bool,
        gEdge: &EdgeRef,
        orientation: geo::Orientation,
        occupiedRouteChecker: &mut RouteChecker,
        crossedRouteChecker: &mut RouteChecker,
    ) -> GoResult<(Vec<OVGNodeRef>, f64)> {
        self.bindWork(ctx)?;
        self.w().step()?;
        let (gSource, gTarget) = (gEdge.from(), gEdge.to());

        let (source, target, nodes) = {
            let ovg = self.ovg.b();
            (ovg.Centers.get(&gSource).cloned(), ovg.Centers.get(&gTarget).cloned(), ovg.NodesInsideBoundingBox.clone())
        };

        let mut shortestPath: Vec<OVGNodeRef> = Vec::new();
        let mut shortestFlight = math::Inf(1);

        let gEdgeCopy = layoutgraph::NewEdge(&gSource, &gTarget);
        let (fromCol, toCol) = {
            let e = gEdge.b();
            (e.FromTableColumnIndex, e.ToTableColumnIndex)
        };

        for &isVerticalLaunch in order {
            self.w().step()?;
            let mut sourcePorts: Vec<OVGNodeRef> = Vec::new();
            let mut targetPorts: Vec<OVGNodeRef> = Vec::new();
            if isVerticalLaunch && fromCol.is_some() {
                // this can't happen
                continue;
            }
            if fromCol.is_none() {
                let n = self.ovg.b().Ports.get(&gSource).map_or(0, |v| v.len());
                self.w().add(n as u64)?;
                sourcePorts = self.findLaunchings(isVerticalLaunch, orientation, &gSource);
            }
            if toCol.is_none() {
                let n = self.ovg.b().Ports.get(&gTarget).map_or(0, |v| v.len());
                self.w().add(n as u64)?;
                targetPorts = self.findLandings(!isVerticalLaunch, orientation, &gTarget);
            }
            if gEdge.HasTableColumn() {
                self.tableColumnPorts(gEdge, &gSource, &gTarget, &mut sourcePorts, &mut targetPorts)?;
            }

            let mut sourceAnchors: Map<OVGNodeRef, OVGNodeRef> = Map::default();
            let mut targetAnchors: Map<OVGNodeRef, OVGNodeRef> = Map::default();
            let mut sAnchorOrder: Vec<OVGNodeRef> = Vec::new();
            let mut tAnchorOrder: Vec<OVGNodeRef> = Vec::new();

            for sPort in &sourcePorts {
                self.w().step()?;
                let mut curr = sPort.clone();
                loop {
                    self.w().step()?;
                    let mut next: Option<OVGNodeRef> = None;
                    let edges = curr.b().Edges.clone();
                    for e in &edges {
                        self.w().step()?;
                        let adj = curr.Adjacent(e);

                        if isOnFlightPlan(&curr, &adj, orientation, isVerticalLaunch) {
                            next = Some(adj);
                            break;
                        }
                    }
                    let Some(next) = next else { break };
                    if overshot(&next, &gTarget, orientation, isVerticalLaunch, false) {
                        break;
                    }
                    let occupied = occupiedRouteChecker.check(self, &curr, &next)?;
                    if occupied {
                        break;
                    }

                    sourceAnchors.insert(next.clone(), sPort.clone());
                    sAnchorOrder.push(next.clone());
                    curr = next;
                }
            }

            for tPort in &targetPorts {
                self.w().step()?;
                let mut curr = tPort.clone();
                loop {
                    self.w().step()?;
                    let mut next: Option<OVGNodeRef> = None;
                    let edges = curr.b().Edges.clone();
                    for e in &edges {
                        self.w().step()?;
                        let adj = curr.Adjacent(e);

                        if isOnFlightPlan(&curr, &adj, orientation.GetOpposite(), isVerticalLaunch) {
                            next = Some(adj);
                            break;
                        }
                    }
                    let Some(next) = next else { break };
                    if overshot(&next, &gSource, orientation.GetOpposite(), isVerticalLaunch, false) {
                        break;
                    }
                    let occupied = occupiedRouteChecker.check(self, &curr, &next)?;
                    if occupied {
                        break;
                    }

                    targetAnchors.insert(next.clone(), tPort.clone());
                    tAnchorOrder.push(next.clone());
                    curr = next;
                }
            }

            let idealTurnAxes = idealTurnAxes(&gSource, &gTarget);
            // Prefer anchors closer to the midpoint
            self.w().reserveSort(sAnchorOrder.len())?;
            gocompat::sort::SlicesSortStableFunc(&mut sAnchorOrder, |a: &OVGNodeRef, b: &OVGNodeRef| {
                let (aDistance, bDistance) = if isVerticalLaunch {
                    ((a.P().Y - idealTurnAxes[1].val).abs(), (b.P().Y - idealTurnAxes[1].val).abs())
                } else {
                    ((a.P().X - idealTurnAxes[0].val).abs(), (b.P().X - idealTurnAxes[0].val).abs())
                };
                if aDistance < bDistance {
                    -1i64
                } else if bDistance < aDistance {
                    1
                } else {
                    0
                }
            });

            for sAnchor in &sAnchorOrder {
                self.w().step()?;
                let sPort = sourceAnchors[sAnchor].clone();
                for tAnchor in &tAnchorOrder {
                    self.w().step()?;
                    let tPort = targetAnchors[tAnchor].clone();
                    let (sa, ta) = (sAnchor.P(), tAnchor.P());
                    if isVerticalLaunch && sa.Y != ta.Y {
                        continue;
                    }
                    if !isVerticalLaunch && sa.X != ta.X {
                        continue;
                    }
                    let mut d = 0.0;
                    let pairs = [(sPort.clone(), sAnchor.clone()), (sAnchor.clone(), tAnchor.clone()), (tAnchor.clone(), tPort.clone())];
                    let mut intersects = false;
                    for (p0, p1) in &pairs {
                        self.w().step()?;
                        let (a, b) = (p0.P(), p1.P());
                        gEdgeCopy.bm().Points = vec![geo::PointRef::new(a), geo::PointRef::new(b)];
                        let intersectsNode = routeIntersectsNodeGuarded(&nodes, &gEdgeCopy, self.w())?;
                        if intersectsNode {
                            intersects = true;
                            break;
                        }
                        d += geo::EuclideanDistance(a.X, a.Y, b.X, b.Y);
                        let crossed = crossedRouteChecker.check(self, p0, p1)?;
                        if crossed {
                            d += self.crossingCost;
                        }
                    }
                    if intersects {
                        continue;
                    }
                    if strongLaunchPref {
                        if verticalPref && !isVerticalLaunch {
                            d += self.crossingCost / 2.0;
                        }
                        if !verticalPref && isVerticalLaunch {
                            d += self.crossingCost / 2.0;
                        }
                    }
                    if d < shortestFlight {
                        let mut path: Vec<OVGNodeRef> = vec![source.clone().expect("nil OVG center")];
                        let (p, ok) = self.fillPathGuarded(&sPort, sAnchor)?;
                        if !ok {
                            continue;
                        }
                        path.extend(p);
                        let (mut p, ok) = self.fillPathGuarded(sAnchor, tAnchor)?;
                        if !ok {
                            continue;
                        }
                        // add this just for the share check
                        p.push(tAnchor.clone());
                        // We already checked share for sPort to sAnchor and tPort to tAnchor
                        let mut badShare = false;
                        let mut i = 0usize;
                        while i + 2 < p.len() {
                            self.w().step()?;
                            let (c, n) = (p[i].clone(), p[i + 1].clone());
                            let occupied = occupiedRouteChecker.check(self, &c, &n)?;
                            if occupied {
                                badShare = true;
                                break;
                            }
                            i += 1;
                        }
                        if badShare {
                            continue;
                        }
                        path.extend_from_slice(&p[..p.len() - 1]);
                        let (p, ok) = self.fillPathGuarded(tAnchor, &tPort)?;
                        if !ok {
                            continue;
                        }
                        path.extend(p);
                        path.push(tPort.clone());
                        path.push(target.clone().expect("nil OVG center"));

                        let labelPenalty = self.arrowheadLabelOverlapPenalty(gEdge, &path)?;
                        d += labelPenalty;
                        if d < shortestFlight {
                            shortestFlight = d;
                            shortestPath = path;
                        }
                    }
                }
            }
        }

        if shortestFlight != math::Inf(1) {
            shortestFlight += self.turnCost * 2.0;
        }

        Ok((shortestPath, shortestFlight))
    }
}
