// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/routing/route_search_guard.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of routing/route_search_guard.go.
#[allow(unused_imports)]
use crate::*;
use gocompat::context::{Ctx, CtxKey, Done};
use gocompat::{GoError, GoResult, Sentinel, Set};
use layoutgraph::NodeRef;
use std::rc::Rc;

/// Production routing declares at most three alternative flavors. Keeping a
/// separate budget for each flavor makes limit behavior independent of worker
/// scheduling while this cap bounds the aggregate operation to three budgets.
pub(crate) const maxRouteSearchFlavors: u64 = 3;

/// maxRouteSearchWorkUnits is calibrated against representative routing
/// corpora. The larger measured per-flavor peak is 51,125,738 units. 120
/// million leaves over 2x headroom while the
/// independent OVG and graph limits bound topology and allocation.
pub(crate) const maxRouteSearchWorkUnits: u64 = 120_000_000;

pub(crate) const routeSearchContextCheckStride: u64 = 1024;

pub static errRouteSearchWorkLimit: Sentinel = Sentinel::new("TALA route-search work limit exceeded");

/// workBudget is implemented by both the standalone route-stage guard and the
/// route-search guard. Shared geometric helpers can therefore account the work
/// to the operation that invoked them without changing their routing behavior.
///
/// Rust: implemented on the shared pointer types (`routeSearchWorkGuardRef`,
/// `routeWorkGuardRef`), so all methods take `&self`. The `as*` methods are Go's
/// type assertions `guard.(*routeSearchWorkGuard)` / `guard.(*routeWorkGuard)`.
pub trait workBudget {
    fn step(&self) -> GoResult<()>;
    fn add(&self, units: u64) -> GoResult<()>;
    fn check(&self) -> GoResult<()>;
    fn asRouteSearchWorkGuard(&self) -> Option<&routeSearchWorkGuardRef> {
        None
    }
    fn asRouteWorkGuard(&self) -> Option<&routeWorkGuardRef> {
        None
    }
}

/// A Go `workBudget` interface value (shared, non-nil).
pub type workBudgetRef = Rc<dyn workBudget>;

pub struct routeSearchWorkGuard {
    pub(crate) ctx: Ctx,
    pub(crate) done: Option<Done>,
    pub(crate) flavor: RouteGenerationFlavor,
    pub(crate) used: u64,
    pub(crate) limit: u64,
    pub(crate) metrics: Option<routeSearchTelemetryRef>,
    pub(crate) metricID: i64, // telemetry only; initialized to -1 before concurrent workers
    pub(crate) aggregate: Option<workBudgetRef>,
    pub(crate) nextCheck: u64,
    /// The production stage envelope is the sum of independently enforced OVG,
    /// flavor, and post-route envelopes. Charge a flavor's actual total when it
    /// joins instead of contending on the shared counter for every inner-loop
    /// operation. Injected limits retain exact per-operation accounting.
    pub(crate) deferAggregate: bool,
    pub(crate) aggregateCharged: u64,
}

gocompat::go_ptr!(routeSearchWorkGuardRef, routeSearchWorkGuard);

pub(crate) fn newRouteSearchWorkGuard(ctx: &Ctx, flavor: RouteGenerationFlavor, limit: u64) -> GoResult<routeSearchWorkGuardRef> {
    let aggregate = routeAggregateWorkFromContext(ctx);
    let guard = routeSearchWorkGuardRef::new(routeSearchWorkGuard {
        ctx: ctx.clone(),
        done: ctx.Done(),
        flavor,
        used: 0,
        limit,
        metrics: routeSearchTelemetryFromContext(ctx),
        metricID: -1,
        aggregate: aggregate.clone(),
        nextCheck: 0,
        deferAggregate: false,
        aggregateCharged: 0,
    });
    if let Some(agg) = &aggregate {
        if let Some(aggregate) = agg.asRouteWorkGuard() {
            aggregate.enableParallel();
            let deferAggregate = aggregate.b().limit == maxEdgeRoutingStageWorkUnits && limit == maxRouteSearchWorkUnits;
            guard.bm().deferAggregate = deferAggregate;
        }
    }
    guard.check()?;
    guard.bm().nextCheck = routeSearchContextCheckStride;
    Ok(guard)
}

fn routeSearchLimitErr(flavor: &RouteGenerationFlavor, limit: u64) -> GoError {
    GoError::wrap(
        format!("{}: TALA EdgeRouting flavor {} work exceeds limit {}", errRouteSearchWorkLimit.Error(), flavor, limit),
        errRouteSearchWorkLimit.err(),
    )
}

impl routeSearchWorkGuardRef {
    /// bind switches from the caller context used during router construction to the
    /// worker context. The latter is canceled when a sibling flavor panics.
    pub(crate) fn bind(&self, ctx: &Ctx) -> GoResult<()> {
        {
            let mut g = self.bm();
            g.ctx = ctx.clone();
            g.done = ctx.Done();
        }
        self.check()?;
        let mut g = self.bm();
        g.nextCheck = g.used + routeSearchContextCheckStride;
        Ok(())
    }

