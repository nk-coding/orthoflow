// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/routing/snapshot.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of routing/snapshot.go.
#[allow(unused_imports)]
use crate::*;
use layoutgraph::{EdgeRef, EntityID, LabelRef, NodeRef};

/// Pointer types a `pointerSnapshot` can capture (Go generic `*T`).
pub trait snapshotTarget: Clone {
    type V: Clone;
    fn snapshotGet(&self) -> Self::V;
    fn snapshotSet(&self, v: Self::V);
}

impl snapshotTarget for geo::PointRef {
    type V = geo::Point;
    fn snapshotGet(&self) -> geo::Point {
        *self.b()
    }
    fn snapshotSet(&self, v: geo::Point) {
        *self.bm() = v;
    }
}

impl snapshotTarget for LabelRef {
    type V = layoutgraph::Label;
    fn snapshotGet(&self) -> layoutgraph::Label {
        self.b().clone()
    }
    fn snapshotSet(&self, v: layoutgraph::Label) {
        *self.bm() = v;
    }
}

#[derive(Clone)]
pub struct pointerSnapshot<P: snapshotTarget> {
    pub(crate) pointer: Option<P>,
    pub(crate) value: Option<P::V>,
}

impl<P: snapshotTarget> Default for pointerSnapshot<P> {
    fn default() -> Self {
        pointerSnapshot { pointer: None, value: None }
    }
}

pub fn snapshotPointer<P: snapshotTarget>(pointer: Option<&P>) -> pointerSnapshot<P> {
    match pointer {
        None => pointerSnapshot { pointer: None, value: None },
        Some(p) => pointerSnapshot { pointer: Some(p.clone()), value: Some(p.snapshotGet()) },
    }
}

impl<P: snapshotTarget> pointerSnapshot<P> {
    pub fn restore(&self) -> Option<P> {
        let p = self.pointer.as_ref()?;
        p.snapshotSet(self.value.clone().unwrap());
        Some(p.clone())
    }
}

/// Go `exactSliceSnapshot[[]*geo.Point, *geo.Point]`. Rust vectors do not share
/// a backing array, so restoring the exact slice means restoring the captured
/// pointer list (nil stays nil).
#[derive(Clone, Default)]
pub struct exactSliceSnapshot {
    pub(crate) original: Option<Vec<geo::PointRef>>,
}

impl exactSliceSnapshot {
    pub fn restore(&self) -> Option<Vec<geo::PointRef>> {
        self.original.clone()
    }
}

/// Go `layoutgraph.Edge` struct value (every field visible outside layoutgraph).
#[derive(Clone)]
pub struct edgeValue {
    pub D2ID: Option<String>,
    pub ID: EntityID,
    pub From: NodeRef,
    pub To: NodeRef,
    pub SourceArrowhead: layoutgraph::Arrowhead,
    pub TargetArrowhead: layoutgraph::Arrowhead,
    pub MinWidth: i64,
    pub MinHeight: i64,
    pub LabelPercentage: f64,
    pub IsCurve: bool,
    pub FromTableColumnIndex: Option<i64>,
    pub ToTableColumnIndex: Option<i64>,
    pub IsInvisible: bool,
    pub Style: layoutgraph::EdgeStyle,
}

impl edgeValue {
    pub fn of(edge: &EdgeRef) -> edgeValue {
        let e = edge.b();
        edgeValue {
            D2ID: e.D2ID.clone(),
            ID: e.ID,
            From: e.From.clone(),
            To: e.To.clone(),
            SourceArrowhead: e.SourceArrowhead.clone(),
            TargetArrowhead: e.TargetArrowhead.clone(),
            MinWidth: e.MinWidth,
            MinHeight: e.MinHeight,
            LabelPercentage: e.LabelPercentage,
            IsCurve: e.IsCurve,
            FromTableColumnIndex: e.FromTableColumnIndex,
            ToTableColumnIndex: e.ToTableColumnIndex,
            IsInvisible: e.IsInvisible,
            Style: e.Style.clone(),
        }
    }
}

#[derive(Clone)]
pub struct edgeSnapshot {
    pub(crate) value: edgeValue,

    /// Go `d2ID pointerSnapshot[string]` / `fromTableColumnIndex` /
    /// `toTableColumnIndex`: these are values in the Rust Edge and are
    /// restored from `value`.
    pub(crate) points: exactSliceSnapshot,
    pub(crate) pointValues: Vec<pointerSnapshot<geo::PointRef>>,
    pub(crate) label: pointerSnapshot<LabelRef>,
    pub(crate) sourceArrowheadLabel: pointerSnapshot<LabelRef>,
    pub(crate) targetArrowheadLabel: pointerSnapshot<LabelRef>,
}

impl edgeSnapshot {
    /// Go `edgeSnapshot{value: *edge, d2ID: ..., points: {original: edge.Points, backing}, pointValues, label: ...}`.
    /// `pointValues` are the snapshots of every point in `edge.Points` (the
    /// caller accounts the per-point work as Go does).
    pub fn new(edge: &EdgeRef, pointValues: Vec<pointerSnapshot<geo::PointRef>>) -> edgeSnapshot {
        let e = edge.b();
        edgeSnapshot {
            value: edgeValue::of(edge),
            points: exactSliceSnapshot { original: Some(e.Points.clone()) },
            pointValues,
            label: snapshotPointer(e.Label.as_ref()),
            sourceArrowheadLabel: snapshotPointer(e.SourceArrowheadLabel.as_ref()),
            targetArrowheadLabel: snapshotPointer(e.TargetArrowheadLabel.as_ref()),
        }
    }

    pub fn restore(&self, edge: &EdgeRef) {
        self.label.restore();
        self.sourceArrowheadLabel.restore();
        self.targetArrowheadLabel.restore();
        for point in &self.pointValues {
            point.restore();
        }
        let originalPoints = self.points.restore();

        let v = &self.value;
        let mut e = edge.bm();
        e.D2ID = v.D2ID.clone();
        e.ID = v.ID;
        e.From = v.From.clone();
        e.To = v.To.clone();
        e.SourceArrowhead = v.SourceArrowhead.clone();
        e.TargetArrowhead = v.TargetArrowhead.clone();
        e.MinWidth = v.MinWidth;
        e.MinHeight = v.MinHeight;
        e.LabelPercentage = v.LabelPercentage;
        e.IsCurve = v.IsCurve;
        e.FromTableColumnIndex = v.FromTableColumnIndex;
        e.ToTableColumnIndex = v.ToTableColumnIndex;
        e.IsInvisible = v.IsInvisible;
        e.Style = v.Style.clone();
        e.Points = originalPoints.unwrap_or_default();
        e.Label = self.label.pointer.clone();
        e.SourceArrowheadLabel = self.sourceArrowheadLabel.pointer.clone();
        e.TargetArrowheadLabel = self.targetArrowheadLabel.pointer.clone();
    }
}
