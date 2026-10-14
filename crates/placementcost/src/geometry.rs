// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/placementcost/geometry.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

pub(crate) const scoringCancellationCheckInterval: i64 = 64;
pub(crate) const sizelessDirectionDeltaFactor: f64 = 0.25;
pub(crate) const symmetryToleranceBand: f64 = 1.0;

/// SideEdgeSpacing is the minimum spacing used to distinguish aligned side ports.
pub const SideEdgeSpacing: f64 = 40.0;
/// IdealGapSize is the target separation between connected node boundaries.
pub const IdealGapSize: f64 = 2.5 * layoutgraph::ConnectedNodeGap as f64;

/// Go `fmt.Errorf("EdgeLength: %w", err)`.
#[inline(never)]
pub(crate) fn edgeLengthErr(err: GoError) -> GoError {
    GoError::wrap(format!("EdgeLength: {}", err), err)
}

/// Go `if err := ctx.Err(); err != nil { return fmt.Errorf("EdgeLength: %w", err) }`.
#[inline]
pub(crate) fn ctxCheck(ctx: &Ctx) -> GoResult<()> {
    match ctx.Err() {
        Some(err) => Err(edgeLengthErr(err)),
        None => Ok(()),
    }
}

#[inline]
pub(crate) fn scoringCancellationError(ctx: &Ctx, iteration: i64) -> GoResult<()> {
    if iteration % scoringCancellationCheckInterval != 0 {
        return Ok(());
    }
    checkScoringCancellation(ctx)
}

#[inline]
pub(crate) fn checkScoringCancellation(ctx: &Ctx) -> GoResult<()> {
    ctxCheck(ctx)
}

pub(crate) type compassReading = i64;

pub(crate) fn directionCompass(direction: geo::Orientation) -> compassReading {
    match direction {
        geo::BottomLeft => -3,
        geo::Left => -2,
        geo::TopLeft => -1,
        geo::Top => 0,
        geo::TopRight => 1,
        geo::Right => 2,
        geo::BottomRight => 3,
        geo::Bottom => 4,
        _ => 0,
    }
}

pub(crate) fn compassDelta(first: compassReading, second: compassReading) -> compassReading {
    let mut delta = second - first;
    if delta > 4 {
        delta -= 8;
    } else if delta < -4 {
        delta += 8;
    }
    delta
}

pub(crate) fn compassAxisDelta(first: compassReading, second: compassReading) -> compassReading {
    // Go `%` truncates toward zero, as does Rust's.
    let first = (first + 4) % 4;
    let second = (second + 4) % 4;
    let delta = second - first;
    if delta == 3 {
        return -1;
    }
    delta
}

/// Go `geo.Box{TopLeft, Width, Height}` reduced to the values the distance needs.
#[derive(Clone, Copy)]
pub(crate) struct valueBox {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) w: f64,
    pub(crate) h: f64,
}

pub(crate) fn distanceToPoint(node: &NodeRef, point: &geo::Point, includeSizes: bool) -> f64 {
    let tl = node.tlv();
    let mut bx = valueBox { x: tl.X, y: tl.Y, w: 0.0, h: 0.0 };
    if includeSizes {
        bx.w = node.w();
        bx.h = node.h();
    }
    let pointBox = valueBox { x: point.X, y: point.Y, w: 0.0, h: 0.0 };
    distanceBetweenBoxes(bx, pointBox)
}

pub(crate) fn placementDistance(first: &NodeRef, second: &NodeRef, includeSizes: bool) -> f64 {
    let distance = first.DistanceTo(second, includeSizes);
    let f = first.tlv();
    let s = second.tlv();
    let mut xCenter = (f.X - s.X).abs();
    let mut yCenter = (f.Y - s.Y).abs();
    if includeSizes {
        let (fw, fh, sw, sh) = (first.w(), first.h(), second.w(), second.h());
        xCenter = ((f.X + fw / 2.0) - (s.X + sw / 2.0)).abs() / (fw + sw);
        yCenter = ((f.Y + fh / 2.0) - (s.Y + sh / 2.0)).abs() / (fh + sh);
    }
    distance + math::Min(xCenter, yCenter) / 20.0
}

pub(crate) fn distanceBetweenBoxes(first: valueBox, second: valueBox) -> f64 {
    let dx = intervalGap(first.x, first.x + first.w, second.x, second.x + second.w);
    let dy = intervalGap(first.y, first.y + first.h, second.y, second.y + second.h);
    geo::EuclideanDistance(0.0, 0.0, dx, dy)
}

pub(crate) fn intervalGap(firstStart: f64, firstEnd: f64, secondStart: f64, secondEnd: f64) -> f64 {
    if firstEnd < secondStart {
        return secondStart - firstEnd;
    }
    if secondEnd < firstStart {
        return firstStart - secondEnd;
    }
    0.0
}

pub(crate) fn sizelessOrientation(node: &NodeRef, other: &NodeRef) -> geo::Orientation {
    let (Some(ntl), Some(otl)) = (node.tl(), other.tl()) else {
        return geo::NONE;
    };
    let n = *ntl.b();
    let o = *otl.b();
    if n.Y < o.Y {
        if n.X < o.X {
            return geo::TopLeft;
        }
        if o.X < n.X {
            return geo::TopRight;
        }
        return geo::Top;
    }
    if o.Y < n.Y {
        if n.X < o.X {
            return geo::BottomLeft;
        }
        if o.X < n.X {
            return geo::BottomRight;
        }
        return geo::Bottom;
    }
    if o.X < n.X {
        return geo::Right;
    }
    if n.X < o.X {
        return geo::Left;
    }
    geo::NONE
}

pub(crate) fn depth(node: Option<&NodeRef>) -> i64 {
    let mut d = 0;
    let mut cur = node.cloned();
    while let Some(n) = cur {
        d += 1;
        cur = n.containerField();
    }
    d
}

pub(crate) fn distanceBetweenTableColumns(graph: &GraphRef, edge: &EdgeRef, from: &NodeRef, to: &NodeRef) -> f64 {
    let (mut fromPort, mut toPort, hasFromPort, hasToPort, orientation) = edge.FacingTablePortValues(Some(from), Some(to));
    if orientation == geo::NONE {
        let pair: Vec<NodeRef> = vec![from.clone(), to.clone()];
        let (topLeft, bottomRight) = pair.BoundingBox();
        let topLeft = *topLeft.expect("nil pointer dereference").b();
        let bottomRight = *bottomRight.expect("nil pointer dereference").b();
        return 2.0 * geo::EuclideanDistance(topLeft.X, topLeft.Y, bottomRight.X, bottomRight.Y);
    }
    let mut multiplier = 2.0;
    if orientation == geo::Left || orientation == geo::Right {
        multiplier = 1.0;
    }
    if !hasFromPort {
        let tl = from.tlv();
        fromPort = geo::Point { X: tl.X + from.w() / 2.0, Y: tl.Y + from.h() / 2.0 };
    }
    if !hasToPort {
        let tl = to.tlv();
        toPort = geo::Point { X: tl.X + to.w() / 2.0, Y: tl.Y + to.h() / 2.0 };
    }
    let mut gapCost = 0.0;
    if (fromPort.X - toPort.X).abs() < IdealGapSize {
        gapCost = 2.0 * graph.b().CellSize;
    }
    multiplier * (gapCost + geo::EuclideanDistance(fromPort.X, fromPort.Y, toPort.X, toPort.Y))
}
