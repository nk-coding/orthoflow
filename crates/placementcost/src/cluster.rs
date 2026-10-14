// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/placementcost/cluster.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

pub(crate) fn clusterExactlyTwoExternalConnectedNodes(cluster: &layoutgraph::ClusterRef) -> (Option<NodeRef>, Option<NodeRef>, bool) {
    let (currentGraph, abductions) = {
        let c = cluster.b();
        (c.Nodes[0].graph(), c.EdgeAbductions.clone())
    };
    let mut first: Option<NodeRef> = None;
    let mut second: Option<NodeRef> = None;
    let mut moreThanTwo = false;
    for edgeAbduction in &abductions {
        let ea = edgeAbduction.b();
        let mut externalNode: Option<NodeRef> = None;
        if ea.OriginallyFrom.is_none() && ea.OriginallyTo.is_some() {
            // edge coming into cluster
            let cf = ea.CurrentFrom.as_ref().expect("nil pointer dereference: CurrentFrom");
            if cf.has_tl() && cf.graph() == currentGraph {
                externalNode = Some(cf.clone());
            }
        } else if ea.OriginallyTo.is_none() && ea.OriginallyFrom.is_some() {
            // edge exiting from cluster
            let ct = ea.CurrentTo.as_ref().expect("nil pointer dereference: CurrentTo");
            if ct.has_tl() && ct.graph() == currentGraph {
                externalNode = Some(ct.clone());
            }
        }
        if externalNode.is_none() || externalNode == first || externalNode == second {
            continue;
        }
        if first.is_none() {
            first = externalNode;
            continue;
        }
        if second.is_none() {
            second = externalNode;
            continue;
        }
        moreThanTwo = true;
    }
    let exactlyTwo = !moreThanTwo && first.is_some() && second.is_some();
    (first, second, exactlyTwo)
}
