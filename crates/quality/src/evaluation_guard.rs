// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/quality/evaluation_guard.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

pub(crate) const maxEvaluationWorkUnits: i64 = 50_000_000;
pub(crate) const calibratedPublicEvaluationCorpusFloor: i64 = 50_000;
pub(crate) const calibratedPublicEvaluationCorpusCeil: i64 = 75_000;

pub(crate) fn newEvaluationWorkGuard(ctx: &Ctx, limit: i64) -> GoResult<WorkGuardRef> {
    let guard = limits::NewWorkGuard(ctx, "Evaluate", limit)?;
    if limit < 0 {
        return Err(errorf!("TALA Evaluate work limit must not be negative"));
    }
    Ok(guard)
}

pub(crate) fn chargeEvaluationWork(guard: &WorkGuardRef, amount: i64) -> GoResult<()> {
    if amount < 0 {
        return Err(errorf!("TALA Evaluate work charge must not be negative"));
    }
    guard.Add(amount)
}

/// chargeEvaluationAreaWork accounts a conservative upper bound for the legacy
/// bounding-box kernel before it runs.
pub(crate) fn chargeEvaluationAreaWork(g: &GraphRef, guard: &WorkGuardRef) -> GoResult<()> {
    let (nodes, edges) = {
        let gb = g.b();
        (gb.Nodes.clone(), gb.Edges.clone())
    };
    chargeEvaluationWork(guard, nodes.len() as i64)?;
    for node in &nodes {
        if let Some(l) = node.label() {
            if l.b().Position.IsOutside() {
                for _ in 0..4 {
                    chargeEvaluationWork(guard, nodes.len() as i64)?;
                }
            }
        }
    }
    for edge in &edges {
        let mut multiplier = 1;
        let e = edge.b();
        let npoints = e.Points.len() as i64;
        if let Some(l) = &e.Label {
            if npoints != 0 && l.b().Position != label::Unset {
                multiplier += 1;
            }
        }
        if e.SourceArrowheadLabel.is_some() {
            multiplier += 1;
        }
        if e.TargetArrowheadLabel.is_some() {
            multiplier += 1;
        }
        drop(e);
        for _ in 0..multiplier {
            chargeEvaluationWork(guard, npoints)?;
        }
    }
    Ok(())
}

/// evaluationIsDescendantOf is a cancellation- and budget-aware ancestry check.
pub(crate) fn evaluationIsDescendantOf(
    maybeDescendant: Option<&NodeRef>,
    maybeAncestor: Option<&NodeRef>,
    guard: &WorkGuardRef,
) -> GoResult<bool> {
    let mut maybeDescendant = maybeDescendant.cloned();
    let mut depth = 0i64;
    loop {
        guard.Step()?;
        if layoutgraph::nodeEq(maybeAncestor, maybeDescendant.as_ref()) {
            return Ok(true);
        }
        let Some(d) = maybeDescendant.clone() else { return Ok(false) };
        if depth >= layoutgraph::MaxTopologyDepth {
            return Err(errorf!("TALA Evaluate ancestry depth exceeds limit {}", layoutgraph::MaxTopologyDepth));
        }
        maybeDescendant = d.AncestryParent();
        if maybeDescendant.is_none() {
            return Ok(maybeAncestor.is_none());
        }
        depth += 1;
    }
}
