// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/routing/ovg_edge_router.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of routing/ovg_edge_router.go.
#[allow(unused_imports)]
use crate::*;
use gocompat::context::{self, Ctx};
use gocompat::{errorf, math, GoError, GoResult, Map, Set};
use layoutgraph::{ClusterRef, EdgeRef, GraphRef, NodeRef};

/// Keeps the state for OVG edge search with data structure to quickly check if a given
/// edge is already in some route or if it intersects with other routes.
pub struct ovgEdgeRouter {
    pub(crate) flavor: RouteGenerationFlavor,
    pub(crate) graph: GraphRef,
    pub(crate) ovg: OVGRef,
    pub(crate) edges: Vec<EdgeRef>,
    pub(crate) routes: Vec<RouteRef>,
    pub(crate) routedEdges: Vec<EdgeRef>,
    pub(crate) positionedLabels: Vec<labeling::PositionedArrowheadLabel>,
    pub(crate) pointToRoute: Map<geo::Point, Vec<RouteRef>>,
    pub(crate) turnCost: f64,
    pub(crate) crossingCost: f64,
    pub(crate) nonCenterPortCost: f64,
    pub(crate) edgeSet: ovgEdgeSet,
    pub(crate) hasNearbyEdge: Set<OVGEdgeRef>,
    pub(crate) overlappingRoutes: Map<OVGEdgeRef, Vec<RouteRef>>,
    pub(crate) fixedOverlaps: Set<NodeRef>,
    pub(crate) fixedOverlapsSet: bool,
    pub(crate) verticalHops: Vec<Option<OVGNodeRef>>,
    pub(crate) horizontalHops: Vec<Option<OVGNodeRef>>,
    /// Go `nodeContext []*searchNodeContext` (nil = not created this search).
    pub(crate) nodeContext: Vec<Option<searchNodeContext>>,
    pub(crate) searchQueue: priorityQueue,
    /// Usually, edge routing occurs before label placement. When the router is used
    /// independently, labels are already placed and the edges should move out of the way.
    pub(crate) considerNodeLabels: bool,
    pub(crate) work: Option<routeSearchWorkGuardRef>,
}

pub(crate) fn checkEdgeRoutingCanceled(ctx: &Ctx) -> GoResult<()> {
    if let Some(err) = ctx.Err() {
        return Err(GoError::wrap(format!("EdgeRouting: {}", err), err));
    }
    Ok(())
}

pub(crate) fn isContextTermination(err: &GoError) -> bool {
    err.Is(&context::Canceled) || err.Is(&context::DeadlineExceeded)
}

#[derive(Clone, Copy, Debug, Default)]
pub struct searchNodeContext {
    pub(crate) verticalDistance: f64,
    pub(crate) horizontalDistance: f64,
    pub(crate) verticalEntry: Option<priorityQueueEntryRef>,
    pub(crate) horizontalEntry: Option<priorityQueueEntryRef>,
}

impl searchNodeContext {
    fn fresh() -> searchNodeContext {
        searchNodeContext { verticalDistance: math::Inf(1), horizontalDistance: math::Inf(1), verticalEntry: None, horizontalEntry: None }
    }
}

fn pointMapHas(m: &Map<geo::Point, bool>, p: &geo::Point) -> bool {
    m.contains_key(p)
}

fn pointMapTrue(m: &Map<geo::Point, bool>, p: &geo::Point) -> bool {
    m.get(p).copied().unwrap_or(false)
}

impl ovgEdgeRouter {
    /// Go `router.work` (non-nil after construction).
    #[inline]
    pub(crate) fn w(&self) -> &routeSearchWorkGuardRef {
        self.work.as_ref().expect("nil route-search work guard")
    }

    pub(crate) fn bindWork(&mut self, ctx: &Ctx) -> GoResult<()> {
        match &self.work {
            None => {
                let guard = newRouteSearchWorkGuard(ctx, self.flavor.clone(), maxRouteSearchWorkUnits)?;
                self.work = Some(guard);
                Ok(())
            }
            Some(w) => w.bind(ctx),
        }
    }
}

pub(crate) fn newOVGEdgeRouterWithWorkLimit(
    ctx: &Ctx,
    flavor: RouteGenerationFlavor,
    ovg: &OVGRef,
    g: &GraphRef,
    existingRoutes: &[RouteRef],
    edges: &[EdgeRef],
    workLimit: u64,
) -> GoResult<ovgEdgeRouter> {
    let guard = newRouteSearchWorkGuard(ctx, flavor.clone(), workLimit)?;
    let r = newOVGEdgeRouterInner(flavor, ovg, g, existingRoutes, edges, &guard);
    if r.is_err() {
        let _ = guard.finish();
    }
    r
}

fn newOVGEdgeRouterInner(
    flavor: RouteGenerationFlavor,
    ovg: &OVGRef,
    g: &GraphRef,
    existingRoutes: &[RouteRef],
    edges: &[EdgeRef],
    guard: &routeSearchWorkGuardRef,
) -> GoResult<ovgEdgeRouter> {
    guard.add(edges.len() as u64)?;
    let mut orderedEdges: Vec<EdgeRef> = edges.to_vec();

    let mut clusterNodes: Set<NodeRef> = Set::default();
    let clusters = g.b().Clusters.values();
    for c in &clusters {
        guard.step()?;
        for cn in &c.b().Nodes {
            guard.step()?;
            clusterNodes.insert(cn.clone());
        }
    }
    for edge in &orderedEdges {
        guard.step()?;
        let (from, to) = {
            let e = edge.b();
            (e.From.clone(), e.To.clone())
        };
        guard.reserveSum(from.b().Edges.len() as u64, to.b().Edges.len() as u64)?;
    }
    guard.reserveSort(orderedEdges.len())?;
    guard.reserveSort(orderedEdges.len())?;
    sortEdges(&flavor, &mut orderedEdges, &clusterNodes);

    let (turnCost, crossingCost, nonCenterPortCost) = (g.TurnCost(), g.CrossingCost(), g.NonCenterPortCost());
    let mut router = ovgEdgeRouter {
        flavor,
        graph: g.clone(),
        ovg: ovg.clone(),
        routes: Vec::new(),
        routedEdges: Vec::new(),
        positionedLabels: Vec::new(),
        edges: orderedEdges,
        pointToRoute: Map::default(),
        turnCost,
        crossingCost,
        nonCenterPortCost,
        edgeSet: newOvgEdgeSet(),
        hasNearbyEdge: Set::default(),
        overlappingRoutes: Map::default(),
        fixedOverlaps: Set::default(),
        fixedOverlapsSet: false,
        verticalHops: Vec::new(),
        horizontalHops: Vec::new(),
        nodeContext: Vec::new(),
        searchQueue: priorityQueue::default(),
        considerNodeLabels: false,
        work: Some(guard.clone()),
    };

    for route in existingRoutes {
        guard.step()?;
        router.addRoute(route)?;
    }

    guard.check()?;
    Ok(router)
}

fn cmpF64(a: f64, b: f64) -> i64 {
    if a < b {
        -1
    } else if b < a {
        1
    } else {
        0
    }
}

