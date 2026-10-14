// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/graphjson/representation.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Fixture representation (Go struct tags mirrored with serde attributes).
use crate::*;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeMap;

/// Go's encoding/json rejects non-finite floats.
pub(crate) fn goF64<S: Serializer>(v: &f64, s: S) -> Result<S::Ok, S::Error> {
    if !v.is_finite() {
        return Err(serde::ser::Error::custom(format!("json: unsupported value: {}", gocompat::strconv::FormatFloatV(*v))));
    }
    s.serialize_f64(*v)
}

pub(crate) fn goOptF64<S: Serializer>(v: &Option<f64>, s: S) -> Result<S::Ok, S::Error> {
    match v {
        None => s.serialize_none(),
        Some(f) => goF64(f, s),
    }
}

/// JSON `null` decodes to the Go zero value (nil slice / nil map / zero struct).
pub(crate) fn nullDefault<'de, D: Deserializer<'de>, T: Deserialize<'de> + Default>(d: D) -> Result<T, D::Error> {
    Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
}

fn isFalse(b: &bool) -> bool {
    !*b
}
fn isZeroInt(i: &i64) -> bool {
    *i == 0
}

/// SerializedLabel is the fixture representation of label geometry.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct SerializedLabel {
    pub Text: String,
    pub Position: String,
    #[serde(serialize_with = "goF64")]
    pub Width: f64,
    #[serde(serialize_with = "goF64")]
    pub Height: f64,
}

/// SerializedIcon is the fixture representation of icon placement.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct SerializedIcon {
    pub Position: String,
}

/// SerializedPoint is the fixed JSON representation of a geometric point.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct SerializedPoint {
    #[serde(rename = "x", serialize_with = "goF64")]
    pub X: f64,
    #[serde(rename = "y", serialize_with = "goF64")]
    pub Y: f64,
}

/// SerializedScalar is the JSON shape of one D2 style value.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct SerializedScalar {
    #[serde(rename = "value")]
    pub Value: String,
}

/// SerializedStyle is the edge-style portion of graph JSON.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct SerializedStyle {
    #[serde(rename = "opacity", skip_serializing_if = "Option::is_none")]
    pub Opacity: Option<SerializedScalar>,
    #[serde(rename = "stroke", skip_serializing_if = "Option::is_none")]
    pub Stroke: Option<SerializedScalar>,
    #[serde(rename = "fill", skip_serializing_if = "Option::is_none")]
    pub Fill: Option<SerializedScalar>,
    #[serde(rename = "fillPattern", skip_serializing_if = "Option::is_none")]
    pub FillPattern: Option<SerializedScalar>,
    #[serde(rename = "strokeWidth", skip_serializing_if = "Option::is_none")]
    pub StrokeWidth: Option<SerializedScalar>,
    #[serde(rename = "strokeDash", skip_serializing_if = "Option::is_none")]
    pub StrokeDash: Option<SerializedScalar>,
    #[serde(rename = "borderRadius", skip_serializing_if = "Option::is_none")]
    pub BorderRadius: Option<SerializedScalar>,
    #[serde(rename = "shadow", skip_serializing_if = "Option::is_none")]
    pub Shadow: Option<SerializedScalar>,
    #[serde(rename = "3d", skip_serializing_if = "Option::is_none")]
    pub ThreeDee: Option<SerializedScalar>,
    #[serde(rename = "multiple", skip_serializing_if = "Option::is_none")]
    pub Multiple: Option<SerializedScalar>,
    #[serde(rename = "font", skip_serializing_if = "Option::is_none")]
    pub Font: Option<SerializedScalar>,
    #[serde(rename = "fontSize", skip_serializing_if = "Option::is_none")]
    pub FontSize: Option<SerializedScalar>,
    #[serde(rename = "fontColor", skip_serializing_if = "Option::is_none")]
    pub FontColor: Option<SerializedScalar>,
    #[serde(rename = "animated", skip_serializing_if = "Option::is_none")]
    pub Animated: Option<SerializedScalar>,
    #[serde(rename = "bold", skip_serializing_if = "Option::is_none")]
    pub Bold: Option<SerializedScalar>,
    #[serde(rename = "italic", skip_serializing_if = "Option::is_none")]
    pub Italic: Option<SerializedScalar>,
    #[serde(rename = "underline", skip_serializing_if = "Option::is_none")]
    pub Underline: Option<SerializedScalar>,
    #[serde(rename = "filled", skip_serializing_if = "Option::is_none")]
    pub Filled: Option<SerializedScalar>,
    #[serde(rename = "doubleBorder", skip_serializing_if = "Option::is_none")]
    pub DoubleBorder: Option<SerializedScalar>,
    #[serde(rename = "textTransform", skip_serializing_if = "Option::is_none")]
    pub TextTransform: Option<SerializedScalar>,
}

