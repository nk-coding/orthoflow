// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/layoutgraph/edge.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

pub(crate) const edgeStrokeWidth: f64 = 3.0;

pub struct Edge {
    pub D2ID: Option<String>,
    pub ID: EntityID,
    /// Go `*Node`; never nil in a valid graph (the port does not model nil endpoints).
    pub From: NodeRef,
    pub To: NodeRef,
    pub Points: Vec<geo::PointRef>,

    pub SourceArrowhead: Arrowhead,
    pub TargetArrowhead: Arrowhead,
    pub SourceArrowheadLabel: Option<LabelRef>,
    pub TargetArrowheadLabel: Option<LabelRef>,

    pub MinWidth: i64,
    pub MinHeight: i64,
    pub Label: Option<LabelRef>,
    pub LabelPercentage: f64,
    pub IsCurve: bool,

    // edges between SQL Table shapes
    pub FromTableColumnIndex: Option<i64>,
    pub ToTableColumnIndex: Option<i64>,

    pub IsInvisible: bool,
    pub Style: EdgeStyle,

    // hierarchyRankWeight is transient metadata used only by the temporary
    // simple DAG passed to hierarchy ranking.
    pub(crate) hierarchyRankWeight: i64,
    pub(crate) hierarchyRankWeightSet: bool,
}

impl Edge {
    /// Go struct value copy (pointer fields shared, slices copied by header).
    pub(crate) fn shallowCopy(&self) -> Edge {
        Edge {
            D2ID: self.D2ID.clone(),
            ID: self.ID,
            From: self.From.clone(),
            To: self.To.clone(),
            Points: self.Points.clone(),
            SourceArrowhead: self.SourceArrowhead.clone(),
            TargetArrowhead: self.TargetArrowhead.clone(),
            SourceArrowheadLabel: self.SourceArrowheadLabel.clone(),
            TargetArrowheadLabel: self.TargetArrowheadLabel.clone(),
            MinWidth: self.MinWidth,
            MinHeight: self.MinHeight,
            Label: self.Label.clone(),
            LabelPercentage: self.LabelPercentage,
            IsCurve: self.IsCurve,
            FromTableColumnIndex: self.FromTableColumnIndex,
            ToTableColumnIndex: self.ToTableColumnIndex,
            IsInvisible: self.IsInvisible,
            Style: self.Style.clone(),
            hierarchyRankWeight: self.hierarchyRankWeight,
            hierarchyRankWeightSet: self.hierarchyRankWeightSet,
        }
    }
}

gocompat::go_ptr!(EdgeRef, Edge);

impl EdgeRef {
    pub fn EquivalentStyles(&self, e2: &EdgeRef) -> bool {
        self.stylesMatchOneWay(e2) && e2.stylesMatchOneWay(self)
    }

    pub(crate) fn stylesMatchOneWay(&self, e2: &EdgeRef) -> bool {
        let s1 = self.b().Style.clone();
        let s2 = e2.b().Style.clone();
        fn m(a: &Option<StyleScalar>, b: &Option<StyleScalar>) -> bool {
            match a {
                Some(av) => matches!(b, Some(bv) if bv.Value == av.Value),
                None => true,
            }
        }
        if !m(&s1.Opacity, &s2.Opacity) {
            return false;
        }
        if !m(&s1.Stroke, &s2.Stroke) {
            return false;
        }
        if !m(&s1.StrokeWidth, &s2.StrokeWidth) {
            return false;
        }
        if !m(&s1.StrokeDash, &s2.StrokeDash) {
            return false;
        }
        if !m(&s1.Animated, &s2.Animated) {
            return false;
        }

        true
    }
}

pub fn NewEdge(from: &NodeRef, to: &NodeRef) -> EdgeRef {
    EdgeRef::new(Edge {
        D2ID: None,
        ID: 0,
        From: from.clone(),
        To: to.clone(),
        Points: Vec::new(),
        SourceArrowhead: String::new(),
        TargetArrowhead: String::new(),
        SourceArrowheadLabel: None,
        TargetArrowheadLabel: None,
        MinWidth: 0,
        MinHeight: 0,
        Label: None,
        LabelPercentage: 0.0,
        IsCurve: false,
        FromTableColumnIndex: None,
        ToTableColumnIndex: None,
        IsInvisible: false,
        Style: EdgeStyle::default(),
        hierarchyRankWeight: 0,
        hierarchyRankWeightSet: false,
    })
}

