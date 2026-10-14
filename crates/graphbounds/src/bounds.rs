// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/graphbounds/bounds.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

/// WorkStepper is the aggregate accounting contract shared by callers. Bounds
/// never create or reset a budget: every scan is charged to the caller's guard.
/// (Same method set as `layoutgraph::SharedWorkStepper`, which
/// `limits::WorkGuardRef` implements.)
pub use layoutgraph::SharedWorkStepper as WorkStepper;

const LABEL_PADDING: f64 = label::PADDING as f64;

fn isExtreme(nodes: &[NodeRef], node: &NodeRef, disqualifies: &dyn Fn(&NodeRef) -> bool, guard: &dyn WorkStepper) -> GoResult<bool> {
    for other in nodes {
        guard.Step()?;
        if other == node || !other.has_tl() {
            continue;
        }
        if disqualifies(other) {
            return Ok(false);
        }
    }
    Ok(true)
}

/// NodeBoundingBox computes the packing-compatible bounds for one placed node. When
/// allNodes is non-nil, outside labels and icons reserve their exact boundary
/// space and outside-label extremity scans consume the shared work budget.
pub fn NodeBoundingBox(
    node: Option<&NodeRef>,
    allNodes: Option<&[NodeRef]>,
    guard: &dyn WorkStepper,
) -> GoResult<(geo::PointRef, geo::PointRef)> {
    guard.Step()?;
    let node = match node {
        Some(n) if n.has_tl() => n,
        _ => return Err(invariant::New("BinPack bounding box contains an unplaced node")),
    };
    let ntl = node.tlv();
    let (nw, nh) = (node.w(), node.h());
    let mut topLeft = ntl;
    let mut bottomRight = geo::Point { X: (topLeft.X + nw).round(), Y: (topLeft.Y + nh).round() };
    let (dx, dy) = node.ModifierElementAdjustments();
    if dx != 0.0 || dy != 0.0 {
        topLeft.Y -= dy;
        bottomRight.X += dx;
    }
    topLeft.X -= node.loopOffset(geo::Left);
    topLeft.Y -= node.loopOffset(geo::Top);
    bottomRight.X += node.loopOffset(geo::Right);
    bottomRight.Y += node.loopOffset(geo::Bottom);

    if let (Some(lbl), Some(allNodes)) = (node.label(), allNodes) {
        let (lpos, lw, lh) = {
            let l = lbl.b();
            (l.Position, l.Width, l.Height)
        };
        if lpos.IsOutside() {
            let labelTopLeft = *node.LabelTopLeft(lpos, lw, lh).b();
            let boundaryPadding = LABEL_PADDING;
            let outsidePadding = 2.0 * LABEL_PADDING;
            if labelTopLeft.X < topLeft.X {
                let extreme = isExtreme(allNodes, node, &|other| other.x() < ntl.X, guard)?;
                let padding = if extreme { boundaryPadding } else { outsidePadding };
                topLeft.X = (labelTopLeft.X - padding).floor();
            }
            if labelTopLeft.Y < topLeft.Y {
                let extreme = isExtreme(allNodes, node, &|other| other.y() < ntl.Y, guard)?;
                let padding = if extreme { boundaryPadding } else { outsidePadding };
                topLeft.Y = (labelTopLeft.Y - padding).floor();
            }
            if labelTopLeft.X > bottomRight.X {
                let extreme = isExtreme(allNodes, node, &|other| other.x() + other.w() > ntl.X + nw, guard)?;
                let padding = if extreme { boundaryPadding } else { outsidePadding };
                bottomRight.X = (labelTopLeft.X + lw + padding).ceil();
            }
            if labelTopLeft.Y > bottomRight.Y {
                let extreme = isExtreme(allNodes, node, &|other| other.y() + other.h() > ntl.Y + nh, guard)?;
                let padding = if extreme { boundaryPadding } else { outsidePadding };
                bottomRight.Y = (labelTopLeft.Y + lh + padding).ceil();
            }
        }
    }
    if let (Some(icon), Some(_)) = (node.icon(), allNodes) {
        let ipos = icon.b().Position;
        if !node.IsImage() && ipos.IsOutside() {
            let iconSize = layoutgraph::MaxIconSize as f64;
            let iconTopLeft = *node.LabelTopLeft(ipos, iconSize, iconSize).b();
            let outsidePadding = 2.0 * LABEL_PADDING;
            topLeft.X = gocompat::math::Min(topLeft.X, (iconTopLeft.X - outsidePadding).floor());
            topLeft.Y = gocompat::math::Min(topLeft.Y, (iconTopLeft.Y - outsidePadding).floor());
            bottomRight.X = gocompat::math::Max(bottomRight.X, (iconTopLeft.X + iconSize + outsidePadding).ceil());
            bottomRight.Y = gocompat::math::Max(bottomRight.Y, (iconTopLeft.Y + iconSize + outsidePadding).ceil());
        }
    }
    Ok((geo::PointRef::new(topLeft), geo::PointRef::new(bottomRight)))
}

