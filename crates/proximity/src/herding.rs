// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/proximity/herding.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

fn ctxErr(ctx: &Ctx) -> GoResult<()> {
    if let Some(err) = ctx.Err() {
        return Err(GoError::wrap(format!("AssignHerds: {}", err), err));
    }
    Ok(())
}

/// AssignHerds chooses a shared container side for siblings whose external
/// cousins should remain mutually accessible during placement.
pub fn AssignHerds(ctx: &Ctx, graph: &GraphRef, root: Option<&NodeRef>, abductions: &[EdgeAbductionRef]) -> GoResult<()> {
    ctxErr(ctx)?;
    let (mut grouped, mut cousins) = GroupSheep(ctx, graph, root, abductions)?;
    let mut uncleOrder: Vec<NodeRef> = Vec::new();
    for (uncle, nodes) in grouped.clone() {
        if nodes.len() <= 1 {
            grouped.remove(&uncle);
            cousins.remove(&uncle);
        } else {
            uncleOrder.push(uncle);
        }
    }
    gocompat::sort::SlicesSortFunc(&mut uncleOrder, |a: &NodeRef, b: &NodeRef| gocompat::sort::cmpCompare(a.id(), b.id()));

    let components = connectedHerds(ctx, &uncleOrder, &grouped)?;
    let mut unbiasedSide: usize = 0;
    let containers = graph.containers();
    let cousinsOf = |uncle: &NodeRef, node: &NodeRef| -> Vec<NodeRef> {
        cousins.get(uncle).and_then(|m| m.get(node)).cloned().unwrap_or_default()
    };
    for component in &components {
        let mut sides: Vec<geo::Orientation> = vec![geo::Top, geo::Right, geo::Bottom, geo::Left];
        let mut preferred = geo::NONE;
        for uncle in &component.uncles {
            ctxErr(ctx)?;
            let children = containers.at(&Some(uncle.clone()));
            if children.is_empty() {
                return Err(invariant::Errorf(format!("herding uncle {} has no children", uncle.DebugID())));
            }
            if !children[0].has_tl() {
                continue;
            }
            for node in grouped.get(uncle).cloned().unwrap_or_default() {
                for cousin in cousinsOf(uncle, &node) {
                    let Some(assignment) = cousin.b().HerdAssignment.clone() else { continue };
                    let orientation = assignment.b().Orientation;
                    if !orientation.IsHorizontal() && !orientation.IsVertical() {
                        return Err(invariant::Errorf(format!("cousin {} has an invalid herd orientation", cousin.DebugID())));
                    }
                    let bothSides = CanUseBothSides(uncle, orientation);
                    let opposite = orientation.GetOpposite();
                    sides.retain(|side| !(*side != opposite && !(bothSides && *side == orientation)));
                    if preferred == geo::NONE {
                        preferred = opposite;
                        if bothSides && assignment.SameSidePairCount() < assignment.OppositeSidePairCount() {
                            preferred = orientation;
                        }
                    }
                }
            }
        }
        if sides.is_empty() {
            for node in &component.nodes {
                node.bm().HerdAssignment = None;
            }
            continue;
        }
        if preferred == geo::NONE {
            preferred = sides[unbiasedSide % sides.len()];
            unbiasedSide += 1;
        } else if !sides.contains(&preferred) {
            preferred = sides[0];
        }
        for node in &component.nodes {
            let ha = layoutgraph::NewHerdAssignment();
            ha.bm().Orientation = preferred;
            node.bm().HerdAssignment = Some(ha);
        }
        for uncle in &component.uncles {
            if !containers.at(&Some(uncle.clone()))[0].has_tl() {
                continue;
            }
            for node in grouped.get(uncle).cloned().unwrap_or_default() {
                for cousin in cousinsOf(uncle, &node) {
                    let Some(ca) = cousin.b().HerdAssignment.clone() else { continue };
                    let na = node.b().HerdAssignment.clone().unwrap();
                    if preferred == ca.b().Orientation {
                        na.PairSameSide(uncle);
                        ca.PairSameSide(uncle);
                    } else {
                        na.PairOppositeSide(uncle);
                        ca.PairOppositeSide(uncle);
                    }
                }
            }
        }
    }
    ApplyVirally(ctx, &uncleOrder, &grouped)?;

    let clusters = graph.clusters();
    for uncle in &uncleOrder {
        for node in grouped.get(uncle).cloned().unwrap_or_default() {
            let ha = node.b().HerdAssignment.clone();
            if let Some(ha) = ha {
                if node.IsClusterVessel() {
                    let cluster = clusters.get(&node).unwrap();
                    let o = ha.b().Orientation;
                    let arrangement = cluster.b().Arrangement.clone();
                    if (o == geo::Top || o == geo::Bottom) && arrangement == layoutgraph::Column {
                        cluster.bm().Arrangement = layoutgraph::Row;
                    } else if (o == geo::Left || o == geo::Right) && arrangement == layoutgraph::Row {
                        cluster.bm().Arrangement = layoutgraph::Column;
                    }
                }
            }
        }
    }
    Ok(())
}

