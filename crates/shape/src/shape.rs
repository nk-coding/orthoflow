// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from lib/shape/shape.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;
use geo::{BoxRef, Intersectable, Point, PointRef, Segment, VectorExt};
use gocompat::math;
use std::rc::Rc;

pub const SQUARE_TYPE: &str = "Square";
pub const REAL_SQUARE_TYPE: &str = "RealSquare";
pub const PARALLELOGRAM_TYPE: &str = "Parallelogram";
pub const DOCUMENT_TYPE: &str = "Document";
pub const CYLINDER_TYPE: &str = "Cylinder";
pub const QUEUE_TYPE: &str = "Queue";
pub const PAGE_TYPE: &str = "Page";
pub const PACKAGE_TYPE: &str = "Package";
pub const STEP_TYPE: &str = "Step";
pub const CALLOUT_TYPE: &str = "Callout";
pub const STORED_DATA_TYPE: &str = "StoredData";
pub const PERSON_TYPE: &str = "Person";
pub const C4_PERSON_TYPE: &str = "C4Person";
pub const DIAMOND_TYPE: &str = "Diamond";
pub const OVAL_TYPE: &str = "Oval";
pub const CIRCLE_TYPE: &str = "Circle";
pub const HEXAGON_TYPE: &str = "Hexagon";
pub const CLOUD_TYPE: &str = "Cloud";
pub const TABLE_TYPE: &str = "Table";
pub const CLASS_TYPE: &str = "Class";
pub const TEXT_TYPE: &str = "Text";
pub const CODE_TYPE: &str = "Code";
pub const IMAGE_TYPE: &str = "Image";

pub(crate) const defaultPadding: f64 = 40.0;

/// Go `shape.Shape` interface. Use as `Rc<dyn Shape>` ([`ShapeRc`]).
///
/// Default methods are `baseShape`'s; `GetInsidePlacement`'s default dispatches
/// `GetInnerBox` dynamically, exactly like Go's `(*s.FullShape).GetInnerBox()`.
pub trait Shape {
    /// The embedded `*baseShape`.
    fn base(&self) -> &baseShape;

    fn Is(&self, shapeType: &str) -> bool {
        self.base().Type == shapeType
    }
    fn GetType(&self) -> String {
        self.base().Type.clone()
    }
    fn AspectRatio1(&self) -> bool {
        false
    }
    fn IsRectangular(&self) -> bool {
        false
    }
    fn GetBox(&self) -> BoxRef {
        self.base().Box.clone()
    }
    fn GetInnerBox(&self) -> BoxRef {
        self.base().Box.clone()
    }
    fn GetInnerBoxForContent(&self, _width: f64, _height: f64) -> Option<BoxRef> {
        None
    }
    fn SetInnerBoxAspectRatio(&self, _aspectRatio: f64) {}
    fn GetInsidePlacement(&self, _width: f64, _height: f64, paddingX: f64, paddingY: f64) -> Point {
        let innerTL = self.GetInnerBox().TL().get();
        Point::new(innerTL.X + paddingX / 2.0, innerTL.Y + paddingY / 2.0)
    }
    fn GetDimensionsToFit(&self, width: f64, height: f64, paddingX: f64, paddingY: f64) -> (f64, f64) {
        (math::Ceil(width + paddingX), math::Ceil(height + paddingY))
    }
    fn GetDefaultPadding(&self) -> (f64, f64) {
        (defaultPadding, defaultPadding)
    }
    fn Perimeter(&self) -> Vec<Rc<dyn Intersectable>> {
        Vec::new()
    }
    fn GetSVGPathData(&self) -> Vec<String> {
        Vec::new()
    }
    /// `PathCommandProvider.GetPathCommands`; `None` when the shape does not implement it.
    fn GetPathCommands(&self) -> Option<Vec<Vec<svg::PathCommand>>> {
        None
    }
}

/// Go `shape.Shape` interface value.
pub type ShapeRc = Rc<dyn Shape>;

/// `shape.GetPathCommands(s)`
pub fn GetPathCommands(s: &dyn Shape) -> Option<Vec<Vec<svg::PathCommand>>> {
    s.GetPathCommands()
}

pub(crate) fn pathCommands(paths: &[&svg::SvgPathContext]) -> Vec<Vec<svg::PathCommand>> {
    paths.iter().map(|p| p.PathCommands()).collect()
}

/// Go `baseShape` (always used through a pointer, shared by shape copies).
#[derive(Debug)]
pub struct baseShape {
    pub Type: String,
    pub Box: BoxRef,
}

impl baseShape {
    pub(crate) fn new(t: &str, box_: BoxRef) -> Rc<baseShape> {
        Rc::new(baseShape { Type: t.to_string(), Box: box_ })
    }
}