pub(crate) fn sortEdges(flavor: &RouteGenerationFlavor, edges: &mut Vec<EdgeRef>, clusterNodes: &Set<NodeRef>) {
    if *flavor == ShortestToLongest {
        let mut keyed: Vec<(f64, EdgeRef)> = edges.iter().map(|e| (e.EuclideanDistance(), e.clone())).collect();
        gocompat::sort::SlicesSortStableFunc(&mut keyed, |a, b| cmpF64(a.0, b.0));
        *edges = keyed.into_iter().map(|(_, e)| e).collect();
    } else if *flavor == LongestToShortest {
        let mut keyed: Vec<(f64, EdgeRef)> = edges.iter().map(|e| (e.EuclideanDistance(), e.clone())).collect();
        gocompat::sort::SlicesSortStableFunc(&mut keyed, |a, b| {
            let (aDistance, bDistance) = (a.0, b.0);
            if aDistance > bDistance {
                -1
            } else if bDistance > aDistance {
                1
            } else {
                0
            }
        });
        *edges = keyed.into_iter().map(|(_, e)| e).collect();
    } else if *flavor == TopDownLeftRight {
        // (fromNode, fromTL, toTL) with From above To
        struct K {
            from: NodeRef,
            ftl: geo::Point,
            ttl: geo::Point,
            e: EdgeRef,
        }
        let mut keyed: Vec<K> = edges
            .iter()
            .map(|e| {
                let (mut f, mut t) = {
                    let eb = e.b();
                    (eb.From.clone(), eb.To.clone())
                };
                // Ensure that edge.From is above edge.To for both edges.
                if f.y() > t.y() {
                    std::mem::swap(&mut f, &mut t);
                }
                K { ftl: f.tlv(), ttl: t.tlv(), from: f, e: e.clone() }
            })
            .collect();
        gocompat::sort::SlicesSortStableFunc(&mut keyed, |a, b| {
            if a.ftl.Y != b.ftl.Y {
                return cmpF64(a.ftl.Y, b.ftl.Y);
            }
            // a and b start in the same row.
            if a.from != b.from {
                return cmpF64(a.ftl.X, b.ftl.X);
            }
            // Same from node, maybe different targets.
            if a.ttl.Y == b.ttl.Y {
                return cmpF64(a.ttl.X, b.ttl.X);
            }
            // Return the top-most target.
            cmpF64(a.ttl.Y, b.ttl.Y)
        });
        *edges = keyed.into_iter().map(|k| k.e).collect();
    }

    // Clusters go first, looks best
    let mut keyed: Vec<(bool, EdgeRef)> = edges
        .iter()
        .map(|e| {
            let eb = e.b();
            (clusterNodes.contains(&eb.From) || clusterNodes.contains(&eb.To), e.clone())
        })
        .collect();
    gocompat::sort::SlicesSortStableFunc(&mut keyed, |a, b| {
        let (connectedA, connectedB) = (a.0, b.0);
        if connectedA && !connectedB {
            -1
        } else if !connectedA && connectedB {
            1
        } else {
            0
        }
    });
    *edges = keyed.into_iter().map(|(_, e)| e).collect();
}

/// Go `GenerateRouteResponse` result of a route search: (OVG nodes, distance).
pub(crate) type routeSearchResult = (Vec<OVGNodeRef>, f64);

impl ovgEdgeRouter {
    pub(crate) fn routeLine(&mut self, ctx: &Ctx, edge: &EdgeRef) -> GoResult<routeSearchResult> {
        self.bindWork(ctx)?;
        let w = self.w().clone();
        let mut checkWork = || w.step();
        let r = routeLineChecked(&self.graph, edge, &self.routedEdges, &self.routes, &mut checkWork, Some(self.w() as &dyn workBudget));

        let (from, to) = {
            let e = edge.b();
            (e.From.clone(), e.To.clone())
        };
        match r {
            Err(err) => {
                if err.Is(&layoutgraph::ErrInvalidCandidate) {
                    let ovg = self.ovg.b();
                    let fromCenter = ovg.Centers.get(&from).cloned().expect("missing source center");
                    let toCenter = ovg.Centers.get(&to).cloned().expect("missing target center");
                    let mut fromPort: Option<geo::Point> = None;
                    let mut toPort: Option<geo::Point> = None;
                    let mut segment = geo::Segment { Start: fromCenter.PRef(), End: toCenter.PRef() };
                    let intersections = from.boxValue().Intersections(&segment);
                    if !intersections.is_empty() {
                        fromPort = Some(*intersections[0].b());
                        segment.Start = intersections[0].clone();
                    }
                    let intersections = to.boxValue().Intersections(&segment);
                    if !intersections.is_empty() {
                        toPort = Some(*intersections[0].b());
                    }
                    // These only happen when there's overlap. It'll look terrible regardless, so just make it work
                    let fromPort = fromPort.unwrap_or_else(|| fromCenter.P());
                    let toPort = toPort.unwrap_or_else(|| toCenter.P());
                    let (fc, tc) = (fromCenter.P(), toCenter.P());
                    let ovgNodes = vec![fromCenter.clone(), NewOVGNode(fromPort), NewOVGNode(toPort), toCenter.clone()];
                    return Ok((ovgNodes, geo::EuclideanDistance(fc.X, fc.Y, tc.X, tc.Y)));
                }
                Err(err)
            }
            Ok((fromPort, toPort, cost)) => {
                let (fromPort, toPort) = (*fromPort.b(), *toPort.b());
                let ovg = self.ovg.b();
                let ovgNodes = vec![
                    ovg.Centers.get(&from).cloned().expect("missing source center"),
                    ovg.OccupiedPoints.get(&fromPort).cloned().expect("nil OVG node for route line port"),
                    ovg.OccupiedPoints.get(&toPort).cloned().expect("nil OVG node for route line port"),
                    ovg.Centers.get(&to).cloned().expect("missing target center"),
                ];
                Ok((ovgNodes, cost))
            }
        }
    }

    pub(crate) fn generateRoutes(&mut self, ctx: &Ctx, straightLineFallback: bool) -> GenerateRouteResponse {
        let mut resp = GenerateRouteResponse::default();
        resp.Flavor = self.flavor.clone();
        if let Err(err) = self.bindWork(ctx) {
            resp.Err = Some(err);
            if let Some(w) = &self.work {
                let _ = w.finish();
            }
            return resp;
        }
        resp.work = self.work.clone();
        self.generateRoutesInner(ctx, straightLineFallback, &mut resp);
        let finishErr = self.w().finish();
        if resp.Err.is_none() {
            if let Err(err) = finishErr {
                resp.Err = Some(err);
                resp.Routes = Vec::new();
            }
        }
        resp
    }

    fn generateRoutesInner(&mut self, ctx: &Ctx, straightLineFallback: bool, resp: &mut GenerateRouteResponse) {
        let mut totalDistance = 0.0f64;
        let edges = self.edges.clone();
        for edge in &edges {
            if let Err(err) = self.w().step() {
                resp.Err = Some(err);
                return;
            }
            let mut ovgNodes: Vec<OVGNodeRef>;
            let mut distance: f64;

            match self.slingshot(ctx, edge) {
                Ok((n, d)) => {
                    ovgNodes = n;
                    distance = d;
                }
                Err(err) => {
                    let mut r = Err(err);
                    if straightLineFallback {
                        r = self.routeLine(ctx, edge);
                    }
                    match r {
                        Ok((n, d)) => {
                            ovgNodes = n;
                            distance = d;
                        }
                        Err(err) => {
                            resp.Err = Some(err);
                            return;
                        }
                    }
                }
            }
            if ovgNodes.len() <= 1 {
                let mut r = self.search(ctx, edge);
                if let Err(err) = &r {
                    if isContextTermination(err) {
                        resp.Err = Some(err.clone());
                        return;
                    }
                    if straightLineFallback {
                        r = self.routeLine(ctx, edge);
                    }
                }
                match r {
                    Ok((n, d)) => {
                        ovgNodes = n;
                        distance = d;
                    }
                    Err(err) => {
                        resp.Err = Some(err);
                        return;
                    }
                }
                if ovgNodes.len() <= 1 {
                    let message = format!("Route '{}' has size {}", edge.DebugID(), ovgNodes.len());
                    resp.Err = Some(invariant::New(message));
                    return;
                }
            }

            totalDistance += distance;
            let route = RouteRef::new(Route {
                GEdge: edge.clone(),
                FromPort: ovgNodes[1].P(),
                ToPort: ovgNodes[ovgNodes.len() - 2].P(),
                OVGNodes: ovgNodes,
            });
            if let Err(err) = self.addRoute(&route) {
                resp.Err = Some(err);
                return;
            }
        }

        resp.Routes = self.routes.clone();
        resp.Distance = totalDistance;
    }