pub(crate) struct herdComponent {
    pub(crate) nodes: Vec<NodeRef>,
    pub(crate) uncles: Vec<NodeRef>,
}

/// connectedHerds joins groups that share a node, retaining deterministic order.
pub(crate) fn connectedHerds(
    ctx: &Ctx,
    herdOrder: &[NodeRef],
    herds: &Map<NodeRef, Vec<NodeRef>>,
) -> GoResult<Vec<herdComponent>> {
    let mut byNode: Map<NodeRef, Vec<NodeRef>> = Map::default();
    for uncle in herdOrder {
        for node in herds.get(uncle).map(|v| v.as_slice()).unwrap_or(&[]) {
            byNode.entry(node.clone()).or_default().push(uncle.clone());
        }
    }
    let mut seenUncles: Set<NodeRef> = Set::default();
    let mut seenNodes: Set<NodeRef> = Set::default();
    let mut components: Vec<herdComponent> = Vec::new();
    for uncle in herdOrder {
        if seenUncles.contains(uncle) {
            continue;
        }
        let mut component = herdComponent { nodes: Vec::new(), uncles: vec![uncle.clone()] };
        seenUncles.insert(uncle.clone());
        let mut i = 0;
        while i < component.uncles.len() {
            ctxErr(ctx)?;
            let u = component.uncles[i].clone();
            for node in herds.get(&u).cloned().unwrap_or_default() {
                if seenNodes.contains(&node) {
                    continue;
                }
                seenNodes.insert(node.clone());
                component.nodes.push(node.clone());
                for related in byNode.get(&node).cloned().unwrap_or_default() {
                    if !seenUncles.contains(&related) {
                        seenUncles.insert(related.clone());
                        component.uncles.push(related);
                    }
                }
            }
            i += 1;
        }
        components.push(component);
    }
    Ok(components)
}

/// ApplyVirally propagates each known herd orientation through its ordered groups.
pub fn ApplyVirally(ctx: &Ctx, herdOrder: &[NodeRef], herds: &Map<NodeRef, Vec<NodeRef>>) -> GoResult<()> {
    loop {
        ctxErr(ctx)?;
        let mut end = true;
        for uncle in herdOrder {
            ctxErr(ctx)?;
            let nodes = herds.get(uncle).cloned().unwrap_or_default();
            let mut assignment: Option<HerdAssignmentRef> = None;
            for node in &nodes {
                if let Some(ha) = node.b().HerdAssignment.clone() {
                    if ha.b().Orientation != geo::NONE {
                        assignment = Some(ha);
                        break;
                    }
                }
            }
            if let Some(assignment) = assignment {
                let ao = assignment.b().Orientation;
                for node in &nodes {
                    let ha = node.b().HerdAssignment.clone();
                    match ha {
                        None => {
                            end = false;
                            node.bm().HerdAssignment = Some(assignment.Copy());
                        }
                        Some(ha) => {
                            let o = ha.b().Orientation;
                            if o != geo::NONE && o != ao {
                                return Err(invariant::Errorf(format!(
                                    "node {} has herd orientation {}; expected {}",
                                    node.DebugID(),
                                    o.ToString(),
                                    ao.ToString()
                                )));
                            }
                        }
                    }
                }
            }
        }
        if end {
            return Ok(());
        }
    }
}

/// CanUseBothSides reports whether node is long enough perpendicular to
/// orientation to host herd members on both parallel sides.
pub fn CanUseBothSides(node: &NodeRef, orientation: geo::Orientation) -> bool {
    let (w, h) = (node.w(), node.h());
    let isWide = w >= 2.0 * h;
    let isTall = h >= 2.0 * w;
    (orientation == geo::Top || orientation == geo::Bottom) && isWide
        || (orientation == geo::Left || orientation == geo::Right) && isTall
}

