// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/nodeshape/shape_test.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;
use ::shape as s;

#[test]
fn TestNew() {
    let tests: &[(&str, Kind)] = &[
        ("", Square),
        (s::SQUARE_TYPE, Square),
        (s::REAL_SQUARE_TYPE, RealSquare),
        (s::PARALLELOGRAM_TYPE, Parallelogram),
        (s::DOCUMENT_TYPE, Document),
        (s::CYLINDER_TYPE, Cylinder),
        (s::QUEUE_TYPE, Queue),
        (s::PAGE_TYPE, Page),
        (s::PACKAGE_TYPE, Package),
        (s::STEP_TYPE, Step),
        (s::CALLOUT_TYPE, Callout),
        (s::STORED_DATA_TYPE, StoredData),
        (s::PERSON_TYPE, Person),
        (s::C4_PERSON_TYPE, C4Person),
        (s::DIAMOND_TYPE, Diamond),
        (s::OVAL_TYPE, Oval),
        (s::CIRCLE_TYPE, Circle),
        (s::HEXAGON_TYPE, Hexagon),
        (s::CLOUD_TYPE, Cloud),
        (s::TABLE_TYPE, Table),
        (s::CLASS_TYPE, Class),
        (s::TEXT_TYPE, Text),
        (s::CODE_TYPE, Code),
        (s::IMAGE_TYPE, Image),
    ];
    for (typeName, want) in tests {
        let box_ = geo::NewBox(geo::NewPoint(10.0, 20.0), 100.0, 50.0);
        let (got, kind, ok) = New(typeName, box_.clone());
        assert!(ok);
        let got = got.unwrap();
        assert_eq!(kind, *want);
        assert_eq!(got.GetType(), *typeName);
        assert!(got.GetBox() == box_, "must retain the supplied box pointer");
        let wantKindString = if typeName.is_empty() { s::SQUARE_TYPE } else { typeName };
        assert_eq!(kind.String(), wantKindString);
    }
}

#[test]
fn TestNewRejectsUnsupportedShape() {
    let (got, _, ok) = New("unsupported", geo::NewBox(geo::NewPoint(0.0, 0.0), 10.0, 10.0));
    assert!(!ok && got.is_none());
}

#[test]
fn TestUnknownKindString() {
    assert_eq!(Kind(255).String(), "");
}

#[test]
fn TestTableShapeColumns() {
    let (got, _, ok) = New(s::TABLE_TYPE, geo::NewBox(geo::NewPoint(0.0, 0.0), 100.0, 100.0));
    assert!(ok);
    let got = got.unwrap();
    assert_eq!(NumColumns(&*got), 0);
    SetNumColumns(&*got, 4);
    assert_eq!(NumColumns(&*got), 4);
}

#[test]
fn TestSnapPointPercentages() {
    let p = tableSnapPointPercentages(4);
    assert_eq!(p[0].len(), 3);
    assert_eq!(p[1].len(), 4);
    assert_eq!(p[2].len(), 3);
    assert_eq!(p[3].len(), 4);
    let ys = [0.3, 0.5, 0.7, 0.9];
    for i in 0..4 {
        assert_eq!(p[1][i].b().YPercentage, ys[i]);
        assert_eq!(p[3][i].b().YPercentage, ys[i]);
    }
    let p = tableSnapPointPercentages(0);
    assert_eq!((p[0].len(), p[1].len(), p[2].len(), p[3].len()), (3, 1, 3, 1));
    assert_eq!(p[1][0].b().YPercentage, 0.5);
    assert_eq!(p[1][0].b().XPercentage, 0.0);
    assert_eq!(p[3][0].b().YPercentage, 0.5);
    assert_eq!(p[3][0].b().XPercentage, 1.0);
}

#[test]
fn TestCenterPortIndices() {
    assert_eq!(tableCenterPortIndices(4).len(), 0);
    for o in [geo::Top, geo::Left, geo::Bottom, geo::Right] {
        assert_eq!(tableCenterPortIndex(4, o), -1);
    }
    assert_eq!(tableCenterPortIndices(0), vec![1, 3, 5, 7]);
    assert_eq!(tableCenterPortIndex(0, geo::Top), 1);
    assert_eq!(tableCenterPortIndex(0, geo::Left), 3);
    assert_eq!(tableCenterPortIndex(0, geo::Bottom), 5);
    assert_eq!(tableCenterPortIndex(0, geo::Right), 7);
}

#[test]
fn TestMirroredPortIndices() {
    assert_eq!(tableMirroredPortIndices(4).len(), 0);
    let m = tableMirroredPortIndices(0);
    assert_eq!(m[&1], 5);
    assert_eq!(m[&5], 1);
    assert_eq!(m[&7], 3);
    assert_eq!(m[&3], 7);
}

#[test]
fn TestPortIndices() {
    assert_eq!(tablePortIndices(5, geo::Top), vec![0, 1, 2]);
    assert_eq!(tablePortIndices(5, geo::Left), vec![3, 4, 5, 6, 7]);
    assert_eq!(tablePortIndices(5, geo::Bottom), vec![8, 9, 10]);
    assert_eq!(tablePortIndices(5, geo::Right), vec![11, 12, 13, 14, 15]);
    assert_eq!(tablePortIndices(5, geo::TopLeft), vec![0, 1, 2, 3, 4, 5, 6, 7]);
    assert_eq!(tablePortIndices(5, geo::TopRight), vec![0, 1, 2, 11, 12, 13, 14, 15]);
    assert_eq!(tablePortIndices(5, geo::BottomLeft), vec![8, 9, 10, 3, 4, 5, 6, 7]);
    assert_eq!(tablePortIndices(5, geo::BottomRight), vec![8, 9, 10, 11, 12, 13, 14, 15]);
    assert_eq!(tablePortIndices(0, geo::Top), vec![0, 1, 2]);
    assert_eq!(tablePortIndices(0, geo::Left), vec![3]);
    assert_eq!(tablePortIndices(0, geo::Bottom), vec![4, 5, 6]);
    assert_eq!(tablePortIndices(0, geo::Right), vec![7]);
}