    pub(crate) fn reserveProduct(&self, a: u64, b: u64) -> GoResult<()> {
        let (product, ok) = limits::CheckedMulUint64(a, b);
        if !ok {
            self.check()?;
            return Err(GoError::wrap(
                format!("{}: route-search work arithmetic overflow", errRouteSearchWorkLimit.Error()),
                errRouteSearchWorkLimit.err(),
            ));
        }
        self.add(product)
    }

    pub(crate) fn reserveSum(&self, a: u64, b: u64) -> GoResult<()> {
        let (sum, ok) = limits::CheckedAddUint64(a, b);
        if !ok {
            self.check()?;
            return Err(GoError::wrap(
                format!("{}: route-search work arithmetic overflow", errRouteSearchWorkLimit.Error()),
                errRouteSearchWorkLimit.err(),
            ));
        }
        self.add(sum)
    }

    pub(crate) fn reserveSort(&self, length: usize) -> GoResult<()> {
        if length < 2 {
            return self.step();
        }
        let mut levels = 0u64;
        let mut remaining = (length - 1) as u64;
        while remaining > 0 {
            levels += 1;
            remaining >>= 1;
        }
        self.reserveProduct(length as u64, levels)
    }

    pub(crate) fn finish(&self) -> GoResult<()> {
        let (metrics, used) = {
            let g = self.b();
            (g.metrics.clone(), g.used)
        };
        if let Some(m) = metrics {
            m.record(self, used);
        }
        let pending = {
            let g = self.b();
            if g.deferAggregate && g.used != g.aggregateCharged {
                Some((g.used - g.aggregateCharged, g.aggregate.clone().expect("deferred aggregate")))
            } else {
                None
            }
        };
        if let Some((pending, aggregate)) = pending {
            aggregate.add(pending)?;
            let mut g = self.bm();
            g.aggregateCharged = g.used;
        }
        self.check()
    }

    /// Go `guard.used` (read).
    pub(crate) fn used(&self) -> u64 {
        self.b().used
    }
}

impl workBudget for routeSearchWorkGuardRef {
    #[inline]
    fn step(&self) -> GoResult<()> {
        // Single-step calls sit in the innermost scans. Poll every iteration so a
        // synthetic context without a Done signal remains observable immediately.
        // Standard contexts use the same bounded polling stride as the bulk path.
        let aggregate = {
            let mut g = self.bm();
            if g.done.is_none() || g.used >= g.nextCheck {
                if let Some(err) = cachedContextErr(&g.ctx, &g.done) {
                    return Err(GoError::wrap(format!("EdgeRouting: {}", err), err));
                }
                g.nextCheck = g.used + routeSearchContextCheckStride;
            }
            if g.used >= g.limit {
                if let Some(err) = cachedContextErr(&g.ctx, &g.done) {
                    return Err(GoError::wrap(format!("EdgeRouting: {}", err), err));
                }
                return Err(routeSearchLimitErr(&g.flavor, g.limit));
            }
            g.used += 1;
            if g.deferAggregate || g.aggregate.is_none() {
                return Ok(());
            }
            g.aggregate.clone().unwrap()
        };
        aggregate.add(1)
    }

    fn add(&self, units: u64) -> GoResult<()> {
        let aggregate = {
            let mut g = self.bm();
            if g.used > g.limit || units > g.limit - g.used {
                if let Some(err) = cachedContextErr(&g.ctx, &g.done) {
                    return Err(GoError::wrap(format!("EdgeRouting: {}", err), err));
                }
                return Err(routeSearchLimitErr(&g.flavor, g.limit));
            }
            g.used += units;
            if !g.deferAggregate {
                g.aggregate.clone()
            } else {
                None
            }
        };
        if let Some(aggregate) = aggregate {
            aggregate.add(units)?;
        }
        let mut g = self.bm();
        if g.done.is_none() || units == 0 || g.used >= g.nextCheck {
            if let Some(err) = cachedContextErr(&g.ctx, &g.done) {
                return Err(GoError::wrap(format!("EdgeRouting: {}", err), err));
            }
            g.nextCheck = g.used + routeSearchContextCheckStride;
        }
        Ok(())
    }

    #[inline]
    fn check(&self) -> GoResult<()> {
        let g = self.b();
        if let Some(err) = cachedContextErr(&g.ctx, &g.done) {
            return Err(GoError::wrap(format!("EdgeRouting: {}", err), err));
        }
        Ok(())
    }

    fn asRouteSearchWorkGuard(&self) -> Option<&routeSearchWorkGuardRef> {
        Some(self)
    }
}

/// routeSearchTelemetry is installed by calibration tests through context.
#[derive(Default)]
pub struct routeSearchTelemetry {
    pub(crate) samples: Vec<u64>,
}

gocompat::go_ptr!(routeSearchTelemetryRef, routeSearchTelemetry);

/// SearchTelemetry collects per-flavor work samples for routing diagnostics
/// and calibration.
pub struct SearchTelemetry {
    pub(crate) state: routeSearchTelemetryRef,
}

impl Default for SearchTelemetry {
    fn default() -> Self {
        SearchTelemetry { state: routeSearchTelemetryRef::new(routeSearchTelemetry::default()) }
    }
}

