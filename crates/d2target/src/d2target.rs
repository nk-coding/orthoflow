// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2target/d2target.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;
use geo::{PointRef, RouteExt};
use gocompat::math;

pub const DEFAULT_ICON_SIZE: i64 = 32;
pub const MIN_ARROWHEAD_STROKE_WIDTH: i64 = 2;
pub const ARROWHEAD_PADDING: f64 = 2.0;
pub const CONNECTION_ICON_LABEL_GAP: i64 = 8;

/// Go `d2target.Text`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Text {
    pub Label: String,
    pub FontSize: i64,
    pub FontFamily: String,
    pub Language: String,
    pub Color: String,
    pub Italic: bool,
    pub Bold: bool,
    pub Underline: bool,
    pub LabelWidth: i64,
    pub LabelHeight: i64,
    pub LabelFill: String,
}

/// Go `d2target.Arrowhead` (a string type).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct Arrowhead(pub String);

impl Arrowhead {
    pub fn new(s: &str) -> Arrowhead {
        Arrowhead(s.to_string())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Go `d2target.Arrowhead(s)` conversion.
pub fn ToArrowheadString(s: &str) -> Arrowhead {
    Arrowhead::new(s)
}

pub const NoArrowhead: &str = "none";
pub const ArrowArrowhead: &str = "arrow";
pub const UnfilledTriangleArrowhead: &str = "unfilled-triangle";
pub const TriangleArrowhead: &str = "triangle";
pub const DiamondArrowhead: &str = "diamond";
pub const FilledDiamondArrowhead: &str = "filled-diamond";
pub const CircleArrowhead: &str = "circle";
pub const FilledCircleArrowhead: &str = "filled-circle";
pub const CrossArrowhead: &str = "cross";
pub const BoxArrowhead: &str = "box";
pub const FilledBoxArrowhead: &str = "filled-box";
pub const LineArrowhead: &str = "line";
pub const CfOne: &str = "cf-one";
pub const CfMany: &str = "cf-many";
pub const CfOneRequired: &str = "cf-one-required";
pub const CfManyRequired: &str = "cf-many-required";
pub const DefaultArrowhead: &str = TriangleArrowhead;

pub fn ToArrowhead(arrowheadType: &str, filled: Option<bool>) -> Arrowhead {
    let a = match arrowheadType {
        DiamondArrowhead => {
            if filled == Some(true) {
                FilledDiamondArrowhead
            } else {
                DiamondArrowhead
            }
        }
        CircleArrowhead => {
            if filled == Some(true) {
                FilledCircleArrowhead
            } else {
                CircleArrowhead
            }
        }
        NoArrowhead => NoArrowhead,
        ArrowArrowhead => ArrowArrowhead,
        TriangleArrowhead => {
            if filled == Some(false) {
                UnfilledTriangleArrowhead
            } else {
                TriangleArrowhead
            }
        }
        CrossArrowhead => CrossArrowhead,
        BoxArrowhead => {
            if filled == Some(true) {
                FilledBoxArrowhead
            } else {
                BoxArrowhead
            }
        }
        CfOne => CfOne,
        CfMany => CfMany,
        CfOneRequired => CfOneRequired,
        CfManyRequired => CfManyRequired,
        _ => {
            if DefaultArrowhead == TriangleArrowhead && filled == Some(false) {
                UnfilledTriangleArrowhead
            } else {
                DefaultArrowhead
            }
        }
    };
    Arrowhead::new(a)
}

impl Arrowhead {
    pub fn Dimensions(&self, strokeWidth: f64) -> (f64, f64) {
        let (mut baseWidth, mut baseHeight, mut widthMultiplier, mut heightMultiplier) = (0.0, 0.0, 0.0, 0.0);
        match self.0.as_str() {
            ArrowArrowhead => {
                baseWidth = 4.0;
                baseHeight = 4.0;
                widthMultiplier = 4.0;
                heightMultiplier = 4.0;
            }
            TriangleArrowhead => {
                baseWidth = 4.0;
                baseHeight = 4.0;
                widthMultiplier = 3.0;
                heightMultiplier = 4.0;
            }
            UnfilledTriangleArrowhead => {
                baseWidth = 7.0;
                baseHeight = 7.0;
                widthMultiplier = 3.0;
                heightMultiplier = 4.0;
            }
            LineArrowhead => {
                widthMultiplier = 5.0;
                heightMultiplier = 8.0;
            }
            FilledDiamondArrowhead => {
                baseWidth = 11.0;
                baseHeight = 7.0;
                widthMultiplier = 5.5;
                heightMultiplier = 3.5;
            }
            DiamondArrowhead => {
                baseWidth = 11.0;
                baseHeight = 9.0;
                widthMultiplier = 5.5;
                heightMultiplier = 4.5;
            }
            CrossArrowhead => {
                baseWidth = 7.0;
                baseHeight = 7.0;
                widthMultiplier = 5.0;
                heightMultiplier = 5.0;
            }
            FilledCircleArrowhead | CircleArrowhead => {
                baseWidth = 8.0;
                baseHeight = 8.0;
                widthMultiplier = 5.0;
                heightMultiplier = 5.0;
            }
            FilledBoxArrowhead | BoxArrowhead => {
                baseWidth = 6.0;
                baseHeight = 6.0;
                widthMultiplier = 5.0;
                heightMultiplier = 5.0;
            }
            CfOne | CfMany | CfOneRequired | CfManyRequired => {
                baseWidth = 9.0;
                baseHeight = 9.0;
                widthMultiplier = 4.5;
                heightMultiplier = 4.5;
            }
            _ => {}
        }
        // go2.Max: a > b ? a : b
        let m = MIN_ARROWHEAD_STROKE_WIDTH as f64;
        let clippedStrokeWidth = if m > strokeWidth { m } else { strokeWidth };
        (baseWidth + clippedStrokeWidth * widthMultiplier, baseHeight + clippedStrokeWidth * heightMultiplier)
    }
}

/// Go `d2target.Connection` (only the fields the layout engine touches, plus the
/// scalar defaults set by `BaseConnection`).
#[derive(Clone, Debug)]
pub struct Connection {
    pub ID: String,
    pub Src: String,
    pub SrcArrow: Arrowhead,
    pub SrcLabel: Option<Text>,
    pub Dst: String,
    pub DstArrow: Arrowhead,
    pub DstLabel: Option<Text>,
    pub Opacity: f64,
    pub StrokeDash: f64,
    pub StrokeWidth: i64,
    pub BorderRadius: f64,
    /// embedded `Text`
    pub Text: Text,
    pub LabelPosition: String,
    pub LabelPercentage: f64,
    pub Route: Vec<PointRef>,
    pub IsCurve: bool,
}

/// `d2target.BaseConnection()`
pub fn BaseConnection() -> Connection {
    Connection {
        ID: String::new(),
        Src: String::new(),
        SrcArrow: Arrowhead::new(NoArrowhead),
        SrcLabel: None,
        Dst: String::new(),
        DstArrow: Arrowhead::new(NoArrowhead),
        DstLabel: None,
        Opacity: 1.0,
        StrokeDash: 0.0,
        StrokeWidth: 2,
        BorderRadius: 10.0,
        Text: Text { Italic: true, FontFamily: "DEFAULT".to_string(), ..Default::default() },
        LabelPosition: String::new(),
        LabelPercentage: 0.0,
        Route: Vec::new(),
        IsCurve: false,
    }
}

impl Connection {
    pub fn GetLabelTopLeft(&self) -> Option<PointRef> {
        let (point, _) = label::FromString(&self.LabelPosition).GetPointOnRoute(
            &self.Route,
            self.StrokeWidth as f64,
            self.LabelPercentage,
            self.Text.LabelWidth as f64,
            self.Text.LabelHeight as f64,
        );
        point
    }

    pub fn GetArrowheadLabelPosition(&self, isDst: bool) -> PointRef {
        let (width, height);
        if isDst {
            let l = self.DstLabel.as_ref().expect("nil pointer dereference: DstLabel");
            width = l.LabelWidth as f64;
            height = l.LabelHeight as f64;
        } else {
            let l = self.SrcLabel.as_ref().expect("nil pointer dereference: SrcLabel");
            width = l.LabelWidth as f64;
            height = l.LabelHeight as f64;
        }

        let mut index: usize = 0;
        if isDst {
            index = self.Route.len() - 2;
        }
        let (start, end) = (self.Route[index].get(), self.Route[index + 1].get());
        let (normalX, normalY) = geo::GetUnitNormalVector(end.X, end.Y, start.X, start.Y);

        let shift = math::Abs(normalX) * (height / 2.0 + label::PADDING as f64) + math::Abs(normalY) * (width / 2.0 + label::PADDING as f64);

        let length = self.Route.Length();
        let position: f64;
        if isDst {
            let mut p = 1.0;
            if length > 0.0 {
                p -= shift / length;
            }
            position = p;
        } else {
            let mut p = 0.0;
            if length > 0.0 {
                p = shift / length;
            }
            position = p;
        }

        let strokeWidth = self.StrokeWidth as f64;

        let (labelTL, _) = label::UnlockedTop.GetPointOnRoute(&self.Route, strokeWidth, position, width, height);
        let labelTL = labelTL.expect("UnlockedTop always yields a point");

        let mut arrowSize: f64 = 0.0;
        if isDst && self.DstArrow.0 != NoArrowhead {
            arrowSize = self.DstArrow.Dimensions(strokeWidth).1;
        } else if self.SrcArrow.0 != NoArrowhead {
            arrowSize = self.SrcArrow.Dimensions(strokeWidth).1;
        }

        if arrowSize > 0.0 {
            let offset = (arrowSize / 2.0 + ARROWHEAD_PADDING) - strokeWidth / 2.0 - label::PADDING as f64;
            if offset > 0.0 {
                let mut l = labelTL.bm();
                l.X += normalX * offset;
                l.Y += normalY * offset;
            }
        }
        labelTL
    }
}
