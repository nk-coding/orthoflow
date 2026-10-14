// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/routing/swap_ports.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of routing/swap_ports.go.
#[allow(unused_imports)]
use crate::*;
use gocompat::context::Ctx;
use gocompat::{GoError, GoResult, Map, Set};
use layoutgraph::{EdgeRef, GraphRef, NodeRef};

// SwapAllEdgePorts applies port swapping to every node under one atomic
// route-stage budget and rollback boundary.
pub fn SwapAllEdgePorts(ctx: &Ctx, graph: &GraphRef) -> GoResult<()> {
    swapAllEdgePortsWithWorkLimit(ctx, graph, maxRouteStageWorkUnits)
}

pub(crate) fn swapAllEdgePortsWithWorkLimit(ctx: &Ctx, graph: &GraphRef, workLimit: u64) -> GoResult<()> {
    runAtomicRouteStage(ctx, "SwapEdgePorts", graph, &[], workLimit, |guard| {
        for node in graph.nodes() {
            guard.step()?;
            swapEdgePortsGuarded(&node, guard)?;
        }
        Ok(())
    })
}

/// Go `map[geo.Orientation][]*layoutgraph.Edge`. Go ranges over it in random
/// order; each orientation's work touches only its own edges, so a fixed
/// order is equivalent.
pub(crate) type orientationEdges = Map<geo::Orientation, Vec<EdgeRef>>;

const orientationOrder: [geo::Orientation; 4] = [geo::Top, geo::Right, geo::Bottom, geo::Left];

pub(crate) fn swapEdgePortsGuarded(n: &NodeRef, guard: &routeWorkGuardRef) -> GoResult<bool> {
    let mut orientationToEdges: orientationEdges = Map::default();
    let mut portToEdges: Map<geo::Point, Vec<EdgeRef>> = Map::default();
    for e in n.edges() {
        guard.step()?;
        // TODO: remove `len(e.Points) < 3` and handle straight edges
        if e.IsLoop() || e.b().Points.len() < 3 || e.HasTableColumn() {
            continue;
        }
        if e.HasLargeArrowheadLabel() {
            continue;
        }
        let (p1, p2, _) = pointsAt(&e, n);
        let (p1, p2) = (p1.get(), p2.get());
        let o = if p1.X < p2.X {
            geo::Right
        } else if p1.X > p2.X {
            geo::Left
        } else if p1.Y < p2.Y {
            geo::Bottom
        } else {
            geo::Top
        };
        orientationToEdges.entry(o).or_default().push(e.clone());
        portToEdges.entry(p1).or_default().push(e.clone());
    }

    let swappedSameSide = swapEdgesOnSameSide(n, &mut orientationToEdges, &portToEdges, guard)?;
    let swappedAdjacentSides = swapEdgesOnAdjacentSides(n, &orientationToEdges, &portToEdges, guard)?;

    Ok(swappedAdjacentSides || swappedSameSide)
}

// swapEdgesOnSameSide swaps edges on the same node side if they can be swapped
pub(crate) fn swapEdgesOnSameSide(
    n: &NodeRef,
    orientationToEdges: &mut orientationEdges,
    portToEdges: &Map<geo::Point, Vec<EdgeRef>>,
    guard: &routeWorkGuardRef,
) -> GoResult<bool> {
    let mut swapped = false;
    for o in orientationOrder {
        let Some(edges) = orientationToEdges.get_mut(&o) else { continue };
        guard.step()?;
        // sorts by orientation
        if o.IsVertical() {
            stableSortRouteValues(
                edges,
                |a, b| {
                    let (p1, _, p3) = pointsAt(a, n);
                    let (q1, _, q3) = pointsAt(b, n);
                    if p1.X() == q1.X() {
                        return p3.X() < q3.X();
                    }
                    p1.X() < q1.X()
                },
                guard,
            )?;
        } else {
            stableSortRouteValues(
                edges,
                |a, b| {
                    let (p1, _, p3) = pointsAt(a, n);
                    let (q1, _, q3) = pointsAt(b, n);
                    if p1.Y() == q1.Y() {
                        return p3.Y() < q3.Y();
                    }
                    p1.Y() < q1.Y()
                },
                guard,
            )?;
        }
        let mut i = 0usize;
        while i + 1 < edges.len() {
            guard.step()?;
            if edgesIntersect(&edges[i], &edges[i + 1], n, o) && canSwapPorts(&edges[i], &edges[i + 1], n, portToEdges) {
                let (p1, p2, _) = pointsAt(&edges[i], n);
                let (q1, q2, _) = pointsAt(&edges[i + 1], n);
                swapCoordinate(&p1, &q1, o);
                swapCoordinate(&p2, &q2, o);
                let doesNotPass =
                    doesNotPassThroughNodesGuarded(&n.g(), &[n.clone()], &p1.get(), &p2.get(), &q1.get(), &q2.get(), guard)?;
                if doesNotPass {
                    edges.swap(i, i + 1);
                    swapped = true;
                } else {
                    swapCoordinate(&p1, &q1, o);
                    swapCoordinate(&p2, &q2, o);
                }
            }
            i += 1;
        }
    }
    Ok(swapped)
}

