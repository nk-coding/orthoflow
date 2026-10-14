// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Copyright 2026 Niklas Krieger
//
// Rust-only helpers of the layoutgraph port (no Go counterpart).

//! Rust-only helpers (no Go counterpart): shared Go map handles, workStepper
//! glue, and short accessors for the node geometry stored in the aliased
//! `Node.Box`.
use crate::*;
use std::cell::{Ref, RefCell, RefMut};
use std::hash::Hash;
use std::rc::Rc;

/// A Go map value that is shared by assignment (Go maps are reference types).
///
/// Graph-level maps (`Containers`, `Clusters`, ...) are aliased between a
/// graph and its split subgraphs (`CopyEntitiesFrom`), and transactions
/// restore their contents in place, so they are modelled as a shared handle.
/// `Clone` copies the handle (Go `m2 := m`); use [`GoMap::deep_clone`] for
/// `maps.Clone`.
pub struct GoMap<K, V>(pub Rc<RefCell<Map<K, V>>>);

impl<K, V> Clone for GoMap<K, V> {
    fn clone(&self) -> Self {
        GoMap(self.0.clone())
    }
}

impl<K: Eq + Hash, V> Default for GoMap<K, V> {
    fn default() -> Self {
        GoMap::new()
    }
}

impl<K, V> std::fmt::Debug for GoMap<K, V> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "GoMap({:#x})", Rc::as_ptr(&self.0) as *const () as usize)
    }
}

impl<K: Eq + Hash, V> GoMap<K, V> {
    /// `make(map[K]V)`
    pub fn new() -> Self {
        GoMap(Rc::new(RefCell::new(Map::default())))
    }
    pub fn from_map(m: Map<K, V>) -> Self {
        GoMap(Rc::new(RefCell::new(m)))
    }
    #[inline]
    #[track_caller]
    pub fn b(&self) -> Ref<'_, Map<K, V>> {
        self.0.borrow()
    }
    #[inline]
    #[track_caller]
    pub fn bm(&self) -> RefMut<'_, Map<K, V>> {
        self.0.borrow_mut()
    }
    /// `len(m)`
    #[inline]
    pub fn len(&self) -> usize {
        self.0.borrow().len()
    }
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.0.borrow().is_empty()
    }
    /// `_, ok := m[k]`
    #[inline]
    pub fn has(&self, k: &K) -> bool {
        self.0.borrow().contains_key(k)
    }
    /// `m[k] = v`
    #[inline]
    pub fn set(&self, k: K, v: V) {
        self.0.borrow_mut().insert(k, v);
    }
    /// `delete(m, k)`
    #[inline]
    pub fn delete(&self, k: &K) {
        self.0.borrow_mut().remove(k);
    }
    /// `clear(m)`
    #[inline]
    pub fn clear(&self) {
        self.0.borrow_mut().clear();
    }
    /// Go map identity (`reflect.ValueOf(m).Pointer()`).
    #[inline]
    pub fn ptr_eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl<K: Eq + Hash + Clone, V: Clone> GoMap<K, V> {
    /// `v, ok := m[k]` (cloned value).
    #[inline]
    pub fn get(&self, k: &K) -> Option<V> {
        self.0.borrow().get(k).cloned()
    }
    /// Snapshot of the keys (iteration order is the Rust map order).
    pub fn keys(&self) -> Vec<K> {
        self.0.borrow().keys().cloned().collect()
    }
    /// Snapshot of the values.
    pub fn values(&self) -> Vec<V> {
        self.0.borrow().values().cloned().collect()
    }
    /// Snapshot of the entries, safe to iterate while mutating the map.
    pub fn entries(&self) -> Vec<(K, V)> {
        self.0.borrow().iter().map(|(k, v)| (k.clone(), v.clone())).collect()
    }
    /// `maps.Clone(m)` into a new independent map handle.
    pub fn deep_clone(&self) -> Self {
        GoMap::from_map(self.0.borrow().clone())
    }
    /// Plain copy of the contents.
    pub fn to_map(&self) -> Map<K, V> {
        self.0.borrow().clone()
    }
}

