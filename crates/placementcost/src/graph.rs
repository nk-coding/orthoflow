// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/placementcost/graph.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.
//
// The `fnv64a` helper follows Go's src/hash/fnv/fnv.go (Go 1.27.2,
// BSD-3-Clause, Copyright 2011 The Go Authors; see the LICENSES file).

use crate::*;

const invalidEdgeLengthState: u64 = 0;

/// Go `hash/fnv` New64a.
struct fnv64a(u64);

impl fnv64a {
    const offset64: u64 = 14695981039346656037;
    const prime64: u64 = 1099511628211;

    fn new() -> fnv64a {
        fnv64a(Self::offset64)
    }
    #[inline]
    fn write(&mut self, data: &[u8]) {
        let mut h = self.0;
        for &c in data {
            h ^= c as u64;
            h = h.wrapping_mul(Self::prime64);
        }
        self.0 = h;
    }
    #[inline]
    fn writeU64(&mut self, v: u64) {
        self.write(&v.to_le_bytes());
    }
}

pub(crate) fn placementEdgeLengthState(graph: &GraphRef, costs: layoutgraph::RoutingCostState, options: &EdgeLengthOptions) -> u64 {
    let mut h = fnv64a::new();

    // Process booleans
    let mut flags: u8 = 0;
    if options.IncludeNodeSizes {
        flags |= 1 << 0;
    }
    if options.EnforceMinimumGap {
        flags |= 1 << 1;
    }
    if options.PenalizeDirection {
        flags |= 1 << 2;
    }
    h.write(&[flags]);

    let g = graph.b();
    // Process graph costs
    h.writeU64(g.CellSize.to_bits());
    h.writeU64(costs.Turn.to_bits());
    h.writeU64(costs.NonCenterPort.to_bits());
    h.writeU64(costs.Crossing.to_bits());

    // Process nodes
    for n in &g.Nodes {
        let nb = n.b();
        let bx = nb.Box.b();
        let Some(tl) = bx.TopLeft.as_ref() else {
            return invalidEdgeLengthState;
        };
        let p = *tl.b();
        h.writeU64(nb.ID as u64);
        h.writeU64(p.X.to_bits());
        h.writeU64(p.Y.to_bits());
        h.writeU64(bx.Width.to_bits());
        h.writeU64(bx.Height.to_bits());
        for e in &nb.Edges {
            h.writeU64(e.b().ID as u64);
        }
    }

    // Process edge abductions
    if let Some(abductions) = &options.EdgeAbductions {
        for ea in abductions {
            let ea = ea.b();
            h.writeU64(ea.Edge.b().ID as u64);
            if let Some(n) = &ea.OriginallyFrom {
                h.writeU64(n.id() as u64);
            }
            if let Some(n) = &ea.OriginallyTo {
                h.writeU64(n.id() as u64);
            }
            if let Some(n) = &ea.CurrentTo {
                h.writeU64(n.id() as u64);
            }
            if let Some(n) = &ea.CurrentFrom {
                h.writeU64(n.id() as u64);
            }
        }
    }

    h.0
}

/// EdgeLength evaluates the graph's placement cost and may populate its
/// graph-owned edge-length cache.
///
/// Go fans the per-node costs out to goroutines writing per-index results and
/// sums them in index order; this port evaluates them sequentially in index order.
pub fn EdgeLength(ctx: &Ctx, graph: &GraphRef, options: &EdgeLengthOptions) -> GoResult<f64> {
    ctxCheck(ctx)?;

    let edgeLengthState = placementEdgeLengthState(graph, graph.RoutingCosts(), options);
    if edgeLengthState != invalidEdgeLengthState {
        let (d, ok) = graph.LookupEdgeLengthCost(edgeLengthState);
        if ok {
            return Ok(d);
        }
    }

    let symmetryCost = graph.b().CellSize;
    let mut totalSum = 0.0;
    let abductions = options.EdgeAbductions.as_deref();

    let nodes = graph.nodes();
    for n in &nodes {
        let mut nl = NodeEdgeLength(ctx, n, options)?;
        if options.IncludeNodeSizes {
            let columnCrossingCost = ColumnCrossingCost(ctx, n, abductions)?;
            let symmetry = nodeSymmetry(ctx, n, abductions, true)?;
            nl += columnCrossingCost;
            nl -= symmetry * symmetryCost * n.b().Edges.len() as f64;
        }
        ctxCheck(ctx)?;
        totalSum += nl;
    }

    let crossings = GraphEdgeCrossings(ctx, graph)?;
    totalSum += graph.CrossingCost() * crossings as f64;
    ctxCheck(ctx)?;
    if edgeLengthState != invalidEdgeLengthState {
        graph.StoreEdgeLengthCost(edgeLengthState, totalSum);
    }

    Ok(totalSum)
}