/// GroupSheep groups root's children by their external uncle and records the
/// cousin connections that define each group.
pub fn GroupSheep(
    ctx: &Ctx,
    graph: &GraphRef,
    root: Option<&NodeRef>,
    abductions: &[EdgeAbductionRef],
) -> GoResult<(Map<NodeRef, Vec<NodeRef>>, Map<NodeRef, Map<NodeRef, Vec<NodeRef>>>)> {
    let mut byUncle: Map<NodeRef, Vec<NodeRef>> = Map::default();
    let mut toCousin: Map<NodeRef, Map<NodeRef, Vec<NodeRef>>> = Map::default();
    let mut used = vec![false; abductions.len()];
    for node in graph.containers().at(&root.cloned()) {
        ctxErr(ctx)?;
        for (i, abduction) in abductions.iter().enumerate() {
            ctxErr(ctx)?;
            if used[i] {
                continue;
            }
            let a = abduction.b().clone();
            let from = groupVessel(a.OriginallyFrom.as_ref());
            let to = groupVessel(a.OriginallyTo.as_ref());
            let mut cousin: Option<NodeRef> = None;
            let mut current: Option<NodeRef> = None;
            if a.OriginallyTo.is_some() && (from.as_ref() == Some(&node) || descendantOf(from.as_ref(), Some(&node))) {
                if let Some(ct) = &a.CurrentTo {
                    if !ct.IsContainer() {
                        continue;
                    }
                }
                match &to {
                    Some(t) if t.OwningContainer().is_some() => {}
                    _ => continue,
                }
                cousin = to.clone();
                current = a.CurrentTo.clone();
            } else if a.OriginallyFrom.is_some() && (to.as_ref() == Some(&node) || descendantOf(to.as_ref(), Some(&node))) {
                if let Some(cf) = &a.CurrentFrom {
                    if !cf.IsContainer() {
                        continue;
                    }
                }
                match &from {
                    Some(f) if f.OwningContainer().is_some() => {}
                    _ => continue,
                }
                cousin = from.clone();
                current = a.CurrentFrom.clone();
            }
            let Some(mut cousin) = cousin else { continue };
            used[i] = true;
            while cousin.OwningContainer() != current {
                let (cluster, sequence) = {
                    let c = cousin.b();
                    (c.Cluster.clone(), c.Sequence.clone())
                };
                if let Some(c) = cluster {
                    cousin = c.b().Vessel.clone();
                } else if let Some(s) = sequence {
                    cousin = s.b().Vessel.clone();
                } else {
                    // Go dereferences a nil container here (panic) if the chain ends.
                    cousin = cousin.OwningContainer().expect("nil pointer dereference");
                }
            }
            let uncle = cousin.OwningContainer();
            let Some(uncle) = uncle else { continue };
            if !uncle.IsContainer() {
                continue;
            }
            let m = toCousin.entry(uncle.clone()).or_default();
            if !m.contains_key(&node) {
                byUncle.entry(uncle.clone()).or_default().push(node.clone());
            }
            m.entry(node.clone()).or_default().push(cousin);
        }
    }
    Ok((byUncle, toCousin))
}

pub(crate) fn descendantOf(node: Option<&NodeRef>, ancestor: Option<&NodeRef>) -> bool {
    let mut node = node.cloned();
    while let Some(n) = node {
        if Some(&n) == ancestor {
            return true;
        }
        node = parentOf(&n);
    }
    ancestor.is_none()
}

/// SyncHerdFences updates the coordinate constraint for every assigned herd to
/// the current graph boundary.
pub fn SyncHerdFences(graph: &GraphRef) {
    let (topLeft, bottomRight) = graph.BoundingBox();
    for node in graph.nodes() {
        let (ha, fixed) = {
            let n = node.b();
            (n.HerdAssignment.clone(), n.FixedTopLeft.is_some())
        };
        let Some(ha) = ha else { continue };
        if fixed {
            continue;
        }
        let o = ha.b().Orientation;
        if o == geo::Top {
            ha.bm().Val = topLeft.as_ref().unwrap().b().Y;
        } else if o == geo::Bottom {
            ha.bm().Val = bottomRight.as_ref().unwrap().b().Y;
        } else if o == geo::Left {
            ha.bm().Val = topLeft.as_ref().unwrap().b().X;
        } else if o == geo::Right {
            ha.bm().Val = bottomRight.as_ref().unwrap().b().X;
        }
    }
}