impl EdgeRef {
    pub(crate) fn isTreeEdge(&self) -> bool {
        let (from, to) = (self.from(), self.to());
        if from.g().b().NodeToTree.has(&from) {
            return true;
        }
        if to.g().b().NodeToTree.has(&to) {
            return true;
        }
        false
    }

    pub fn SourcePort(&self) -> Option<geo::PointRef> {
        self.b().Points.first().cloned()
    }

    pub fn TargetPort(&self) -> Option<geo::PointRef> {
        self.b().Points.last().cloned()
    }

    pub(crate) fn entityID(&self) -> EntityID {
        self.b().ID
    }

    pub(crate) fn isLoop(&self) -> bool {
        let e = self.b();
        e.From == e.To
    }

    pub fn HasSourceArrow(&self) -> bool {
        let e = self.b();
        !e.SourceArrowhead.is_empty() && e.SourceArrowhead != NoArrowhead
    }

    pub fn HasTargetArrow(&self) -> bool {
        let e = self.b();
        !e.TargetArrowhead.is_empty() && e.TargetArrowhead != NoArrowhead
    }

    pub(crate) fn isDirected(&self) -> bool {
        self.HasSourceArrow() != self.HasTargetArrow()
    }

    pub(crate) fn isBidirectional(&self) -> bool {
        self.HasSourceArrow() && self.HasTargetArrow()
    }

    pub(crate) fn isUndirected(&self) -> bool {
        !self.HasSourceArrow() && !self.HasTargetArrow()
    }

    /// directedEndpoints returns the semantic source and target of a directed edge.
    /// Edge.From and Edge.To preserve declaration order, so a source-arrow-only edge
    /// points from To to From.
    pub(crate) fn directedEndpoints(&self) -> (Option<NodeRef>, Option<NodeRef>, bool) {
        if !self.isDirected() {
            return (None, None, false);
        }
        if self.HasSourceArrow() {
            return (Some(self.to()), Some(self.from()), true);
        }
        (Some(self.from()), Some(self.to()), true)
    }

    pub(crate) fn ownArrowheadsMatch(&self) -> bool {
        let e = self.b();
        e.SourceArrowhead == e.TargetArrowhead
    }

    pub(crate) fn hasMatchingArrowheads(&self, otherEdge: &EdgeRef) -> bool {
        let (s1, t1) = {
            let e = self.b();
            (e.SourceArrowhead.clone(), e.TargetArrowhead.clone())
        };
        let (s2, t2) = {
            let e = otherEdge.b();
            (e.SourceArrowhead.clone(), e.TargetArrowhead.clone())
        };
        if s1 == s2 && t1 == t2 {
            return true;
        }
        if s1 == t2 && t1 == s2 {
            return true;
        }
        false
    }

    pub(crate) fn hasArrowTo(&self, node: &NodeRef) -> bool {
        (*node == self.from() && self.HasSourceArrow()) || (*node == self.to() && self.HasTargetArrow())
    }

    pub(crate) fn arrowheadTo(&self, node: &NodeRef) -> Arrowhead {
        let e = self.b();
        if *node == e.From {
            e.SourceArrowhead.clone()
        } else {
            e.TargetArrowhead.clone()
        }
    }

    pub fn DebugID(&self) -> String {
        let mut arrow = "-".to_string();
        if self.HasSourceArrow() {
            arrow = format!("<{}", arrow);
        }
        if self.HasTargetArrow() {
            arrow.push('>');
        }
        if !self.HasSourceArrow() && !self.HasTargetArrow() {
            arrow.push('-');
        }
        let (from, to, fi, ti, lbl) = {
            let e = self.b();
            (e.From.clone(), e.To.clone(), e.FromTableColumnIndex, e.ToTableColumnIndex, e.Label.clone())
        };
        let mut fromS = from.DebugID();
        if let Some(i) = fi {
            fromS = format!("{}[{}]", fromS, i);
        }
        let mut toS = to.DebugID();
        if let Some(i) = ti {
            toS = format!("{}[{}]", toS, i);
        }
        let mut label = String::new();
        if let Some(l) = lbl {
            label = format!(": {}", l.b().Text);
        }
        format!("{} {} {}{}", fromS, arrow, toS, label)
    }

