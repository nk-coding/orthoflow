// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/routing/route.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of routing/route.go.
#[allow(unused_imports)]
use crate::*;
use gocompat::GoResult;
use layoutgraph::EdgeRef;

pub struct Route {
    pub GEdge: EdgeRef,
    pub OVGNodes: Vec<OVGNodeRef>,
    pub FromPort: geo::Point,
    pub ToPort: geo::Point,
}

gocompat::go_ptr!(RouteRef, Route);

impl Route {
    pub(crate) fn createSegmentEndpoints(&self) -> Vec<geo::PointRef> {
        let mut points: Vec<geo::PointRef> = Vec::new();
        let n = &self.OVGNodes;

        points.push(geo::PointRef::new(n[1].P()));
        let mut i = 2usize;
        while i + 2 < n.len() {
            let lastPoint = n[i - 1].P();
            let currentPoint = n[i].P();
            let nextPoint = n[i + 1].P();
            if (currentPoint.X == lastPoint.X) && (currentPoint.X != nextPoint.X) {
                points.push(geo::PointRef::new(currentPoint));
            } else if (currentPoint.Y == lastPoint.Y) && (currentPoint.Y != nextPoint.Y) {
                points.push(geo::PointRef::new(currentPoint));
            }
            i += 1;
        }
        points.push(geo::PointRef::new(n[n.len() - 2].P()));
        points
    }

    /// The entire route is colinear
    pub(crate) fn isEntireColinear(&self, from: &OVGNodeRef, to: &OVGNodeRef) -> bool {
        let (from, to) = (from.P(), to.P());
        let n = &self.OVGNodes;
        if from.Y == to.Y {
            let y = from.Y;
            let (mut minX, mut maxX) = (from.X, to.X);
            if maxX < minX {
                std::mem::swap(&mut minX, &mut maxX);
            }
            let mut i = 1usize;
            while i + 2 < n.len() {
                let (rFrom, rTo) = (n[i].P(), n[i + 1].P());
                if rFrom.Y != y || rTo.Y != y {
                    return false;
                }
                let (mut rMinX, mut rMaxX) = (rFrom.X, rTo.X);
                if rMaxX < rMinX {
                    std::mem::swap(&mut rMinX, &mut rMaxX);
                }
                if rMinX < minX || rMaxX > maxX {
                    return false;
                }
                i += 1;
            }
            return true;
        }

        if from.X == to.X {
            let x = from.X;
            let (mut minY, mut maxY) = (from.Y, to.Y);
            if maxY < minY {
                std::mem::swap(&mut minY, &mut maxY);
            }
            let mut i = 1usize;
            while i + 2 < n.len() {
                let (rFrom, rTo) = (n[i].P(), n[i + 1].P());
                if rFrom.X != x || rTo.X != x {
                    return false;
                }
                let (mut rMinY, mut rMaxY) = (rFrom.Y, rTo.Y);
                if rMaxY < rMinY {
                    std::mem::swap(&mut rMinY, &mut rMaxY);
                }
                if rMinY < minY || rMaxY > maxY {
                    return false;
                }
                i += 1;
            }
            return true;
        }

        false
    }

