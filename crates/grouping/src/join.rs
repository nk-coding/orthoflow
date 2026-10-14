// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/grouping/join.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

#[derive(Clone)]
pub(crate) struct joinNodePositionSnapshot {
    node: NodeRef,
    pointer: Option<geo::PointRef>,
    value: geo::Point,
}

/// joinPositionJournal keeps capture order because separate nodes may share a
/// TopLeft pointer and first reach the journal after different movements.
#[derive(Default)]
pub(crate) struct joinPositionJournal {
    captured: Set<NodeRef>,
    snapshots: Vec<joinNodePositionSnapshot>,
}

impl joinPositionJournal {
    pub(crate) fn capture(&mut self, node: &NodeRef) -> bool {
        if self.captured.contains(node) {
            return false;
        }
        self.captured.insert(node.clone());
        let pointer = node.tl();
        let value = pointer.as_ref().map(|p| *p.b()).unwrap_or_default();
        self.snapshots.push(joinNodePositionSnapshot { node: node.clone(), pointer, value });
        true
    }

    pub(crate) fn moveWithChildren(&mut self, node: &NodeRef, dx: f64, dy: f64, guard: &WorkGuardRef) -> GoResult<()> {
        if dx == 0.0 && dy == 0.0 {
            return Ok(());
        }
        let firstCapture = self.capture(node);
        let Some(graph) = node.graph() else {
            guard.Step()?;
            // Preserve the legacy malformed-node panic after journaling the root.
            node.MoveWithChildren(dx, dy);
            return guard.Finish();
        };
        let mut hasDescendants = node.IsContainer() && !graph.containers().at(&Some(node.clone())).is_empty();
        if node.IsClusterVessel() {
            hasDescendants = hasDescendants || graph.clusters().get(node).map_or(false, |c| !c.b().Nodes.is_empty());
        }
        hasDescendants = hasDescendants || graph.sequences().get(node).map_or(false, |s| !s.b().Nodes.is_empty());
        let mut descendants: Vec<NodeRef> = Vec::new();
        if hasDescendants {
            descendants = graph.AllDescendantNodesWithWorkGuard(Some(node), true, guard)?;
        }
        if firstCapture {
            for descendant in &descendants {
                self.capture(descendant);
            }
        }
        guard.Add(1 + descendants.len() as i64)?;
        node.Translate(dx, dy);
        for descendant in &descendants {
            descendant.Translate(dx, dy);
        }
        Ok(())
    }

    pub(crate) fn restore(&self) {
        for snapshot in self.snapshots.iter().rev() {
            snapshot.node.set_tl(snapshot.pointer.clone());
            if let Some(p) = &snapshot.pointer {
                *p.bm() = snapshot.value;
            }
        }
    }
}

pub(crate) fn joinMedian(points: &[geo::PointRef], guard: &WorkGuardRef) -> GoResult<geo::PointRef> {
    guard.Add(2 * points.len() as i64)?;
    let mut width = 1usize;
    while width < points.len() {
        guard.Add(2 * points.len() as i64)?;
        if width > points.len() / 2 {
            break;
        }
        width *= 2;
    }
    Ok(geo::PointsExt::GetMedian(points))
}

pub(crate) fn joinContainerFixedOrigin(
    graph: &GraphRef,
    container: Option<&NodeRef>,
    guard: &WorkGuardRef,
) -> GoResult<Option<geo::PointRef>> {
    for node in graph.nodes() {
        guard.Step()?;
        if node.OwningContainer().as_ref() != container {
            continue;
        }
        if let Some(origin) = node.FixedOrigin() {
            return Ok(Some(origin));
        }
    }
    Ok(None)
}

fn ctxErr(ctx: &Ctx) -> GoResult<()> {
    if let Some(err) = ctx.Err() {
        return Err(GoError::wrap(format!("PlaceNodes: {}", err), err));
    }
    Ok(())
}