fn countEdgeCrossingsExcludingSharedNodes(ctx: &Ctx, edges: &[EdgeRef]) -> GoResult<i64> {
    let mut crossings: i64 = 0;
    // Endpoint centers are pure functions of node geometry; compute them once.
    let ends: Vec<(NodeRef, NodeRef, geo::Point, geo::Point)> = edges
        .iter()
        .map(|e| {
            let (f, t) = (e.from(), e.to());
            let fc = *f.Center().b();
            let tc = *t.Center().b();
            (f, t, fc, tc)
        })
        .collect();
    let n = edges.len() as i64;
    let mut i: i64 = 0;
    while i < n - 1 {
        scoringCancellationError(ctx, i)?;
        let (f1, t1, s1, e1) = &ends[i as usize];
        for j in (i + 1)..n {
            scoringCancellationError(ctx, j - i - 1)?;
            let (f2, t2, s2, e2) = &ends[j as usize];

            // Skip if edges share a common node
            if f1 == f2 || f1 == t2 || t1 == f2 || t1 == t2 {
                continue;
            }

            if layoutgraph::SegmentsCross(s1, e1, s2, e2) {
                crossings += 1;
            }
        }
        i += 1;
    }
    ctxCheck(ctx)?;
    Ok(crossings)
}

/// GraphEdgeCrossings counts crossings between edges that do not share a node.
pub fn GraphEdgeCrossings(ctx: &Ctx, graph: &GraphRef) -> GoResult<i64> {
    ctxCheck(ctx)?;
    let edges = graph.edges();
    if edges.is_empty() {
        ctxCheck(ctx)?;
        return Ok(0);
    }

    let mut levelGroup: Map<i64, usize> = Map::default();
    let mut groups: Vec<Vec<EdgeRef>> = Vec::new();
    for (i, edge) in edges.iter().enumerate() {
        scoringCancellationError(ctx, i as i64)?;
        let (from, to) = (edge.from(), edge.to());
        if !from.has_tl() || !to.has_tl() {
            continue;
        }
        let fromLevel = from.ContainerLevel();
        let toLevel = to.ContainerLevel();
        if fromLevel != toLevel {
            continue;
        }
        let groupIndex = *levelGroup.entry(fromLevel).or_insert_with(|| {
            groups.push(Vec::new());
            groups.len() - 1
        });
        groups[groupIndex].push(edge.clone());
    }

    // Go iterates the level map in random order; the integer sum is order-free,
    // but cancellation checks are not, so iterate the groups in creation order.
    let mut totalCrossings: i64 = 0;
    for group in &groups {
        let crossings = countEdgeCrossingsExcludingSharedNodes(ctx, group)?;
        totalCrossings += crossings;
    }

    ctxCheck(ctx)?;
    Ok(totalCrossings)
}

/// ContainerAlignmentCost scores misalignment between peer containers.
pub fn ContainerAlignmentCost(ctx: &Ctx, graph: &GraphRef) -> GoResult<f64> {
    ctxCheck(ctx)?;
    let mut l = 0.0;
    let nodes = graph.nodes();
    let n = nodes.len() as i64;
    let mut i: i64 = 0;
    while i < n - 1 {
        scoringCancellationError(ctx, i)?;
        let n1 = &nodes[i as usize];
        if !n1.IsContainer() {
            i += 1;
            continue;
        }
        for j in (i + 1)..n {
            scoringCancellationError(ctx, j - i - 1)?;
            let n2 = &nodes[j as usize];
            if !n2.IsContainer() {
                continue;
            }
            if n1.EffectiveContainer() != n2.EffectiveContainer() {
                continue;
            }
            if n1.w() == n2.w() && n1.h() == n2.h() {
                if n1.x() != n2.x() && n1.y() != n2.y() {
                    l += graph.NonCenterPortCost();
                }
            }
        }
        i += 1;
    }
    ctxCheck(ctx)?;
    Ok(l)
}
