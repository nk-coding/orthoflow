// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/placement/cluster_connections.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.
//
// Port of placement/cluster_connections.go
#[allow(unused_imports)]
use crate::*;
use gocompat::Set;
use layoutgraph::{ClusterRef, NodeRef};

// clusterExternalConnectedNodes returns the positioned endpoints outside the
// cluster that remain in the cluster nodes' current placement graph. The
// edge-abduction order is significant: callers use the stable first-seen order
// when comparing opposite-side placements.
pub(crate) fn clusterExternalConnectedNodes(cluster: &ClusterRef) -> Vec<NodeRef> {
    let (currentGraph, abductions) = {
        let c = cluster.b();
        (c.Nodes[0].graph(), c.EdgeAbductions.clone())
    };
    let sameGraph = |n: &NodeRef| match (n.graph(), &currentGraph) {
        (None, None) => true,
        (Some(a), Some(b)) => a == *b,
        _ => false,
    };
    let mut set: Set<NodeRef> = Set::default();
    let mut externalNodes: Vec<NodeRef> = Vec::new();
    for edgeAbduction in &abductions {
        let ea = edgeAbduction.b().clone();
        if ea.OriginallyFrom.is_none() && ea.OriginallyTo.is_some() {
            let cf = ea.CurrentFrom.clone().expect("nil pointer dereference: CurrentFrom");
            if cf.has_tl() && sameGraph(&cf) {
                if !set.contains(&cf) {
                    externalNodes.push(cf.clone());
                    set.insert(cf);
                }
            }
        } else if ea.OriginallyTo.is_none() && ea.OriginallyFrom.is_some() {
            let ct = ea.CurrentTo.clone().expect("nil pointer dereference: CurrentTo");
            if ct.has_tl() && sameGraph(&ct) {
                if !set.contains(&ct) {
                    externalNodes.push(ct.clone());
                    set.insert(ct);
                }
            }
        }
    }
    externalNodes
}
