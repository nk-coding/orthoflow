// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/nodeshape/shape_table.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;
use gocompat::math;
use std::cell::Cell;

/// Go `nodeshape.shapeTable` (embeds `*shapeSquare`).
pub struct shapeTable {
    pub shapeSquare: shapeSquare,
    pub(crate) numColumns: Cell<i64>,
}

impl ::shape::Shape for shapeTable {
    fn base(&self) -> &::shape::baseShape {
        self.shapeSquare.Shape.base()
    }
    fn Is(&self, shapeType: &str) -> bool {
        self.shapeSquare.Shape.Is(shapeType)
    }
    fn GetType(&self) -> String {
        self.shapeSquare.Shape.GetType()
    }
    fn AspectRatio1(&self) -> bool {
        self.shapeSquare.Shape.AspectRatio1()
    }
    fn IsRectangular(&self) -> bool {
        self.shapeSquare.Shape.IsRectangular()
    }
    fn GetBox(&self) -> geo::BoxRef {
        self.shapeSquare.Shape.GetBox()
    }
    fn GetInnerBox(&self) -> geo::BoxRef {
        self.shapeSquare.Shape.GetInnerBox()
    }
    fn GetInnerBoxForContent(&self, width: f64, height: f64) -> Option<geo::BoxRef> {
        self.shapeSquare.Shape.GetInnerBoxForContent(width, height)
    }
    fn SetInnerBoxAspectRatio(&self, aspectRatio: f64) {
        self.shapeSquare.Shape.SetInnerBoxAspectRatio(aspectRatio)
    }
    fn GetInsidePlacement(&self, width: f64, height: f64, paddingX: f64, paddingY: f64) -> geo::Point {
        self.shapeSquare.Shape.GetInsidePlacement(width, height, paddingX, paddingY)
    }
    fn GetDimensionsToFit(&self, width: f64, height: f64, paddingX: f64, paddingY: f64) -> (f64, f64) {
        self.shapeSquare.Shape.GetDimensionsToFit(width, height, paddingX, paddingY)
    }
    fn GetDefaultPadding(&self) -> (f64, f64) {
        self.shapeSquare.Shape.GetDefaultPadding()
    }
    fn Perimeter(&self) -> Vec<std::rc::Rc<dyn geo::Intersectable>> {
        self.shapeSquare.Shape.Perimeter()
    }
    fn GetSVGPathData(&self) -> Vec<String> {
        self.shapeSquare.Shape.GetSVGPathData()
    }
    fn GetPathCommands(&self) -> Option<Vec<Vec<::shape::svg::PathCommand>>> {
        self.shapeSquare.Shape.GetPathCommands()
    }
}

/// `nodeshape.NumColumns(s)`: 0 for non-table shapes.
pub fn NumColumns(s: &dyn Shape) -> i64 {
    match s.as_table() {
        None => 0,
        Some(t) => t.numColumns.get(),
    }
}

/// `nodeshape.SetNumColumns(s, n)`: no-op for non-table shapes.
pub fn SetNumColumns(s: &dyn Shape, numColumns: i64) {
    if let Some(t) = s.as_table() {
        t.numColumns.set(numColumns);
    }
}

/// `nodeshape.TablePortIndex(s, orientation, columnIndex) (index, ok)`
pub fn TablePortIndex(s: &dyn Shape, orientation: geo::Orientation, columnIndex: i64) -> (i64, bool) {
    let table = match s.as_table() {
        None => return (0, false),
        Some(t) => t,
    };
    let mut numColumns = table.numColumns.get();
    if numColumns == 0 {
        numColumns = 1;
    }
    if columnIndex < 0 || columnIndex >= numColumns {
        panic!("table column port index out of range");
    }
    match orientation {
        geo::Left => (3 + columnIndex, true),
        geo::Right => (6 + numColumns + columnIndex, true),
        _ => (0, false),
    }
}

/// `nodeshape.TableColumnPortValue(s, orientation, columnIndex) (port, ok)`
pub fn TableColumnPortValue(s: &dyn Shape, orientation: geo::Orientation, columnIndex: i64) -> (geo::Point, bool) {
    let table = match s.as_table() {
        Some(t) if orientation == geo::Left || orientation == geo::Right => t,
        _ => return (geo::Point::default(), false),
    };
    let numColumns = table.numColumns.get();
    if numColumns == 0 {
        if columnIndex != 0 {
            panic!("table column port index out of range");
        }
    } else if columnIndex < 0 || columnIndex >= numColumns {
        panic!("table column port index out of range");
    }
    let mut yPercentage = 0.5;
    if numColumns > 0 {
        let rowHeightPercentage = 1.0 / (numColumns as f64 + 1.0);
        let mut percentage = rowHeightPercentage + rowHeightPercentage / 2.0;
        for _ in 0..columnIndex {
            percentage += rowHeightPercentage;
        }
        percentage = math::Round(percentage * 10_000.0) / 10_000.0;
        yPercentage = geo::TruncateDecimals(percentage);
    }
    let mut xPercentage = 0.0;
    if orientation == geo::Right {
        xPercentage = 1.0;
    }
    let b = ::shape::Shape::GetBox(table).get();
    let tl = b.TL().get();
    (geo::Point { X: tl.X + math::Round(b.Width * xPercentage), Y: tl.Y + math::Round(b.Height * yPercentage) }, true)
}


