// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/layoutgraph/edge_style.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

#[allow(unused_imports)]
use crate::*;

/// Arrowhead is TALA's layout-time arrowhead value. The adapter translates
/// between D2 renderer arrowheads and this owned string type at the engine
/// boundary; layout algorithms only need stable value equality.
pub type Arrowhead = String;

pub const NoArrowhead: &str = "none";
pub const TriangleArrowhead: &str = "triangle";

/// StyleScalar carries the value of one edge style attribute. Parser
/// provenance belongs to the D2 adapter and must not enter the layout engine.
/// (Go `*StyleScalar` → `Option<StyleScalar>`: the pointee is never mutated.)
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StyleScalar {
    pub Value: String,
}

/// EdgeStyle is the complete style record carried through TALA's layout graph.
/// Routing currently compares only a subset of these fields, but every value
/// must survive cloning and fixture round-trips unchanged.
#[derive(Clone, Debug, Default)]
pub struct EdgeStyle {
    pub Opacity: Option<StyleScalar>,
    pub Stroke: Option<StyleScalar>,
    pub Fill: Option<StyleScalar>,
    pub FillPattern: Option<StyleScalar>,
    pub StrokeWidth: Option<StyleScalar>,
    pub StrokeDash: Option<StyleScalar>,
    pub BorderRadius: Option<StyleScalar>,
    pub Shadow: Option<StyleScalar>,
    pub ThreeDee: Option<StyleScalar>,
    pub Multiple: Option<StyleScalar>,
    pub Font: Option<StyleScalar>,
    pub FontSize: Option<StyleScalar>,
    pub FontColor: Option<StyleScalar>,
    pub Animated: Option<StyleScalar>,
    pub Bold: Option<StyleScalar>,
    pub Italic: Option<StyleScalar>,
    pub Underline: Option<StyleScalar>,
    pub Filled: Option<StyleScalar>,
    pub DoubleBorder: Option<StyleScalar>,
    pub TextTransform: Option<StyleScalar>,
}