    /// Among center ports, slightly favor going through center ports where the mirrored center port is occupied
    pub(crate) fn centerSymmetricalPorts(
        &mut self,
        ctx: &Ctx,
        gSource: &NodeRef,
        gTarget: &NodeRef,
        sourcePortsUsed: &Map<geo::Point, bool>,
        targetPortsUsed: &Map<geo::Point, bool>,
    ) -> GoResult<Set<OVGNodeRef>> {
        let mut nodeIsCenterSymmetricalPort: Set<OVGNodeRef> = Set::default();
        let ovg = self.ovg.b();
        let empty = Vec::new();
        let sourcePorts = ovg.Ports.get(gSource).unwrap_or(&empty);
        let targetPorts = ovg.Ports.get(gTarget).unwrap_or(&empty);
        let w = self.w();
        w.reserveSum(sourcePorts.len() as u64, targetPorts.len() as u64)?;
        let sourceMirroredPorts = gSource.MirroredPorts();
        let targetMirroredPorts = gTarget.MirroredPorts();
        for port in sourcePorts {
            w.step()?;
            // If this is a port number that has a mirror
            if let Some(mirroredPort) = sourceMirroredPorts.get(&layoutgraph::PointKey(port.P())) {
                // If the mirrored port is occupied, making this one desirable
                if pointMapHas(sourcePortsUsed, mirroredPort) {
                    nodeIsCenterSymmetricalPort.insert(port.clone());
                }
            }
        }
        // Do the same thing for target ports
        for port in targetPorts {
            w.step()?;
            if let Some(mirroredPort) = targetMirroredPorts.get(&layoutgraph::PointKey(port.P())) {
                if pointMapHas(targetPortsUsed, mirroredPort) {
                    nodeIsCenterSymmetricalPort.insert(port.clone());
                }
            }
        }

        checkEdgeRoutingCanceled(ctx)?;
        Ok(nodeIsCenterSymmetricalPort)
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn usedPorts(
        &mut self,
        ctx: &Ctx,
        gSource: &NodeRef,
        gTarget: &NodeRef,
        source: &OVGNodeRef,
        target: &OVGNodeRef,
        sourceClusterNodes: &Set<NodeRef>,
        targetClusterNodes: &Set<NodeRef>,
        undesirableClusterArrangement: bool,
    ) -> GoResult<(Map<geo::Point, bool>, Map<geo::Point, bool>, Map<geo::Point, bool>, Map<geo::Point, bool>, Map<geo::Point, bool>, Map<geo::Point, bool>)> {
        let mut sourcePortsUsed: Map<geo::Point, bool> = Map::default();
        let mut targetPortsUsed: Map<geo::Point, bool> = Map::default();
        let mut duplicateSourcePortsUsed: Map<geo::Point, bool> = Map::default();
        let mut duplicateTargetPortsUsed: Map<geo::Point, bool> = Map::default();
        let mut sharedClusterSourcePortsUsed: Map<geo::Point, bool> = Map::default();
        let mut sharedClusterTargetPortsUsed: Map<geo::Point, bool> = Map::default();

        let ovg = self.ovg.b();
        let empty = Vec::new();
        let sourcePorts = ovg.Ports.get(gSource).unwrap_or(&empty);
        let targetPorts = ovg.Ports.get(gTarget).unwrap_or(&empty);
        let w = self.w();
        let noRoutes: Vec<RouteRef> = Vec::new();
        for node in [source, target] {
            w.step()?;
            // we might process the same route, but since it's just assigning to maps, it's fine
            for route in self.pointToRoute.get(&node.P()).unwrap_or(&noRoutes) {
                w.step()?;
                let r = route.b();
                let (rFrom, rTo) = {
                    let e = r.GEdge.b();
                    (e.From.clone(), e.To.clone())
                };
                if (rFrom != *gSource) && (rTo != *gSource) && (rFrom != *gTarget) && (rTo != *gTarget) {
                    continue;
                }

                // note ports of duplicate connections
                if (rFrom == *gSource) && (rTo == *gTarget) {
                    duplicateSourcePortsUsed.insert(r.FromPort, true);
                    duplicateTargetPortsUsed.insert(r.ToPort, true);
                }

                // ClusterPortSharing Step 1.
                if !undesirableClusterArrangement {
                    if rTo == *gTarget {
                        if sourceClusterNodes.contains(&rFrom) {
                            sharedClusterTargetPortsUsed.insert(r.ToPort, true);
                        }
                    }
                    if rFrom == *gSource {
                        if targetClusterNodes.contains(&rTo) {
                            sharedClusterSourcePortsUsed.insert(r.FromPort, true);
                        }
                    }
                }

                for routeNode in &r.OVGNodes {
                    w.step()?;
                    for portNode in sourcePorts {
                        w.step()?;
                        if routeNode == portNode {
                            sourcePortsUsed.insert(portNode.P(), true);
                            break;
                        }
                    }
                    for portNode in targetPorts {
                        w.step()?;
                        if routeNode == portNode {
                            targetPortsUsed.insert(portNode.P(), true);
                            break;
                        }
                    }
                }
            }
        }
        checkEdgeRoutingCanceled(ctx)?;
        Ok((sourcePortsUsed, targetPortsUsed, duplicateSourcePortsUsed, duplicateTargetPortsUsed, sharedClusterSourcePortsUsed, sharedClusterTargetPortsUsed))
    }

    /// Basically Dijkstra's shortest path with some other checks for overlap and alternative routes
    pub(crate) fn search(&mut self, ctx: &Ctx, gEdge: &EdgeRef) -> GoResult<routeSearchResult> {
        self.bindWork(ctx)?;
        let ovgRef = self.ovg.clone();
        let g = ovgRef.b();
        let originalGraph = self.graph.clone();
        let (gSource, gTarget, sourceLabel, targetLabel, fromCol, toCol) = {
            let e = gEdge.b();
            (e.From.clone(), e.To.clone(), e.SourceArrowheadLabel.clone(), e.TargetArrowheadLabel.clone(), e.FromTableColumnIndex, e.ToTableColumnIndex)
        };
        let hasTableColumn = gEdge.HasTableColumn();
        let isBetweenTableColumns = gEdge.IsBetweenTableColumns();
        let isDirected = gEdge.IsDirected();
        let source = g.Centers.get(&gSource).cloned().expect("missing source center");
        let target = g.Centers.get(&gTarget).cloned().expect("missing target center");
        let work = self.w().clone();
        let wk: &dyn workBudget = &work;

        // Preallocate needed data structures with appropriate capacity
        let nodeCount = g.Nodes.len();
        // Account zeroing the three node-indexed arrays before allocating them.
        work.reserveProduct(nodeCount as u64, 3)?;

        let mut fromFacingPorts: Map<geo::Point, bool> = gocompat::NewMapCap(16);
        let mut toFacingPorts: Map<geo::Point, bool> = gocompat::NewMapCap(16);

        // Arrays for tracking state during path finding
        self.verticalHops.clear();
        self.verticalHops.resize(nodeCount, None);
        self.horizontalHops.clear();
        self.horizontalHops.resize(nodeCount, None);
        self.nodeContext.clear();
        self.nodeContext.resize(nodeCount, None);

        // Pre-allocate slices for temp storage with reasonable capacity
        let mut occupiedEdges: Vec<EdgeRef> = Vec::with_capacity(32);
        let mut overlappingEdges: Vec<EdgeRef> = Vec::with_capacity(32);
        let mut nodeLabelBoxes: Vec<geo::Box> = Vec::with_capacity(32);

        // Fill nodeLabelBoxes if we need to consider node labels
        if self.considerNodeLabels {
            let nodes = originalGraph.b().Nodes.clone();
            for n in &nodes {
                work.step()?;
                if *n == gSource || *n == gTarget {
                    continue;
                }
                let label = n.b().Label.clone();
                if let Some(l) = label {
                    let l = l.b().clone();
                    if l.Text != "" {
                        // Go builds a fake square node around the label box; only its box is used.
                        nodeLabelBoxes.push(geo::Box { TopLeft: Some(n.LabelTopLeft(l.Position, l.Width, l.Height)), Width: l.Width, Height: l.Height });
                    }
                }
            }
        }

        let mut overlap = false;
        // That's fine, could be overlapped
        let fromOrientation = gSource.Orientation(&gTarget);
        if fromOrientation == geo::NONE {
            overlap = true;
        }
        let toOrientation = gTarget.Orientation(&gSource);
        if toOrientation == geo::NONE {
            overlap = true;
        }

        drop(g);
        let quick = self.quickRoute(ctx, gEdge, overlap, fromOrientation)?;
        if let Some(q) = quick {
            return Ok(q);
        }
        let g = ovgRef.b();

        let mut nonAncestors: Vec<NodeRef> = Vec::new();
        if sourceLabel.is_some() || targetLabel.is_some() {
            let nodes = self.graph.b().Nodes.clone();
            nonAncestors = filterEdgeAncestorsGuarded(gEdge, &nodes, wk)?;
        }

        let sourceContainer = gSource.containerField();
        let targetContainer = gTarget.containerField();
        let isSourceDescendantOfTarget = routeSearchIsDescendantOf(wk, Some(&gSource), Some(&gTarget))?;
        let isTargetDescendantOfSource = routeSearchIsDescendantOf(wk, Some(&gTarget), Some(&gSource))?;

        // Fast access to find if a node is a cluster node (cluster nodes sharing routes is nice)
        let (mut sourceClusterNodes, mut targetClusterNodes) = sourceAndTargetClusterNodesGuarded(&originalGraph, &gSource, &gTarget, wk)?;
        let isSourceCluster = !sourceClusterNodes.is_empty();
        let isTargetCluster = !targetClusterNodes.is_empty();

        let undesirableClusterArrangement = self.isUndesirableClusterArrangement(&gSource, &gTarget);

        let mut allowSrcPortSharing = false;
        let mut allowDstPortSharing = false;
        let mut preferFacingPorts = false;
        let mut nonFacingPortsCost = 0.0f64;
        if (isSourceCluster || isTargetCluster) && !undesirableClusterArrangement {
            // use facing ports for properly arranged clusters
            preferFacingPorts = true;
            nonFacingPortsCost = self.turnCost * 2.0;
            allowSrcPortSharing = true;
            allowDstPortSharing = true;
            for port in gSource.PortsByOrientation(fromOrientation.GetOpposite()) {
                work.step()?;
                fromFacingPorts.insert(port, true);
            }
            for port in gTarget.PortsByOrientation(toOrientation.GetOpposite()) {
                work.step()?;
                toFacingPorts.insert(port, true);
            }
        }

        let mut sharedRouteCost: f64 = basicallyInfinity;
        let mut blockedSourcePorts: Map<geo::Point, bool> = gocompat::NewMapCap(16);
        let mut blockedTargetPorts: Map<geo::Point, bool> = gocompat::NewMapCap(16);

        let empty: Vec<OVGNodeRef> = Vec::new();
        let sourcePortsAll = g.Ports.get(&gSource).unwrap_or(&empty);
        let targetPortsAll = g.Ports.get(&gTarget).unwrap_or(&empty);
        if !hasTableColumn {
            work.reserveProduct(sourcePortsAll.len() as u64, targetPortsAll.len() as u64)?;
            // Copy overlapping ports into our pooled maps
            for (k, v) in gSource.OverlappingPorts(&gTarget) {
                blockedSourcePorts.insert(k.0, v);
            }
            for (k, v) in gTarget.OverlappingPorts(&gSource) {
                blockedTargetPorts.insert(k.0, v);
            }
        } else {
            drop(g);
            let (sourcePorts, targetPorts) = self.tablePorts(gEdge, &gSource, &gTarget, false)?;
            let g2 = ovgRef.b();
            let sourcePortsAll = g2.Ports.get(&gSource).unwrap_or(&empty);
            let targetPortsAll = g2.Ports.get(&gTarget).unwrap_or(&empty);
            let inSet = |s: &Option<Set<OVGNodeRef>>, p: &OVGNodeRef| s.as_ref().map_or(false, |s| s.contains(p));
            if isBetweenTableColumns {
                sharedRouteCost = basicallyInfinity / 10.0;
                allowSrcPortSharing = true;
                allowDstPortSharing = true;
                for port in sourcePortsAll {
                    work.step()?;
                    if !inSet(&sourcePorts, port) {
                        blockedSourcePorts.insert(port.P(), true);
                    }
                }
                for port in targetPortsAll {
                    work.step()?;
                    if !inSet(&targetPorts, port) {
                        blockedTargetPorts.insert(port.P(), true);
                    }
                }
                for e in &gSource.edges() {
                    work.step()?;
                    targetClusterNodes.insert(gSource.Adjacent(e));
                }
                for e in &gTarget.edges() {
                    work.step()?;
                    sourceClusterNodes.insert(gTarget.Adjacent(e));
                }
            } else if fromCol.is_some() {
                allowSrcPortSharing = true;
                for port in sourcePortsAll {
                    work.step()?;
                    if !inSet(&sourcePorts, port) {
                        blockedSourcePorts.insert(port.P(), true);
                    }
                }
                for e in &gTarget.edges() {
                    work.step()?;
                    sourceClusterNodes.insert(gTarget.Adjacent(e));
                }
            } else if toCol.is_some() {
                allowDstPortSharing = true;
                for port in targetPortsAll {
                    work.step()?;
                    if !inSet(&targetPorts, port) {
                        blockedTargetPorts.insert(port.P(), true);
                    }
                }
                for e in &gSource.edges() {
                    work.step()?;
                    targetClusterNodes.insert(gSource.Adjacent(e));
                }
            }
        }
        let g = ovgRef.b();

        let idealTurnAxes = idealTurnAxes(&gSource, &gTarget);

        drop(g);
        let (sourcePortsUsed, targetPortsUsed, duplicateSourcePortsUsed, duplicateTargetPortsUsed, sharedClusterSourcePortsUsed, sharedClusterTargetPortsUsed) =
            self.usedPorts(ctx, &gSource, &gTarget, &source, &target, &sourceClusterNodes, &targetClusterNodes, undesirableClusterArrangement)?;
        let nodeIsCenterSymmetricalPort = self.centerSymmetricalPorts(ctx, &gSource, &gTarget, &sourcePortsUsed, &targetPortsUsed)?;

        if !self.fixedOverlapsSet {
            let nodes = originalGraph.b().Nodes.clone();
            self.fixedOverlaps = fixedOverlapsForRoute(&nodes, &work)?;
            self.fixedOverlapsSet = true;
        }

        // Positioned label boxes (Go reads router.positionedLabels[i].Box).
        let positionedLabelBoxes: Vec<geo::Box> = self.positionedLabels.iter().map(|l| l.Box.clone()).collect();

        // Create the initial source node context
        let sourceIndex = source.Index();
        self.nodeContext[sourceIndex] = Some(searchNodeContext::fresh());

        // Priority queue storage is reusable because a flavor routes its edges
        // sequentially.
        self.searchQueue.reset();

        // Each node can be visited twice -- once from each direction. Seed the
        // vertical state first to make the equal-cost source preference explicit.
        self.searchQueue.push(0.0, &source, false, Some(wk))?;
        self.searchQueue.push(0.0, &source, true, Some(wk))?;

        let gSourceCluster: Option<ClusterRef> = gSource.clusterField();
        let evenClusterTurn = match &gSourceCluster {
            Some(c) => {
                let cb = c.b();
                cb.Arrangement == cb.DesiredArrangement && cb.Nodes.len() % 2 == 0
            }
            None => false,
        };

        while !self.searchQueue.empty() {
            work.step()?;
            // Get the least distance node, regardless of direction
            let leastDistanceEntry = match self.searchQueue.pop(Some(wk)) {
                Ok(e) => e,
                Err(err) => {
                    if err.Is(&errRouteSearchWorkLimit) || isContextTermination(&err) {
                        return Err(err);
                    }
                    return Err(invariant::New(format!("Path not found for '{}'. Error {}", gEdge.DebugID(), err)));
                }
            };
            let (isFromHorizontal, leastDistanceNode, leastDistance) = {
                let e = self.searchQueue.entry(leastDistanceEntry);
                (e.isHorizontal, e.node.clone().unwrap(), e.priority)
            };

            if leastDistanceNode == target {
                let routeNodes = self.bestRoute(ctx, &gSource, &gTarget, &source, &target)?;
                return Ok((routeNodes, leastDistance));
            }

            let ldIndex = leastDistanceNode.Index();
            let lastNode: Option<OVGNodeRef> = if isFromHorizontal { self.horizontalHops[ldIndex].clone() } else { self.verticalHops[ldIndex].clone() };
            let ldPoint = leastDistanceNode.P();
            let ldIsSource = leastDistanceNode == source;
            let ldIsTarget = leastDistanceNode == target;

            // Reset occupiedEdges array for reuse
            occupiedEdges.clear();
            if !ldIsSource && !ldIsTarget {
                if let Some(pointRoutes) = self.pointToRoute.get(&ldPoint) {
                    for route in pointRoutes {
                        work.step()?;
                        occupiedEdges.push(route.b().GEdge.clone());
                    }
                }
            }

            let isOnNodeOfRoute = !occupiedEdges.is_empty();
            let areAllOccupiedRoutesShareable = edgeCanOverlapEdgesGuarded(gEdge, &occupiedEdges, Some(&sourceClusterNodes), Some(&targetClusterNodes), wk)?;

            let (ldSourcePort, ldIsPortOfTarget, ldIsCenterPortOfTarget, ldContainer, ldEdges) = {
                let n = leastDistanceNode.b();
                (n.portMetadataFor(&gSource), n.isPortOf(&gTarget), n.isCenterPortOf(&gTarget), n.Container.clone(), n.Edges.clone())
            };
            let ldIsPortOfSource = ldSourcePort.is_some();

            for e in &ldEdges {
                work.step()?;
                let adjacentNode = leastDistanceNode.adjacent(e);
                if Some(&adjacentNode) == lastNode.as_ref() {
                    continue;
                }
                let adj = adjacentNode.b();
                let adjPoint = adj.Point;

                // Prevent exiting source port node at weird angles
                if let Some(sourcePort) = ldSourcePort {
                    let validExit = sourcePort.directions.any(|direction| {
                        // When overlapped, the requirements are relaxed a little.
                        if overlap {
                            if direction == geo::Top || direction == geo::Bottom {
                                return adjPoint.Y != ldPoint.Y;
                            } else if direction == geo::Right || direction == geo::Left {
                                return adjPoint.X != ldPoint.X;
                            }
                            return true;
                        }

                        if direction == geo::Top {
                            adjPoint.Y < ldPoint.Y
                        } else if direction == geo::Bottom {
                            adjPoint.Y > ldPoint.Y
                        } else if direction == geo::Right {
                            adjPoint.X > ldPoint.X
                        } else if direction == geo::Left {
                            adjPoint.X < ldPoint.X
                        } else {
                            true
                        }
                    });
                    if !validExit {
                        continue;
                    }
                }

                let adjTargetPort = adj.portMetadataFor(&gTarget);
                if let Some(targetPort) = adjTargetPort {
                    if pointMapHas(&blockedTargetPorts, &adjPoint) {
                        continue;
                    }

                    // Prevent approaching target port node from weird angles. Coincident
                    // rounded ports can represent several sides of the same node, so one
                    // valid approach is enough.
                    let validApproach = targetPort.directions.any(|direction| {
                        if overlap {
                            if direction == geo::Top || direction == geo::Bottom {
                                return adjPoint.Y != ldPoint.Y;
                            } else if direction == geo::Right || direction == geo::Left {
                                return adjPoint.X != ldPoint.X;
                            }
                            return true;
                        }

                        if direction == geo::Top {
                            adjPoint.Y > ldPoint.Y
                        } else if direction == geo::Bottom {
                            adjPoint.Y < ldPoint.Y
                        } else if direction == geo::Right {
                            adjPoint.X < ldPoint.X
                        } else if direction == geo::Left {
                            adjPoint.X > ldPoint.X
                        } else {
                            true
                        }
                    });
                    if !validApproach {
                        continue;
                    }
                }

                let adjIsTarget = adjacentNode == target;
                let adjIsPortOfSource = adj.isPortOf(&gSource);
                let adjIsPortOfTarget = adjTargetPort.is_some();
                if !ldIsSource && !adjIsTarget && !adjIsPortOfSource && !adjIsPortOfTarget {
                    // Cannot go through a container unless the target or source is a descendant of that container (or is that container)
                    if let Some(adjContainer) = &adj.Container {
                        let sourceInside = routeSearchIsDescendantOf(wk, sourceContainer.as_ref(), Some(adjContainer))?;
                        let targetInside = routeSearchIsDescendantOf(wk, targetContainer.as_ref(), Some(adjContainer))?;
                        if !sourceInside && !targetInside {
                            if !self.fixedOverlaps.contains(adjContainer) {
                                continue;
                            }
                        }
                    }
                }

                // only use adjacent nodes that are vertically or horizontally aligned with leastDistanceNode (no diagonals), unless using source/target center
                if !adjIsTarget && !ldIsSource {
                    if (adjPoint.X != ldPoint.X) && (adjPoint.Y != ldPoint.Y) {
                        continue;
                    }
                }

                if adjIsPortOfSource {
                    if pointMapHas(&blockedSourcePorts, &adjPoint) {
                        continue;
                    }
                }

                let mut nextDistance: f64;
                if ldIsSource {
                    nextDistance = 1.0;
                    // To avoid overlapping duplicate edges, avoid using the same source port as a duplicate edge
                    if !allowSrcPortSharing && pointMapTrue(&duplicateSourcePortsUsed, &adjPoint) {
                        nextDistance = basicallyInfinity;
                    }
                    // ClusterPortSharing Step 2a.
                    if !sharedClusterSourcePortsUsed.is_empty() {
                        if !pointMapHas(&sharedClusterSourcePortsUsed, &adjPoint) {
                            // If it takes more than 2 turns to get there, not worth sharing
                            nextDistance += 2.0 * self.turnCost;
                        }
                    }
                    if !pointMapHas(&fromFacingPorts, &adjPoint) && preferFacingPorts {
                        nextDistance += nonFacingPortsCost;
                    }

                    // Looks nicer when arrows come out of center ports
                    if !adj.isCenterPortOf(&gSource) {
                        nextDistance += self.nonCenterPortCost;
                    } else {
                        // Looks even slightly nicer when it comes out a port that has an edge going through the symmetrical port
                        if !nodeIsCenterSymmetricalPort.contains(&adjacentNode) {
                            // Only prefer it all else being equal
                            nextDistance += 1.0;
                        }
                    }
                } else {
                    nextDistance = e.Distance;
                }

                // Cannot go outside a container if both the source and target are inside that container
                // but for nested going to parent, there may not be enough paths otherwise, so we allow that
                if sourceContainer.is_some() && sourceContainer == targetContainer && !(isSourceDescendantOfTarget || isTargetDescendantOfSource) {
                    if adj.Container.is_none() || adj.Container != sourceContainer {
                        // Note: penalty is if adjacent is outside the source container, and the edge from least distance to adjacent enters a different container
                        if ldContainer != adj.Container {
                            nextDistance += self.turnCost * 4.0;
                        }
                    }
                }

                // Penalize routing between ports of the same node
                {
                    let ld = leastDistanceNode.b();
                    let mut shares = false;
                    for (owner, _) in ld.portOwners() {
                        if adj.isPortOf(owner) {
                            shares = true;
                            break;
                        }
                    }
                    if shares {
                        nextDistance += self.turnCost * 4.0;
                    }
                }

                // APPLY SPECIAL WEIGHTS --------------
                if adjIsTarget {
                    nextDistance = 1.0;
                    // To avoid overlapping duplicate edges, don't use the same target port as a duplicate edge
                    if !allowDstPortSharing && pointMapTrue(&duplicateTargetPortsUsed, &ldPoint) {
                        nextDistance = basicallyInfinity;
                    }
                    // ClusterPortSharing Step 2b.
                    if !sharedClusterTargetPortsUsed.is_empty() {
                        if !pointMapHas(&sharedClusterTargetPortsUsed, &ldPoint) {
                            nextDistance += 2.0 * self.turnCost;
                        }
                    }
                    if !pointMapHas(&toFacingPorts, &ldPoint) && preferFacingPorts {
                        nextDistance += nonFacingPortsCost;
                    }

                    if ldIsPortOfTarget {
                        if !ldIsCenterPortOfTarget {
                            nextDistance += self.nonCenterPortCost;
                        } else {
                            if !nodeIsCenterSymmetricalPort.contains(&leastDistanceNode) {
                                nextDistance += 1.0;
                            }
                        }
                    }
                } else if adj.IsNodeCenter {
                    nextDistance = basicallyInfinity;
                } else {
                    // Reset overlappingEdges array for reuse
                    overlappingEdges.clear();
                    let mut isProhibitedSharing = false;

                    if !ldIsSource && !ldIsTarget {
                        if let Some(routesOnEdge) = self.overlappingRoutes.get(e) {
                            for route in routesOnEdge {
                                let r = route.b();
                                work.reserveProduct(r.OVGNodes.len() as u64 + 1, 2)?;
                                overlappingEdges.push(r.GEdge.clone());
                                if isDirected && r.isOpposingColinear(&leastDistanceNode, &adjacentNode) {
                                    isProhibitedSharing = true;
                                    break;
                                }
                                if adj.IsTunnel {
                                    // A tunnel route cannot overlap an entirely other route
                                    if r.isEntireColinear(&adjacentNode, &leastDistanceNode) || r.isEntireColinear(&leastDistanceNode, &adjacentNode) {
                                        isProhibitedSharing = true;
                                        break;
                                    }
                                }
                            }
                        }
                    }

                    if !isProhibitedSharing {
                        let canOverlap = edgeCanOverlapEdgesGuarded(gEdge, &overlappingEdges, Some(&sourceClusterNodes), Some(&targetClusterNodes), wk)?;
                        isProhibitedSharing = !overlappingEdges.is_empty() && !canOverlap;
                    }

                    let mut isAdjacentNodeOnRoute = false;
                    if !isProhibitedSharing && !ldIsSource && !ldIsTarget {
                        isAdjacentNodeOnRoute = self.pointToRoute.get(&adjPoint).map_or(false, |v| !v.is_empty());
                    }

                    if !isProhibitedSharing && !isOnNodeOfRoute && !isAdjacentNodeOnRoute && !ldIsSource {
                        if self.hasNearbyEdge.contains(e) {
                            isProhibitedSharing = true;
                        }
                    }

                    if isProhibitedSharing {
                        nextDistance = sharedRouteCost;
                    } else {
                        let mut isCrossing = false;
                        // There's two cases of crossing:
                        // 1. When an OVG node is at the intersection
                        // 2. When there isn't

                        // Case 1
                        if isOnNodeOfRoute && !isAdjacentNodeOnRoute && !areAllOccupiedRoutesShareable {
                            isCrossing = true;
                        }

                        // Case 2
                        if !isCrossing && !ldIsSource {
                            // If coming from source or going towards target, it'll intersect with other edges, and that's fine
                            // Even if node is not on a route, still have to check for if it's crossing while not sharing a node.
                            drop(adj);
                            isCrossing = self.edgeSet.intersectsWithGuarded(e, wk)?;
                        }

                        if isCrossing {
                            nextDistance += self.crossingCost;
                        }
                    }
                }
                let adj = adjacentNode.b();

                // Check intersection with positioned labels (avoiding allocation)
                let (ef, et) = (e.From.P(), e.To.P());
                for labelBox in &positionedLabelBoxes {
                    work.step()?;
                    if segmentIntersectsBox(&ef, &et, labelBox) {
                        nextDistance += basicallyInfinity;
                        break;
                    }
                }

                // Check intersection with node label boxes
                for labelBox in &nodeLabelBoxes {
                    work.step()?;
                    if segmentIntersectsBox(&ef, &et, labelBox) {
                        nextDistance += self.turnCost;
                    }
                }

                // Handle arrow labels
                if ldIsPortOfSource {
                    if sourceLabel.is_some() {
                        let routePoints = vec![geo::PointRef::new(ldPoint), geo::PointRef::new(adjPoint)];
                        let al = labeling::PositionArrowheadLabel(gEdge, false, &routePoints).expect("nil arrowhead label");
                        let labelCost = positionedArrowheadLabelCostGuarded(&al, &nonAncestors, &self.positionedLabels, &self.routes, &[], wk)?;
                        nextDistance += labelCost;
                    }
                }
                if adjIsPortOfTarget {
                    if targetLabel.is_some() {
                        let routePoints = vec![geo::PointRef::new(ldPoint), geo::PointRef::new(adjPoint)];
                        let al = labeling::PositionArrowheadLabel(gEdge, true, &routePoints).expect("nil arrowhead label");
                        let labelCost = positionedArrowheadLabelCostGuarded(&al, &nonAncestors, &self.positionedLabels, &self.routes, &[], wk)?;
                        nextDistance += labelCost;
                    }
                }

                // Don't count coming from source or heading to target as turns
                if let Some(lastNode) = &lastNode {
                    if (!adjIsTarget && !ldIsSource) && (*lastNode != source) {
                        let turns = (!isFromHorizontal && (adjPoint.X != ldPoint.X)) || (isFromHorizontal && (adjPoint.Y != ldPoint.Y));
                        if turns {
                            // Knock off a few points for turning at an ideal axis
                            let mut multiplier = 1.0f64;
                            for idealTurnAxis in &idealTurnAxes {
                                work.step()?;
                                if idealTurnAxis.isX {
                                    if math::Abs(adjPoint.X - idealTurnAxis.val) <= idealTurnAxisTolerance {
                                        multiplier = idealTurnMultiplier;
                                        break;
                                    }
                                } else {
                                    if math::Abs(adjPoint.Y - idealTurnAxis.val) <= idealTurnAxisTolerance {
                                        multiplier = idealTurnMultiplier;
                                        break;
                                    }
                                }
                            }
                            if multiplier != 1.0 {
                                if evenClusterTurn {
                                    multiplier = idealTurnEvenClusterMultiplier;
                                }
                            }
                            // Turns right or left
                            if !isFromHorizontal && (adjPoint.X != ldPoint.X) {
                                nextDistance += self.turnCost * multiplier;
                                // Not allowed to turn too close to the target, looks ugly
                                // but you gotta do what you gotta do for weird situations like child routing to container
                                if !(isSourceDescendantOfTarget || isTargetDescendantOfSource) {
                                    if leastDistanceNode.distanceToBoundary(&gTarget) <= turnEndpointClearance {
                                        nextDistance += self.turnCost;
                                    }
                                    if leastDistanceNode.distanceToBoundary(&gSource) <= turnEndpointClearance {
                                        nextDistance += self.turnCost;
                                    }
                                }
                            }
                            // Turns up or down
                            if isFromHorizontal && (adjPoint.Y != ldPoint.Y) {
                                nextDistance += self.turnCost * multiplier;
                                if !(isSourceDescendantOfTarget || isTargetDescendantOfSource) {
                                    if leastDistanceNode.distanceToBoundary(&gTarget) <= turnEndpointClearance {
                                        nextDistance += self.turnCost;
                                    }
                                    if leastDistanceNode.distanceToBoundary(&gSource) <= turnEndpointClearance {
                                        nextDistance += self.turnCost;
                                    }
                                }
                            }
                        }
                    }
                }

                // All else being equal, penalize taking routes that are too close to nodes
                if !ldIsSource && !adjIsTarget {
                    if let Some(near) = &adj.IsNearPort {
                        if !near.is_empty() {
                            if near.contains(&gSource) {
                                nextDistance += nodeProximityPenalty;
                            } else if near.contains(&gTarget) {
                                nextDistance += nodeProximityPenalty;
                            }
                        }
                    }
                }

                let maybeNewDistance = leastDistance + nextDistance;
                let adjIndex = adj.Index;
                drop(adj);

                // Create node context if it doesn't exist
                if self.nodeContext[adjIndex].is_none() {
                    self.nodeContext[adjIndex] = Some(searchNodeContext::fresh());
                }

                if adjPoint.Y == ldPoint.Y {
                    // Horizontal path
                    if maybeNewDistance < self.nodeContext[adjIndex].unwrap().horizontalDistance {
                        // If we already visited this node horizontally, enqueue the current hop
                        // to potentially be revisited
                        if let Some(hop) = self.horizontalHops[adjIndex].clone() {
                            let hopDistance = self.nodeContext[hop.Index()].expect("nil node context").horizontalDistance;
                            self.searchQueue.push(hopDistance, &hop, true, Some(wk))?;
                        }

                        // Create or update entry
                        match self.nodeContext[adjIndex].unwrap().horizontalEntry {
                            None => {
                                let horizontalEntry = self.searchQueue.push(maybeNewDistance, &adjacentNode, true, Some(wk))?;
                                self.nodeContext[adjIndex].as_mut().unwrap().horizontalEntry = Some(horizontalEntry);
                            }
                            Some(entry) => {
                                self.searchQueue.decrease(Some(entry), maybeNewDistance, Some(wk))?;
                            }
                        }

                        self.nodeContext[adjIndex].as_mut().unwrap().horizontalDistance = maybeNewDistance;
                        self.horizontalHops[adjIndex] = Some(leastDistanceNode.clone());
                    }
                } else {
                    // Vertical path
                    if maybeNewDistance < self.nodeContext[adjIndex].unwrap().verticalDistance {
                        if let Some(hop) = self.verticalHops[adjIndex].clone() {
                            let hopDistance = self.nodeContext[hop.Index()].expect("nil node context").verticalDistance;
                            self.searchQueue.push(hopDistance, &hop, false, Some(wk))?;
                        }

                        match self.nodeContext[adjIndex].unwrap().verticalEntry {
                            None => {
                                let verticalEntry = self.searchQueue.push(maybeNewDistance, &adjacentNode, false, Some(wk))?;
                                self.nodeContext[adjIndex].as_mut().unwrap().verticalEntry = Some(verticalEntry);
                            }
                            Some(entry) => {
                                self.searchQueue.decrease(Some(entry), maybeNewDistance, Some(wk))?;
                            }
                        }

                        self.nodeContext[adjIndex].as_mut().unwrap().verticalDistance = maybeNewDistance;
                        self.verticalHops[adjIndex] = Some(leastDistanceNode.clone());
                    }
                }
            }
        }

        Err(errorf!("path not found for '{}'. Queue is empty", gEdge.DebugID()))
    }

    /// If two non-diagonal nodes are below (MinRouteNodeClearance+1) * 2 from each other and connected, there won't be a direct path, and it'll take a roundabout path which can block other paths.
    /// To alleviate this, we directly connect fromFacingPort to toFacingPort, no need to search
    pub(crate) fn quickRoute(&mut self, ctx: &Ctx, gEdge: &EdgeRef, overlap: bool, fromOrientation: geo::Orientation) -> GoResult<Option<routeSearchResult>> {
        self.bindWork(ctx)?;
        checkEdgeRoutingCanceled(ctx)?;
        let originalGraph = self.graph.clone();
        let (gSource, gTarget) = {
            let e = gEdge.b();
            (e.From.clone(), e.To.clone())
        };
        let (source, target) = {
            let g = self.ovg.b();
            (g.Centers.get(&gSource).cloned().expect("missing source center"), g.Centers.get(&gTarget).cloned().expect("missing target center"))
        };
        if !overlap
            && !fromOrientation.IsDiagonal()
            && gSource.DistanceTo(&gTarget, true) < 2.0 * (layoutgraph::MinRouteNodeClearance + 1.0)
            && (gSource.DistanceTo(&gTarget, true) > layoutgraph::MinArrowheadClearance)
        {
            let work = self.w().clone();
            let mut edges: Vec<EdgeRef> = Vec::new();
            for route in &self.routes {
                let r = route.b();
                work.add(r.OVGNodes.len() as u64 + 1)?;
                let ge = r.GEdge.b();
                let edge = layoutgraph::NewEdge(&ge.From, &ge.To);
                {
                    let mut eb = edge.bm();
                    eb.Points = r.createSegmentEndpoints();
                    eb.SourceArrowhead = ge.SourceArrowhead.clone();
                    eb.TargetArrowhead = ge.TargetArrowhead.clone();
                    eb.SourceArrowheadLabel = ge.SourceArrowheadLabel.clone();
                    eb.TargetArrowheadLabel = ge.TargetArrowheadLabel.clone();
                    eb.Label = ge.Label.clone();
                }
                edges.push(edge);
            }
            checkEdgeRoutingCanceled(ctx)?;
            let w2 = work.clone();
            let mut checkWork = || w2.step();
            let r = routeLineChecked(&originalGraph, gEdge, &edges, &[], &mut checkWork, Some(&work as &dyn workBudget));

            match r {
                Ok((sourcePort, targetPort, lineCost)) => {
                    return Ok(Some((vec![source, NewOVGNode(*sourcePort.b()), NewOVGNode(*targetPort.b()), target], lineCost)));
                }
                Err(err) => {
                    if isContextTermination(&err) {
                        return Err(err);
                    }
                }
            }
        }
        Ok(None)
    }

    pub(crate) fn isUndesirableClusterArrangement(&self, gSource: &NodeRef, gTarget: &NodeRef) -> bool {
        // If the cluster nodes are in a bad arrangement for the gSource->gTarget edge, e.g. c1 and c2 to t
        // then they should not be treated as clusters for the purposes of route searching
        if let Some(c) = gSource.clusterField() {
            if !clusterHasDesirableArrangementTo(&c, gTarget) {
                return true;
            }
        }
        if let Some(c) = gTarget.clusterField() {
            if !clusterHasDesirableArrangementTo(&c, gSource) {
                return true;
            }
        }
        false
    }
}

pub(crate) fn clusterHasDesirableArrangementTo(cluster: &ClusterRef, node: &NodeRef) -> bool {
    let mut bounds = cluster.b().Vessel.clone();
    if !cluster.IsActive() {
        // The vessel may be out of sync after cluster placement. Cluster nodes
        // remain arranged in visual order, so their first and last boxes define
        // the same routing extent used by the original implementation.
        let nodes = cluster.b().Nodes.clone();
        let topLeft = nodes[0].tlv();
        let last = &nodes[nodes.len() - 1];
        let mut bottomRight = last.tlv();
        bottomRight.X += last.w();
        bottomRight.Y += last.h();
        let mut n = layoutgraph::Node::default();
        n.Box = geo::BoxRef::new(geo::Box { TopLeft: Some(geo::PointRef::new(topLeft)), Width: bottomRight.X - topLeft.X, Height: bottomRight.Y - topLeft.Y });
        bounds = NodeRef::new(n);
    }
    let o = bounds.Orientation(node);
    let arrangement = cluster.b().Arrangement.clone();
    if o == geo::Top || o == geo::Bottom {
        arrangement == layoutgraph::Row
    } else if o == geo::Left || o == geo::Right {
        arrangement == layoutgraph::Column
    } else {
        false
    }
}

impl ovgEdgeRouter {
    /// bestRoute finds the lowest-cost route to target, choosing between vertical
    /// and horizontal hops.
    pub(crate) fn bestRoute(&mut self, ctx: &Ctx, gSource: &NodeRef, gTarget: &NodeRef, source: &OVGNodeRef, target: &OVGNodeRef) -> GoResult<Vec<OVGNodeRef>> {
        self.bindWork(ctx)?;
        let work = self.w().clone();
        let mut sequence: Vec<OVGNodeRef> = Vec::new();

        let idealTurnAxes = idealTurnAxes(gSource, gTarget);
        let evenClusterTurn = match gSource.clusterField() {
            Some(c) => {
                let cb = c.b();
                cb.Arrangement == cb.DesiredArrangement && cb.Nodes.len() % 2 == 0
            }
            None => false,
        };

        let mut curr: Option<OVGNodeRef> = Some(target.clone());
        let mut prev: Option<OVGNodeRef> = None;
        let mut next: Option<OVGNodeRef> = None;
        let mut visited: Set<OVGNodeRef> = Set::default();
        while let Some(c) = curr.clone() {
            work.step()?;
            if visited.contains(&c) {
                // Cycle detected, take corrective action
                break;
            }
            visited.insert(c.clone());
            sequence.push(c.clone());

            let isHorizontal;
            let ci = c.Index();
            let entry = self.nodeContext[ci].expect("nil pointer dereference: node context");
            let verticalDistance = entry.verticalDistance;
            let horizontalDistance = entry.horizontalDistance;
            let (vh, hh) = (self.verticalHops[ci].clone(), self.horizontalHops[ci].clone());
            if vh.is_some() && hh.is_some() {
                if verticalDistance < horizontalDistance {
                    next = vh.clone();
                    isHorizontal = false;
                } else {
                    next = hh.clone();
                    isHorizontal = true;
                }
            } else if vh.is_some() {
                next = vh.clone();
                isHorizontal = false;
            } else {
                next = hh.clone();
                isHorizontal = true;
            }

            // Because the cost of a turn is baked into the node (node T) it turns into, and not the intersection (node I)
            // When another route comes in without a turn, it'll set lower weights from node T onwards, erasing the cost of turn,
            // But node I is not changed, and the best route would still be the one with the turn
            // So we reintroduce the cost to node I here.
            if let (Some(p), Some(nx)) = (&prev, &next) {
                if nx != source {
                    let (pp, cp) = (p.P(), c.P());
                    // If the next one leads to a turn ...
                    if ((pp.X == cp.X) && isHorizontal) || ((pp.Y == cp.Y) && !isHorizontal) {
                        let mut alternativeDistance = 0.0f64;
                        let mut alternative: Option<OVGNodeRef> = None;
                        let mut multiplier = 1.0f64;

                        for idealTurnAxis in &idealTurnAxes {
                            work.step()?;
                            if idealTurnAxis.isX {
                                if math::Abs(cp.X - idealTurnAxis.val) <= idealTurnAxisTolerance {
                                    multiplier = idealTurnMultiplier;
                                    break;
                                }
                            } else {
                                if math::Abs(cp.Y - idealTurnAxis.val) <= idealTurnAxisTolerance {
                                    multiplier = idealTurnMultiplier;
                                    break;
                                }
                            }
                        }
                        if multiplier != 1.0 {
                            if evenClusterTurn {
                                multiplier = idealTurnEvenClusterMultiplier;
                            }
                        }
                        // And the margin is only by turn distance ...
                        if isHorizontal && (geo::PrecisionCompare(horizontalDistance + self.turnCost * multiplier, verticalDistance, geo::PRECISION) > 0) {
                            // Then we should use the other direction
                            alternativeDistance = verticalDistance;
                            alternative = vh.clone();
                        }
                        if !isHorizontal && (geo::PrecisionCompare(verticalDistance + self.turnCost * multiplier, horizontalDistance, geo::PRECISION) > 0) {
                            alternativeDistance = horizontalDistance;
                            alternative = hh.clone();
                        }
                        // If alternative is basically infinity, it'll cycle
                        if alternative.is_some() && alternativeDistance < basicallyInfinity {
                            next = alternative;
                        }
                    }
                }
            }

            prev = Some(c);
            curr = next.clone();
        }
        let (mut left, mut right) = (0i64, sequence.len() as i64 - 1);
        while left < right {
            work.step()?;
            sequence.swap(left as usize, right as usize);
            left += 1;
            right -= 1;
        }
        Ok(sequence)
    }