// pointsAt returns the points composing the two segments starting at n.
pub(crate) fn pointsAt(e: &EdgeRef, n: &NodeRef) -> (geo::PointRef, geo::PointRef, geo::PointRef) {
    let e = e.b();
    if e.From == *n {
        return (e.Points[0].clone(), e.Points[1].clone(), e.Points[2].clone());
    }
    let l = e.Points.len();
    (e.Points[l - 1].clone(), e.Points[l - 2].clone(), e.Points[l - 3].clone())
}

// swapCoordinate swaps the coordinate (x/y) opposite to the given orienation, e.g., if `o` is vertical, swaps x
pub(crate) fn swapCoordinate(p: &geo::PointRef, q: &geo::PointRef, o: geo::Orientation) {
    let (pv, qv) = (p.get(), q.get());
    if o.IsVertical() {
        p.bm().X = qv.X;
        q.bm().X = pv.X;
    } else {
        p.bm().Y = qv.Y;
        q.bm().Y = pv.Y;
    }
}

pub(crate) fn doesNotPassThroughNodesGuarded(
    g: &GraphRef,
    ns: &[NodeRef],
    p1: &geo::Point,
    p2: &geo::Point,
    q1: &geo::Point,
    q2: &geo::Point,
    guard: &routeWorkGuardRef,
) -> GoResult<bool> {
    'OUTER: for node in g.nodes() {
        guard.step()?;
        for n in ns {
            guard.step()?;
            if node == *n || n.IsDescendantOf(Some(&node)) {
                continue 'OUTER;
            }
        }
        let pad = 2.0 * pathNodeProximityFloor;
        let tl = node.tlv();
        let paddedBox = geo::Box {
            TopLeft: Some(geo::NewPoint(tl.X - pad, tl.Y - pad)),
            Width: node.w() + pad * 2.0,
            Height: node.h() + pad * 2.0,
        };
        if segmentIntersectsBox(p1, p2, &paddedBox) || segmentIntersectsBox(q1, q2, &paddedBox) {
            return Ok(false);
        }
    }
    Ok(true)
}

// edgesIntersect checks if the given edges intersect at `n`
pub(crate) fn edgesIntersect(e1: &EdgeRef, e2: &EdgeRef, n: &NodeRef, o: geo::Orientation) -> bool {
    let (p1, p2, p3) = pointsAt(e1, n);
    let (q1, q2, q3) = pointsAt(e2, n);
    let (p1, p2, p3) = (p1.get(), p2.get(), p3.get());
    let (q1, q2, q3) = (q1.get(), q2.get(), q3.get());
    match o {
        geo::Left => {
            let bottomIntersection = q2.X < p2.X && p3.Y > q1.Y;
            let topIntersection = p2.X < q2.X && q3.Y < p1.Y;
            topIntersection || bottomIntersection
        }
        geo::Bottom => {
            let leftIntersection = p2.Y > q2.Y && q3.X < p1.X;
            let rightIntersection = q2.Y > p2.Y && p3.X > q1.X;
            leftIntersection || rightIntersection
        }
        geo::Top => {
            let leftIntersection = p2.Y < q2.Y && q3.X < p1.X;
            let rightIntersection = q2.Y < p2.Y && p3.X > q1.X;
            leftIntersection || rightIntersection
        }
        geo::Right => {
            let topIntersection = p2.X > q2.X && q3.Y < p1.Y;
            let bottomIntersection = q2.X > p2.X && p3.Y > q1.Y;
            topIntersection || bottomIntersection
        }
        _ => false,
    }
}

