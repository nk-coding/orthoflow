// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/routing/coordinator.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of routing/coordinator.go.
#[allow(unused_imports)]
use crate::*;
use gocompat::context::Ctx;
use gocompat::math;
use gocompat::sort;
use gocompat::{GoError, GoResult, Map, Set};
use labeling::PositionedArrowheadLabel;
use layoutgraph::{EdgeRef, EdgesMethods, GraphRef, NodeRef, NodesMethods};
use std::fmt;

/// Go `type RouteGenerationFlavor string` (zero value "").
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct RouteGenerationFlavor(pub &'static str);

pub const ShortestToLongest: RouteGenerationFlavor = RouteGenerationFlavor("ShortestToLongest");
pub const LongestToShortest: RouteGenerationFlavor = RouteGenerationFlavor("LongestToShortest");
pub const Default: RouteGenerationFlavor = RouteGenerationFlavor("Default");
pub const TopDownLeftRight: RouteGenerationFlavor = RouteGenerationFlavor("TopDownLeftRight");

impl fmt::Display for RouteGenerationFlavor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

pub struct GenerateRouteResponse {
    pub Routes: Vec<RouteRef>,
    pub Distance: f64,
    pub Err: Option<GoError>,
    pub Flavor: RouteGenerationFlavor,
    pub(crate) work: Option<routeSearchWorkGuardRef>,
}

impl std::default::Default for GenerateRouteResponse {
    fn default() -> Self {
        GenerateRouteResponse { Routes: Vec::new(), Distance: 0.0, Err: None, Flavor: RouteGenerationFlavor(""), work: None }
    }
}

// Go runs the flavors in goroutines (runRouteFlavorWorker) and restores the
// declared router order before selecting results (collectRouteFlavorResponses).
// The port runs them sequentially in that order. Worker panics propagate.
pub(crate) fn generateRouteFlavorResponses(
    ctx: &Ctx,
    routers: &mut [ovgEdgeRouter],
    straightLineFallback: bool,
) -> GoResult<Vec<GenerateRouteResponse>> {
    generateRouteFlavorResponsesWith(ctx, routers, |router| router.flavor, straightLineFallback, |router, ctx, fallback| {
        router.generateRoutes(ctx, fallback)
    })
}

/// Go `generateRouteFlavorResponsesWith(ctx, routers, fallback, worker)`; the
/// router type is generic so tests can inject flavor-only routers.
pub(crate) fn generateRouteFlavorResponsesWith<R>(
    ctx: &Ctx,
    routers: &mut [R],
    flavorOf: impl Fn(&R) -> RouteGenerationFlavor,
    straightLineFallback: bool,
    mut worker: impl FnMut(&mut R, &Ctx, bool) -> GenerateRouteResponse,
) -> GoResult<Vec<GenerateRouteResponse>> {
    if routers.len() as u64 > maxRouteSearchFlavors as u64 {
        return Err(GoError::wrap(
            format!(
                "{}: edge routing declared {} flavors; limit {}",
                errRouteSearchWorkLimit.Error(),
                routers.len(),
                maxRouteSearchFlavors
            ),
            errRouteSearchWorkLimit.err(),
        ));
    }
    let (workerCtx, cancelWorkers) = gocompat::context::WithCancel(ctx);
    let mut responses: Vec<GenerateRouteResponse> = Vec::with_capacity(routers.len());
    for router in routers.iter_mut() {
        // runRouteFlavorWorker: the coordinator owns flavor identity and ordering.
        let flavor = flavorOf(router);
        let mut response = worker(router, &workerCtx, straightLineFallback);
        response.Flavor = flavor;
        responses.push(response);
    }
    // collectRouteFlavorResponses
    let ctxErr = ctx.Err();
    cancelWorkers();
    if let Some(ctxErr) = ctxErr {
        return Err(GoError::wrap(format!("EdgeRouting: {}", ctxErr), ctxErr));
    }
    Ok(responses)
}

pub(crate) fn successfulRouteFlavorResponses(
    responses: Vec<GenerateRouteResponse>,
) -> GoResult<Map<RouteGenerationFlavor, GenerateRouteResponse>> {
    let mut successful: Map<RouteGenerationFlavor, GenerateRouteResponse> = Map::default();
    let mut firstError: Option<GoError> = None;
    for response in responses {
        if let Some(err) = &response.Err {
            // The whole-stage aggregate is different: once exhausted, the stage fails.
            if err.Is(&errRouteStageWorkLimit) {
                return Err(err.clone());
            }
            if firstError.is_none() {
                firstError = Some(err.clone());
            }
            continue;
        }
        successful.insert(response.Flavor, response);
    }

    if !successful.is_empty() {
        return Ok(successful);
    }
    if let Some(err) = firstError {
        return Err(err);
    }
    Err(invariant::New("edge routing produced no flavor responses"))
}

// finalizeSelectedRoutes accounts route grouping, sorting, reversal, and point
// materialization against the winning flavor's remaining budget. All fallible
// work completes before graph edges are changed, preserving route atomicity.
pub(crate) fn finalizeSelectedRoutes(
    ctx: &Ctx,
    g: &GraphRef,
    routes: Vec<RouteRef>,
    selectedEdges: Option<&Set<EdgeRef>>,
    work: Option<&routeSearchWorkGuardRef>,
    reorderParallelEdges: bool,
) -> GoResult<()> {
    let Some(work) = work else {
        return Err(invariant::New("selected edge-routing flavor has no work guard"));
    };
    work.bind(ctx)?;
    let result = finalizeSelectedRoutesBound(g, routes, selectedEdges, work, reorderParallelEdges);
    if result.is_err() {
        let _ = work.finish();
    }
    result
}

fn finalizeSelectedRoutesBound(
    g: &GraphRef,
    mut routes: Vec<RouteRef>,
    selectedEdges: Option<&Set<EdgeRef>>,
    work: &routeSearchWorkGuardRef,
    reorderParallelEdges: bool,
) -> GoResult<()> {
    if let Some(selectedEdges) = selectedEdges {
        let mut selectedRoutes: Vec<RouteRef> = Vec::with_capacity(selectedEdges.len());
        let mut found: Set<EdgeRef> = Set::default();
        for route in &routes {
            work.step()?;
            let gEdge = route.b().GEdge.clone();
            if !selectedEdges.contains(&gEdge) {
                continue;
            }
            if found.contains(&gEdge) {
                return Err(invariant::New("selected edge-routing flavor produced duplicate routes for an edge"));
            }
            found.insert(gEdge);
            selectedRoutes.push(route.clone());
        }
        if found.len() != selectedEdges.len() {
            return Err(invariant::New("selected edge-routing flavor omitted a requested edge"));
        }
        routes = selectedRoutes;
    }

    for route in &routes {
        work.step()?;
        let n = route.b().OVGNodes.len();
        if n < 2 {
            return Err(invariant::New("selected edge-routing flavor produced an invalid route"));
        }
        for _ in 0..n {
            work.step()?;
        }
    }

    if reorderParallelEdges {
        reorderSelectedRoutes(g, &routes, work)?;
    }

    work.add(routes.len() as u64)?;
    let mut updates: Vec<(EdgeRef, Vec<geo::PointRef>)> = Vec::with_capacity(routes.len());
    for route in &routes {
        work.step()?;
        work.add(route.b().OVGNodes.len() as u64)?;
        let r = route.b();
        updates.push((r.GEdge.clone(), r.createSegmentEndpoints()));
    }
    work.finish()?;

    // The commit contains no fallible work.
    for (edge, points) in updates {
        edge.bm().Points = points;
    }
    Ok(())
}

pub(crate) fn reorderSelectedRoutes(g: &GraphRef, routes: &[RouteRef], work: &routeSearchWorkGuardRef) -> GoResult<()> {
    work.add(routes.len() as u64)?;
    let mut buckets: Vec<Vec<RouteRef>> = Vec::new();
    let mut grouped: Set<RouteRef> = Set::default();
    for (index, route) in routes.iter().enumerate() {
        work.step()?;
        if grouped.contains(route) {
            continue;
        }
        let mut bucket: Vec<RouteRef> = vec![route.clone()];
        grouped.insert(route.clone());
        for other in &routes[index + 1..] {
            work.step()?;
            if grouped.contains(other) {
                continue;
            }
            let canSwap = route.canSwapEdgesGuarded(other, routes, Some(work as &dyn workBudget))?;
            if canSwap {
                bucket.push(other.clone());
                grouped.insert(other.clone());
            }
        }
        if bucket.len() > 1 {
            buckets.push(bucket);
        }
    }

    let gEdges = g.edges();
    work.add(gEdges.len() as u64)?;
    let mut edgeIndices: Map<EdgeRef, i64> = Map::default();
    for (index, edge) in gEdges.iter().enumerate() {
        work.step()?;
        edgeIndices.insert(edge.clone(), index as i64);
    }

    for bucket in buckets.iter_mut() {
        work.step()?;
        work.reserveSort(bucket.len())?;
        // Sort bucket from left-right or top-bottom.
        sort::SortSlice(bucket, |e1, e2| {
            let (r1, r2) = (e1.b(), e2.b());
            let (n1, n2) = (r1.GEdge.from(), r1.GEdge.to());
            let sameFrom = r1.GEdge.from() == r2.GEdge.from();
            if n1.Orientation(&n2).IsVertical() {
                if sameFrom {
                    return r1.FromPort.X < r2.FromPort.X;
                }
                return r1.FromPort.X < r2.ToPort.X;
            }
            if sameFrom {
                return r1.FromPort.Y < r2.FromPort.Y;
            }
            r1.FromPort.Y < r2.ToPort.Y
        });

        work.add(bucket.len() as u64)?;
        let mut edges: Vec<EdgeRef> = Vec::with_capacity(bucket.len());
        for route in bucket.iter() {
            work.step()?;
            edges.push(route.b().GEdge.clone());
        }
        work.reserveSort(edges.len())?;
        sort::SortSlice(&mut edges, |a, b| {
            let left = edgeIndices.get(a).copied().unwrap_or(-1);
            let right = edgeIndices.get(b).copied().unwrap_or(-1);
            left < right
        });

        for (index, route) in bucket.iter().enumerate() {
            work.step()?;
            let differs = route.b().GEdge.from() != edges[index].from();
            if differs {
                work.add(route.b().OVGNodes.len() as u64)?;
                let mut r = route.bm();
                let (fp, tp) = (r.FromPort, r.ToPort);
                r.FromPort = tp;
                r.ToPort = fp;
                r.OVGNodes.reverse();
            }
            route.bm().GEdge = edges[index].clone();
        }
    }
    work.check()
}

fn cacheRoutingCosts(g: &GraphRef) {
    // cache these ahead of time
    g.CrossingCost();
    g.TurnCost();
    g.NonCenterPortCost();
}

fn routeFlavors(g: &GraphRef) -> Vec<RouteGenerationFlavor> {
    let first = g.b().Nodes[0].clone();
    if first.b().Hierarchy.is_some() {
        return vec![TopDownLeftRight];
    }
    vec![ShortestToLongest, LongestToShortest, Default]
}

/// Runs the routers, picks the least-distance successful flavor in declared
/// order, and commits its routes.
fn selectAndFinalizeRoutes(
    ctx: &Ctx,
    g: &GraphRef,
    flavors: &[RouteGenerationFlavor],
    routers: &mut [ovgEdgeRouter],
    selectedEdges: Option<&Set<EdgeRef>>,
    reorderParallelEdges: bool,
) -> GoResult<()> {
    let responses = generateRouteFlavorResponses(ctx, routers, true)?;

    let mut bestRoutes: Vec<RouteRef> = Vec::new();
    let mut bestWork: Option<routeSearchWorkGuardRef> = None;
    let mut leastDistance = math::Inf(1);

    let flavorToResponse = successfulRouteFlavorResponses(responses)?;

    // compare responses in fixed order
    for flavor in flavors {
        if let Some(response) = flavorToResponse.get(flavor) {
            if geo::PrecisionCompare(response.Distance, leastDistance, geo::PRECISION) < 0 {
                leastDistance = response.Distance;
                bestRoutes = response.Routes.clone();
                bestWork = response.work.clone();
            }
        }
    }

    finalizeSelectedRoutes(ctx, g, bestRoutes, selectedEdges, bestWork.as_ref(), reorderParallelEdges)
}

pub(crate) fn routeEdgesWithResourceGuards(
    ctx: &Ctx,
    g: &GraphRef,
    nearbyNodes: &[NodeRef],
    ovgGuard: &ovgBuildGuardRef,
    searchWorkLimit: u64,
) -> GoResult<OVGRef> {
    let ovg = buildOVGFromGraphWithGuard(g, nearbyNodes, ovgGuard)?;

    // Route all tree edges first, then route the remaining edges
    let mut existingRoutes: Vec<RouteRef> = Vec::new();
    let mut isTreeEdge = g.TreeEdgeMap();
    g.AddIsolatedTreeEdges(&mut isTreeEdge);

    let mut existingPortArrowheads: Map<OVGNodeRef, Set<layoutgraph::Arrowhead>> = Map::default();
    let mut isRouted: Set<EdgeRef> = Set::default();
    let nodeToTree = g.b().NodeToTree.clone();
    for node in g.nodes() {
        // we route each tree edge by iterating over the tree nodes, and routing with its sentinel edge
        if let Some(tree) = nodeToTree.get(&node) {
            let sentinelEdge = tree.b().SentinelEdge.clone().expect("nil pointer dereference: SentinelEdge");
            // we don't route the edge from tree root to rootSentinel here since the root may move relative to the rootSentinel
            if !isTreeEdge.contains_key(&sentinelEdge) {
                continue;
            }

            let route = {
                let o = ovg.b();
                match routeSentinelEdge(&tree, &o.Ports, &o.Centers, &existingPortArrowheads) {
                    Ok(route) => route,
                    Err(_) => continue,
                }
            };

            existingRoutes.push(route.clone());
            let (fromPort, toPort) = {
                let r = route.b();
                (r.OVGNodes[1].clone(), r.OVGNodes[r.OVGNodes.len() - 2].clone())
            };
            let (sourceArrowhead, targetArrowhead) = {
                let e = sentinelEdge.b();
                (e.SourceArrowhead.clone(), e.TargetArrowhead.clone())
            };
            existingPortArrowheads.entry(fromPort.clone()).or_default();
            existingPortArrowheads.entry(toPort.clone()).or_default();
            existingPortArrowheads.get_mut(&fromPort).unwrap().insert(sourceArrowhead);
            existingPortArrowheads.get_mut(&toPort).unwrap().insert(targetArrowhead);
            isRouted.insert(sentinelEdge);
        } else {
            for routedEdge in loops::Route(&node) {
                existingRoutes.push(makeLoopRoute(&routedEdge, &ovg.b()));
                isRouted.insert(routedEdge);
            }
        }
    }
    let mut unroutedEdges: Vec<EdgeRef> = Vec::new();
    for e in g.edges() {
        if !isRouted.contains(&e) {
            unroutedEdges.push(e);
        }
    }

    cacheRoutingCosts(g);

    let flavors = routeFlavors(g);
    let mut routers: Vec<ovgEdgeRouter> = Vec::with_capacity(flavors.len());
    for flavor in &flavors {
        let router = newOVGEdgeRouterWithWorkLimit(ctx, *flavor, &ovg, g, &existingRoutes, &unroutedEdges, searchWorkLimit)?;
        routers.push(router);
    }
    selectAndFinalizeRoutes(ctx, g, &flavors, &mut routers, None, true)?;

    Ok(ovg)
}

pub(crate) fn routeAdditionalEdgesWithLimits(ctx: &Ctx, g: &GraphRef, edges: &[EdgeRef], limits: ovgBuildLimits) -> GoResult<OVGRef> {
    routeAdditionalEdgesWithResourceLimits(ctx, g, edges, limits, maxRouteSearchWorkUnits)
}

pub(crate) fn routeAdditionalEdgesWithResourceLimits(
    ctx: &Ctx,
    g: &GraphRef,
    edges: &[EdgeRef],
    limits: ovgBuildLimits,
    searchWorkLimit: u64,
) -> GoResult<OVGRef> {
    let mut newEdges: Set<EdgeRef> = Set::default();
    for e in edges {
        newEdges.insert(e.clone());
    }

    let ovg = buildOVGFromGraphWithLimits(ctx, g, &[], limits)?;
    let guard = ovg.b().buildGuard.clone().expect("OVG without build guard");

    let mut existingRoutes: Vec<RouteRef> = Vec::new();
    for e in g.edges() {
        guard.bm().step()?;
        if newEdges.contains(&e) {
            continue;
        }

        let mut ovgNodes: Vec<OVGNodeRef> = Vec::new();
        let mut lastPoint: Option<geo::Point> = None;

        let mut addPoint = |p: geo::Point| -> GoResult<()> {
            guard.bm().step()?;
            // Skip if this point is the same as the last one
            if let Some(last) = lastPoint {
                if last == p {
                    return Ok(());
                }
            }
            lastPoint = Some(p);

            let existing = ovg.b().OccupiedPoints.get(&p).cloned();
            if let Some(ovgNode) = existing {
                ovgNodes.push(ovgNode);
            } else {
                let ovgNode = NewOVGNode(p);
                // The base OVG was indexed before authoritative routes were added.
                ovgNode.bm().Index = ovg.b().Nodes.len();
                ovgNodes.push(ovgNode.clone());
                guard.bm().addNodeUnchecked(&mut ovg.bm(), &ovgNode)?;
            }

            // Only connect if we have at least 2 different points
            if ovgNodes.len() > 1 {
                let (a, b) = (ovgNodes[ovgNodes.len() - 2].clone(), ovgNodes[ovgNodes.len() - 1].clone());
                guard.bm().connect(&mut ovg.bm(), &a, &b)?;
            }
            Ok(())
        };

        addPoint(e.from().Center().get())?;
        for p in e.points() {
            addPoint(p.get())?;
        }
        addPoint(e.to().Center().get())?;

        // Only create route if we have at least 2 different points
        if ovgNodes.len() > 1 {
            let points = e.points();
            let route = RouteRef::new(Route {
                GEdge: e.clone(),
                OVGNodes: ovgNodes,
                FromPort: points[0].get(),
                ToPort: points[points.len() - 1].get(),
            });
            existingRoutes.push(route);
        }
    }

    cacheRoutingCosts(g);

    let flavors = routeFlavors(g);
    let mut routers: Vec<ovgEdgeRouter> = Vec::with_capacity(flavors.len());
    for flavor in &flavors {
        let mut router = newOVGEdgeRouterWithWorkLimit(ctx, *flavor, &ovg, g, &existingRoutes, edges, searchWorkLimit)?;
        router.considerNodeLabels = true;
        routers.push(router);
    }
    selectAndFinalizeRoutes(ctx, g, &flavors, &mut routers, Some(&newEdges), false)?;

    Ok(ovg)
}

// For Trees, ideal route between parent & child is S shaped bend or straight line
pub(crate) fn routeSentinelEdge(
    tree: &layoutgraph::TreeRef,
    portNodes: &Map<NodeRef, Vec<OVGNodeRef>>,
    centers: &Map<NodeRef, OVGNodeRef>,
    portArrowheads: &Map<OVGNodeRef, Set<layoutgraph::Arrowhead>>,
) -> GoResult<RouteRef> {
    // Note: we are the child node since we are considering the sentinel edge
    let edge = tree.b().SentinelEdge.clone().expect("nil pointer dereference: SentinelEdge");
    let treePath = treeEdgePath(tree, portNodes, portArrowheads);

    let nodesFromSourcePortToTargetPort = routeInSShape(&treePath)?;

    // validate route
    let (from, to) = (edge.from(), edge.to());
    for n in &nodesFromSourcePortToTargetPort {
        if n.isPort() {
            let mut validOwner = false;
            for (owner, _) in n.portOwners() {
                if from.IsDescendantOf(Some(&owner)) || to.IsDescendantOf(Some(&owner)) {
                    validOwner = true;
                    break;
                }
            }
            if !validOwner {
                return Err(GoError::new("routing through other node's port"));
            }
        }
    }

    let mut ovgNodes: Vec<OVGNodeRef> = Vec::with_capacity(nodesFromSourcePortToTargetPort.len() + 2);
    ovgNodes.push(centers.get(&from).cloned().expect("tree route source has no OVG center"));
    ovgNodes.extend(nodesFromSourcePortToTargetPort);
    ovgNodes.push(centers.get(&to).cloned().expect("tree route target has no OVG center"));

    Ok(RouteRef::new(Route {
        GEdge: edge,
        OVGNodes: ovgNodes,
        FromPort: treePath.SourcePortNode.P(),
        ToPort: treePath.TargetPortNode.P(),
    }))
}

// create the straight line or S shaped route between source and target ports
// return the ovg nodes along the way (including source and target port)
pub(crate) fn routeInSShape(treePath: &TreeEdgePath) -> GoResult<Vec<OVGNodeRef>> {
    let sourceOrientationToTarget = treePath.SourceOrientationToTarget;
    let mut current = treePath.SourcePortNode.clone();
    let getAdjInDirection = |current: &OVGNodeRef, currentOrientationToAdjacent: geo::Orientation| -> Option<OVGNodeRef> {
        let mut closest: Option<OVGNodeRef> = None;
        let c = current.P();
        let edges = current.b().Edges.clone();
        for e in &edges {
            let adjNode = current.adjacent(e);
            let a = adjNode.P();
            let cl = closest.as_ref().map(|n| n.P());
            if currentOrientationToAdjacent == geo::Left {
                // if current is Left of adjacent, we want the closest y-aligned adjacent node to the Right of current
                if c.Y == a.Y && a.X > c.X {
                    if cl.is_none() || a.X < cl.unwrap().X {
                        closest = Some(adjNode);
                    }
                }
            } else if currentOrientationToAdjacent == geo::Right {
                if c.Y == a.Y && a.X < c.X {
                    if cl.is_none() || a.X > cl.unwrap().X {
                        closest = Some(adjNode);
                    }
                }
            } else if currentOrientationToAdjacent == geo::Top {
                if c.X == a.X && a.Y > c.Y {
                    if cl.is_none() || a.Y < cl.unwrap().Y {
                        closest = Some(adjNode);
                    }
                }
            } else if c.X == a.X && a.Y < c.Y {
                if cl.is_none() || a.Y > cl.unwrap().Y {
                    closest = Some(adjNode);
                }
            }
        }
        closest
    };

    let mut nodes: Vec<OVGNodeRef> = vec![treePath.SourcePortNode.clone()];

    // Step 1. source port to source midpoint
    while !nonNilEquals(&current.P(), &treePath.SourceMidpoint) {
        match getAdjInDirection(&current, sourceOrientationToTarget) {
            None => return Err(invariant::New("couldn't find s shaped route (source port to source midpoint)")),
            Some(n) => current = n,
        }
        nodes.push(current.clone());
    }

    // Step 2. source midpoint to target midpoint
    let (sp, tp) = (treePath.SourcePortNode.P(), treePath.TargetPortNode.P());
    let crossDirection = if sourceOrientationToTarget == geo::Left || sourceOrientationToTarget == geo::Right {
        if sp.Y < tp.Y {
            geo::Top
        } else {
            geo::Bottom
        }
    } else if sp.X < tp.X {
        geo::Left
    } else {
        geo::Right
    };

    while !nonNilEquals(&current.P(), &treePath.TargetMidpoint) {
        match getAdjInDirection(&current, crossDirection) {
            None => return Err(invariant::New("couldn't find s shaped route (source midpoint to target midpoint)")),
            Some(n) => current = n,
        }
        nodes.push(current.clone());
    }

    // Step 3. target midpoint to target port
    while !nonNilEquals(&current.P(), &tp) {
        match getAdjInDirection(&current, sourceOrientationToTarget) {
            None => return Err(invariant::New("couldn't find s shaped route (target midpoint to target port)")),
            Some(n) => current = n,
        }
        nodes.push(current.clone());
    }

    Ok(nodes)
}

// ---------------------------------------------------------------------------
// Straight-line routing
// ---------------------------------------------------------------------------

pub(crate) fn routeLine(
    ctx: &Ctx,
    g: &GraphRef,
    edge: &EdgeRef,
    allEdges: &[EdgeRef],
    routes: &[RouteRef],
) -> GoResult<(geo::PointRef, geo::PointRef, f64)> {
    routeLineChecked(g, edge, allEdges, routes, &mut || checkEdgeRoutingCanceled(ctx), None)
}

pub(crate) fn routeLineGuarded(
    g: &GraphRef,
    edge: &EdgeRef,
    allEdges: &[EdgeRef],
    routes: &[RouteRef],
    guard: &routeWorkGuardRef,
) -> GoResult<(geo::PointRef, geo::PointRef, f64)> {
    routeLineChecked(g, edge, allEdges, routes, &mut || guard.step(), Some(guard))
}

fn portEdgeMap(m: Map<layoutgraph::PointKey, Vec<EdgeRef>>) -> Map<geo::Point, Vec<EdgeRef>> {
    m.into_iter().map(|(k, v)| (k.0, v)).collect()
}

/// `routes` empty stands for Go's nil slice (a non-nil slice is always
/// aligned with allEdges, so an empty one never reaches the index).
pub(crate) fn routeLineChecked(
    g: &GraphRef,
    edge: &EdgeRef,
    allEdges: &[EdgeRef],
    routes: &[RouteRef],
    checkWork: &mut dyn FnMut() -> GoResult<()>,
    guard: Option<&dyn workBudget>,
) -> GoResult<(geo::PointRef, geo::PointRef, f64)> {
    checkWork()?;
    let (from, to) = (edge.from(), edge.to());
    let fromOrientation = from.Orientation(&to);
    if fromOrientation == geo::NONE {
        return Err(layoutgraph::ErrInvalidCandidate.err());
    }
    let fromPorts: Vec<geo::PointRef> =
        from.PortsByOrientation(fromOrientation.GetOpposite()).into_iter().map(geo::PointRef::new).collect();

    let toOrientation = to.Orientation(&from);
    if toOrientation == geo::NONE {
        return Err(layoutgraph::ErrInvalidCandidate.err());
    }
    let toPorts: Vec<geo::PointRef> =
        to.PortsByOrientation(toOrientation.GetOpposite()).into_iter().map(geo::PointRef::new).collect();

    let (mut fromPortEdges, mut toPortEdges) = match guard {
        None => (portEdgeMap(allEdges.PortEdges(&from)), portEdgeMap(allEdges.PortEdges(&to))),
        Some(guard) => (portEdgesGuarded(allEdges, &from, guard)?, portEdgesGuarded(allEdges, &to, guard)?),
    };
    // we filter out the current edge here to keep the logic for arrowhead matching simpler
    fn filterEdge(
        edge: &EdgeRef,
        portEdges: &[EdgeRef],
        checkWork: &mut dyn FnMut() -> GoResult<()>,
    ) -> GoResult<Vec<EdgeRef>> {
        let mut filteredEdges = Vec::new();
        for e in portEdges {
            checkWork()?;
            if e != edge {
                filteredEdges.push(e.clone());
            }
        }
        Ok(filteredEdges)
    }
    for portEdges in fromPortEdges.values_mut() {
        *portEdges = filterEdge(edge, portEdges, checkWork)?;
    }
    for portEdges in toPortEdges.values_mut() {
        *portEdges = filterEdge(edge, portEdges, checkWork)?;
    }

    let fromNodeCenterPorts: Set<geo::Point> = from.CenterPorts().into_iter().collect();
    let toNodeCenterPorts: Set<geo::Point> = to.CenterPorts().into_iter().collect();
    let (sourceClusterNodes, targetClusterNodes) = match guard {
        None => sourceAndTargetClusterNodes(g, &from, &to),
        Some(guard) => sourceAndTargetClusterNodesGuarded(g, &from, &to, guard)?,
    };

    let (hasSourceLabel, hasTargetLabel) = {
        let e = edge.b();
        (e.SourceArrowheadLabel.is_some(), e.TargetArrowheadLabel.is_some())
    };
    let mut nonAncestors: Vec<NodeRef> = Vec::new();
    if hasSourceLabel || hasTargetLabel {
        nonAncestors = match guard {
            None => filterEdgeAncestors(edge, &g.nodes()),
            Some(guard) => filterEdgeAncestorsGuarded(edge, &g.nodes(), guard)?,
        };
    }

    let mut positionedLabels: Vec<PositionedArrowheadLabel> = Vec::new();
    for (i, e) in allEdges.iter().enumerate() {
        checkWork()?;
        if e == edge {
            continue;
        }
        let route: Vec<geo::PointRef> = if !routes.is_empty() { routes[i].b().createSegmentEndpoints() } else { e.points() };
        if route.is_empty() {
            continue;
        }
        if let Some(guard) = guard {
            guard.add(route.len() as u64)?;
        }
        if let Some(pal) = labeling::PositionArrowheadLabel(e, false, &route) {
            positionedLabels.push(pal);
        }
        if let Some(pal) = labeling::PositionArrowheadLabel(e, true, &route) {
            positionedLabels.push(pal);
        }
    }

    let mut bestCost = math::Inf(1);
    let mut best: Option<(geo::PointRef, geo::PointRef)> = None;
    let graphNodes = g.nodes();
    let (sourceArrowhead, targetArrowhead) = {
        let e = edge.b();
        (e.SourceArrowhead.clone(), e.TargetArrowhead.clone())
    };

    // for each pair, place an edge, get the cost, but multiply it by some penalty factor for being non-orthogonal
    for fromPortRef in &fromPorts {
        checkWork()?;
        let fromPort = fromPortRef.get();
        let mut fromPortCost = 0.0;
        // Note: remove fromPortEdges could be empty if there are no edges after filtering
        if let Some(portEdges) = fromPortEdges.get(&fromPort).filter(|v| !v.is_empty()) {
            // we don't consider ports used by a duplicate edge
            let duplicate = match guard {
                None => edge.HasDuplicateIn(portEdges),
                Some(guard) => edgeHasDuplicateInGuarded(edge, portEdges, guard)?,
            };
            if duplicate {
                continue;
            }

            // if the port is to be shared, all the edges must match arrowheads
            let mut allMatch = true;
            for otherFromPortEdge in portEdges {
                checkWork()?;
                let o = otherFromPortEdge.b();
                if o.From == from {
                    // other edge source arrow must match
                    if sourceArrowhead != o.SourceArrowhead {
                        allMatch = false;
                        break;
                    }
                } else if sourceArrowhead != o.TargetArrowhead {
                    // other edge target arrow must match
                    allMatch = false;
                    break;
                }
            }
            if !allMatch {
                continue;
            }

            // sharing a port with matching arrowheads is not ideal so other source ports should be preferred
            if edge.HasSourceArrow() {
                fromPortCost = maxSideLength(&from, fromOrientation) / 2.0;
            }
        }

        for toPortRef in &toPorts {
            checkWork()?;
            let toPort = toPortRef.get();
            let mut toPortCost = 0.0;
            if let Some(portEdges) = toPortEdges.get(&toPort).filter(|v| !v.is_empty()) {
                let duplicate = match guard {
                    None => edge.HasDuplicateIn(portEdges),
                    Some(guard) => edgeHasDuplicateInGuarded(edge, portEdges, guard)?,
                };
                if duplicate {
                    continue;
                }

                let mut allMatch = true;
                for otherToPortEdge in portEdges {
                    checkWork()?;
                    let o = otherToPortEdge.b();
                    if o.To == to {
                        if targetArrowhead != o.TargetArrowhead {
                            allMatch = false;
                            break;
                        }
                    } else if targetArrowhead != o.SourceArrowhead {
                        allMatch = false;
                        break;
                    }
                }
                if !allMatch {
                    continue;
                }

                toPortCost = maxSideLength(&to, toOrientation) / 2.0;
            }

            let edgeOption = layoutgraph::NewEdge(&from, &to);
            edgeOption.bm().Points = vec![fromPortRef.clone(), toPortRef.clone()];

            // don't consider option if edge crosses over another node
            let intersectsNode = match guard {
                None => graphNodes.IntersectsNode(&edgeOption),
                Some(guard) => routeIntersectsNodeGuarded(&graphNodes, &edgeOption, guard)?,
            };
            if intersectsNode {
                continue;
            }

            let otherEdges = filterEdge(edge, allEdges, checkWork)?;

            let overlappingEdges = match guard {
                None => findOverlappingEdges(&fromPort, &toPort, &otherEdges),
                Some(guard) => overlappingEdgesGuarded(&fromPort, &toPort, &otherEdges, guard)?,
            };
            if !overlappingEdges.is_empty() {
                let canOverlap = match guard {
                    None => edgeCanOverlapEdges(edge, &overlappingEdges, Some(&sourceClusterNodes), Some(&targetClusterNodes)),
                    Some(guard) => edgeCanOverlapEdgesGuarded(
                        edge,
                        &overlappingEdges,
                        Some(&sourceClusterNodes),
                        Some(&targetClusterNodes),
                        guard,
                    )?,
                };
                if !canOverlap {
                    continue;
                }
            }

            let routeCost = match guard {
                None => estimateRouteCost(&otherEdges, &edgeOption),
                Some(guard) => estimateRouteCostGuarded(&otherEdges, &edgeOption, guard)?,
            };
            let mut cost = fromPortCost + toPortCost + routeCost;

            if fromPort.X != toPort.X && fromPort.Y != toPort.Y {
                let mut isAxisAligned = false;
                // Reward axis-aligned lines
                if geo::PrecisionCompare(fromPort.X, toPort.X, layoutgraph::AxisAlignmentTolerance) == 0 {
                    isAxisAligned = true;
                }
                if geo::PrecisionCompare(fromPort.Y, toPort.Y, layoutgraph::AxisAlignmentTolerance) == 0 {
                    isAxisAligned = true;
                }
                if !isAxisAligned {
                    cost *= nonOrthogonalFactor;
                }
            }

            // skip straight edges with sharp angles with source or target node
            if hasSharpAngleToBorder(&fromPort, &toPort, fromOrientation) {
                continue;
            }
            if hasSharpAngleToBorder(&fromPort, &toPort, toOrientation) {
                continue;
            }

            // Reward lines that go through center
            if !fromNodeCenterPorts.contains(&fromPort) {
                cost += g.NonCenterPortCost();
            }
            if !toNodeCenterPorts.contains(&toPort) {
                cost += g.NonCenterPortCost();
            }

            let routePoints = [fromPortRef.clone(), toPortRef.clone()];
            if hasSourceLabel {
                let pal = labeling::PositionArrowheadLabel(edge, false, &routePoints).expect("nil pointer dereference");
                cost += match guard {
                    None => positionedArrowheadLabelCost(&pal, &nonAncestors, &positionedLabels, &[], &otherEdges),
                    Some(guard) => {
                        positionedArrowheadLabelCostGuarded(&pal, &nonAncestors, &positionedLabels, &[], &otherEdges, guard)?
                    }
                };
            }
            if hasTargetLabel {
                let pal = labeling::PositionArrowheadLabel(edge, true, &routePoints).expect("nil pointer dereference");
                cost += match guard {
                    None => positionedArrowheadLabelCost(&pal, &nonAncestors, &positionedLabels, &[], &otherEdges),
                    Some(guard) => {
                        positionedArrowheadLabelCostGuarded(&pal, &nonAncestors, &positionedLabels, &[], &otherEdges, guard)?
                    }
                };
            }

            // get the best cost of these pairs
            if cost < bestCost {
                best = Some((fromPortRef.clone(), toPortRef.clone()));
                bestCost = cost;
            }
        }
    }

    if bestCost == math::Inf(1) {
        return Err(layoutgraph::ErrInvalidCandidate.err());
    }
    let (bestFromPort, bestToPort) = best.unwrap();
    Ok((bestFromPort, bestToPort, bestCost))
}

// return true if gEdge can overlap with the other edges provided
pub(crate) fn edgeCanOverlapEdges(
    edge: &EdgeRef,
    otherEdges: &[EdgeRef],
    sourceClusterNodes: Option<&Set<NodeRef>>,
    targetClusterNodes: Option<&Set<NodeRef>>,
) -> bool {
    if otherEdges.is_empty() {
        return true;
    }
    {
        let e = edge.b();
        if e.TargetArrowheadLabel.is_some() || e.SourceArrowheadLabel.is_some() {
            return false;
        }
    }
    for e in otherEdges {
        {
            let o = e.b();
            if o.TargetArrowheadLabel.is_some() || o.SourceArrowheadLabel.is_some() {
                return false;
            }
        }
        if !edge.EquivalentStyles(e) {
            return false;
        }
    }

    let mut isClusterEdge = false;
    if let Some(sourceClusterNodes) = sourceClusterNodes.filter(|s| !s.is_empty()) {
        let mut hasNonSourceClusterEdge = false;
        for otherEdge in otherEdges {
            let isFromSourceCluster = sourceClusterNodes.contains(&otherEdge.from());
            let isToSourceCluster = sourceClusterNodes.contains(&otherEdge.to());
            if !isFromSourceCluster && !isToSourceCluster {
                hasNonSourceClusterEdge = true;
                break;
            }
        }
        if !hasNonSourceClusterEdge {
            isClusterEdge = true;
        }
    }
    if !isClusterEdge {
        if let Some(targetClusterNodes) = targetClusterNodes.filter(|s| !s.is_empty()) {
            let mut hasNonTargetClusterEdge = false;
            for otherEdge in otherEdges {
                let isFromTargetCluster = targetClusterNodes.contains(&otherEdge.from());
                let isToTargetCluster = targetClusterNodes.contains(&otherEdge.to());
                if !isFromTargetCluster && !isToTargetCluster {
                    hasNonTargetClusterEdge = true;
                    break;
                }
            }
            if !hasNonTargetClusterEdge {
                isClusterEdge = true;
            }
        }
    }

    let (from, to, sourceArrowhead, targetArrowhead) = {
        let e = edge.b();
        (e.From.clone(), e.To.clone(), e.SourceArrowhead.clone(), e.TargetArrowhead.clone())
    };

    // if directed edges share a source or target, they can share an edge
    let mut allMatchingDirected = edge.IsDirected();
    if allMatchingDirected {
        for otherEdge in otherEdges {
            if !otherEdge.IsDirected() {
                allMatchingDirected = false;
                break;
            }
            let o = otherEdge.b();
            if !(to == o.To || from == o.From) {
                allMatchingDirected = false;
                break;
            }
            if targetArrowhead != o.TargetArrowhead || sourceArrowhead != o.SourceArrowhead {
                allMatchingDirected = false;
                break;
            }
        }
    }
    if allMatchingDirected {
        return true;
    }

    // bidirectional / undirected edges can overlap as long as source/target arrowheads match and
    // if they are part of the same cluster or if all edges share a node
    let mut allUndirected = false;
    let allBidirectional = edge.IsBidirectional() && edge.OwnArrowheadsMatch();
    if allBidirectional {
        for otherEdge in otherEdges {
            if !otherEdge.IsBidirectional() || !edge.MatchingArrowheads(otherEdge) {
                return false;
            }
        }
    } else {
        allUndirected = edge.IsUndirected();
        if allUndirected {
            for otherEdge in otherEdges {
                if !otherEdge.IsUndirected() {
                    return false;
                }
            }
        }
    }
    if !allBidirectional && !allUndirected {
        return false;
    }

    // Note: ovg_edge_router treats edges between tables columns as clusters so those are also included here
    // if edges are of the same cluster, bidirectional / undirected edges can overlap
    if isClusterEdge {
        return true;
    }

    // Note: assuming no self-loops here (node appears once per edge)
    let mut nodeCounts: Map<NodeRef, i64> = Map::default();
    *nodeCounts.entry(from).or_insert(0) += 1;
    *nodeCounts.entry(to).or_insert(0) += 1;
    for otherEdge in otherEdges {
        *nodeCounts.entry(otherEdge.from()).or_insert(0) += 1;
        *nodeCounts.entry(otherEdge.to()).or_insert(0) += 1;
    }
    // if all edges are connected to a shared node, then they can overlap without it becoming unclear which nodes are connected
    let totalEdgeCount = otherEdges.len() as i64 + 1;
    nodeCounts.values().any(|&count| count == totalEdgeCount)
}

pub(crate) fn isVerticalOrHorizontalOverlap(start: &geo::Point, end: &geo::Point, otherStart: &geo::Point, otherEnd: &geo::Point) -> bool {
    let bothVertical = start.X == end.X && otherStart.X == otherEnd.X;
    let bothHorizontal = start.Y == end.Y && otherStart.Y == otherEnd.Y;
    if bothVertical || bothHorizontal {
        if intersects(otherStart, otherEnd, start, end) {
            return true;
        }
    }
    false
}

pub(crate) fn findOverlappingEdges(start: &geo::Point, end: &geo::Point, otherEdges: &[EdgeRef]) -> Vec<EdgeRef> {
    let mut overlappingEdges = Vec::new();
    for e in otherEdges {
        let points = pointValues(&e.b().Points);
        let mut i = 0usize;
        while i + 1 < points.len() {
            if isVerticalOrHorizontalOverlap(start, end, &points[i], &points[i + 1]) {
                overlappingEdges.push(e.clone());
                break;
            }
            i += 1;
        }
    }
    overlappingEdges
}

pub(crate) fn sourceAndTargetClusterNodes(g: &GraphRef, source: &NodeRef, target: &NodeRef) -> (Set<NodeRef>, Set<NodeRef>) {
    let mut sourceClusterNodes: Set<NodeRef> = Set::default();
    let mut targetClusterNodes: Set<NodeRef> = Set::default();

    let clusters = g.b().Clusters.clone();
    for key in g.ClusterOrder() {
        let cluster = clusters.get(&key).expect("nil cluster");
        let nodes = cluster.b().Nodes.clone();
        let mut isSource = false;
        let mut isTarget = false;
        for clusterNode in &nodes {
            if clusterNode == source {
                isSource = true;
            }
            if clusterNode == target {
                isTarget = true;
            }
        }

        if isSource {
            for clusterNode in &nodes {
                if clusterNode == source {
                    continue;
                }
                sourceClusterNodes.insert(clusterNode.clone());
            }
        }

        if isTarget {
            for clusterNode in &nodes {
                if clusterNode == target {
                    continue;
                }
                targetClusterNodes.insert(clusterNode.clone());
            }
        }
    }
    (sourceClusterNodes, targetClusterNodes)
}

// return the length of the side of the node
// if orientation is on a corner return whichever side is longer
pub(crate) fn maxSideLength(node: &NodeRef, orientation: geo::Orientation) -> f64 {
    if orientation == geo::Top || orientation == geo::Bottom {
        node.w()
    } else if orientation == geo::Left || orientation == geo::Right {
        node.h()
    } else {
        math::Max(node.w(), node.h())
    }
}

// ---------------------------------------------------------------------------
// StraightEdgesFallback
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct straightEdgeFallbackChange {
    edge: EdgeRef,
    points: Vec<geo::PointRef>,
}

#[derive(Default)]
struct straightEdgeFallbackRollback {
    graph: Option<GraphRef>,
    costs: layoutgraph::RoutingCostState,
    costsCaptured: bool,
    firstChange: Option<straightEdgeFallbackChange>,
    moreChanges: Vec<straightEdgeFallbackChange>,
}

impl straightEdgeFallbackRollback {
    fn captureCosts(&mut self, graph: &GraphRef) {
        if self.costsCaptured {
            return;
        }
        self.graph = Some(graph.clone());
        self.costs = graph.RoutingCosts();
        self.costsCaptured = true;
    }

    fn record(&mut self, edge: &EdgeRef, points: Vec<geo::PointRef>) {
        let change = straightEdgeFallbackChange { edge: edge.clone(), points };
        if self.firstChange.is_none() {
            self.firstChange = Some(change);
            return;
        }
        self.moreChanges.push(change);
    }

    fn restore(&self) {
        for change in self.moreChanges.iter().rev() {
            change.edge.bm().Points = change.points.clone();
        }
        if let Some(change) = &self.firstChange {
            change.edge.bm().Points = change.points.clone();
        }
        if self.costsCaptured {
            self.graph.as_ref().unwrap().RestoreRoutingCosts(self.costs);
        }
    }
}

fn isStraightEdgeFallbackCandidate(edge: &EdgeRef, clusterNodes: &Set<NodeRef>, isTreeEdge: &Map<EdgeRef, bool>) -> bool {
    let (from, to) = (edge.from(), edge.to());
    if clusterNodes.contains(&from) {
        return false;
    }
    if clusterNodes.contains(&to) {
        return false;
    }
    if isTreeEdge.contains_key(edge) {
        return false;
    }
    if from.b().Hierarchy.is_some() {
        return false;
    }
    !edge.HasTableColumn()
}

pub fn StraightEdgesFallback(ctx: &Ctx, g: &GraphRef) -> GoResult<()> {
    checkEdgeRoutingCanceled(ctx)?;

    let mut clusterNodes: Set<NodeRef> = Set::default();
    for cluster in g.b().Clusters.values() {
        checkEdgeRoutingCanceled(ctx)?;
        for cn in cluster.b().Nodes.iter() {
            checkEdgeRoutingCanceled(ctx)?;
            clusterNodes.insert(cn.clone());
        }
    }
    let mut isTreeEdge = g.TreeEdgeMap();
    g.AddIsolatedTreeEdges(&mut isTreeEdge);
    checkEdgeRoutingCanceled(ctx)?;
    for (index, edge) in g.edges().iter().enumerate() {
        checkEdgeRoutingCanceled(ctx)?;
        if !isStraightEdgeFallbackCandidate(edge, &clusterNodes, &isTreeEdge) {
            continue;
        }
        return straightEdgesFallbackFrom(ctx, g, &clusterNodes, &isTreeEdge, index);
    }
    Ok(())
}

fn straightEdgesFallbackFrom(
    ctx: &Ctx,
    g: &GraphRef,
    clusterNodes: &Set<NodeRef>,
    isTreeEdge: &Map<EdgeRef, bool>,
    firstIndex: usize,
) -> GoResult<()> {
    let mut rollback = straightEdgeFallbackRollback::default();
    rollback.captureCosts(g);
    let result = (|| -> GoResult<()> {
        let edges = g.edges();
        for (offset, edge) in edges[firstIndex..].iter().enumerate() {
            if offset != 0 {
                checkEdgeRoutingCanceled(ctx)?;
                if !isStraightEdgeFallbackCandidate(edge, clusterNodes, isTreeEdge) {
                    continue;
                }
            }
            let originalPoints = edge.points();
            let (changed, err) = tryStraightEdgeFallback(ctx, g, edge);
            if changed {
                rollback.record(edge, originalPoints);
            }
            if let Some(err) = err {
                return Err(err);
            }
        }
        checkEdgeRoutingCanceled(ctx)
    })();
    if result.is_err() {
        rollback.restore();
    }
    result
}

pub(crate) fn tryStraightEdgeFallback(ctx: &Ctx, g: &GraphRef, edge: &EdgeRef) -> (bool, Option<GoError>) {
    if let Err(err) = checkEdgeRoutingCanceled(ctx) {
        return (false, Some(err));
    }

    let allEdges = g.edges();
    let originalCost = estimateRouteCost(&allEdges, edge);
    let (fromPort, toPort, mut lineCost) = match routeLine(ctx, g, edge, &allEdges, &[]) {
        Ok(v) => v,
        Err(err) => {
            if layoutgraph::IsCandidateRejection(Some(&err)) {
                // A straight replacement is optional. Keep the original route
                // when no valid line exists.
                return (false, None);
            }
            return (false, Some(err));
        }
    };

    // If dejitter fails but the segment is really short, we would prefer a straight line (all else equivalent)
    let points = pointValues(&edge.b().Points);
    if points.len() == 4 {
        if geo::EuclideanDistance(points[1].X, points[1].Y, points[2].X, points[2].Y) <= lowerJitterThreshold {
            // remove the straight line penalty from this line
            lineCost /= nonOrthogonalFactor;
        }
    }

    if let Err(err) = checkEdgeRoutingCanceled(ctx) {
        return (false, Some(err));
    }
    if lineCost < originalCost {
        edge.bm().Points = vec![fromPort, toPort];
        return (true, None);
    }
    (false, None)
}

// true if angle between line and border (based on orientation) is less than min angle
pub(crate) fn hasSharpAngleToBorder(fromPoint: &geo::Point, toPoint: &geo::Point, orientation: geo::Orientation) -> bool {
    if orientation == geo::Top || orientation == geo::Bottom {
        let dx = toPoint.X - fromPoint.X;
        let dy = toPoint.Y - fromPoint.Y;
        let mut angle = (180.0 * math::Atan2(dy, dx) / math::Pi).abs();
        if angle > 90.0 {
            angle = 180.0 - angle;
        }
        return angle < minStraightEdgeAngle;
    } else if orientation == geo::Left || orientation == geo::Right {
        let dx = toPoint.X - fromPoint.X;
        let dy = toPoint.Y - fromPoint.Y;
        let mut angle = (180.0 * math::Atan2(dx, dy) / math::Pi).abs();
        if angle > 90.0 {
            angle = 180.0 - angle;
        }
        return angle < minStraightEdgeAngle;
    }
    false
}

// ---------------------------------------------------------------------------
// ReorderDuplicates
// ---------------------------------------------------------------------------

pub fn ReorderDuplicates(ctx: &Ctx, g: &GraphRef) -> GoResult<()> {
    reorderDuplicatesInEdges(ctx, &g.edges())
}

pub(crate) fn reorderDuplicatesInEdges(ctx: &Ctx, edges: &[EdgeRef]) -> GoResult<()> {
    let checkCanceled = || -> GoResult<()> {
        if let Some(err) = ctx.Err() {
            return Err(GoError::wrap(format!("ReorderDuplicates: {}", err), err));
        }
        Ok(())
    };
    checkCanceled()?;
    // Go: (edge, slice header, copy of the pointers); restoring copies the
    // pointers back into the original backing array and reassigns the header.
    let mut snapshots: Vec<(EdgeRef, Vec<geo::PointRef>)> = Vec::with_capacity(edges.len());
    let mut seen: Set<EdgeRef> = Set::default();
    for edge in edges {
        if seen.contains(edge) {
            continue;
        }
        seen.insert(edge.clone());
        snapshots.push((edge.clone(), edge.points()));
    }
    let result = (|| -> GoResult<()> {
        // look for labeled straight edges that have duplicates
        for e in edges {
            checkCanceled()?;
            if e.HasTableColumn() {
                continue;
            }
            if e.b().Label.is_none() || !e.IsStraight() {
                continue;
            }

            let hasOverlappingEnd = e.HasOverlappingEnd();

            let mut duplicates: Vec<usize> = Vec::new();
            for (i, other) in edges.iter().enumerate() {
                checkCanceled()?;
                if other == e {
                    continue;
                }
                if !other.IsDuplicateOf(e) || !other.IsStraight() {
                    continue;
                }
                // Note: only need to check for matching arrowheads if one of the edges overlaps another edge
                if hasOverlappingEnd || other.HasOverlappingEnd() {
                    let (mut sourceArrow, mut targetArrow) = {
                        let eb = e.b();
                        (eb.SourceArrowhead.clone(), eb.TargetArrowhead.clone())
                    };
                    if other.from() == e.to() {
                        // Note: since other is in opposite direction, we reverse points when swapping below
                        std::mem::swap(&mut sourceArrow, &mut targetArrow);
                    }
                    let o = other.b();
                    if o.TargetArrowhead == targetArrow && o.SourceArrowhead == sourceArrow {
                        duplicates.push(i);
                    }
                } else {
                    duplicates.push(i);
                }
            }

            if duplicates.len() > 1 {
                let points = pointValues(&e.b().Points);
                let start = points[0];
                let end = points[points.len() - 1];

                let dx = end.X - start.X;
                let dy = end.Y - start.Y;

                let mut sourceOrder: Vec<EdgeRef> = Vec::with_capacity(duplicates.len() + 1);
                let mut targetOrder: Vec<EdgeRef> = Vec::with_capacity(duplicates.len() + 1);

                sourceOrder.push(e.clone());
                targetOrder.push(e.clone());

                for &i in &duplicates {
                    let dup = edges[i].clone();
                    sourceOrder.push(dup.clone());
                    targetOrder.push(dup);
                }

                sortDuplicateOrders(&mut sourceOrder, &mut targetOrder, dy > dx);
                checkCanceled()?;

                let mut consistentOrder = true;
                for i in 0..sourceOrder.len() {
                    if sourceOrder[i] != targetOrder[i] {
                        consistentOrder = false;
                        break;
                    }
                }
                if !consistentOrder {
                    continue;
                }
                let mut currentIndex: i64 = -1;
                for i in 0..sourceOrder.len() {
                    if sourceOrder[i] == *e {
                        currentIndex = i as i64;
                        break;
                    }
                }
                if currentIndex == 0 || currentIndex == sourceOrder.len() as i64 - 1 {
                    continue;
                }
                let first = sourceOrder[0].clone();
                let last = sourceOrder[sourceOrder.len() - 1].clone();
                if first.b().Label.is_some() && last.b().Label.is_some() {
                    continue;
                }
                checkCanceled()?;

                if last.b().Label.is_none() {
                    swapEdgePoints(e, &last);
                    if last.from() == e.to() {
                        // need to reverse order of points
                        e.bm().Points.reverse();
                        last.bm().Points.reverse();
                    }
                } else if first.b().Label.is_none() {
                    swapEdgePoints(e, &first);
                    if first.from() == e.to() {
                        e.bm().Points.reverse();
                        first.bm().Points.reverse();
                    }
                }
            }
        }
        checkCanceled()
    })();
    if result.is_err() {
        for (edge, points) in &snapshots {
            edge.bm().Points = points.clone();
        }
    }
    result
}

// ---------------------------------------------------------------------------
// Crosshatch
// ---------------------------------------------------------------------------

pub fn Crosshatch(ctx: &Ctx, g: &GraphRef) -> GoResult<()> {
    crosshatchWithWorkLimit(ctx, g, maxRouteStageWorkUnits)
}

pub(crate) fn crosshatchWithWorkLimit(ctx: &Ctx, g: &GraphRef, workLimit: u64) -> GoResult<()> {
    runAtomicRouteStage(ctx, "Crosshatch", g, &[], workLimit, |guard| {
        let clusters = g.b().Clusters.clone();
        let mut vessels: Vec<NodeRef> = Vec::with_capacity(clusters.len());
        for vessel in clusters.keys() {
            guard.step()?;
            vessels.push(vessel);
        }
        layoutgraph::SortNodesByID(&mut vessels);
        for vessel in &vessels {
            guard.step()?;
            let Some(cluster) = clusters.get(vessel) else {
                return Err(invariant::New("nil cluster while crosshatching routes"));
            };
            let externalEdges = externalEdgesForClusterGuarded(&cluster, guard)?;
            if externalEdges.len() < 2 {
                continue;
            }

            let portToEdges = groupEdgesByClusterPortGuarded(&cluster, &externalEdges, guard)?;
            let mut ports: Vec<geo::Point> = Vec::with_capacity(portToEdges.len());
            for port in portToEdges.keys() {
                guard.step()?;
                ports.push(*port);
            }
            sort::SortSlice(&mut ports, |a, b| {
                if a.X != b.X {
                    return a.X < b.X;
                }
                a.Y < b.Y
            });
            for port in &ports {
                let edges = &portToEdges[port];
                if edges.len() < 2 {
                    continue;
                }
                for edge in edges {
                    convertToStraightLineGuarded(g, edge, guard)?;
                }
            }
        }
        Ok(())
    })
}

fn externalEdgesForClusterGuarded(cluster: &layoutgraph::ClusterRef, guard: &routeWorkGuardRef) -> GoResult<Vec<EdgeRef>> {
    let mut externalEdges = Vec::new();
    let abductions = cluster.b().EdgeAbductions.clone();
    for edgeAbduction in &abductions {
        guard.step()?;
        externalEdges.push(edgeAbduction.b().Edge.clone());
    }
    Ok(externalEdges)
}

fn groupEdgesByClusterPortGuarded(
    cluster: &layoutgraph::ClusterRef,
    edges: &[EdgeRef],
    guard: &routeWorkGuardRef,
) -> GoResult<Map<geo::Point, Vec<EdgeRef>>> {
    let mut portToEdges: Map<geo::Point, Vec<EdgeRef>> = Map::default();
    let clusterNodes = cluster.b().Nodes.clone();

    for edge in edges {
        guard.step()?;
        // Find which cluster node this edge connects to and get the port
        let mut clusterPort: Option<geo::PointRef> = None;
        let (from, to) = (edge.from(), edge.to());
        for clusterNode in &clusterNodes {
            guard.step()?;
            if from == *clusterNode {
                clusterPort = edge.SourcePort();
                break;
            } else if to == *clusterNode {
                clusterPort = edge.TargetPort();
                break;
            }
        }

        if let Some(clusterPort) = clusterPort {
            portToEdges.entry(clusterPort.get()).or_default().push(edge.clone());
        }
    }

    Ok(portToEdges)
}

fn convertToStraightLineGuarded(g: &GraphRef, edge: &EdgeRef, guard: &routeWorkGuardRef) -> GoResult<()> {
    guard.step()?;
    let (from, to) = (edge.from(), edge.to());
    if !from.has_tl() || !to.has_tl() {
        return Err(invariant::New("invalid edge while crosshatching routes"));
    }
    let (ft, tt) = (from.tlv(), to.tlv());
    let fromCenter = geo::Point { X: ft.X + from.w() / 2.0, Y: ft.Y + from.h() / 2.0 };
    let toCenter = geo::Point { X: tt.X + to.w() / 2.0, Y: tt.Y + to.h() / 2.0 };

    let fromBorder = findLineBorderIntersection(&fromCenter, &toCenter, &from);
    let toBorder = findLineBorderIntersection(&toCenter, &fromCenter, &to);

    let intersects = straightLineIntersectsNonAncestorNodesGuarded(g, edge, &fromBorder, &toBorder, guard)?;
    if intersects {
        return Ok(());
    }
    guard.finish()?;
    edge.bm().Points = vec![geo::PointRef::new(fromBorder), geo::PointRef::new(toBorder)];
    guard.finish()?;
    Ok(())
}

// findLineBorderIntersection finds where a line from nodeCenter towards targetPoint intersects the node's border
pub(crate) fn findLineBorderIntersection(nodeCenter: &geo::Point, targetPoint: &geo::Point, node: &NodeRef) -> geo::Point {
    // Calculate direction vector
    let dx = targetPoint.X - nodeCenter.X;
    let dy = targetPoint.Y - nodeCenter.Y;

    if dx == 0.0 && dy == 0.0 {
        return *nodeCenter;
    }

    // Node boundaries
    let tl = node.tlv();
    let left = tl.X;
    let right = tl.X + node.w();
    let top = tl.Y;
    let bottom = tl.Y + node.h();

    let mut t: f64 = math::Inf(1);

    // Check intersection with each edge of the rectangle
    if dx > 0.0 {
        t = math::Min(t, (right - nodeCenter.X) / dx);
    } else if dx < 0.0 {
        t = math::Min(t, (left - nodeCenter.X) / dx);
    }

    if dy > 0.0 {
        t = math::Min(t, (bottom - nodeCenter.Y) / dy);
    } else if dy < 0.0 {
        t = math::Min(t, (top - nodeCenter.Y) / dy);
    }

    if t == math::Inf(1) || t <= 0.0 {
        return *nodeCenter;
    }

    geo::Point { X: nodeCenter.X + t * dx, Y: nodeCenter.Y + t * dy }
}

// straightLineIntersectsNonAncestorNodes checks if a straight line between two points
// intersects any nodes that are not ancestors of the edge's endpoints
fn straightLineIntersectsNonAncestorNodesGuarded(
    g: &GraphRef,
    edge: &EdgeRef,
    p1: &geo::Point,
    p2: &geo::Point,
    guard: &routeWorkGuardRef,
) -> GoResult<bool> {
    let (from, to) = (edge.from(), edge.to());
    let (p1, p2) = (geo::PointRef::new(*p1), geo::PointRef::new(*p2));
    for node in g.nodes() {
        guard.step()?;
        // Skip if this node is an ancestor of either endpoint
        let fromDescendant = isDescendantOfWithRouteGuard(Some(&from), Some(&node), guard)?;
        let toDescendant = isDescendantOfWithRouteGuard(Some(&to), Some(&node), guard)?;
        if fromDescendant || toDescendant {
            continue;
        }

        // Skip if this node is one of the endpoints
        if node == from || node == to {
            continue;
        }

        // Check if the straight line passes through this node
        if node.PassesThrough(Some(&p1), Some(&p2)) {
            return Ok(true);
        }
    }

    Ok(false)
}
