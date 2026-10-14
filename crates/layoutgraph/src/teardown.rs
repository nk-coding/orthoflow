// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Copyright 2026 Niklas Krieger
//
// Rust-only Rc-cycle teardown of the layoutgraph port (no Go counterpart).

//! Rust-only (no Go counterpart): breaks the Rc reference cycles of a graph
//! (node <-> graph, node <-> edge, vessel <-> cluster/sequence, tree parent
//! links, hierarchy level maps) so it can be freed. Added for the tala/WASM
//! layer, which lays out many graphs in one long-lived process. The graph must
//! not be used afterwards.
use crate::*;

pub fn Teardown(g: &GraphRef) {
    let mut nodes: Vec<NodeRef> = Vec::new();
    let mut seen: Set<NodeRef> = Set::default();
    let mut edges: Vec<EdgeRef> = Vec::new();
    let mut seenEdges: Set<EdgeRef> = Set::default();
    let mut trees: Vec<TreeRef> = Vec::new();
    let mut seenTrees: Set<TreeRef> = Set::default();
    let mut graphs: Vec<GraphRef> = vec![g.clone()];
    let mut seenGraphs: Set<GraphRef> = Set::default();
    seenGraphs.insert(g.clone());

    fn push(n: NodeRef, nodes: &mut Vec<NodeRef>, seen: &mut Set<NodeRef>) {
        if seen.insert(n.clone()) {
            nodes.push(n);
        }
    }

    let mut gi = 0;
    let mut ni = 0;
    loop {
        let mut progressed = false;
        while gi < graphs.len() {
            progressed = true;
            let graph = graphs[gi].clone();
            gi += 1;
            let gb = graph.b();
            for n in &gb.Nodes {
                push(n.clone(), &mut nodes, &mut seen);
            }
            for e in &gb.Edges {
                if seenEdges.insert(e.clone()) {
                    edges.push(e.clone());
                }
            }
            for (k, v) in gb.Containers.entries() {
                if let Some(k) = k {
                    push(k, &mut nodes, &mut seen);
                }
                for n in v {
                    push(n, &mut nodes, &mut seen);
                }
            }
            for (k, c) in gb.Clusters.entries() {
                push(k, &mut nodes, &mut seen);
                let cb = c.b();
                push(cb.Vessel.clone(), &mut nodes, &mut seen);
                for n in &cb.Nodes {
                    push(n.clone(), &mut nodes, &mut seen);
                }
            }
            for (k, s) in gb.Sequences.entries() {
                push(k, &mut nodes, &mut seen);
                for n in &s.b().Nodes {
                    push(n.clone(), &mut nodes, &mut seen);
                }
            }
            for (k, ts) in gb.Trees.entries() {
                if let Some(k) = k {
                    push(k, &mut nodes, &mut seen);
                }
                for t in ts {
                    if seenTrees.insert(t.clone()) {
                        trees.push(t);
                    }
                }
            }
            for (k, t) in gb.NodeToTree.entries() {
                push(k, &mut nodes, &mut seen);
                if seenTrees.insert(t.clone()) {
                    trees.push(t);
                }
            }
            for (k, v) in gb.Hubs.entries() {
                push(k, &mut nodes, &mut seen);
                for n in v {
                    push(n, &mut nodes, &mut seen);
                }
            }
            for (k, v) in gb.CommonUncleSiblings.entries() {
                push(k, &mut nodes, &mut seen);
                for n in v {
                    push(n, &mut nodes, &mut seen);
                }
            }
            for (k, _) in gb.Directions.entries() {
                if let Some(k) = k {
                    push(k, &mut nodes, &mut seen);
                }
            }
        }
        let mut ti = 0;
        while ti < trees.len() {
            let t = trees[ti].clone();
            ti += 1;
            let tb = t.b();
            push(tb.Node.clone(), &mut nodes, &mut seen);
            for c in tb.Children.iter().chain(tb.Parent.iter()) {
                if seenTrees.insert(c.clone()) {
                    trees.push(c.clone());
                }
            }
        }
        while ni < nodes.len() {
            progressed = true;
            let n = nodes[ni].clone();
            ni += 1;
            let nb = n.b();
            for e in &nb.Edges {
                if seenEdges.insert(e.clone()) {
                    edges.push(e.clone());
                }
            }
            for m in &nb.Nears {
                push(m.clone(), &mut nodes, &mut seen);
            }
            if let Some(c) = &nb.Container {
                push(c.clone(), &mut nodes, &mut seen);
            }
            if let Some(c) = &nb.Cluster {
                let cb = c.b();
                push(cb.Vessel.clone(), &mut nodes, &mut seen);
                for m in &cb.Nodes {
                    push(m.clone(), &mut nodes, &mut seen);
                }
                if let Some(cg) = &cb.Graph {
                    if seenGraphs.insert(cg.clone()) {
                        graphs.push(cg.clone());
                    }
                }
            }
            if let Some(s) = &nb.Sequence {
                let sb = s.b();
                push(sb.Vessel.clone(), &mut nodes, &mut seen);
                for m in &sb.Nodes {
                    push(m.clone(), &mut nodes, &mut seen);
                }
                if let Some(sg) = &sb.Graph {
                    if seenGraphs.insert(sg.clone()) {
                        graphs.push(sg.clone());
                    }
                }
            }
            if let Some(ng) = &nb.Graph {
                if seenGraphs.insert(ng.clone()) {
                    graphs.push(ng.clone());
                }
            }
            for k in nb.LongDistanceNeighborRequirements.keys() {
                push(k.clone(), &mut nodes, &mut seen);
            }
        }
        let mut ei = 0;
        while ei < edges.len() {
            let e = edges[ei].clone();
            ei += 1;
            let (f, t) = (e.from(), e.to());
            push(f, &mut nodes, &mut seen);
            push(t, &mut nodes, &mut seen);
        }
        if !progressed && ni >= nodes.len() && gi >= graphs.len() {
            break;
        }
    }

    for t in &trees {
        let mut tb = t.bm();
        tb.Parent = None;
        tb.Children = Vec::new();
        tb.SentinelEdge = None;
    }
    for n in &nodes {
        let (cluster, sequence, hierarchy, herd) = {
            let mut nb = n.bm();
            nb.Edges = Vec::new();
            nb.Nears = Set::default();
            nb.Graph = None;
            nb.Container = None;
            nb.LongDistanceNeighborRequirements = Map::default();
            (nb.Cluster.take(), nb.Sequence.take(), nb.Hierarchy.take(), nb.HerdAssignment.take())
        };
        if let Some(c) = cluster {
            if let Ok(mut cb) = c.0.try_borrow_mut() {
                cb.Nodes = Vec::new();
                cb.EdgeAbductions = Vec::new();
                cb.Graph = None;
                cb.Container = None;
            }
        }
        if let Some(s) = sequence {
            if let Ok(mut sb) = s.0.try_borrow_mut() {
                sb.Nodes = Vec::new();
                sb.EdgeAbductions = Vec::new();
                sb.Graph = None;
                sb.Container = None;
            }
        }
        if let Some(h) = hierarchy {
            if let Ok(hb) = h.0.try_borrow() {
                hb.level.clear();
            }
        }
        if let Some(ha) = herd {
            if let Ok(mut hb) = ha.0.try_borrow_mut() {
                hb.sameSidePaired = Set::default();
                hb.oppositeSidePaired = Set::default();
            }
        }
    }
    for graph in &graphs {
        let mut gb = graph.bm();
        gb.Nodes = Vec::new();
        gb.Edges = Vec::new();
        for (_, c) in gb.Clusters.entries() {
            if let Ok(mut cb) = c.0.try_borrow_mut() {
                cb.Nodes = Vec::new();
                cb.EdgeAbductions = Vec::new();
                cb.Graph = None;
                cb.Container = None;
            }
        }
        for (_, s) in gb.Sequences.entries() {
            if let Ok(mut sb) = s.0.try_borrow_mut() {
                sb.Nodes = Vec::new();
                sb.EdgeAbductions = Vec::new();
                sb.Graph = None;
                sb.Container = None;
            }
        }
        gb.Containers.clear();
        gb.Clusters.clear();
        gb.Trees.clear();
        gb.NodeToTree.clear();
        gb.Hubs.clear();
        gb.Sequences.clear();
        gb.Directions.clear();
        gb.CommonUncleSiblings.clear();
        gb.edgeLengthCache = None;
    }
}
