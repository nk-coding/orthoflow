// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/loops/loops_test.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;
use layoutgraph::{Label, LabelRef, NewEdge, NewNode};

fn ids(v: &[EdgeRef]) -> Vec<usize> {
    v.iter().map(|e| e.addr()).collect()
}

#[test]
fn TestEdgesInOrderPreservesEqualLabelOrder() {
    let node = NewNode(1, 10.0, 10.0);
    let newLabeledEdge = |width: f64, height: f64| {
        let edge = NewEdge(&node, &node);
        edge.bm().Label = Some(LabelRef::new(Label::new("", label::Position::default(), width, height)));
        edge
    };
    let firstEqual = newLabeledEdge(10.0, 10.0);
    let larger = newLabeledEdge(20.0, 10.0);
    let secondEqual = newLabeledEdge(20.0, 5.0);
    let smaller = newLabeledEdge(5.0, 10.0);
    let input = vec![firstEqual.clone(), larger.clone(), secondEqual.clone(), smaller.clone()];
    let got = edgesInOrder(&input);
    let want = vec![smaller.clone(), firstEqual.clone(), secondEqual.clone(), larger.clone()];
    assert_eq!(ids(&got), ids(&want));
    assert_eq!(ids(&input), ids(&[firstEqual, larger, secondEqual, smaller]));
}

#[test]
fn TestEdgesInOrderPreservesOneSidedArrowOrder() {
    let node = NewNode(1, 10.0, 10.0);
    let targetOnly = NewEdge(&node, &node);
    targetOnly.bm().TargetArrowhead = layoutgraph::TriangleArrowhead.into();
    let sourceOnly = NewEdge(&node, &node);
    sourceOnly.bm().SourceArrowhead = layoutgraph::TriangleArrowhead.into();
    let plain = NewEdge(&node, &node);
    let got = edgesInOrder(&[targetOnly.clone(), plain.clone(), sourceOnly.clone()]);
    assert_eq!(ids(&got), ids(&[targetOnly, sourceOnly, plain]));
}

#[test]
fn TestEdgesInOrderPreservesNaNLabelTie() {
    let node = NewNode(1, 10.0, 10.0);
    let nonFinite = NewEdge(&node, &node);
    nonFinite.bm().Label = Some(LabelRef::new(Label::new("", label::Position::default(), f64::NAN, 10.0)));
    let finite = NewEdge(&node, &node);
    finite.bm().Label = Some(LabelRef::new(Label::new("", label::Position::default(), 10.0, 10.0)));
    let got = edgesInOrder(&[nonFinite.clone(), finite.clone()]);
    assert!(got[0] == nonFinite && got[1] == finite);
}