    pub(crate) fn isBetweenTableColumns(&self) -> bool {
        let e = self.b();
        e.FromTableColumnIndex.is_some() && e.ToTableColumnIndex.is_some()
    }

    pub(crate) fn hasTableColumn(&self) -> bool {
        let e = self.b();
        e.FromTableColumnIndex.is_some() || e.ToTableColumnIndex.is_some()
    }

    pub(crate) fn isClusterEdge(&self) -> bool {
        self.from().b().Cluster.is_some() || self.to().b().Cluster.is_some()
    }

    pub(crate) fn euclideanDistance(&self) -> f64 {
        self.from().distance(&self.to(), true)
    }

    pub(crate) fn segmentCount(&self) -> i64 {
        // One point for source and one for target centers = 2 for a straight line
        self.b().Points.len() as i64 - 1
    }

    // reconnect takes care of reassigning an edge
    pub(crate) fn reconnect(&self, newEndpoint: &NodeRef, isTo: bool) {
        if isTo {
            self.to().removeEdge(self);
            newEndpoint.addEdge(self);

            self.bm().To = newEndpoint.clone();
        } else {
            self.from().removeEdge(self);
            newEndpoint.addEdge(self);

            self.bm().From = newEndpoint.clone();
        }
    }

    // isAxisAligned checks if the connected ends are axis aligned
    // Test is to draw a line through the center and see if it overlaps
    pub(crate) fn isAxisAligned(&self) -> bool {
        let (from, to) = (self.from(), self.to());

        if self.hasTableColumn() {
            let ports = self.facingTablePorts(None, None);
            if ports.hasFrom && ports.hasTo {
                if geo::PrecisionCompare(ports.from.Y, ports.to.Y, AxisAlignmentTolerance) == 0 {
                    return true;
                }
            } else if ports.hasFrom {
                if geo::PrecisionCompare(ports.from.Y, to.y() + to.h() / 2.0, AxisAlignmentTolerance) == 0 {
                    return true;
                }
            } else if ports.hasTo {
                if geo::PrecisionCompare(ports.to.Y, from.y() + from.h() / 2.0, AxisAlignmentTolerance) == 0 {
                    return true;
                }
            } else if geo::PrecisionCompare(from.x(), to.x(), AxisAlignmentTolerance) == 0 {
                // if the tables are stacked, consider them aligned one of the sides is aligned
                return true;
            } else if geo::PrecisionCompare(from.x() + from.w(), to.x() + to.w(), AxisAlignmentTolerance) == 0 {
                // if the tables are stacked, consider them aligned one of the sides is aligned
                return true;
            }
            return false;
        }

        // Horizontal alignment
        if geo::PrecisionCompare(from.y() + from.h() / 2.0, to.y() + to.h() / 2.0, AxisAlignmentTolerance) == 0 {
            return true;
        }

        // Vertical alignment
        if geo::PrecisionCompare(from.x() + from.w() / 2.0, to.x() + to.w() / 2.0, AxisAlignmentTolerance) == 0 {
            return true;
        }

        false
    }

    pub(crate) fn isDuplicateOf(&self, otherEdge: &EdgeRef) -> bool {
        let (f, t) = (self.from(), self.to());
        let (of, ot) = (otherEdge.from(), otherEdge.to());
        if ot == t && of == f {
            return true;
        }
        if of == t && ot == f {
            return true;
        }

        false
    }

    pub(crate) fn hasDuplicateIn(&self, edges: &[EdgeRef]) -> bool {
        for otherEdge in edges {
            if otherEdge.isDuplicateOf(self) {
                return true;
            }
        }
        false
    }
}

/// Go `type Edges []*Edge`.
pub type Edges = Vec<EdgeRef>;

/// Methods of Go's `Edges` slice type.
pub trait EdgesMethods {
    fn portEdges(&self, node: &NodeRef) -> Map<PointKey, Vec<EdgeRef>>;
    fn PortEdges(&self, node: &NodeRef) -> Map<PointKey, Vec<EdgeRef>>;
}