/// MaxSearchWorkUnits is the calibrated per-flavor route-search budget.
pub const MaxSearchWorkUnits: u64 = maxRouteSearchWorkUnits;

/// WithSearchTelemetry attaches route-search diagnostics to a routing context.
pub fn WithSearchTelemetry(ctx: &Ctx, telemetry: Option<&SearchTelemetry>) -> Ctx {
    match telemetry {
        None => ctx.clone(),
        Some(t) => withRouteSearchTelemetry(ctx, &t.state),
    }
}

impl SearchTelemetry {
    /// WorkSamples returns a stable copy of the recorded per-flavor work totals.
    pub fn WorkSamples(&self) -> Vec<u64> {
        self.state.snapshot()
    }
}

static routeSearchTelemetryKey: CtxKey = CtxKey::new("routeSearchTelemetryKey");

pub(crate) fn withRouteSearchTelemetry(ctx: &Ctx, telemetry: &routeSearchTelemetryRef) -> Ctx {
    ctx.WithValue(&routeSearchTelemetryKey, telemetry.clone())
}

pub(crate) fn routeSearchTelemetryFromContext(ctx: &Ctx) -> Option<routeSearchTelemetryRef> {
    ctx.ValueAs::<routeSearchTelemetryRef>(&routeSearchTelemetryKey)
}

impl routeSearchTelemetryRef {
    pub(crate) fn record(&self, guard: &routeSearchWorkGuardRef, work: u64) {
        let mut t = self.bm();
        let mut g = guard.bm();
        if g.metricID < 0 {
            g.metricID = t.samples.len() as i64;
            t.samples.push(work);
        } else {
            t.samples[g.metricID as usize] = work;
        }
    }

    pub(crate) fn snapshot(&self) -> Vec<u64> {
        self.b().samples.clone()
    }
}

pub(crate) fn routeSearchIsDescendantOf(guard: &dyn workBudget, maybeDescendant: Option<&NodeRef>, maybeAncestor: Option<&NodeRef>) -> GoResult<bool> {
    let mut maybeDescendant: Option<NodeRef> = maybeDescendant.cloned();
    loop {
        guard.step()?;
        if maybeAncestor == maybeDescendant.as_ref() {
            return Ok(true);
        }
        let d = match &maybeDescendant {
            None => return Ok(false),
            Some(d) => d.clone(),
        };
        let db = d.b();
        if let Some(c) = &db.Container {
            let c = c.clone();
            drop(db);
            maybeDescendant = Some(c);
        } else if let Some(cl) = &db.Cluster {
            let v = cl.b().Vessel.clone();
            drop(db);
            maybeDescendant = Some(v);
        } else if let Some(s) = &db.Sequence {
            let v = s.b().Vessel.clone();
            drop(db);
            maybeDescendant = Some(v);
        } else {
            return Ok(maybeAncestor.is_none());
        }
    }
}

pub(crate) fn routeSearchHasFixedAncestor(guard: &dyn workBudget, node: &NodeRef) -> GoResult<bool> {
    let mut current = Some(node.clone());
    while let Some(c) = current {
        guard.step()?;
        if c.b().FixedTopLeft.is_some() {
            return Ok(true);
        }
        current = c.OwningContainer();
    }
    Ok(false)
}

/// fixedOverlapsForRoute mirrors fixedOverlapsFor while accounting hierarchy
/// and pair scans. Each flavor derives its own immutable result so budget use is
/// independent of worker scheduling and shared-cache ownership.
pub(crate) fn fixedOverlapsForRoute(nodes: &[NodeRef], guard: &routeSearchWorkGuardRef) -> GoResult<Set<NodeRef>> {
    let mut fixedNodes: Vec<NodeRef> = Vec::with_capacity(nodes.len() / 4);
    for node in nodes {
        guard.step()?;
        let fixed = routeSearchHasFixedAncestor(guard, node)?;
        if fixed {
            fixedNodes.push(node.clone());
        }
    }

    let mut overlaps: Set<NodeRef> = Set::default();
    for node in &fixedNodes {
        guard.step()?;
        if overlaps.contains(node) {
            continue;
        }
        for other in nodes {
            guard.step()?;
            let nodeBelowOther = routeSearchIsDescendantOf(guard, Some(node), Some(other))?;
            let otherBelowNode = routeSearchIsDescendantOf(guard, Some(other), Some(node))?;
            if node == other || nodeBelowOther || otherBelowNode {
                continue;
            }
            if nodeOverlapsBox(node, other) {
                overlaps.insert(node.clone());
                let fixed = routeSearchHasFixedAncestor(guard, other)?;
                if fixed {
                    overlaps.insert(other.clone());
                }
                break;
            }
        }
    }

    guard.check()?;
    Ok(overlaps)
}

/// Go `node.Overlaps(other.Box)` (promoted geo.Box method).
pub(crate) fn nodeOverlapsBox(node: &NodeRef, other: &NodeRef) -> bool {
    let ob = other.boxValue();
    let nb = node.boxValue();
    nb.Overlaps(&ob)
}
