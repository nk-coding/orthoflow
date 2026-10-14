// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/loops/loops.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;
use std::cell::RefCell;
use std::rc::Rc;

pub(crate) const sharedPortLoopGap: f64 = 30.0;

/// ComputeOffsets refreshes loop extents for every graph node.
pub fn ComputeOffsets(g: &layoutgraph::GraphRef) {
    let nodes = g.b().Nodes.clone();
    for node in &nodes {
        UpdateOffsets(node);
    }
}

/// UpdateOffsets refreshes the space reserved around node for self-edge routes.
pub fn UpdateOffsets(node: &NodeRef) {
    // need to set node TopLeft because the node position is required to get the ports
    // which are required for routing
    let tlIsNil = !node.has_tl();
    if tlIsNil {
        node.set_tl(Some(geo::NewPoint(0.0, 0.0)));
    }
    computeOffsets(node);
    if tlIsNil {
        node.set_tl(None);
    }
}

/// computeLoopOffsets precomputes loops and their contribution to node bounds
/// for later delta and bounding-box calculations.
pub(crate) fn computeOffsets(node: &NodeRef) {
    node.bm().LoopOffsets = gocompat::Map::default();

    let (tl, br) = node.BoundingBox(None);
    let (loopTL, loopBR) = node.BoundingBox(None);
    let tl = *tl.b();
    let br = *br.b();
    let mut loopTL = *loopTL.b();
    let mut loopBR = *loopBR.b();
    for edge in routeLoops(node) {
        if !edge.IsLoop() {
            continue;
        }
        let (eTL, eBR) = edge.BoundingBoxValues();
        loopTL.X = math::Min(loopTL.X, eTL.X);
        loopTL.Y = math::Min(loopTL.Y, eTL.Y);
        loopBR.X = math::Max(loopBR.X, eBR.X);
        loopBR.Y = math::Max(loopBR.Y, eBR.Y);
        // need to reset because if these are set based on `0, 0`, graph bounding box breaks during placement
        edge.bm().Points = Vec::new();
    }

    if nonNilEquals(Some(&tl), Some(&loopTL)) && nonNilEquals(Some(&br), Some(&loopBR)) {
        return;
    }

    let mut n = node.bm();
    let lo = &mut n.LoopOffsets;
    lo.insert(geo::Left, math::Round(math::Abs(tl.X - loopTL.X)));
    lo.insert(geo::Right, math::Round(math::Abs(br.X - loopBR.X)));
    lo.insert(geo::Top, math::Round(math::Abs(tl.Y - loopTL.Y)));
    lo.insert(geo::Bottom, math::Round(math::Abs(br.Y - loopBR.Y)));

    let g = |lo: &gocompat::Map<geo::Orientation, f64>, o| lo.get(&o).copied().unwrap_or(0.0);
    let v = math::Max(g(lo, geo::Top), g(lo, geo::Left));
    lo.insert(geo::TopLeft, v);
    let v = math::Max(g(lo, geo::Top), g(lo, geo::Right));
    lo.insert(geo::TopRight, v);
    let v = math::Max(g(lo, geo::Bottom), g(lo, geo::Left));
    lo.insert(geo::BottomLeft, v);
    let v = math::Max(g(lo, geo::Bottom), g(lo, geo::Right));
    lo.insert(geo::BottomRight, v);
}

/// loopPorts works as sort of context for a given ports pair while routing loops
pub(crate) struct loopPorts {
    pub(crate) verticalOffset: f64,
    pub(crate) horizontalOffset: f64,
    pub(crate) hasSourceArrowhead: Option<bool>,
    pub(crate) hasTargetArrowhead: Option<bool>,
    pub(crate) sourceArrowhead: layoutgraph::Arrowhead,
    pub(crate) targetArrowhead: layoutgraph::Arrowhead,
    pub(crate) from: geo::PointRef,
    pub(crate) fromDirection: geo::Orientation,
    pub(crate) to: geo::PointRef,
    pub(crate) toDirection: geo::Orientation,
}

pub(crate) type loopPortsRef = Rc<RefCell<loopPorts>>;

pub(crate) fn newLoopPorts(
    from: geo::PointRef,
    to: geo::PointRef,
    fromDirection: geo::Orientation,
    toDirection: geo::Orientation,
) -> loopPortsRef {
    Rc::new(RefCell::new(loopPorts {
        verticalOffset: 0.0,
        horizontalOffset: 0.0,
        hasSourceArrowhead: None,
        hasTargetArrowhead: None,
        sourceArrowhead: Default::default(),
        targetArrowhead: Default::default(),
        from,
        fromDirection,
        to,
        toDirection,
    }))
}