// canSwapPorts checks if the ports of the given edges can be swapped
pub(crate) fn canSwapPorts(e1: &EdgeRef, e2: &EdgeRef, n: &NodeRef, portToEdges: &Map<geo::Point, Vec<EdgeRef>>) -> bool {
    let (p1, _, _) = pointsAt(e1, n);
    let (p2, _, _) = pointsAt(e2, n);
    let count = |p: &geo::PointRef| portToEdges.get(&p.get()).map_or(0, |v| v.len());

    if count(&p1) == 1 && count(&p2) == 1 {
        // only one edge at each port, so arrowheads don't matter
        return true;
    }

    // check if they have the same arrowheads
    let p1Arrowhead = e1.ArrowheadTo(n);
    let p2Arrowhead = e2.ArrowheadTo(n);
    p1Arrowhead == p2Arrowhead
}

pub(crate) fn refineEdgeGuarded(g: &GraphRef, e: &EdgeRef, guard: &routeWorkGuardRef) -> GoResult<bool> {
    guard.step()?;
    if e.IsLoop() || e.b().Points.len() <= 2 || e.IsBetweenTableColumns() || e.IsTreeEdge() {
        return Ok(false);
    }
    let mut occupiedPorts = occupiedPortsGuarded(&e.from(), guard)?;
    let toPorts = occupiedPortsGuarded(&e.to(), guard)?;
    for port in toPorts.iter() {
        guard.step()?;
        occupiedPorts.insert(*port);
    }

    if isEdgeSShaped(e) {
        return makeStraightLineGuarded(g, e, &occupiedPorts, guard);
    } else if isEdgeLShaped(e) {
        return makeStraightLineGuarded(g, e, &occupiedPorts, guard);
    } else if isEdgeUShaped(e) {
        return Ok(false);
    }

    let mut swapped = false;
    let points = e.points();
    if isSShaped(&points[..4]) {
        // the first 3 segments are S shaped, try to turn them into L shaped
        let p = sToLShapedBendPoint(&points[..4]);
        let doesNotPass = doesNotPassThroughNodesGuarded(
            g,
            &[e.from(), e.to()],
            &points[0].get(),
            &p.get(),
            &p.get(),
            &points[3].get(),
            guard,
        )?;
        if doesNotPass {
            removeEdgePointsGuarded(e, guard, &[1, 2, 3])?;
            insertEdgePointGuarded(e, &p, 1, guard)?;
            swapped = true;
        }
    }

    Ok(swapped)
}

pub(crate) fn occupiedPortsGuarded(n: &NodeRef, guard: &routeWorkGuardRef) -> GoResult<Set<geo::Point>> {
    let mut ports: Set<geo::Point> = Set::default();
    for e in n.edges() {
        guard.step()?;
        if e.from() == *n {
            ports.insert(e.SourcePort().expect("nil pointer dereference").get());
        } else {
            ports.insert(e.TargetPort().expect("nil pointer dereference").get());
        }
    }
    Ok(ports)
}

pub(crate) fn makeStraightLineGuarded(
    g: &GraphRef,
    e: &EdgeRef,
    occupiedPorts: &Set<geo::Point>,
    guard: &routeWorkGuardRef,
) -> GoResult<bool> {
    let fromPort = e.SourcePort().expect("nil pointer dereference").get();
    let toPort = e.TargetPort().expect("nil pointer dereference").get();
    let step = pathNodeProximityFloor;
    let (ranges, isHorizontal) = tunnelRangesBetweenGuarded(g, &e.from(), &e.to(), false, guard)?;
    for r in ranges.unwrap_or_default().iter() {
        guard.step()?;
        let (rstart, rend) = (r.start, r.end);
        // avoid having edges alined with the node boundaries
        let mut v = rstart + step;
        while v <= rend - step {
            guard.step()?;
            let (from, to) = if isHorizontal {
                (geo::Point { X: fromPort.X, Y: v }, geo::Point { X: toPort.X, Y: v })
            } else {
                (geo::Point { X: v, Y: fromPort.Y }, geo::Point { X: v, Y: toPort.Y })
            };
            if occupiedPorts.contains(&from) || occupiedPorts.contains(&to) {
                v += step;
                continue;
            }
            // the ports are free and tunnels ensure there's no node in between
            // so we can make this edge a straight line
            e.bm().Points = vec![geo::PointRef::new(from), geo::PointRef::new(to)];
            return Ok(true);
        }
    }
    Ok(false)
}

