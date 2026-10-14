// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/routing/ovg_edge.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of routing/ovg_edge.go.
#[allow(unused_imports)]
use crate::*;
use std::rc::Rc;

pub struct OVGEdge {
    pub From: OVGNodeRef,
    pub To: OVGNodeRef,

    pub Distance: f64,
}

/// Go `*OVGEdge`. OVG edges are never mutated after construction (Go sets
/// Distance right after allocation in Connect), so the pointer is an immutable
/// `Rc`. Equality/hash are by address (Go pointer `==`).
#[derive(Clone)]
pub struct OVGEdgeRef(pub Rc<OVGEdge>);

impl std::ops::Deref for OVGEdgeRef {
    type Target = OVGEdge;
    #[inline]
    fn deref(&self) -> &OVGEdge {
        &self.0
    }
}
impl PartialEq for OVGEdgeRef {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for OVGEdgeRef {}
impl std::hash::Hash for OVGEdgeRef {
    #[inline]
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        state.write_usize(self.addr());
    }
}
impl std::fmt::Debug for OVGEdgeRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "OVGEdgeRef({:?}->{:?})", self.From.P(), self.To.P())
    }
}
impl OVGEdgeRef {
    #[inline]
    pub fn addr(&self) -> usize {
        Rc::as_ptr(&self.0) as *const () as usize
    }
    #[inline]
    pub fn ptr_eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

pub fn NewOVGEdge(from: &OVGNodeRef, to: &OVGNodeRef) -> OVGEdgeRef {
    OVGEdgeRef(Rc::new(OVGEdge { From: from.clone(), To: to.clone(), Distance: 0.0 }))
}

pub(crate) fn newOVGEdgeWithDistance(from: &OVGNodeRef, to: &OVGNodeRef, distance: f64) -> OVGEdgeRef {
    OVGEdgeRef(Rc::new(OVGEdge { From: from.clone(), To: to.clone(), Distance: distance }))
}

impl OVGEdge {
    #[inline]
    pub(crate) fn isVertical(&self) -> bool {
        self.From.X() == self.To.X()
    }

    #[inline]
    pub(crate) fn isHorizontal(&self) -> bool {
        self.From.Y() == self.To.Y()
    }

    pub(crate) fn sharePoints(&self, other: &OVGEdge) -> bool {
        let (f, t, of, ot) = (self.From.P(), self.To.P(), other.From.P(), other.To.P());
        nonNilEquals(&f, &of) || nonNilEquals(&f, &ot) || nonNilEquals(&t, &of) || nonNilEquals(&t, &ot)
    }
}

/// geo.Point.Equals skipping nil check
#[inline]
pub(crate) fn nonNilEquals(p1: &geo::Point, p2: &geo::Point) -> bool {
    p1.X == p2.X && p1.Y == p2.Y
}