impl EdgesMethods for [EdgeRef] {
    // return map to lookup the edges connected to each port of the node
    fn portEdges(&self, node: &NodeRef) -> Map<PointKey, Vec<EdgeRef>> {
        let mut portEdges: Map<PointKey, Vec<EdgeRef>> = Map::default();
        for edge in self {
            let port = if edge.from() == *node {
                edge.SourcePort()
            } else if edge.to() == *node {
                edge.TargetPort()
            } else {
                continue;
            };

            let Some(port) = port else { continue };

            let key = PointKey(*port.b());
            portEdges.entry(key).or_default().push(edge.clone());
        }
        portEdges
    }
    fn PortEdges(&self, node: &NodeRef) -> Map<PointKey, Vec<EdgeRef>> {
        self.portEdges(node)
    }
}

impl EdgeRef {
    pub fn Length(&self) -> f64 {
        let points = self.points();
        points.Length()
    }

    /// LabelTopLeft returns the top-left point for a label of the given size and
    /// position on edge.
    pub fn LabelTopLeft(&self, labelPosition: label::Position, width: f64, height: f64) -> Option<geo::PointRef> {
        let (points, pct) = {
            let e = self.b();
            (e.Points.clone(), e.LabelPercentage)
        };
        let (point, _) = labelPosition.GetPointOnRoute(&points, edgeStrokeWidth, pct, width, height);
        point
    }

    pub(crate) fn bounds(&self) -> (geo::PointRef, geo::PointRef) {
        let (tl, br) = self.boundingBoxValues();
        (geo::PointRef::new(tl), geo::PointRef::new(br))
    }

    pub(crate) fn boundingBoxValues(&self) -> (geo::Point, geo::Point) {
        let mut tl = geo::Point { X: f64::INFINITY, Y: f64::INFINITY };
        let mut br = geo::Point { X: f64::NEG_INFINITY, Y: f64::NEG_INFINITY };

        let (points, label, srcLabel, tgtLabel, srcArrow, tgtArrow) = {
            let e = self.b();
            (
                e.Points.clone(),
                e.Label.clone(),
                e.SourceArrowheadLabel.clone(),
                e.TargetArrowheadLabel.clone(),
                e.SourceArrowhead.clone(),
                e.TargetArrowhead.clone(),
            )
        };

        for p in &points {
            let p = *p.b();
            tl.X = fmin(tl.X, p.X);
            tl.Y = fmin(tl.Y, p.Y);
            br.X = fmax(br.X, p.X);
            br.Y = fmax(br.Y, p.Y);
        }

        if let Some(l) = &label {
            let (lpos, lw, lh) = {
                let l = l.b();
                (l.Position, l.Width, l.Height)
            };
            if !points.is_empty() && lpos != label::Unset {
                let labelTL = *self.LabelTopLeft(lpos, lw, lh).expect("nil pointer dereference").b();
                tl.X = fmin(tl.X, labelTL.X);
                tl.Y = fmin(tl.Y, labelTL.Y);
                br.X = fmax(br.X, labelTL.X + lw);
                br.Y = fmax(br.Y, labelTL.Y + lh);
            }
        }
        if !points.is_empty() {
            if let Some(l) = &srcLabel {
                let (lw, lh) = {
                    let l = l.b();
                    (l.Width, l.Height)
                };
                let labelTL = labelgeom::ArrowheadTopLeft(&points, false, &srcArrow, &tgtArrow, lw, lh);
                let labelTL = *labelTL.b();
                tl.X = fmin(tl.X, labelTL.X);
                tl.Y = fmin(tl.Y, labelTL.Y);
                br.X = fmax(br.X, labelTL.X + lw);
                br.Y = fmax(br.Y, labelTL.Y + lh);
            }
            if let Some(l) = &tgtLabel {
                let (lw, lh) = {
                    let l = l.b();
                    (l.Width, l.Height)
                };
                let labelTL = labelgeom::ArrowheadTopLeft(&points, true, &srcArrow, &tgtArrow, lw, lh);
                let labelTL = *labelTL.b();
                tl.X = fmin(tl.X, labelTL.X);
                tl.Y = fmin(tl.Y, labelTL.Y);
                br.X = fmax(br.X, labelTL.X + lw);
                br.Y = fmax(br.Y, labelTL.Y + lh);
            }
        }

        tl.X = tl.X.round();
        tl.Y = tl.Y.round();
        br.X = br.X.round();
        br.Y = br.Y.round();
        (tl, br)
    }

