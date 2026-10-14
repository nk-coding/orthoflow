// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/graphjson/codec_test.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Ports of selected graphjson codec tests.
use crate::*;
use layoutgraph::{NewDecodedHierarchy, NewGraph, NewNode};

fn bg() -> Ctx {
    gocompat::context::Background()
}

#[test]
fn TestUnmarshalRejectsUnknownFieldsAndTrailingData() {
    for (data, want) in [
        (r#"{"obsolete":true}"#, "unknown field"),
        (r#"{} {}"#, "trailing"),
        (r#"null"#, "cannot deserialize a nil graph"),
    ] {
        let graph = NewGraph();
        let existing = graph.AddNode(&NewNode(1, 10., 10.));
        let err = Unmarshal(&bg(), data.as_bytes(), Some(&graph)).unwrap_err();
        assert!(err.Error().contains(want), "{} vs {}", err, want);
        assert!(graph.nodes().len() == 1 && graph.nodes()[0] == existing && existing.graph().as_ref() == Some(&graph));
    }
}

#[test]
fn TestDeserializeRejectsReservedAndDuplicateIDs() {
    let mk = |nodes: Vec<i64>| SerializedGraph { Nodes: nodes.into_iter().map(|id| SerializedNode { ID: id, ..Default::default() }).collect(), ..Default::default() };
    let g = NewGraph();
    let err = Deserialize(&bg(), Some(&mk(vec![0])), Some(&g)).unwrap_err();
    assert!(err.Error().contains("uses reserved node ID 0"));
    let err = Deserialize(&bg(), Some(&mk(vec![1, 1])), Some(&g)).unwrap_err();
    assert!(err.Error().contains("duplicate node ID"));
    let mut sg = mk(vec![1, 2]);
    sg.Trees.insert(1, vec![SerializedTree { Node: SerializedNode { ID: 2, ..Default::default() }, SentinelEdge: SerializedEdge { FromNode: 1, ToNode: 2, ..Default::default() }, ..Default::default() }]);
    let err = Deserialize(&bg(), Some(&sg), Some(&g)).unwrap_err();
    assert!(err.Error().contains("uses reserved serialized edge ID 0"));
    let mut sg = mk(vec![1]);
    sg.Hierarchies.push(SerializedHierarchy { Levels: [(5, 1)].into_iter().collect() });
    let err = Deserialize(&bg(), Some(&sg), Some(&g)).unwrap_err();
    assert!(err.Error().contains("hierarchy references missing node"));
}

#[test]
fn TestDeserializeReadsSyntheticUnsetEdgeID() {
    let sg = SerializedGraph {
        Nodes: vec![SerializedNode { ID: 1, ..Default::default() }, SerializedNode { ID: 2, ..Default::default() }],
        Edges: vec![SerializedEdge { ID: -1, OriginalID: Some(0), FromNode: 1, ToNode: 2, ..Default::default() }],
        ..Default::default()
    };
    let g = NewGraph();
    Deserialize(&bg(), Some(&sg), Some(&g)).unwrap();
    assert_eq!(g.edges().len(), 1);
    assert_eq!(g.edges()[0].id(), 0);
    // Serializing unset IDs allocates negative references and records OriginalID.
    let out = Serialize(&bg(), Some(&g)).unwrap();
    assert_eq!(out.Edges[0].ID, -1);
    assert_eq!(out.Edges[0].OriginalID, Some(0));
}

#[test]
fn TestGraphCopyAllowsUnsetEdgeIDs() {
    let graph = NewGraph();
    let a = graph.AddNode(&NewNode(1, 10., 10.));
    let b = graph.AddNode(&NewNode(2, 10., 10.));
    let c = graph.AddNode(&NewNode(3, 10., 10.));
    graph.Connect(&a, &b);
    graph.Connect(&b, &c);
    let copy = layoutgraph::Clone(&bg(), Some(&graph)).unwrap();
    assert_eq!(copy.edges().len(), 2);
    assert!(copy.edges().iter().all(|e| e.id() == 0));
}

#[test]
fn TestSerializeEmitsSharedHierarchyOnce() {
    let graph = NewGraph();
    let a = graph.AddNode(&NewNode(1, 10., 10.));
    let b = graph.AddNode(&NewNode(2, 10., 10.));
    let levels = layoutgraph::GoMap::new();
    levels.set(a.clone(), 0);
    levels.set(b.clone(), 1);
    let h = NewDecodedHierarchy(levels);
    a.bm().Hierarchy = Some(h.clone());
    b.bm().Hierarchy = Some(h.clone());
    let s = Serialize(&bg(), Some(&graph)).unwrap();
    assert_eq!(s.Hierarchies.len(), 1);
    assert_eq!(s.Hierarchies[0].Levels.len(), 2);
    let copy = layoutgraph::Clone(&bg(), Some(&graph)).unwrap();
    let n = copy.nodes();
    assert!(n[0].b().Hierarchy.is_some() && n[0].b().Hierarchy == n[1].b().Hierarchy);
}

#[test]
fn TestMarshalRejectsNonFiniteFloats() {
    let graph = NewGraph();
    let a = graph.AddNode(&NewNode(1, f64::NAN, 10.));
    let _ = a;
    assert!(Marshal(&bg(), Some(&graph)).is_err());
}

#[test]
fn TestStyleRoundTrip() {
    let style = SerializedStyle {
        Opacity: Some(SerializedScalar { Value: "o".into() }),
        ThreeDee: Some(SerializedScalar { Value: "3".into() }),
        TextTransform: Some(SerializedScalar { Value: "t".into() }),
        ..Default::default()
    };
    let sg = SerializedGraph {
        Nodes: vec![SerializedNode { ID: 1, ..Default::default() }, SerializedNode { ID: 2, ..Default::default() }],
        Edges: vec![SerializedEdge { ID: 3, FromNode: 1, ToNode: 2, Style: style.clone(), ..Default::default() }],
        ..Default::default()
    };
    let g = NewGraph();
    Deserialize(&bg(), Some(&sg), Some(&g)).unwrap();
    let out = Serialize(&bg(), Some(&g)).unwrap();
    assert_eq!(out.Edges[0].Style, style);
    let json = String::from_utf8(Marshal(&bg(), Some(&g)).unwrap()).unwrap();
    assert!(json.contains(r#""3d":{"value":"3"}"#));
}