    /// findOverlappingRoutes: OVG routes only overlap if both OVG nodes share the same route.
    pub(crate) fn findOverlappingRoutes(&mut self, start: &OVGNodeRef, end: &OVGNodeRef) -> GoResult<Vec<RouteRef>> {
        let mut overlappingRoutes: Vec<RouteRef> = Vec::new();
        let mut routeSet: Set<RouteRef> = Set::default();
        let work = self.w().clone();

        // find all overlapping edges
        let overlaps = self.edgeSet.overlappingEdgesGuarded(&NewOVGEdge(start, end), &work)?;
        let noRoutes: Vec<RouteRef> = Vec::new();
        for overlapping in &overlaps {
            work.step()?;
            for rf in self.pointToRoute.get(&overlapping.From.P()).unwrap_or(&noRoutes) {
                work.step()?;
                for rt in self.pointToRoute.get(&overlapping.To.P()).unwrap_or(&noRoutes) {
                    work.step()?;
                    // find the route that contains the edge
                    // only add the route if not seen before
                    if !routeSet.contains(rt) && rt == rf {
                        routeSet.insert(rt.clone());
                        overlappingRoutes.push(rt.clone());
                    }
                }
            }
        }
        Ok(overlappingRoutes)
    }

    pub(crate) fn addRoute(&mut self, route: &RouteRef) -> GoResult<()> {
        let work = self.w().clone();
        let r = route.b();
        let mut dups: Set<OVGNodeRef> = gocompat::NewSetCap(r.OVGNodes.len());
        let isLoop = r.GEdge.IsLoop();
        let routeNodes: &[OVGNodeRef] = if isLoop {
            // The source and target centers are the same node for a loop.
            &r.OVGNodes[1..]
        } else {
            &r.OVGNodes
        };
        for (i, n) in routeNodes.iter().enumerate() {
            work.step()?;
            if dups.contains(n) {
                let p = n.P();
                return Err(errorf!(
                    "found duplicate OVGNode (&{{X:{} Y:{}}}) at index {} in route {}",
                    gocompat::strconv::FormatFloatV(p.X),
                    gocompat::strconv::FormatFloatV(p.Y),
                    i,
                    r.GEdge.DebugID()
                ));
            }
            dups.insert(n.clone());
        }

        let (sourceLabel, targetLabel, invisible) = {
            let e = r.GEdge.b();
            (e.SourceArrowheadLabel.is_some(), e.TargetArrowheadLabel.is_some(), e.IsInvisible)
        };
        if sourceLabel {
            work.add(r.OVGNodes.len() as u64)?;
            let routePoints = r.createSegmentEndpoints();
            let al = labeling::PositionArrowheadLabel(&r.GEdge, false, &routePoints).expect("nil arrowhead label");
            self.positionedLabels.push(al);
        }
        if targetLabel {
            work.add(r.OVGNodes.len() as u64)?;
            let routePoints = r.createSegmentEndpoints();
            let al = labeling::PositionArrowheadLabel(&r.GEdge, true, &routePoints).expect("nil arrowhead label");
            self.positionedLabels.push(al);
        }

        self.routes.push(route.clone());
        self.routedEdges.push(r.GEdge.clone());
        let n = r.OVGNodes.len();
        for i in 0..n {
            work.step()?;
            let start = &r.OVGNodes[i];
            self.pointToRoute.entry(start.P()).or_default().push(route.clone());

            if i > 0 && i + 2 < n {
                // only keep edges of the OVG path, ignore edges from port to center
                let end = &r.OVGNodes[i + 1];
                self.edgeSet.addGuarded(&NewOVGEdge(start, end), &work)?;
            }
        }

        if !invisible {
            let ovg = self.ovg.b();
            let mut i = 1usize;
            while i + 2 < n {
                work.step()?;
                let routeNode = r.OVGNodes[i].P();
                let nextRouteNode = r.OVGNodes[i + 1].P();
                if routeNode.X == nextRouteNode.X {
                    // vertical
                    let (mut routeTop, mut routeBottom) = (routeNode.Y, nextRouteNode.Y);
                    if routeBottom < routeTop {
                        std::mem::swap(&mut routeTop, &mut routeBottom);
                    }
                    // Route coordinates may be fractional, so scan the indexed axes rather
                    // than iterating integer coordinates around routeNode.X.
                    for (xk, ovgEdges) in ovg.VerticalEdges.iter() {
                        work.step()?;
                        let x = f64::from_bits(*xk);
                        if math::Abs(x - routeNode.X) > pathNodeProximityFloor {
                            continue;
                        }
                        for ovgEdge in ovgEdges {
                            work.step()?;
                            let (mut edgeTop, mut edgeBottom) = (ovgEdge.From.Y(), ovgEdge.To.Y());
                            if edgeBottom < edgeTop {
                                std::mem::swap(&mut edgeTop, &mut edgeBottom);
                            }
                            if !(edgeBottom < routeTop || routeBottom < edgeTop) {
                                self.hasNearbyEdge.insert(ovgEdge.clone());
                                if x == routeNode.X {
                                    self.overlappingRoutes.entry(ovgEdge.clone()).or_default().push(route.clone());
                                }
                            }
                        }
                    }
                } else {
                    let (mut routeLeft, mut routeRight) = (routeNode.X, nextRouteNode.X);
                    if routeRight < routeLeft {
                        std::mem::swap(&mut routeLeft, &mut routeRight);
                    }
                    for (yk, ovgEdges) in ovg.HorizontalEdges.iter() {
                        work.step()?;
                        let y = f64::from_bits(*yk);
                        if math::Abs(y - routeNode.Y) > pathNodeProximityFloor {
                            continue;
                        }
                        for ovgEdge in ovgEdges {
                            work.step()?;
                            let (mut edgeLeft, mut edgeRight) = (ovgEdge.From.X(), ovgEdge.To.X());
                            if edgeRight < edgeLeft {
                                std::mem::swap(&mut edgeLeft, &mut edgeRight);
                            }
                            if !(edgeRight < routeLeft || routeRight < edgeLeft) {
                                self.hasNearbyEdge.insert(ovgEdge.clone());
                                if y == routeNode.Y {
                                    self.overlappingRoutes.entry(ovgEdge.clone()).or_default().push(route.clone());
                                }
                            }
                        }
                    }
                }
                i += 1;
            }
        }
        work.check()
    }

