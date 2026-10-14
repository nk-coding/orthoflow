// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from lib/geo/segment.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;
use gocompat::math;

/// Go `geo.Intersectable`.
pub trait Intersectable {
    fn Intersections(&self, segment: &Segment) -> Vec<PointRef>;
}

/// Go `geo.Segment` (value; copies share Start/End). `*geo.Segment` is [`SegmentRef`].
#[derive(Clone, Debug)]
pub struct Segment {
    pub Start: PointRef,
    pub End: PointRef,
}

gocompat::go_ptr!(SegmentRef, Segment);

/// `geo.NewSegment(from, to)` → `*Segment`
pub fn NewSegment(from: PointRef, to: PointRef) -> SegmentRef {
    SegmentRef::new(Segment { Start: from, End: to })
}

impl Segment {
    pub fn new(from: PointRef, to: PointRef) -> Segment {
        Segment { Start: from, End: to }
    }

    pub fn Overlaps(&self, otherS: &Segment, isHorizontal: bool, buffer: f64) -> bool {
        let (ss, se) = (self.Start.get(), self.End.get());
        let (os, oe) = (otherS.Start.get(), otherS.End.get());
        if isHorizontal {
            if math::Min(ss.Y, se.Y) - math::Max(os.Y, oe.Y) >= buffer {
                return false;
            }
            if math::Min(os.Y, oe.Y) - math::Max(ss.Y, se.Y) >= buffer {
                return false;
            }
            true
        } else {
            if math::Min(ss.X, se.X) - math::Max(os.X, oe.X) >= buffer {
                return false;
            }
            if math::Min(os.X, oe.X) - math::Max(ss.X, se.X) >= buffer {
                return false;
            }
            true
        }
    }

    pub fn Intersects(&self, otherSegment: &Segment) -> bool {
        IntersectionPoint(&self.Start, &self.End, &otherSegment.Start, &otherSegment.End).is_some()
    }

    pub fn ToString(&self) -> String {
        format!("{} -> {}", self.Start.ToString(), self.End.ToString())
    }

    /// Go returns nil (empty) when there is no intersection.
    pub fn Intersections(&self, otherSegment: &Segment) -> Vec<PointRef> {
        match IntersectionPoint(&self.Start, &self.End, &otherSegment.Start, &otherSegment.End) {
            None => Vec::new(),
            Some(p) => vec![p],
        }
    }

    pub fn GetBounds(&self, segments: &[SegmentRef], buffer: f64) -> (f64, f64) {
        let mut ceil = math::Inf(1);
        let mut floor = math::Inf(-1);
        let (ss, se) = (self.Start.get(), self.End.get());
        if ss.X == se.X && ss.Y == se.Y {
            return (floor, ceil);
        }
        let isHorizontal = ss.X == se.X;
        for otherSegment in segments {
            let o = otherSegment.b();
            let (os, oe) = (o.Start.get(), o.End.get());
            if isHorizontal {
                if oe.Y < ss.Y - buffer {
                    continue;
                }
                if os.Y > se.Y + buffer {
                    continue;
                }
                if os.X <= ss.X {
                    floor = math::Max(floor, os.X);
                }
                if os.X > ss.X {
                    ceil = math::Min(ceil, os.X);
                }
            } else {
                if oe.X < ss.X - buffer {
                    continue;
                }
                if os.X > se.X + buffer {
                    continue;
                }
                if os.Y <= ss.Y {
                    floor = math::Max(floor, os.Y);
                }
                if os.Y > ss.Y {
                    ceil = math::Min(ceil, os.Y);
                }
            }
        }
        (floor, ceil)
    }

    pub fn Length(&self) -> f64 {
        let (s, e) = (self.Start.get(), self.End.get());
        EuclideanDistance(s.X, s.Y, e.X, e.Y)
    }

    pub fn ToVector(&self) -> Vector {
        let (s, e) = (self.Start.get(), self.End.get());
        vec![e.X - s.X, e.Y - s.Y]
    }
}

impl SegmentRef {
    #[inline]
    pub fn get(&self) -> Segment {
        self.b().clone()
    }
    #[inline]
    pub fn Start(&self) -> PointRef {
        self.b().Start.clone()
    }
    #[inline]
    pub fn End(&self) -> PointRef {
        self.b().End.clone()
    }
    pub fn Overlaps(&self, otherS: &Segment, isHorizontal: bool, buffer: f64) -> bool {
        self.b().Overlaps(otherS, isHorizontal, buffer)
    }
    pub fn Intersects(&self, otherSegment: &Segment) -> bool {
        self.b().Intersects(otherSegment)
    }
    pub fn ToString(&self) -> String {
        self.b().ToString()
    }
    pub fn Intersections(&self, otherSegment: &Segment) -> Vec<PointRef> {
        self.b().Intersections(otherSegment)
    }
    pub fn GetBounds(&self, segments: &[SegmentRef], buffer: f64) -> (f64, f64) {
        self.b().GetBounds(segments, buffer)
    }
    pub fn Length(&self) -> f64 {
        self.b().Length()
    }
    pub fn ToVector(&self) -> Vector {
        self.b().ToVector()
    }
}

impl Intersectable for Segment {
    fn Intersections(&self, segment: &Segment) -> Vec<PointRef> {
        Segment::Intersections(self, segment)
    }
}

impl Intersectable for SegmentRef {
    fn Intersections(&self, segment: &Segment) -> Vec<PointRef> {
        self.b().Intersections(segment)
    }
}