// sToLShapedBendPoint returns the bend point that transforms an S-shaped route
// into an L-shaped route.
pub(crate) fn sToLShapedBendPoint(ps: &[geo::PointRef]) -> geo::PointRef {
    if ps[0].X() == ps[1].X() {
        // vertical
        return geo::NewPoint(ps[1].X(), ps[3].Y());
    }
    geo::NewPoint(ps[3].X(), ps[1].Y())
}

pub(crate) fn isEdgeUShaped(e: &EdgeRef) -> bool {
    isUShaped(&e.points())
}

pub(crate) fn isEdgeSShaped(e: &EdgeRef) -> bool {
    isSShaped(&e.points())
}

pub(crate) fn isSShaped(ps: &[geo::PointRef]) -> bool {
    ps.len() == 4 && !isUShaped(ps)
}

pub(crate) fn isUShaped(ps: &[geo::PointRef]) -> bool {
    if ps.len() != 4 {
        return false;
    }
    let (p0, p1, p2, p3) = (ps[0].get(), ps[1].get(), ps[2].get(), ps[3].get());

    if p0.X == p1.X {
        let sameDirection = geo::Sign(p0.Y - p1.Y) == geo::Sign(p3.Y - p2.Y);
        let isHorizontal = p1.Y == p2.Y;
        return sameDirection && isHorizontal;
    }

    let sameDirection = geo::Sign(p0.X - p1.X) == geo::Sign(p3.X - p2.X);
    let isVertical = p1.X == p2.X;
    sameDirection && isVertical
}

pub(crate) fn isEdgeLShaped(e: &EdgeRef) -> bool {
    isLShaped(&e.points())
}

pub(crate) fn isLShaped(ps: &[geo::PointRef]) -> bool {
    if ps.len() != 3 {
        return false;
    }

    let isVertical = ps[0].X() == ps[1].X();
    let isHorizontal = ps[1].Y() == ps[2].Y();
    (isVertical && isHorizontal) || (!isVertical && !isHorizontal)
}

// swapPoints swaps both X and Y of the given points
pub(crate) fn swapPoints(p1: &geo::PointRef, p2: &geo::PointRef) {
    let (a, b) = (p1.get(), p2.get());
    p1.set(geo::Point { X: b.X, Y: b.Y });
    p2.set(geo::Point { X: a.X, Y: a.Y });
}

pub(crate) fn copyEdgePointsGuarded(edge: &EdgeRef, guard: &routeWorkGuardRef) -> GoResult<Vec<geo::PointRef>> {
    let mut points = Vec::with_capacity(edge.b().Points.len());
    for point in edge.points() {
        guard.step()?;
        points.push(point.Copy());
    }
    Ok(points)
}

pub(crate) fn insertEdgePointGuarded(edge: &EdgeRef, point: &geo::PointRef, index: usize, guard: &routeWorkGuardRef) -> GoResult<()> {
    let existing = edge.points();
    if index > existing.len() {
        return Err(GoError::new(format!(
            "TALA {} route insertion index {} is out of bounds",
            guard.location(),
            index
        )));
    }
    let mut points = Vec::with_capacity(existing.len() + 1);
    for (pointIndex, e) in existing.iter().enumerate() {
        guard.step()?;
        if pointIndex == index {
            points.push(point.clone());
        }
        points.push(e.clone());
    }
    if index == existing.len() {
        points.push(point.clone());
    }
    edge.bm().Points = points;
    Ok(())
}