/// BoundingBox computes the packing-compatible bounds for a node set.
pub fn BoundingBox(nodes: &[NodeRef], guard: &dyn WorkStepper) -> GoResult<(geo::PointRef, geo::PointRef)> {
    if nodes.is_empty() {
        guard.Finish()?;
        return Ok((geo::NewPoint(f64::NEG_INFINITY, f64::NEG_INFINITY), geo::NewPoint(f64::INFINITY, f64::INFINITY)));
    }
    let (mut minX, mut minY) = (f64::INFINITY, f64::INFINITY);
    let (mut maxX, mut maxY) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
    for node in nodes {
        let (topLeft, bottomRight) = NodeBoundingBox(Some(node), Some(nodes), guard)?;
        let (tl, br) = (*topLeft.b(), *bottomRight.b());
        minX = gocompat::math::Min(minX, tl.X);
        minY = gocompat::math::Min(minY, tl.Y);
        maxX = gocompat::math::Max(maxX, br.X);
        maxY = gocompat::math::Max(maxY, br.Y);
    }
    guard.Finish()?;
    Ok((geo::NewPoint(minX, minY), geo::NewPoint(maxX, maxY)))
}

fn containerLevel(node: &NodeRef, guard: &dyn WorkStepper) -> GoResult<i64> {
    let mut level = 0i64;
    let mut seen: Set<NodeRef> = Set::default();
    let mut current = Some(node.clone());
    while let Some(c) = current {
        guard.Step()?;
        if seen.contains(&c) {
            return Err(invariant::New("BinPack found a cycle in container ancestry"));
        }
        seen.insert(c.clone());
        level += 1;
        current = c.OwningContainer();
    }
    Ok(level)
}

fn fixedOrigin(nodes: &[NodeRef], guard: &dyn WorkStepper) -> GoResult<Option<geo::PointRef>> {
    let mut container: Option<NodeRef> = None;
    let mut minimumLevel = i64::MAX;
    for node in nodes {
        guard.Step()?;
        let current = node.OwningContainer();
        let Some(current) = current else {
            container = None;
            break;
        };
        let level = containerLevel(&current, guard)?;
        if level < minimumLevel {
            minimumLevel = level;
            container = Some(current);
        }
    }
    for child in nodes {
        guard.Step()?;
        if child.OwningContainer() != container {
            continue;
        }
        if let Some(origin) = child.FixedOrigin() {
            return Ok(Some(origin));
        }
    }
    guard.Finish()?;
    Ok(None)
}

/// FixedBoundingBox computes node-set bounds and substitutes the inherited fixed origin
/// used by packing when one of the relevant children is pinned.
pub fn FixedBoundingBox(nodes: &[NodeRef], guard: &dyn WorkStepper) -> GoResult<(geo::PointRef, geo::PointRef)> {
    let (mut topLeft, bottomRight) = BoundingBox(nodes, guard)?;
    let origin = fixedOrigin(nodes, guard)?;
    if let Some(origin) = origin {
        topLeft = origin;
    }
    guard.Finish()?;
    Ok((topLeft, bottomRight))
}
