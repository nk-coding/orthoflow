// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/placementcost/flow_continuity.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

// Experimental placement preferences. The completed-layout score is unchanged.
const flowSpineWeight: f64 = 1.0;
const flowBranchWeight: f64 = 1.0;

const incomingDirection: u8 = 1 << 0;
const outgoingDirection: u8 = 1 << 1;

#[derive(Clone, Default)]
struct ray {
    node: Option<NodeRef>,
    x: f64,
    y: f64,
    directions: u8,
}

/// flowContinuityCost treats paths through a node as a visual unit.
pub(crate) fn flowContinuityCost(node: &NodeRef, s: &edgeScratch) -> f64 {
    let graph = node.g();
    let (edges, cluster, sequence, herd) = {
        let n = node.b();
        if n.Edges.len() < 2 || n.Edges.len() > 8 {
            return 0.0;
        }
        (n.Edges.clone(), n.Cluster.is_some(), n.Sequence.is_some(), n.HerdAssignment.is_some())
    };
    if !node.has_tl() || cluster || sequence || herd {
        return 0.0;
    }
    let containers = graph.b().Containers.clone();
    if containers.b().get(&Some(node.clone())).map_or(0, |v| v.len()) != 0 {
        return 0.0;
    }
    let mut rays: [ray; 8] = Default::default();
    let mut n = 0usize;
    let tl = node.tlv();
    let (cx, cy) = (tl.X + node.w() / 2.0, tl.Y + node.h() / 2.0);
    let nodeContainer = node.containerField();
    for (i, e) in edges.iter().enumerate() {
        let (isInvisible, from, to) = {
            let eb = e.b();
            (eb.IsInvisible, eb.From.clone(), eb.To.clone())
        };
        if isInvisible || from == to || e.HasTableColumn() || e.HasSourceArrow() == e.HasTargetArrow() || s.nRepl[i] != *node {
            continue;
        }
        let adj = &s.aRepl[i];
        if !adj.has_tl() || adj.containerField() != nodeContainer {
            continue;
        }
        let mut incoming = to == *node;
        if e.HasSourceArrow() {
            incoming = !incoming;
        }
        let mut direction = outgoingDirection;
        if incoming {
            direction = incomingDirection;
        }
        let mut duplicate = false;
        for j in 0..n {
            if rays[j].node.as_ref() == Some(adj) {
                // Parallel edges share one geometric ray.
                rays[j].directions |= direction;
                duplicate = true;
                break;
            }
        }
        if duplicate {
            continue;
        }
        let atl = adj.tlv();
        let (x, y) = (atl.X + adj.w() / 2.0 - cx, atl.Y + adj.h() / 2.0 - cy);
        let length = math::Hypot(x, y);
        if length == 0.0 {
            continue;
        }
        rays[n] = ray { node: Some(adj.clone()), x: x / length, y: y / length, directions: direction };
        n += 1;
    }
    let mut spine = f64::INFINITY;
    let (mut branchSum, mut branches) = (0.0, 0i64);
    for i in 0..n {
        for j in (i + 1)..n {
            let dot = math::Max(-1.0, math::Min(1.0, rays[i].x * rays[j].x + rays[i].y * rays[j].y));
            let (a, b) = (rays[i].directions, rays[j].directions);
            if a & incomingDirection != 0 && b & outgoingDirection != 0 || a & outgoingDirection != 0 && b & incomingDirection != 0 {
                spine = math::Min(spine, 1.0 + dot);
            }
            if a & b != 0 {
                branchSum += math::Max(0.0, 2.0 * dot - 1.0);
                branches += 1;
            }
        }
    }
    let mut cost = 0.0;
    if !(spine.is_infinite() && spine > 0.0) {
        cost += flowSpineWeight * spine;
    }
    if branches > 0 {
        cost += flowBranchWeight * branchSum / branches as f64;
    }
    graph.TurnCost() * cost
}