impl<K: Eq + Hash + Clone, V: Clone + Default> GoMap<K, V> {
    /// Go index expression `m[k]`: the zero value when missing.
    #[inline]
    pub fn at(&self, k: &K) -> V {
        self.0.borrow().get(k).cloned().unwrap_or_default()
    }
}

/// Go index expression on a plain map: zero value when missing.
#[inline]
pub fn mapAt<K: Eq + Hash, V: Clone + Default>(m: &Map<K, V>, k: &K) -> V {
    m.get(k).cloned().unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Node geometry accessors. Go reads/writes `n.TopLeft`, `n.Width`,
// `n.Height` through the embedded (and aliased) geo.Box; these helpers keep
// the RefCell borrows short.
// ---------------------------------------------------------------------------

impl NodeRef {
    /// Go `n.TopLeft` (pointer copy).
    #[inline]
    pub fn tl(&self) -> Option<geo::PointRef> {
        self.b().Box.b().TopLeft.clone()
    }
    /// Go `n.TopLeft != nil`
    #[inline]
    pub fn has_tl(&self) -> bool {
        self.b().Box.b().TopLeft.is_some()
    }
    /// Go `*n.TopLeft` (panics on nil like Go).
    #[inline]
    #[track_caller]
    pub fn tlv(&self) -> geo::Point {
        let n = self.b();
        let bx = n.Box.b();
        let p = bx.TopLeft.as_ref().expect("nil pointer dereference: node.TopLeft");
        let v = *p.b();
        v
    }
    /// Go `n.TopLeft.X`
    #[inline]
    #[track_caller]
    pub fn x(&self) -> f64 {
        self.tlv().X
    }
    /// Go `n.TopLeft.Y`
    #[inline]
    #[track_caller]
    pub fn y(&self) -> f64 {
        self.tlv().Y
    }
    /// Go `n.Width`
    #[inline]
    pub fn w(&self) -> f64 {
        self.b().Box.b().Width
    }
    /// Go `n.Height`
    #[inline]
    pub fn h(&self) -> f64 {
        self.b().Box.b().Height
    }
    /// Go `n.TopLeft = p`
    #[inline]
    pub fn set_tl(&self, p: Option<geo::PointRef>) {
        let bx = self.b().Box.clone();
        bx.bm().TopLeft = p;
    }
    /// Go `n.Width = v`
    #[inline]
    pub fn set_w(&self, v: f64) {
        let bx = self.b().Box.clone();
        bx.bm().Width = v;
    }
    /// Go `n.Height = v`
    #[inline]
    pub fn set_h(&self, v: f64) {
        let bx = self.b().Box.clone();
        bx.bm().Height = v;
    }
    /// Go `n.TopLeft.X = v`
    #[inline]
    #[track_caller]
    pub fn set_x(&self, v: f64) {
        self.tl().expect("nil pointer dereference: node.TopLeft").bm().X = v;
    }
    /// Go `n.TopLeft.Y = v`
    #[inline]
    #[track_caller]
    pub fn set_y(&self, v: f64) {
        self.tl().expect("nil pointer dereference: node.TopLeft").bm().Y = v;
    }
    /// The aliased embedded box (`&n.Box`).
    #[inline]
    pub fn boxRef(&self) -> geo::BoxRef {
        self.b().Box.clone()
    }
    /// Go `n.Box` copied by value (shallow: TopLeft pointer shared).
    #[inline]
    pub fn boxValue(&self) -> geo::Box {
        self.b().Box.b().clone()
    }
    /// Go `n.Graph` (pointer copy).
    #[inline]
    pub fn graph(&self) -> Option<GraphRef> {
        self.b().Graph.clone()
    }
    /// Go `n.Graph` when the caller knows it is non-nil.
    #[inline]
    #[track_caller]
    pub fn g(&self) -> GraphRef {
        self.b().Graph.clone().expect("nil pointer dereference: node.Graph")
    }
    /// Go `n.Container`
    #[inline]
    pub fn containerField(&self) -> Option<NodeRef> {
        self.b().Container.clone()
    }
    /// Go `n.Cluster`
    #[inline]
    pub fn clusterField(&self) -> Option<ClusterRef> {
        self.b().Cluster.clone()
    }
    /// Go `n.Sequence`
    #[inline]
    pub fn sequenceField(&self) -> Option<SequenceRef> {
        self.b().Sequence.clone()
    }
    /// Go `n.Edges` (slice copy of the pointers).
    #[inline]
    pub fn edges(&self) -> Vec<EdgeRef> {
        self.b().Edges.clone()
    }
    /// Go `n.Label` (pointer copy).
    #[inline]
    pub fn label(&self) -> Option<LabelRef> {
        self.b().Label.clone()
    }
    /// Go `n.Icon` (pointer copy).
    #[inline]
    pub fn icon(&self) -> Option<IconRef> {
        self.b().Icon.clone()
    }
    /// Go `n.ID`
    #[inline]
    pub fn id(&self) -> EntityID {
        self.b().ID
    }
    /// Go `n.isContainer`
    #[inline]
    pub fn isContainerFlag(&self) -> bool {
        self.b().isContainer
    }
    /// Go `n.isClusterVessel`
    #[inline]
    pub fn isClusterVesselFlag(&self) -> bool {
        self.b().isClusterVessel
    }
    /// Go `n.FixedTopLeft`
    #[inline]
    pub fn fixedTL(&self) -> Option<geo::PointRef> {
        self.b().FixedTopLeft.clone()
    }
    /// Go `n.LoopOffsets[o]`
    #[inline]
    pub fn loopOffset(&self, o: geo::Orientation) -> f64 {
        self.b().LoopOffsets.get(&o).copied().unwrap_or(0.0)
    }
}

impl EdgeRef {
    /// Go `e.From`
    #[inline]
    pub fn from(&self) -> NodeRef {
        self.b().From.clone()
    }
    /// Go `e.To`
    #[inline]
    pub fn to(&self) -> NodeRef {
        self.b().To.clone()
    }
    /// Go `e.Points` (slice copy; points stay aliased).
    #[inline]
    pub fn points(&self) -> Vec<geo::PointRef> {
        self.b().Points.clone()
    }
    /// Go `e.ID`
    #[inline]
    pub fn id(&self) -> EntityID {
        self.b().ID
    }
}

/// Go `p1 == p2` for optional node pointers.
#[inline]
pub fn nodeEq(a: Option<&NodeRef>, b: Option<&NodeRef>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(x), Some(y)) => x.ptr_eq(y),
        _ => false,
    }
}

/// Go `&geo.Point{X: x, Y: y}` / `geo.NewPoint(x, y)`.
#[inline]
pub(crate) fn newPoint(x: f64, y: f64) -> geo::PointRef {
    geo::PointRef::new(geo::Point { X: x, Y: y })
}

/// Go `math.Max` (NaN-propagating, -0 < +0).
#[inline]
pub(crate) fn fmax(a: f64, b: f64) -> f64 {
    gocompat::math::Max(a, b)
}
/// Go `math.Min`.
#[inline]
pub(crate) fn fmin(a: f64, b: f64) -> f64 {
    gocompat::math::Min(a, b)
}

/// Go builtin `max(a, b, ...)` on floats (NaN-propagating).
#[inline]
pub(crate) fn fmaxN(values: &[f64]) -> f64 {
    let mut m = values[0];
    for &v in &values[1..] {
        m = gocompat::math::BuiltinMax(m, v);
    }
    m
}