pub(crate) fn removeEdgePointsGuarded(edge: &EdgeRef, guard: &routeWorkGuardRef, indices: &[usize]) -> GoResult<()> {
    let existing = edge.points();
    if existing.len() < indices.len() {
        return Err(GoError::new(format!("TALA {} route removal count exceeds route length", guard.location())));
    }
    let mut points = Vec::with_capacity(existing.len() - indices.len());
    'OUTER: for (pointIndex, point) in existing.iter().enumerate() {
        guard.step()?;
        for &removeIndex in indices {
            guard.step()?;
            if pointIndex == removeIndex {
                continue 'OUTER;
            }
        }
        points.push(point.clone());
    }
    edge.bm().Points = points;
    Ok(())
}

// swapEdgesOnAdjacentSides swaps edges on adjacent sides (top/bottom with left/right) if they can be swapped
pub(crate) fn swapEdgesOnAdjacentSides(
    n: &NodeRef,
    orientationToEdges: &orientationEdges,
    portToEdges: &Map<geo::Point, Vec<EdgeRef>>,
    guard: &routeWorkGuardRef,
) -> GoResult<bool> {
    let mut swapped = false;
    let get = |o: geo::Orientation| orientationToEdges.get(&o).cloned().unwrap_or_default();
    let mut topBottomEdges = get(geo::Top);
    topBottomEdges.extend(get(geo::Bottom));
    let mut leftRightEdges = get(geo::Left);
    leftRightEdges.extend(get(geo::Right));
    for aEdge in &topBottomEdges {
        guard.step()?;
        for bEdge in &leftRightEdges {
            guard.step()?;
            // we need to check the edge size here in case we were able to improve it
            if aEdge.b().Points.len() <= 2 {
                break;
            }
            if bEdge.b().Points.len() <= 2 {
                continue;
            }
            if !canSwapPorts(aEdge, bEdge, n, portToEdges) {
                continue;
            }
            let (a1, a2, a3) = pointsAt(aEdge, n);
            let (b1, b2, b3) = pointsAt(bEdge, n);

            let Some(intersection) = geo::IntersectionPoint(&a2, &a3, &b2, &b3) else { continue };

            let aPoints = copyEdgePointsGuarded(aEdge, guard)?;
            let bPoints = copyEdgePointsGuarded(bEdge, guard)?;

            // swap the first segment of the edges
            swapPoints(&a1, &b1);
            swapPoints(&a2, &b2);

            let intersectionOffset = pathNodeProximityFloor / 2.0;
            // insert the intersection
            let aIntersection = intersection.Copy();
            if aEdge.from() == *n {
                insertEdgePointGuarded(aEdge, &aIntersection, 2, guard)?;
            } else {
                let at = aEdge.b().Points.len() - 2;
                insertEdgePointGuarded(aEdge, &aIntersection, at, guard)?;
            }
            // shifts the intersection segment a bit so that it doesn't look like a crossing anymore
            if aIntersection.X() == a2.X() {
                aIntersection.bm().X += intersectionOffset;
                a2.bm().X += intersectionOffset;
            } else {
                aIntersection.bm().Y -= intersectionOffset;
                a2.bm().Y -= intersectionOffset;
            }

            // same as above, but for the other edge
            let bIntersection = intersection.Copy();
            if bEdge.from() == *n {
                insertEdgePointGuarded(bEdge, &bIntersection, 2, guard)?;
            } else {
                let at = bEdge.b().Points.len() - 2;
                insertEdgePointGuarded(bEdge, &bIntersection, at, guard)?;
            }
            if bIntersection.X() == b2.X() {
                bIntersection.bm().X += intersectionOffset;
                b2.bm().X += intersectionOffset;
            } else {
                bIntersection.bm().Y -= intersectionOffset;
                b2.bm().Y -= intersectionOffset;
            }

            let aImproved = refineEdgeGuarded(&n.g(), aEdge, guard)?;
            let bImproved = refineEdgeGuarded(&n.g(), bEdge, guard)?;
            if !aImproved && !bImproved {
                // if none of the edges improved after splitting at the intersection
                // return back as we got 2 extra bends in the place of a single intersection
                aEdge.bm().Points = aPoints;
                bEdge.bm().Points = bPoints;
            } else {
                swapped = true;
            }
        }
    }
    Ok(swapped)
}