    /// tablePorts finds the ports to connect the columns from source to target.
    pub(crate) fn tablePorts(&mut self, edge: &EdgeRef, source: &NodeRef, target: &NodeRef, facingPorts: bool) -> GoResult<(Option<Set<OVGNodeRef>>, Option<Set<OVGNodeRef>>)> {
        let work = self.w().clone();
        let ovg = self.ovg.clone();
        let getPorts = |node: &NodeRef, columnIndex: i64, orientations: &[geo::Orientation]| -> GoResult<Set<OVGNodeRef>> {
            let mut ports: Set<OVGNodeRef> = Set::default();
            for &o in orientations {
                // Port-index derivation scans the node's incident edges to account for
                // loop offsets before indexing the already-bounded port slice.
                work.add(node.b().Edges.len() as u64 + 1)?;
                let shape = node.shape();
                let (mut portIndex, isTableSide) = nodeshape::TablePortIndex(&*shape, o, columnIndex);
                if !isTableSide {
                    portIndex = shape.PortIndices(o)[columnIndex as usize];
                }
                let p = ovg.b().Ports.get(node).expect("missing node ports")[portIndex as usize].clone();
                ports.insert(p);
            }
            Ok(ports)
        };

        let mut sourceOrientations: Vec<geo::Orientation> = Vec::new();
        let mut targetOrientations: Vec<geo::Orientation> = Vec::new();
        if !facingPorts {
            sourceOrientations = vec![geo::Right, geo::Left];
            targetOrientations = vec![geo::Right, geo::Left];
        } else {
            let o = source.Orientation(target);
            if o == geo::Left || o == geo::TopLeft || o == geo::BottomLeft {
                sourceOrientations = vec![geo::Right];
                targetOrientations = vec![geo::Left];
            } else if o == geo::Right || o == geo::TopRight || o == geo::BottomRight {
                sourceOrientations = vec![geo::Left];
                targetOrientations = vec![geo::Right];
            }
        }

        let (fromCol, toCol) = {
            let e = edge.b();
            (e.FromTableColumnIndex, e.ToTableColumnIndex)
        };
        let mut sourcePorts = None;
        let mut targetPorts = None;
        if let Some(c) = fromCol {
            sourcePorts = Some(getPorts(source, c, &sourceOrientations)?);
        }
        if let Some(c) = toCol {
            targetPorts = Some(getPorts(target, c, &targetOrientations)?);
        }
        Ok((sourcePorts, targetPorts))
    }
}