    /// isOpposingColinear returns true if the segment <from, to> spans colinearly in the opposite direction to any segment of the route
    /// This only applies to routes which are directed, otherwise there is no notion of opposite
    /// If the segments share a single endpoint, that doesn't count
    pub(crate) fn isOpposingColinear(&self, from: &OVGNodeRef, to: &OVGNodeRef) -> bool {
        if !self.GEdge.IsDirected() {
            return false;
        }
        let (from, to) = (from.P(), to.P());
        let n = &self.OVGNodes;
        let len = n.len() as i64;
        if from.Y == to.Y {
            let y = from.Y;
            let (mut minX, mut maxX) = (from.X, to.X);
            if maxX < minX {
                std::mem::swap(&mut minX, &mut maxX);
            }
            let mut i: i64 = 0;
            while i < len - 2 {
                if n[(i + 1) as usize].Y() != y {
                    i += 1;
                    i += 1;
                    continue;
                }
                if n[i as usize].Y() != y {
                    i += 1;
                    continue;
                }
                let (rFrom, rTo) = (n[i as usize].P(), n[(i + 1) as usize].P());
                if rTo.X != rFrom.X {
                    let skip = if rFrom.X < rTo.X { maxX < rFrom.X || rTo.X < minX } else { maxX < rTo.X || rFrom.X < minX };
                    if !skip {
                        let (mut rMinX, mut rMaxX) = (rFrom.X, rTo.X);
                        if rMaxX < rMinX {
                            std::mem::swap(&mut rMinX, &mut rMaxX);
                        }

                        if rMinX <= from.X && from.X <= rMaxX && (to.X < rMinX || rMaxX < to.X) {
                            if nonNilEquals(&from, &rFrom) || nonNilEquals(&from, &rTo) {
                                return false;
                            }
                        } else if rMinX <= to.X && to.X <= rMaxX && (from.X < rMinX || rMaxX < from.X) {
                            if nonNilEquals(&to, &rFrom) || nonNilEquals(&to, &rTo) {
                                return false;
                            }
                        }

                        if (rTo.X > rFrom.X) && (to.X < from.X) {
                            return true;
                        } else if (rTo.X < rFrom.X) && (to.X > from.X) {
                            return true;
                        }
                    }
                }
                i += 1;
            }
        } else if from.X == to.X {
            let x = from.X;
            let (mut minY, mut maxY) = (from.Y, to.Y);
            if maxY < minY {
                std::mem::swap(&mut minY, &mut maxY);
            }
            let mut i: i64 = 0;
            while i < len - 2 {
                if n[(i + 1) as usize].X() != x {
                    i += 1;
                    i += 1;
                    continue;
                }
                if n[i as usize].X() != x {
                    i += 1;
                    continue;
                }
                let (rFrom, rTo) = (n[i as usize].P(), n[(i + 1) as usize].P());

                if rTo.Y != rFrom.Y {
                    let skip = if rFrom.Y < rTo.Y { maxY < rFrom.Y || rTo.Y < minY } else { maxY < rTo.Y || rFrom.Y < minY };
                    if !skip {
                        let (mut rMinY, mut rMaxY) = (rFrom.Y, rTo.Y);
                        if rMaxY < rMinY {
                            std::mem::swap(&mut rMinY, &mut rMaxY);
                        }

                        if rMinY <= from.Y && from.Y <= rMaxY && (to.Y < rMinY || rMaxY < to.Y) {
                            if nonNilEquals(&from, &rFrom) || nonNilEquals(&from, &rTo) {
                                return false;
                            }
                        } else if rMinY <= to.Y && to.Y <= rMaxY && (from.Y < rMinY || rMaxY < from.Y) {
                            if nonNilEquals(&to, &rFrom) || nonNilEquals(&to, &rTo) {
                                return false;
                            }
                        }

                        if (rTo.Y > rFrom.Y) && (to.Y < from.Y) {
                            return true;
                        } else if (rTo.Y < rFrom.Y) && (to.Y > from.Y) {
                            return true;
                        }
                    }
                }
                i += 1;
            }
        }

        false
    }
}

impl RouteRef {
    pub(crate) fn createSegmentEndpoints(&self) -> Vec<geo::PointRef> {
        self.b().createSegmentEndpoints()
    }

    /// A route can swap edges with another route if:
    /// - The two endpoints are the same nodes
    /// - No other edges are using the ports
    /// - The two endpoints is not diagonal
    /// - The ports that the two routes are connected to on each node are on the same side
    /// Not self-route
    pub(crate) fn canSwapEdgesGuarded(&self, r2: &RouteRef, allRoutes: &[RouteRef], guard: Option<&dyn workBudget>) -> GoResult<bool> {
        let (rFrom, rTo, rFromPort, rToPort) = {
            let r = self.b();
            let e = r.GEdge.b();
            (e.From.clone(), e.To.clone(), r.FromPort, r.ToPort)
        };
        let (r2From, r2To, r2FromPort, r2ToPort) = {
            let r = r2.b();
            let e = r.GEdge.b();
            (e.From.clone(), e.To.clone(), r.FromPort, r.ToPort)
        };
        if rFrom == rTo {
            return Ok(false);
        }
        if r2From == r2To {
            return Ok(false);
        }
        if !((rFrom == r2From && rTo == r2To) || (rFrom == r2To && rTo == r2From)) {
            return Ok(false);
        }

        if rFrom.Orientation(&rTo).IsDiagonal() {
            return Ok(false);
        }

        if rFrom == r2From && rTo == r2To {
            if rFrom.Orientation(&rTo).IsVertical() {
                if rFromPort.Y != r2FromPort.Y {
                    return Ok(false);
                }
                if rToPort.Y != r2ToPort.Y {
                    return Ok(false);
                }
            }
            if rFrom.Orientation(&rTo).IsHorizontal() {
                if rFromPort.X != r2FromPort.X {
                    return Ok(false);
                }
                if rToPort.X != r2ToPort.X {
                    return Ok(false);
                }
            }
        }

        for r3 in allRoutes {
            if let Some(g) = guard {
                g.step()?;
            }
            if self == r3 || r2 == r3 {
                continue;
            }
            let (r3From, r3To) = {
                let r = r3.b();
                (r.FromPort, r.ToPort)
            };
            if rFromPort == r3From || rFromPort == r3To {
                return Ok(false);
            }
            if rToPort == r3From || rToPort == r3To {
                return Ok(false);
            }
            if r2FromPort == r3From || r2FromPort == r3To {
                return Ok(false);
            }
            if r2ToPort == r3From || r2ToPort == r3To {
                return Ok(false);
            }
        }

        Ok(true)
    }
}