    pub(crate) fn isTargetedTo(&self, n: &NodeRef) -> bool {
        self.hasArrowTo(n)
    }
}

/// Go `EdgeSegment` (embeds geo.Segment).
#[derive(Clone)]
pub struct EdgeSegment {
    pub Segment: geo::Segment,
    pub(crate) edge: EdgeRef,
}

gocompat::go_ptr!(EdgeSegmentRef, EdgeSegment);

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct facingTablePortValues {
    pub(crate) from: geo::Point,
    pub(crate) to: geo::Point,
    pub(crate) hasFrom: bool,
    pub(crate) hasTo: bool,
    pub(crate) orientation: geo::Orientation,
}

impl EdgeRef {
    /// facingTablePorts returns the ports related to the connected tables
    /// if they are properly aligned, without allocating points.
    pub(crate) fn facingTablePorts(&self, abductionFrom: Option<&NodeRef>, abductionTo: Option<&NodeRef>) -> facingTablePortValues {
        if !self.hasTableColumn() {
            return facingTablePortValues { orientation: geo::NONE, ..Default::default() };
        }
        let (fi, ti) = {
            let e = self.b();
            (e.FromTableColumnIndex, e.ToTableColumnIndex)
        };
        let from = abductionFrom.cloned().unwrap_or_else(|| self.from());
        let to = abductionTo.cloned().unwrap_or_else(|| self.to());
        let orientation = from.orientation(&to);
        let mut ports = facingTablePortValues { orientation, ..Default::default() };
        if orientation == geo::TopLeft || orientation == geo::BottomLeft || orientation == geo::Left {
            if let Some(fi) = fi {
                let (p, ok) = from.tableColumnPortValue(geo::Right, fi);
                ports.from = p;
                ports.hasFrom = ok;
            }
            if let Some(ti) = ti {
                let (p, ok) = to.tableColumnPortValue(geo::Left, ti);
                ports.to = p;
                ports.hasTo = ok;
            }
            return ports;
        }
        if orientation == geo::Right || orientation == geo::TopRight || orientation == geo::BottomRight {
            if let Some(fi) = fi {
                let (p, ok) = from.tableColumnPortValue(geo::Left, fi);
                ports.from = p;
                ports.hasFrom = ok;
            }
            if let Some(ti) = ti {
                let (p, ok) = to.tableColumnPortValue(geo::Right, ti);
                ports.to = p;
                ports.hasTo = ok;
            }
            return ports;
        }
        facingTablePortValues { orientation: geo::NONE, ..Default::default() }
    }

    pub fn HasLargeArrowheadLabel(&self) -> bool {
        let e = self.b();
        e.SourceArrowheadLabel.as_ref().map_or(false, |l| l.b().Text.len() > 3)
            || e.TargetArrowheadLabel.as_ref().map_or(false, |l| l.b().Text.len() > 3)
    }

    pub(crate) fn isStraight(&self) -> bool {
        let points: Vec<geo::Point> = self.b().Points.iter().map(|p| *p.b()).collect();
        let mut p = points[0];
        let mut q = points[1];
        for i in 2..points.len() {
            if orientation(&p, &q, &points[i]) != 0.0 {
                return false;
            }
            p = q;
            q = points[i];
        }
        true
    }

    pub(crate) fn hasOverlappingEnd(&self) -> bool {
        let (start, end, from, to) = {
            let e = self.b();
            (e.Points[0].clone(), e.Points[e.Points.len() - 1].clone(), e.From.clone(), e.To.clone())
        };

        for fromEdge in from.edges() {
            if fromEdge == *self {
                continue;
            }
            let (first, last) = {
                let fe = fromEdge.b();
                (fe.Points[0].clone(), fe.Points[fe.Points.len() - 1].clone())
            };
            if nonNilEquals(Some(&first), Some(&start)) || nonNilEquals(Some(&last), Some(&start)) {
                return true;
            }
        }
        for toEdge in to.edges() {
            if toEdge == *self {
                continue;
            }
            let (first, last) = {
                let te = toEdge.b();
                (te.Points[0].clone(), te.Points[te.Points.len() - 1].clone())
            };
            if nonNilEquals(Some(&first), Some(&end)) || nonNilEquals(Some(&last), Some(&end)) {
                return true;
            }
        }
        false
    }
}
