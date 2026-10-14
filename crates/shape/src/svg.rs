// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from lib/svg/path.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of the geometry part of `d2/lib/svg/path.go` (used by shape perimeters).
use crate::*;
use geo::{Intersectable, Point, PointRef};
use gocompat::math;
use gocompat::strconv::FormatFloatV;
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct PathCommandKind(pub u8);
pub const PathCommandMove: PathCommandKind = PathCommandKind(0);
pub const PathCommandLine: PathCommandKind = PathCommandKind(1);
pub const PathCommandCubic: PathCommandKind = PathCommandKind(2);
pub const PathCommandClose: PathCommandKind = PathCommandKind(3);

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PathCommand {
    pub Kind: PathCommandKind,
    pub Control1: Point,
    pub Control2: Point,
    pub End: Point,
}

/// Go `*svg.SvgPathContext`.
pub struct SvgPathContext {
    pub Path: Vec<Rc<dyn Intersectable>>,
    pub Commands: Vec<String>,
    pub(crate) commands: Vec<PathCommand>,
    pub Start: Option<PointRef>,
    pub Current: Option<PointRef>,
    pub TopLeft: PointRef,
    pub ScaleX: f64,
    pub ScaleY: f64,
}

pub(crate) fn chopPrecision(f: f64) -> f64 {
    let result = math::Round(((f * 10000.0) as f32) as f64 / 10000.0);
    if result == 0.0 {
        return 0.0;
    }
    result
}

pub fn NewSVGPathContext(tl: &PointRef, sx: f64, sy: f64) -> SvgPathContext {
    SvgPathContext {
        Path: Vec::new(),
        Commands: Vec::new(),
        commands: Vec::new(),
        Start: None,
        Current: None,
        TopLeft: tl.Copy(),
        ScaleX: sx,
        ScaleY: sy,
    }
}

impl SvgPathContext {
    fn cur(&self) -> &PointRef {
        self.Current.as_ref().expect("nil pointer dereference: SvgPathContext.Current")
    }

    pub fn Relative(&self, base: &PointRef, dx: f64, dy: f64) -> PointRef {
        let b = base.get();
        geo::NewPoint(chopPrecision(b.X + self.ScaleX * dx), chopPrecision(b.Y + self.ScaleY * dy))
    }

    pub fn Absolute(&self, x: f64, y: f64) -> PointRef {
        self.Relative(&self.TopLeft, x, y)
    }

    pub fn StartAt(&mut self, p: PointRef) {
        let pv = p.get();
        self.Commands.push(format!("M {} {}", FormatFloatV(pv.X), FormatFloatV(pv.Y)));
        self.commands.push(PathCommand { Kind: PathCommandMove, End: pv, ..Default::default() });
        self.Current = Some(p.Copy());
        self.Start = Some(p);
    }

    pub fn Z(&mut self) {
        let start = self.Start.as_ref().expect("nil pointer dereference").clone();
        self.Path.push(Rc::new(geo::NewSegment(self.cur().Copy(), start.Copy())));
        self.Commands.push("Z".to_string());
        self.commands.push(PathCommand { Kind: PathCommandClose, ..Default::default() });
        self.Current = Some(start.Copy());
    }

    pub fn L(&mut self, isLowerCase: bool, x: f64, y: f64) {
        let endPoint = if isLowerCase { self.Relative(self.cur(), x, y) } else { self.Absolute(x, y) };
        self.Path.push(Rc::new(geo::NewSegment(self.cur().Copy(), endPoint.clone())));
        let e = endPoint.get();
        self.Commands.push(format!("L {} {}", FormatFloatV(e.X), FormatFloatV(e.Y)));
        self.commands.push(PathCommand { Kind: PathCommandLine, End: e, ..Default::default() });
        self.Current = Some(endPoint.Copy());
    }

    pub fn C(&mut self, isLowerCase: bool, x1: f64, y1: f64, x2: f64, y2: f64, x3: f64, y3: f64) {
        let p = |c: &SvgPathContext, x: f64, y: f64| -> PointRef {
            if isLowerCase {
                c.Relative(c.cur(), x, y)
            } else {
                c.Absolute(x, y)
            }
        };
        let points = vec![self.cur().Copy(), p(self, x1, y1), p(self, x2, y2), p(self, x3, y3)];
        self.Path.push(Rc::new(geo::NewBezierCurve(&points)));
        let (a, b, c) = (points[1].get(), points[2].get(), points[3].get());
        self.Commands.push(format!(
            "C {} {} {} {} {} {}",
            FormatFloatV(a.X),
            FormatFloatV(a.Y),
            FormatFloatV(b.X),
            FormatFloatV(b.Y),
            FormatFloatV(c.X),
            FormatFloatV(c.Y)
        ));
        self.commands.push(PathCommand { Kind: PathCommandCubic, Control1: a, Control2: b, End: c });
        self.Current = Some(points[3].Copy());
    }

    pub fn H(&mut self, isLowerCase: bool, x: f64) {
        let endPoint;
        if isLowerCase {
            endPoint = self.Relative(self.cur(), x, 0.0);
        } else {
            endPoint = self.Absolute(x, 0.0);
            let cy = self.cur().Y();
            endPoint.bm().Y = cy;
        }
        self.Path.push(Rc::new(geo::NewSegment(self.cur().Copy(), endPoint.Copy())));
        let e = endPoint.get();
        self.Commands.push(format!("H {}", FormatFloatV(e.X)));
        self.commands.push(PathCommand { Kind: PathCommandLine, End: e, ..Default::default() });
        self.Current = Some(endPoint.Copy());
    }

    pub fn V(&mut self, isLowerCase: bool, y: f64) {
        let endPoint;
        if isLowerCase {
            endPoint = self.Relative(self.cur(), 0.0, y);
        } else {
            endPoint = self.Absolute(0.0, y);
            let cx = self.cur().X();
            endPoint.bm().X = cx;
        }
        self.Path.push(Rc::new(geo::NewSegment(self.cur().Copy(), endPoint.clone())));
        let e = endPoint.get();
        self.Commands.push(format!("V {}", FormatFloatV(e.Y)));
        self.commands.push(PathCommand { Kind: PathCommandLine, End: e, ..Default::default() });
        self.Current = Some(endPoint.Copy());
    }

    pub fn PathCommands(&self) -> Vec<PathCommand> {
        self.commands.clone()
    }

    pub fn PathData(&self) -> String {
        self.Commands.join(" ")
    }
}