/// Route computes every self-edge route for node.
pub fn Route(n: &NodeRef) -> Vec<EdgeRef> {
    routeLoops(n)
}

/// routeLoops routes loops in a rule based approach and sets the proper label position
pub(crate) fn routeLoops(n: &NodeRef) -> Vec<EdgeRef> {
    let availablePorts = makeLoopPorts(n);

    let mut routedEdges = Vec::new();
    for e in edgesInOrder(&n.edges()) {
        if !e.IsLoop() {
            continue;
        }

        routedEdges.push(e.clone());
        let portsPair = findLoopPortsForEdge(&e, &availablePorts)
            .expect("nil pointer dereference: loop ports");
        let pts = routeLoop(&portsPair.borrow());
        e.bm().Points = pts;
        if let Some(l) = e.b().Label.clone() {
            l.bm().Position = label::OutsideTopCenter;
        }
        let (v, h) = computeEdgeOffset(&e);
        let mut pp = portsPair.borrow_mut();
        pp.verticalOffset = v;
        pp.horizontalOffset = h;
        pp.hasSourceArrowhead = Some(e.HasSourceArrow());
        pp.hasTargetArrowhead = Some(e.HasTargetArrow());
        pp.sourceArrowhead = e.b().SourceArrowhead.clone();
        pp.targetArrowhead = e.b().TargetArrowhead.clone();
    }
    routedEdges
}

fn labelArea(e: &EdgeRef) -> f64 {
    match e.b().Label.clone() {
        Some(l) => {
            let l = l.b();
            l.Width * l.Height
        }
        None => 0.0,
    }
}

/// edgesInOrder sorts edges based on their arrowheads and label sizes
pub(crate) fn edgesInOrder(edges: &[EdgeRef]) -> Vec<EdgeRef> {
    let mut sortedEdges: Vec<EdgeRef> = edges.to_vec();
    let arrowheadScore = |e: &EdgeRef| -> i64 {
        if e.HasSourceArrow() != e.HasTargetArrow() {
            0
        } else if e.HasSourceArrow() {
            1
        } else {
            2
        }
    };
    gocompat::slices::SortStableFunc(&mut sortedEdges, |a, b| -> i64 {
        if a.HasSourceArrow() != b.HasSourceArrow() || a.HasTargetArrow() != b.HasTargetArrow() {
            let aScore = arrowheadScore(a);
            let bScore = arrowheadScore(b);
            return if aScore < bScore {
                -1
            } else if bScore < aScore {
                1
            } else {
                0
            };
        }
        let aArea = labelArea(a);
        let bArea = labelArea(b);
        if aArea < bArea {
            -1
        } else if bArea < aArea {
            1
        } else {
            0
        }
    });
    sortedEdges
}

/// findLoopPortsForEdge finds the ports that are using the same arrowheads in `availablePorts`
pub(crate) fn findLoopPortsForEdge(e: &EdgeRef, availablePorts: &[loopPortsRef]) -> Option<loopPortsRef> {
    let (sa, ta) = {
        let eb = e.b();
        (eb.SourceArrowhead.clone(), eb.TargetArrowhead.clone())
    };
    for ports in availablePorts {
        let p = ports.borrow();
        if p.hasSourceArrowhead.is_some() && p.hasTargetArrowhead.is_some() {
            if p.sourceArrowhead == sa && p.targetArrowhead == ta {
                return Some(ports.clone());
            }
        }
    }
    for ports in availablePorts {
        let p = ports.borrow();
        if p.hasSourceArrowhead.is_none() && p.hasTargetArrowhead.is_none() {
            return Some(ports.clone());
        }
    }
    for ports in availablePorts {
        let p = ports.borrow();
        if let (Some(hs), Some(ht)) = (p.hasSourceArrowhead, p.hasTargetArrowhead) {
            if hs == e.HasSourceArrow() && ht == e.HasTargetArrow() {
                return Some(ports.clone());
            }
        }
    }
    None
}

