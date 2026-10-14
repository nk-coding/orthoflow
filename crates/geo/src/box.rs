// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from lib/geo/box.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

/// Go `geo.Box` (value; copying shares `TopLeft`, exactly like Go). `*geo.Box` is [`BoxRef`].
/// `TopLeft` may be nil in Go (e.g. unplaced nodes), hence `Option`; use [`Box::TL`]
/// where Go dereferences it unconditionally.
#[derive(Clone, Debug, Default)]
pub struct Box {
    pub TopLeft: Option<PointRef>,
    pub Width: f64,
    pub Height: f64,
}

gocompat::go_ptr!(BoxRef, Box);

/// `geo.NewBox(tl, width, height)` → `*Box`
/// `tl` accepts a `PointRef` or an `Option<PointRef>` (nil).
pub fn NewBox(tl: impl Into<Option<PointRef>>, width: f64, height: f64) -> BoxRef {
    BoxRef::new(Box { TopLeft: tl.into(), Width: width, Height: height })
}

impl Box {
    pub fn new(tl: impl Into<Option<PointRef>>, width: f64, height: f64) -> Box {
        Box { TopLeft: tl.into(), Width: width, Height: height }
    }

    /// `b.TopLeft` dereferenced as Go would (panics on nil like a Go nil dereference).
    #[inline]
    #[track_caller]
    pub fn TL(&self) -> &PointRef {
        self.TopLeft.as_ref().expect("nil pointer dereference: Box.TopLeft")
    }

    /// `b.Copy()` (deep copies TopLeft) → `*Box`
    pub fn Copy(&self) -> BoxRef {
        NewBox(self.TL().Copy(), self.Width, self.Height)
    }

    /// Value copy with a fresh TopLeft (same as `*b.Copy()`).
    pub fn CopyValue(&self) -> Box {
        Box { TopLeft: Some(self.TL().Copy()), Width: self.Width, Height: self.Height }
    }

    pub fn Center(&self) -> PointRef {
        let tl = self.TL().b();
        NewPoint(tl.X + self.Width / 2.0, tl.Y + self.Height / 2.0)
    }

    pub fn Intersects(&self, s: &Segment, buffer: f64) -> bool {
        let tl0 = self.TL().get();
        let tl = Point::new(tl0.X - buffer, tl0.Y - buffer);
        let tr = Point::new(tl.X + self.Width + buffer * 2.0, tl.Y);
        let br = Point::new(tr.X, tr.Y + self.Height + buffer * 2.0);
        let bl = Point::new(tl.X, br.Y);
        let (ss, se) = (s.Start.get(), s.End.get());
        if IntersectionPointV(&ss, &se, &tl, &tr).is_some() {
            return true;
        }
        if IntersectionPointV(&ss, &se, &tr, &br).is_some() {
            return true;
        }
        if IntersectionPointV(&ss, &se, &br, &bl).is_some() {
            return true;
        }
        if IntersectionPointV(&ss, &se, &bl, &tl).is_some() {
            return true;
        }
        false
    }

    pub fn Intersections(&self, s: &Segment) -> Vec<PointRef> {
        let mut pts = Vec::new();
        let tl = self.TL().get();
        let tr = Point::new(tl.X + self.Width, tl.Y);
        let br = Point::new(tr.X, tr.Y + self.Height);
        let bl = Point::new(tl.X, br.Y);
        let (ss, se) = (s.Start.get(), s.End.get());
        if let Some(p) = IntersectionPointV(&ss, &se, &tl, &tr) {
            pts.push(PointRef::new(p));
        }
        if let Some(p) = IntersectionPointV(&ss, &se, &tr, &br) {
            pts.push(PointRef::new(p));
        }
        if let Some(p) = IntersectionPointV(&ss, &se, &br, &bl) {
            pts.push(PointRef::new(p));
        }
        if let Some(p) = IntersectionPointV(&ss, &se, &bl, &tl) {
            pts.push(PointRef::new(p));
        }
        pts
    }

    pub fn ToString(&self) -> String {
        format!("{{TopLeft: {}, Width: {:.0}, Height: {:.0}}}", PointToString(self.TopLeft.as_ref()), self.Width, self.Height)
    }

    pub fn Contains(&self, p: &PointRef) -> bool {
        self.ContainsV(&p.b())
    }

    pub fn ContainsV(&self, p: &Point) -> bool {
        let tl = self.TL().b();
        !(p.X < tl.X || tl.X + self.Width < p.X || p.Y < tl.Y || tl.Y + self.Height < p.Y)
    }

    pub fn Overlaps(&self, b2: &Box) -> bool {
        let t1 = self.TL().b();
        let t2 = b2.TL().b();
        (t1.X < (t2.X + b2.Width))
            && ((t1.X + self.Width) > t2.X)
            && (t1.Y < (t2.Y + b2.Height))
            && ((t1.Y + self.Height) > t2.Y)
    }
}

impl BoxRef {
    /// Shallow value copy `*b` (shares TopLeft, like Go).
    #[inline]
    pub fn get(&self) -> Box {
        self.b().clone()
    }
    /// `b.TopLeft` (panics if nil).
    #[inline]
    #[track_caller]
    pub fn TL(&self) -> PointRef {
        self.b().TL().clone()
    }
    /// `b.TopLeft` (possibly nil).
    #[inline]
    pub fn TopLeft(&self) -> Option<PointRef> {
        self.b().TopLeft.clone()
    }
    #[inline]
    pub fn Width(&self) -> f64 {
        self.b().Width
    }
    #[inline]
    pub fn Height(&self) -> f64 {
        self.b().Height
    }
    pub fn Copy(&self) -> BoxRef {
        self.b().Copy()
    }
    pub fn Center(&self) -> PointRef {
        self.b().Center()
    }
    pub fn Intersects(&self, s: &Segment, buffer: f64) -> bool {
        self.b().Intersects(s, buffer)
    }
    pub fn Intersections(&self, s: &Segment) -> Vec<PointRef> {
        self.b().Intersections(s)
    }
    pub fn ToString(&self) -> String {
        self.b().ToString()
    }
    pub fn Contains(&self, p: &PointRef) -> bool {
        self.b().Contains(p)
    }
    pub fn Overlaps(&self, b2: &Box) -> bool {
        self.b().Overlaps(b2)
    }
}

/// nil-aware `b.Copy()`.
pub fn BoxCopy(b: Option<&BoxRef>) -> Option<BoxRef> {
    b.map(|b| b.Copy())
}

/// nil-aware `b.ToString()`.
pub fn BoxToString(b: Option<&BoxRef>) -> String {
    match b {
        None => String::new(),
        Some(b) => b.ToString(),
    }
}

impl Intersectable for Box {
    fn Intersections(&self, segment: &Segment) -> Vec<PointRef> {
        Box::Intersections(self, segment)
    }
}
impl Intersectable for BoxRef {
    fn Intersections(&self, segment: &Segment) -> Vec<PointRef> {
        self.b().Intersections(segment)
    }
}