/// SerializedNode is the fixture representation of one layout node.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct SerializedNode {
    #[serde(rename = "id")]
    pub ID: i64,
    #[serde(rename = "d2id", skip_serializing_if = "Option::is_none")]
    pub D2ID: Option<String>,
    #[serde(rename = "isInvisible", skip_serializing_if = "isFalse")]
    pub IsInvisible: bool,
    #[serde(rename = "width", serialize_with = "goF64")]
    pub Width: f64,
    #[serde(rename = "height", serialize_with = "goF64")]
    pub Height: f64,
    #[serde(rename = "topLeft")]
    pub TopLeft: Option<SerializedPoint>,
    #[serde(rename = "fixedTopLeft", skip_serializing_if = "Option::is_none")]
    pub FixedTopLeft: Option<SerializedPoint>,
    #[serde(rename = "desiredWidth", skip_serializing_if = "Option::is_none", serialize_with = "goOptF64")]
    pub DesiredWidth: Option<f64>,
    #[serde(rename = "desiredHeight", skip_serializing_if = "Option::is_none", serialize_with = "goOptF64")]
    pub DesiredHeight: Option<f64>,
    #[serde(rename = "nears", skip_serializing_if = "Vec::is_empty", deserialize_with = "nullDefault")]
    pub Nears: Vec<i64>,
    #[serde(rename = "fontSize", skip_serializing_if = "Option::is_none")]
    pub FontSize: Option<i64>,
    #[serde(rename = "shapeType")]
    pub ShapeType: String,
    #[serde(rename = "label", skip_serializing_if = "Option::is_none")]
    pub Label: Option<SerializedLabel>,
    #[serde(rename = "icon", skip_serializing_if = "Option::is_none")]
    pub Icon: Option<SerializedIcon>,
    #[serde(rename = "numColumns", skip_serializing_if = "isZeroInt")]
    pub NumColumns: i64,
    #[serde(rename = "is3D", skip_serializing_if = "isFalse")]
    pub Is3D: bool,
    #[serde(rename = "isMultiple", skip_serializing_if = "isFalse")]
    pub IsMultiple: bool,
    #[serde(rename = "forceHierarchy", skip_serializing_if = "isFalse")]
    pub ForceHierarchy: bool,
}

/// SerializedEdge is the fixture representation of one layout edge.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct SerializedEdge {
    #[serde(rename = "id")]
    pub ID: i64,
    /// OriginalID preserves Edge.ID when ID is a serialization-only
    /// reference allocated for an otherwise ambiguous unset ID.
    #[serde(rename = "originalId", skip_serializing_if = "Option::is_none")]
    pub OriginalID: Option<i64>,
    #[serde(rename = "d2id", skip_serializing_if = "Option::is_none")]
    pub D2ID: Option<String>,
    #[serde(rename = "from")]
    pub FromNode: i64,
    #[serde(rename = "isInvisible", skip_serializing_if = "isFalse")]
    pub IsInvisible: bool,
    #[serde(rename = "to")]
    pub ToNode: i64,
    /// Go `[]*SerializedPoint`; a nil slice encodes as `null` (the port
    /// decodes `null` as an empty route).
    #[serde(rename = "points", deserialize_with = "nullDefault")]
    pub Points: Vec<Option<SerializedPoint>>,
    #[serde(rename = "minWidth")]
    pub MinWidth: i64,
    #[serde(rename = "minHeight")]
    pub MinHeight: i64,
    #[serde(rename = "labelPercentage", serialize_with = "goF64")]
    pub LabelPercentage: f64,
    #[serde(rename = "label", skip_serializing_if = "Option::is_none")]
    pub Label: Option<SerializedLabel>,
    #[serde(rename = "fromTableColumnIndex", skip_serializing_if = "Option::is_none")]
    pub FromTableColumnIndex: Option<i64>,
    #[serde(rename = "toTableColumnIndex", skip_serializing_if = "Option::is_none")]
    pub ToTableColumnIndex: Option<i64>,
    #[serde(rename = "style", deserialize_with = "nullDefault")]
    pub Style: SerializedStyle,

    #[serde(rename = "sourceArrowhead")]
    pub SourceArrowhead: String,
    #[serde(rename = "targetArrowhead")]
    pub TargetArrowhead: String,

    #[serde(rename = "sourceArrowheadLabel", skip_serializing_if = "Option::is_none")]
    pub SourceArrowheadLabel: Option<SerializedLabel>,
    #[serde(rename = "targetArrowheadLabel", skip_serializing_if = "Option::is_none")]
    pub TargetArrowheadLabel: Option<SerializedLabel>,
}