/// Expands (inside an `impl shape::Shape for T` block) to forwarding
/// implementations of every `shape::Shape` method to the field
/// `$field: Rc<dyn shape::Shape>` (Go interface embedding). Only usable when the
/// type overrides none of them.
#[macro_export]
macro_rules! delegate_shape_methods {
    ($field:ident) => {
        fn base(&self) -> &$crate::baseShape {
            self.$field.base()
        }
        fn Is(&self, shapeType: &str) -> bool {
            self.$field.Is(shapeType)
        }
        fn GetType(&self) -> String {
            self.$field.GetType()
        }
        fn AspectRatio1(&self) -> bool {
            self.$field.AspectRatio1()
        }
        fn IsRectangular(&self) -> bool {
            self.$field.IsRectangular()
        }
        fn GetBox(&self) -> ::geo::BoxRef {
            self.$field.GetBox()
        }
        fn GetInnerBox(&self) -> ::geo::BoxRef {
            self.$field.GetInnerBox()
        }
        fn GetInnerBoxForContent(&self, width: f64, height: f64) -> Option<::geo::BoxRef> {
            self.$field.GetInnerBoxForContent(width, height)
        }
        fn SetInnerBoxAspectRatio(&self, aspectRatio: f64) {
            self.$field.SetInnerBoxAspectRatio(aspectRatio)
        }
        fn GetInsidePlacement(&self, width: f64, height: f64, paddingX: f64, paddingY: f64) -> ::geo::Point {
            self.$field.GetInsidePlacement(width, height, paddingX, paddingY)
        }
        fn GetDimensionsToFit(&self, width: f64, height: f64, paddingX: f64, paddingY: f64) -> (f64, f64) {
            self.$field.GetDimensionsToFit(width, height, paddingX, paddingY)
        }
        fn GetDefaultPadding(&self) -> (f64, f64) {
            self.$field.GetDefaultPadding()
        }
        fn Perimeter(&self) -> Vec<::std::rc::Rc<dyn ::geo::Intersectable>> {
            self.$field.Perimeter()
        }
        fn GetSVGPathData(&self) -> Vec<String> {
            self.$field.GetSVGPathData()
        }
        fn GetPathCommands(&self) -> Option<Vec<Vec<$crate::svg::PathCommand>>> {
            self.$field.GetPathCommands()
        }
    };
}

/// `shape.NewShape(shapeType, box)`
pub fn NewShape(shapeType: &str, box_: BoxRef) -> ShapeRc {
    match shapeType {
        CALLOUT_TYPE => NewCallout(box_),
        CIRCLE_TYPE => NewCircle(box_),
        CLASS_TYPE => NewClass(box_),
        CLOUD_TYPE => NewCloud(box_),
        CODE_TYPE => NewCode(box_),
        CYLINDER_TYPE => NewCylinder(box_),
        DIAMOND_TYPE => NewDiamond(box_),
        DOCUMENT_TYPE => NewDocument(box_),
        HEXAGON_TYPE => NewHexagon(box_),
        IMAGE_TYPE => NewImage(box_),
        OVAL_TYPE => NewOval(box_),
        PACKAGE_TYPE => NewPackage(box_),
        PAGE_TYPE => NewPage(box_),
        PARALLELOGRAM_TYPE => NewParallelogram(box_),
        PERSON_TYPE => NewPerson(box_),
        C4_PERSON_TYPE => NewC4Person(box_),
        QUEUE_TYPE => NewQueue(box_),
        REAL_SQUARE_TYPE => NewRealSquare(box_),
        STEP_TYPE => NewStep(box_),
        STORED_DATA_TYPE => NewStoredData(box_),
        SQUARE_TYPE => NewSquare(box_),
        TABLE_TYPE => NewTable(box_),
        TEXT_TYPE => NewText(box_),
        _ => Rc::new(shapeSquare { b: baseShape::new(shapeType, box_) }),
    }
}

/// `shape.TraceToShapeBorder(shape, rectBorderPoint, prevPoint)`
pub fn TraceToShapeBorder(shape: &dyn Shape, rectBorderPoint: &PointRef, prevPoint: &PointRef) -> PointRef {
    if shape.Is("") || shape.IsRectangular() {
        return rectBorderPoint.clone();
    }
    let mut scaleSize = shape.GetBox().Width();
    if prevPoint.X() == rectBorderPoint.X() {
        scaleSize = shape.GetBox().Height();
    }
    let mut vector = prevPoint.VectorTo(rectBorderPoint);
    vector = vector.AddLength(scaleSize);
    let extendedSegment = Segment { Start: prevPoint.clone(), End: prevPoint.AddVector(&vector) };

    let mut closestD = math::Inf(1);
    let mut closestPoint = rectBorderPoint.clone();

    let rb = rectBorderPoint.get();
    for perimeterSegment in shape.Perimeter() {
        for intersectingPoint in perimeterSegment.Intersections(&extendedSegment) {
            let ip = intersectingPoint.get();
            let d = geo::EuclideanDistance(rb.X, rb.Y, ip.X, ip.Y);
            if d < closestD {
                closestD = d;
                closestPoint = intersectingPoint;
            }
        }
    }
    // GO-ALIAS: when no intersection is found this truncates rectBorderPoint itself.
    closestPoint.TruncateFloat32();
    let cp = closestPoint.get();
    geo::NewPoint(math::Round(cp.X), math::Round(cp.Y))
}

pub fn LimitAR(width: f64, height: f64, aspectRatio: f64) -> (f64, f64) {
    let (mut width, mut height) = (width, height);
    if width > aspectRatio * height {
        height = math::Round(width / aspectRatio);
    } else if height > aspectRatio * width {
        width = math::Round(height / aspectRatio);
    }
    (width, height)
}