/// JoinDistancedClusters incrementally pulls disconnected placement islands
/// toward their common center without introducing overlap.
pub fn JoinDistancedClusters(ctx: &Ctx, graph: &GraphRef) -> GoResult<()> {
    ctxErr(ctx)?;
    let (existing, sharedGuard) = layoutgraph::ExistingTransactionWorkGuard(ctx, "PlaceNodes")?;
    let nodes = graph.nodes();
    let mut preflightWork = nodes.len() as i64;
    if sharedGuard {
        existing.as_ref().unwrap().Add(preflightWork)?;
    }
    let fixedNodes = graph.FixedNodes();
    if nodes.len() as i64 - fixedNodes.len() as i64 <= 1 {
        if sharedGuard {
            return existing.as_ref().unwrap().Finish();
        }
        return Ok(());
    }
    if graph.b().Edges.is_empty() {
        preflightWork += nodes.len() as i64;
        if sharedGuard {
            existing.as_ref().unwrap().Add(nodes.len() as i64)?;
        }
        let hasJoinReference = nodes.iter().any(|n| {
            let n = n.b();
            !n.Edges.is_empty() || !n.Nears.is_empty()
        });
        if !hasJoinReference {
            if sharedGuard {
                return existing.as_ref().unwrap().Finish();
            }
            return Ok(());
        }
    }
    let guard = if !sharedGuard {
        let g = limits::NewWorkGuard(ctx, "PlaceNodes", limits::MaxTransactionWorkUnits)?;
        g.Add(preflightWork)?;
        g
    } else {
        existing.unwrap()
    };
    let mut positions = joinPositionJournal::default();
    let result = joinInner(ctx, graph, &guard, &fixedNodes, &mut positions);
    if result.is_err() {
        positions.restore();
    }
    result
}