/// makeLoopPorts makes the 4 possible port pairs, one at each corner.
pub(crate) fn makeLoopPorts(n: &NodeRef) -> Vec<loopPortsRef> {
    let ports = n.Ports();
    let shape = n.shape();
    let topIndices = shape.PortIndices(geo::Top);
    let rightIndices = shape.PortIndices(geo::Right);
    let bottomIndices = shape.PortIndices(geo::Bottom);
    let leftIndices = shape.PortIndices(geo::Left);

    let (trTop, trRight) = closestPorts(&ports, &topIndices, &rightIndices);
    let (blBottom, blLeft) = closestPorts(&ports, &bottomIndices, &leftIndices);
    let (ltLeft, ltTop) = closestPorts(&ports, &leftIndices, &topIndices);
    let (brRight, brBottom) = closestPorts(&ports, &rightIndices, &bottomIndices);

    vec![
        newLoopPorts(ltLeft, ltTop, geo::Left, geo::Top),
        newLoopPorts(trTop, trRight, geo::Top, geo::Right),
        newLoopPorts(blBottom, blLeft, geo::Bottom, geo::Left),
        newLoopPorts(brRight, brBottom, geo::Right, geo::Bottom),
    ]
}

pub(crate) fn closestPorts(ports: &[geo::Point], sideA: &[i64], sideB: &[i64]) -> (geo::PointRef, geo::PointRef) {
    let mut closestA = geo::Point::default();
    let mut closestB = geo::Point::default();
    let mut minDistance = math::Inf(1);

    for &indexA in sideA {
        for &indexB in sideB {
            let pa = ports[indexA as usize];
            let pb = ports[indexB as usize];
            let d = geo::EuclideanDistance(pa.X, pa.Y, pb.X, pb.Y);
            if d < minDistance {
                minDistance = d;
                closestA = pa;
                closestB = pb;
            }
        }
    }

    (geo::PointRef::new(closestA), geo::PointRef::new(closestB))
}

/// routeLoop creates the 5-point loop route.
pub(crate) fn routeLoop(ports: &loopPorts) -> Vec<geo::PointRef> {
    let p1 = makeBendPoint(&ports.from, ports.fromDirection, ports.verticalOffset, ports.horizontalOffset)
        .expect("nil bend point");
    let p2 = makeBendPoint(&ports.to, ports.toDirection, ports.verticalOffset, ports.horizontalOffset)
        .expect("nil bend point");
    let intersection = makeIntersectionPoint(&p1, &p2, ports.fromDirection);

    vec![ports.from.clone(), p1, intersection, p2, ports.to.clone()]
}

pub(crate) fn makeBendPoint(
    port: &geo::PointRef,
    direction: geo::Orientation,
    verticalOffset: f64,
    horizontalOffset: f64,
) -> Option<geo::PointRef> {
    let p = *port.b();
    if direction == geo::Top {
        return Some(geo::NewPoint(p.X, p.Y - verticalOffset - sharedPortLoopGap));
    }
    if direction == geo::Bottom {
        return Some(geo::NewPoint(p.X, p.Y + verticalOffset + sharedPortLoopGap));
    }
    if direction == geo::Left {
        return Some(geo::NewPoint(p.X - horizontalOffset - sharedPortLoopGap, p.Y));
    }
    if direction == geo::Right {
        return Some(geo::NewPoint(p.X + horizontalOffset + sharedPortLoopGap, p.Y));
    }
    None
}

pub(crate) fn makeIntersectionPoint(from: &geo::PointRef, to: &geo::PointRef, fromDirection: geo::Orientation) -> geo::PointRef {
    let (f, t) = (*from.b(), *to.b());
    if !fromDirection.IsVertical() {
        return geo::NewPoint(f.X, t.Y);
    }
    geo::NewPoint(t.X, f.Y)
}

/// computeEdgeOffset computes the offset to place the next loop considering the current `edge` and its label
pub(crate) fn computeEdgeOffset(edge: &EdgeRef) -> (f64, f64) {
    let verticalOffset;
    let horizontalOffset;
    let points: Vec<geo::Point> = edge.b().Points.iter().map(|p| *p.b()).collect();
    let n = points.len();
    if points[0].X == points[1].X {
        verticalOffset = math::Abs(points[0].Y - points[1].Y);
        horizontalOffset = math::Abs(points[n - 1].X - points[n - 2].X);
    } else {
        verticalOffset = math::Abs(points[n - 1].Y - points[n - 2].Y);
        horizontalOffset = math::Abs(points[0].X - points[1].X);
    }
    let (mut v, mut h) = (verticalOffset, horizontalOffset);
    if let Some(l) = edge.b().Label.clone() {
        let l = l.b();
        v += l.Height;
        h += l.Width;
    }
    (v, h)
}
