// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/nodeshape/shape.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;
use std::rc::Rc;

/// Go `nodeshape.LabelTier`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct LabelTier(pub i64);
pub const Good: LabelTier = LabelTier(0);
pub const OK: LabelTier = LabelTier(1);
pub const Unideal: LabelTier = LabelTier(2);
pub const Bad: LabelTier = LabelTier(3);

/// Go `nodeshape.Kind` (uint8).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct Kind(pub u8);
pub const Square: Kind = Kind(0);
pub const RealSquare: Kind = Kind(1);
pub const Parallelogram: Kind = Kind(2);
pub const Document: Kind = Kind(3);
pub const Cylinder: Kind = Kind(4);
pub const Queue: Kind = Kind(5);
pub const Page: Kind = Kind(6);
pub const Package: Kind = Kind(7);
pub const Step: Kind = Kind(8);
pub const Callout: Kind = Kind(9);
pub const StoredData: Kind = Kind(10);
pub const Person: Kind = Kind(11);
pub const C4Person: Kind = Kind(12);
pub const Diamond: Kind = Kind(13);
pub const Oval: Kind = Kind(14);
pub const Circle: Kind = Kind(15);
pub const Hexagon: Kind = Kind(16);
pub const Cloud: Kind = Kind(17);
pub const Table: Kind = Kind(18);
pub const Class: Kind = Kind(19);
pub const Text: Kind = Kind(20);
pub const Code: Kind = Kind(21);
pub const Image: Kind = Kind(22);

impl Kind {
    pub fn String(&self) -> String {
        use ::shape as s;
        match *self {
            Callout => s::CALLOUT_TYPE,
            Circle => s::CIRCLE_TYPE,
            Cloud => s::CLOUD_TYPE,
            Cylinder => s::CYLINDER_TYPE,
            Diamond => s::DIAMOND_TYPE,
            Document => s::DOCUMENT_TYPE,
            Hexagon => s::HEXAGON_TYPE,
            Image => s::IMAGE_TYPE,
            Oval => s::OVAL_TYPE,
            Package => s::PACKAGE_TYPE,
            Page => s::PAGE_TYPE,
            Parallelogram => s::PARALLELOGRAM_TYPE,
            Person => s::PERSON_TYPE,
            C4Person => s::C4_PERSON_TYPE,
            Queue => s::QUEUE_TYPE,
            RealSquare => s::REAL_SQUARE_TYPE,
            Square => s::SQUARE_TYPE,
            Step => s::STEP_TYPE,
            StoredData => s::STORED_DATA_TYPE,
            Text => s::TEXT_TYPE,
            Class => s::CLASS_TYPE,
            Table => s::TABLE_TYPE,
            Code => s::CODE_TYPE,
            _ => "",
        }
        .to_string()
    }
}

impl std::fmt::Display for Kind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.String())
    }
}

/// Go `nodeshape.Shape`: D2's `shape.Shape` (supertrait, re-exported as
/// [`D2Shape`]) plus TALA's port / label policy. Use as `Rc<dyn Shape>` ([`ShapeRc`]).
/// Go maps/nil slices map to `Map`/`Set`/`Vec` (nil == empty).
pub trait Shape: ::shape::Shape {
    /// Port groups in top, left, bottom, right order.
    fn SnapPointPercentages(&self) -> Vec<Vec<geo::RelativePointRef>>;
    fn MirroredPortIndices(&self) -> Map<i64, i64>;
    fn CenterPortIndices(&self) -> Vec<i64>;
    fn PortIndices(&self, orientation: geo::Orientation) -> Vec<i64>;
    fn CenterPortIndex(&self, orientation: geo::Orientation) -> i64;
    fn LabelPositionPreferences(&self, tier: LabelTier) -> Set<label::Position>;
    /// Go type assertion `s.(*shapeTable)`.
    fn as_table(&self) -> Option<&shapeTable> {
        None
    }
}

/// Go `nodeshape.Shape` interface value.
pub type ShapeRc = Rc<dyn Shape>;

pub(crate) fn set(items: &[label::Position]) -> Set<label::Position> {
    let mut s = gocompat::NewSetCap(items.len());
    for p in items {
        s.insert(*p);
    }
    s
}

pub(crate) fn mapOf(items: &[(i64, i64)]) -> Map<i64, i64> {
    let mut m = gocompat::NewMapCap(items.len());
    for (k, v) in items {
        m.insert(*k, *v);
    }
    m
}

/// `nodeshape.New(typeName, box)`: `(None, Kind(0), false)` for unsupported types.
pub fn New(typeName: &str, box_: geo::BoxRef) -> (Option<ShapeRc>, Kind, bool) {
    use ::shape as s;
    let base = s::NewShape(typeName, box_);
    let r: (ShapeRc, Kind) = match typeName {
        s::CALLOUT_TYPE => (Rc::new(shapeCallout { Shape: base }), Callout),
        s::CIRCLE_TYPE => (Rc::new(shapeCircle { Shape: base }), Circle),
        s::CLOUD_TYPE => (Rc::new(shapeCloud { Shape: base }), Cloud),
        s::CYLINDER_TYPE => (Rc::new(shapeCylinder { Shape: base }), Cylinder),
        s::DIAMOND_TYPE => (Rc::new(shapeDiamond { Shape: base }), Diamond),
        s::DOCUMENT_TYPE => (Rc::new(shapeDocument { Shape: base }), Document),
        s::HEXAGON_TYPE => (Rc::new(shapeHexagon { Shape: base }), Hexagon),
        s::IMAGE_TYPE => (Rc::new(shapeImage { Shape: base }), Image),
        s::OVAL_TYPE => (Rc::new(shapeOval { Shape: base }), Oval),
        s::PACKAGE_TYPE => (Rc::new(shapePackage { Shape: base }), Package),
        s::PAGE_TYPE => (Rc::new(shapePage { Shape: base }), Page),
        s::PARALLELOGRAM_TYPE => (Rc::new(shapeParallelogram { Shape: base }), Parallelogram),
        s::PERSON_TYPE => (Rc::new(shapePerson { Shape: base }), Person),
        s::C4_PERSON_TYPE => (Rc::new(shapeC4Person { Shape: base }), C4Person),
        s::QUEUE_TYPE => (Rc::new(shapeQueue { Shape: base }), Queue),
        s::REAL_SQUARE_TYPE => (Rc::new(shapeSquare { Shape: base }), RealSquare),
        s::SQUARE_TYPE => (Rc::new(shapeSquare { Shape: base }), Square),
        s::STEP_TYPE => (Rc::new(shapeStep { Shape: base }), Step),
        s::STORED_DATA_TYPE => (Rc::new(shapeStoredData { Shape: base }), StoredData),
        s::TEXT_TYPE => (Rc::new(shapeSquare { Shape: base }), Text),
        s::CLASS_TYPE => (Rc::new(shapeSquare { Shape: base }), Class),
        s::TABLE_TYPE => (Rc::new(shapeTable { shapeSquare: shapeSquare { Shape: base }, numColumns: std::cell::Cell::new(0) }), Table),
        s::CODE_TYPE => (Rc::new(shapeSquare { Shape: base }), Code),
        "" => (Rc::new(shapeSquare { Shape: base }), Square),
        _ => return (None, Kind(0), false),
    };
    (Some(r.0), r.1, true)
}