/// SerializedEdgeAbduction records an edge's temporary grouping endpoints.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct SerializedEdgeAbduction {
    pub Edge: i64,
    pub OriginallyFromNode: i64,
    pub OriginallyToNode: i64,
    pub CurrentFromNode: i64,
    pub CurrentToNode: i64,
}

/// SerializedCluster is the fixture representation of a cluster.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct SerializedCluster {
    #[serde(deserialize_with = "nullDefault")]
    pub Nodes: Vec<i64>,
    pub Arrangement: String,
    pub DesiredArrangement: String,
    #[serde(deserialize_with = "nullDefault")]
    pub EdgeAbductions: Vec<SerializedEdgeAbduction>,
    pub Container: i64,
    pub FixedSize: bool,
    #[serde(serialize_with = "goF64")]
    pub Padding: f64,
}

/// SerializedSequence is the fixture representation of a sequence.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct SerializedSequence {
    #[serde(deserialize_with = "nullDefault")]
    pub Nodes: Vec<i64>,
    #[serde(deserialize_with = "nullDefault")]
    pub EdgeAbductions: Vec<SerializedEdgeAbduction>,
    pub Container: i64,
}

/// SerializedTree is the recursive fixture representation of a layout tree.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct SerializedTree {
    #[serde(deserialize_with = "nullDefault")]
    pub Node: SerializedNode,
    #[serde(deserialize_with = "nullDefault")]
    pub Children: Vec<SerializedTree>,
    #[serde(deserialize_with = "nullDefault")]
    pub SentinelEdge: SerializedEdge,
    pub Orientation: i64,
}

/// SerializedHierarchy records node ranks in one hierarchy.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct SerializedHierarchy {
    #[serde(deserialize_with = "nullDefault")]
    pub Levels: BTreeMap<i64, i64>,
}

/// SerializedGraph is the complete graph fixture representation.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct SerializedGraph {
    #[serde(rename = "nodes", deserialize_with = "nullDefault")]
    pub Nodes: Vec<SerializedNode>,
    #[serde(rename = "edges", deserialize_with = "nullDefault")]
    pub Edges: Vec<SerializedEdge>,
    #[serde(serialize_with = "goF64")]
    pub CellSize: f64,

    #[serde(rename = "isRootHierarchy", skip_serializing_if = "isFalse")]
    pub IsRootHierarchy: bool,
    #[serde(rename = "containers", deserialize_with = "nullDefault")]
    pub Containers: BTreeMap<i64, Vec<i64>>,
    #[serde(rename = "hubs", skip_serializing_if = "BTreeMap::is_empty", deserialize_with = "nullDefault")]
    pub Hubs: BTreeMap<i64, Vec<i64>>,
    #[serde(rename = "clusters", skip_serializing_if = "BTreeMap::is_empty", deserialize_with = "nullDefault")]
    pub Clusters: BTreeMap<i64, SerializedCluster>,
    #[serde(rename = "sequences", skip_serializing_if = "BTreeMap::is_empty", deserialize_with = "nullDefault")]
    pub Sequences: BTreeMap<i64, SerializedSequence>,
    #[serde(rename = "clusterVessels", skip_serializing_if = "BTreeMap::is_empty", deserialize_with = "nullDefault")]
    pub ClusterVessels: BTreeMap<i64, SerializedNode>,
    #[serde(rename = "sequenceVessels", skip_serializing_if = "BTreeMap::is_empty", deserialize_with = "nullDefault")]
    pub SequenceVessels: BTreeMap<i64, SerializedNode>,
    #[serde(rename = "trees", skip_serializing_if = "BTreeMap::is_empty", deserialize_with = "nullDefault")]
    pub Trees: BTreeMap<i64, Vec<SerializedTree>>,
    #[serde(rename = "hierarchies", skip_serializing_if = "Vec::is_empty", deserialize_with = "nullDefault")]
    pub Hierarchies: Vec<SerializedHierarchy>,
    #[serde(rename = "directions", skip_serializing_if = "BTreeMap::is_empty", deserialize_with = "nullDefault")]
    pub Directions: BTreeMap<i64, String>,

    #[serde(rename = "crossingCost", serialize_with = "goF64")]
    pub CrossingCost: f64,
    #[serde(rename = "turnCost", serialize_with = "goF64")]
    pub TurnCost: f64,
    #[serde(rename = "nonCenterPortCost", serialize_with = "goF64")]
    pub NonCenterPortCost: f64,
}