fn joinInner(
    ctx: &Ctx,
    graph: &GraphRef,
    guard: &WorkGuardRef,
    fixedNodes: &[NodeRef],
    positions: &mut joinPositionJournal,
) -> GoResult<()> {
    let cellSize = graph.b().CellSize;
    let clusters = graph.nodes().DistanceClustersWithWorkGuard(3.0 * cellSize, Some(guard))?;
    if clusters.is_empty() {
        // Go returns a nil slice here when nothing needs joining.
        return guard.Finish();
    }

    let target: geo::PointRef;
    if !fixedNodes.is_empty() {
        target = fixedNodes.CenterWithWorkGuard(Some(guard))?;
    } else {
        let mut centers: Vec<geo::PointRef> = Vec::with_capacity(clusters.len());
        for cluster in &clusters {
            centers.push(cluster.CenterWithWorkGuard(Some(guard))?);
        }
        target = joinMedian(&centers, guard)?;
    }
    let target = *target.b();

    let mut inchTowardsTarget =
        |cluster: &[NodeRef], fixedOrigin: Option<geo::Point>, positions: &mut joinPositionJournal| -> GoResult<bool> {
            let (topLeft, bottomRight) = cluster.BoundingBoxWithWorkGuard(Some(guard))?;
            let topLeft = *topLeft.unwrap().b();
            let bottomRight = *bottomRight.unwrap().b();
            let center = geo::Point {
                X: topLeft.X + (bottomRight.X - topLeft.X) / 2.0,
                Y: topLeft.Y + (bottomRight.Y - topLeft.Y) / 2.0,
            };
            let distance = geo::EuclideanDistance(center.X, center.Y, target.X, target.Y);
            let length = math::Max(bottomRight.X - topLeft.X, bottomRight.Y - topLeft.Y);
            if distance < length / 2.0 {
                return Ok(false);
            }
            let (mut deltaX, mut deltaY) = (0.0f64, 0.0f64);
            let orientation = center.GetOrientation(&target);
            if orientation == geo::NONE {
                return Err(invariant::New("No orientation"));
            }
            if orientation == geo::Top || orientation == geo::TopLeft || orientation == geo::TopRight {
                deltaY = 1.0;
            }
            if orientation == geo::Bottom || orientation == geo::BottomLeft || orientation == geo::BottomRight {
                deltaY = -1.0;
            }
            if orientation == geo::Left || orientation == geo::BottomLeft || orientation == geo::TopLeft {
                deltaX = 1.0;
            }
            if orientation == geo::Right || orientation == geo::TopRight || orientation == geo::BottomRight {
                deltaX = -1.0;
            }
            if deltaX == 0.0 && deltaY == 0.0 {
                return Ok(false);
            }
            deltaX *= cellSize;
            deltaY *= cellSize;

            let (mut hasOverlap, mut hasOverlapX, mut hasOverlapY) = (false, false, false);
            for node in cluster {
                let (x, y) = (node.x() + deltaX, node.y() + deltaY);
                if !hasOverlap {
                    if let Some(fo) = fixedOrigin {
                        if x < fo.X {
                            hasOverlapX = true;
                            hasOverlap = true;
                        }
                        if y < fo.Y {
                            hasOverlapY = true;
                            hasOverlap = true;
                        }
                    }
                    if !hasOverlap {
                        hasOverlap =
                            graph.WouldOverlapWithWorkGuard(Some(node), Some(&geo::NewPoint(x, y)), cluster, None, Some(guard))?;
                    }
                }
                positions.moveWithChildren(node, deltaX, deltaY, guard)?;
            }
            if hasOverlap {
                for node in cluster {
                    positions.moveWithChildren(node, -deltaX, -deltaY, guard)?;
                }
            }
            if deltaX != 0.0 && deltaY != 0.0 {
                if hasOverlapX && !hasOverlapY {
                    hasOverlap = false;
                    for node in cluster {
                        let (x, y) = (node.x(), node.y() + deltaY);
                        let mut overlaps = fixedOrigin.map_or(false, |fo| x < fo.X || y < fo.Y);
                        if !overlaps {
                            overlaps =
                                graph.WouldOverlapWithWorkGuard(Some(node), Some(&geo::NewPoint(x, y)), cluster, None, Some(guard))?;
                        }
                        if overlaps {
                            hasOverlap = true;
                        }
                        positions.moveWithChildren(node, 0.0, deltaY, guard)?;
                    }
                    if hasOverlap {
                        for node in cluster {
                            positions.moveWithChildren(node, 0.0, -deltaY, guard)?;
                        }
                    }
                } else if !hasOverlapX && hasOverlapY {
                    hasOverlap = false;
                    for node in cluster {
                        let (x, y) = (node.x() + deltaX, node.y());
                        let mut overlaps = fixedOrigin.map_or(false, |fo| x < fo.X || y < fo.Y);
                        if !overlaps {
                            overlaps =
                                graph.WouldOverlapWithWorkGuard(Some(node), Some(&geo::NewPoint(x, y)), cluster, None, Some(guard))?;
                        }
                        if overlaps {
                            hasOverlap = true;
                        }
                        positions.moveWithChildren(node, deltaX, 0.0, guard)?;
                    }
                    if hasOverlap {
                        for node in cluster {
                            positions.moveWithChildren(node, -deltaX, 0.0, guard)?;
                        }
                    }
                }
            }
            Ok(!hasOverlap)
        };

    for _ in 0..1000 {
        ctxErr(ctx)?;
        guard.Add(1 + clusters.len() as i64)?;
        let mut moved = false;
        for cluster in &clusters {
            ctxErr(ctx)?;
            let mut fixedOrigin: Option<geo::Point> = None;
            if !fixedNodes.is_empty() {
                fixedOrigin = joinContainerFixedOrigin(graph, cluster[0].OwningContainer().as_ref(), guard)?.map(|p| *p.b());
            }
            if inchTowardsTarget(cluster, fixedOrigin, positions)? {
                moved = true;
            }
        }
        if !moved {
            break;
        }
    }
    ctxErr(ctx)?;
    guard.Finish()
}