pub(crate) fn tableSnapPointPercentages(numColumns: i64) -> Vec<Vec<geo::RelativePointRef>> {
    let rp = geo::NewRelativePoint;
    let mut left: Vec<geo::RelativePointRef>;
    let mut right: Vec<geo::RelativePointRef>;
    if numColumns == 0 {
        left = vec![rp(0.0, 0.5)];
        right = vec![rp(1.0, 0.5)];
    } else {
        left = Vec::with_capacity(numColumns as usize + 1);
        right = Vec::with_capacity(numColumns as usize + 1);
        let rowHeightPercentage = 1.0 / (numColumns as f64 + 1.0);
        let midRowPercentage = rowHeightPercentage / 2.0;
        let mut perc = rowHeightPercentage + midRowPercentage;
        while perc < 1.0 {
            let h = math::Round(perc * 10_000.0) / 10_000.0;
            left.push(rp(0.0, h));
            right.push(rp(1.0, h));
            perc += rowHeightPercentage;
        }
    }
    vec![vec![rp(0.25, 0.0), rp(0.5, 0.0), rp(0.75, 0.0)], left, vec![rp(0.25, 1.0), rp(0.5, 1.0), rp(0.75, 1.0)], right]
}

pub(crate) fn tableCenterPortIndices(numColumns: i64) -> Vec<i64> {
    if numColumns > 0 {
        return Vec::new();
    }
    vec![1, 3, 5, 7]
}

pub(crate) fn tableCenterPortIndex(numColumns: i64, o: geo::Orientation) -> i64 {
    if numColumns > 0 {
        return -1;
    }
    let centerPorts = tableCenterPortIndices(numColumns);
    let centerPortsIndex: usize = match o {
        geo::Top => 0,
        geo::Left => 1,
        geo::Bottom => 2,
        geo::Right => 3,
        _ => return -1,
    };
    centerPorts[centerPortsIndex]
}

pub(crate) fn tableMirroredPortIndices(numColumns: i64) -> Map<i64, i64> {
    if numColumns > 0 {
        return Map::default();
    }
    let c = tableCenterPortIndices(numColumns);
    mapOf(&[(c[0], c[2]), (c[2], c[0]), (c[1], c[3]), (c[3], c[1])])
}

pub(crate) fn tablePortIndices(numColumns: i64, orientation: geo::Orientation) -> Vec<i64> {
    let mut numColumns = numColumns;
    if numColumns == 0 {
        numColumns = 1;
    }
    let topIndices: Vec<i64> = vec![0, 1, 2];
    let firstLeftIndex = topIndices[2] + 1;
    let leftIndices: Vec<i64> = (0..numColumns).map(|i| firstLeftIndex + i).collect();
    let firstBottomIndex = leftIndices[numColumns as usize - 1] + 1;
    let bottomIndices: Vec<i64> = (0..3).map(|i| firstBottomIndex + i).collect();
    let firstRightIndex = bottomIndices[2] + 1;
    let rightIndices: Vec<i64> = (0..numColumns).map(|i| firstRightIndex + i).collect();
    let cat = |a: &Vec<i64>, b: &Vec<i64>| -> Vec<i64> { a.iter().chain(b.iter()).copied().collect() };
    match orientation {
        geo::Top => topIndices,
        geo::Left => leftIndices,
        geo::Bottom => bottomIndices,
        geo::Right => rightIndices,
        geo::TopLeft => cat(&topIndices, &leftIndices),
        geo::TopRight => cat(&topIndices, &rightIndices),
        geo::BottomLeft => cat(&bottomIndices, &leftIndices),
        geo::BottomRight => cat(&bottomIndices, &rightIndices),
        _ => Vec::new(),
    }
}

impl Shape for shapeTable {
    fn SnapPointPercentages(&self) -> Vec<Vec<geo::RelativePointRef>> {
        tableSnapPointPercentages(self.numColumns.get())
    }
    fn CenterPortIndices(&self) -> Vec<i64> {
        tableCenterPortIndices(self.numColumns.get())
    }
    fn CenterPortIndex(&self, o: geo::Orientation) -> i64 {
        tableCenterPortIndex(self.numColumns.get(), o)
    }
    fn MirroredPortIndices(&self) -> Map<i64, i64> {
        tableMirroredPortIndices(self.numColumns.get())
    }
    fn PortIndices(&self, orientation: geo::Orientation) -> Vec<i64> {
        tablePortIndices(self.numColumns.get(), orientation)
    }
    /// promoted from shapeSquare
    fn LabelPositionPreferences(&self, tier: LabelTier) -> Set<label::Position> {
        shapeSquare_LabelPositionPreferences(tier)
    }
    fn as_table(&self) -> Option<&shapeTable> {
        Some(self)
    }
}
