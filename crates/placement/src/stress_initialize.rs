// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/placement/stress_initialize.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of placement/stress_initialize.go
use crate::*;
use gocompat::Ctx;
use gocompat::{math, sort, GoResult, Map, Set};
use layoutgraph::{GraphRef, NodeRef, NodesMethods};

/// Go folds the constant `.02/.7` exactly.
const stressEtaRatio: f64 = f64::from_bits(0x3f9d41d41d41d41d);

// initializeByGraphDistance is an experimental, bounded alternative starting
// arrangement. It gives distant parts of the graph distinct space rather than
// minimizing only distances to already placed neighbors. The ordinary orthogonal
// optimizer still owns the final placement.
pub fn initializeByGraphDistance(ctx: &Ctx, g: &GraphRef) -> GoResult<bool> {
    layoutgraph::Validate(ctx, "GraphDistanceInitialization", g)?;
    let (nodes, edges) = {
        let gb = g.b();
        (gb.Nodes.clone(), gb.Edges.clone())
    };
    let n = nodes.len();
    if n < 4 || n > 64 || edges.len() > 256 || g.HasFixedNode() {
        return Ok(false);
    }
    let mut index: Map<NodeRef, usize> = Map::default();
    for (i, node) in nodes.iter().enumerate() {
        index.insert(node.clone(), i);
    }
    let mut d: Vec<Vec<f64>> = vec![vec![0.0; n]; n];
    for i in 0..n {
        for j in 0..n {
            if i != j {
                d[i][j] = math::Inf(1);
            }
        }
    }
    for edge in &edges {
        let (from, to) = {
            let e = edge.b();
            (e.From.clone(), e.To.clone())
        };
        if let (Some(&i), Some(&j)) = (index.get(&from), index.get(&to)) {
            if i != j {
                d[i][j] = 1.0;
                d[j][i] = 1.0;
            }
        }
    }
    // Near-only or indirect connectivity falls back to ordinary initialization.
    for k in 0..n {
        if let Some(err) = ctx.Err() {
            return Err(err);
        }
        for i in 0..n {
            for j in 0..n {
                d[i][j] = math::Min(d[i][j], d[i][k] + d[k][j]);
            }
        }
    }
    for i in 0..n {
        for j in 0..n {
            if math::IsInf(d[i][j], 1) {
                return Ok(false);
            }
        }
    }
    let mut x = vec![0.0f64; n];
    let mut y = vec![0.0f64; n];
    for i in 0..n {
        let a = 2.0 * std::f64::consts::PI * i as f64 / n as f64;
        x[i] = math::Sqrt(n as f64) * math::Cos(a);
        y[i] = math::Sqrt(n as f64) * math::Sin(a);
    }
    // Pairwise relaxation of sum ((EuclideanDistance - graphDistance) /
    // graphDistance)^2. Deterministic sweeps leave the annealing RNG untouched.
    for sweep in 0..48i64 {
        if let Some(err) = ctx.Err() {
            return Err(err);
        }
        let eta = 0.7 * math::Pow(stressEtaRatio, sweep as f64 / 47.0);
        for p in 0..n {
            let i = (p + sweep as usize) % n;
            for q in p + 1..n {
                let j = (q + sweep as usize) % n;
                let (mut dx, mut dy) = (x[i] - x[j], y[i] - y[j]);
                let mut length = math::Hypot(dx, dy);
                if length < 1e-9 {
                    dx = 1e-6;
                    dy = 1e-6;
                    length = math::Sqrt(2.0) * 1e-6;
                }
                let mu = math::Min(1.0, eta / (d[i][j] * d[i][j]));
                let amount = 0.5 * mu * (length - d[i][j]) / length;
                x[i] -= dx * amount;
                y[i] -= dy * amount;
                x[j] += dx * amount;
                y[j] += dy * amount;
            }
        }
    }
    let mut order: Vec<usize> = (0..n).collect();
    let edgeCounts: Vec<usize> = nodes.iter().map(|node| node.b().Edges.len()).collect();
    sort::SortSliceStable(&mut order, |a, b| edgeCounts[*a] > edgeCounts[*b]);
    let mut occupied: Set<(i64, i64)> = Set::default();
    let mut points: Vec<Option<geo::PointRef>> = vec![None; n];
    for &i in &order {
        if let Some(err) = ctx.Err() {
            return Err(err);
        }
        let (tx, ty) = (x[i] * 2.0 + n as f64, y[i] * 2.0 + n as f64);
        let (cx, cy) = (math::F64ToInt(math::Round(tx)), math::F64ToInt(math::Round(ty)));
        let mut best = (cx, cy);
        let mut cost = math::Inf(1);
        // n bounds the radius needed to find an unused integer cell.
        for radius in 0..=(n as i64) {
            for xx in cx - radius..=cx + radius {
                for yy in cy - radius..=cy + radius {
                    if radius > 0 && xx != cx - radius && xx != cx + radius && yy != cy - radius && yy != cy + radius {
                        continue;
                    }
                    let p = (xx, yy);
                    if occupied.contains(&p) {
                        continue;
                    }
                    let c = math::Pow(xx as f64 - tx, 2.0) + math::Pow(yy as f64 - ty, 2.0);
                    if c < cost {
                        best = p;
                        cost = c;
                    }
                }
            }
            if cost < math::Inf(1) {
                break;
            }
        }
        occupied.insert(best);
        points[i] = Some(geo::NewPoint(best.0 as f64, best.1 as f64));
    }
    if let Some(err) = ctx.Err() {
        return Err(err);
    }
    for (i, node) in nodes.iter().enumerate() {
        node.set_tl(points[i].clone());
    }
    Ok(true)
}
