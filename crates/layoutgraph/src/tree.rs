// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/layoutgraph/tree.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

pub struct Tree {
    pub Node: NodeRef,
    pub Parent: Option<TreeRef>,
    pub Children: Vec<TreeRef>,
    pub SentinelEdge: Option<EdgeRef>,
    pub Orientation: geo::Orientation,
}

impl Tree {
    pub(crate) fn shallowCopy(&self) -> Tree {
        Tree {
            Node: self.Node.clone(),
            Parent: self.Parent.clone(),
            Children: self.Children.clone(),
            SentinelEdge: self.SentinelEdge.clone(),
            Orientation: self.Orientation,
        }
    }
}

gocompat::go_ptr!(TreeRef, Tree);

/// Go `type Trees []*Tree`.
pub type Trees = Vec<TreeRef>;

pub fn NewTree(node: &NodeRef) -> TreeRef {
    TreeRef::new(Tree { Node: node.clone(), Parent: None, Children: Vec::new(), SentinelEdge: None, Orientation: geo::Orientation::default() })
}

impl TreeRef {
    pub(crate) fn recordInternalTreeEdges(&self, isTreeEdge: &mut Map<EdgeRef, bool>) {
        let (parent, sentinel, children) = {
            let t = self.b();
            (t.Parent.clone(), t.SentinelEdge.clone(), t.Children.clone())
        };
        if parent.is_some() {
            if let Some(se) = sentinel {
                isTreeEdge.insert(se, true);
            }
        }
        for c in &children {
            c.recordInternalTreeEdges(isTreeEdge);
        }
    }
}

impl GraphRef {
    pub(crate) fn buildIsTreeEdgeMap(&self) -> Map<EdgeRef, bool> {
        // Note: this only includes internal tree edges. It doesn't include the edge between the tree root and the rootSentinel
        let mut isTreeEdge = Map::default();
        let trees = self.b().Trees.clone();
        for rootSentinel in self.TreeOrder() {
            for root in trees.at(&rootSentinel) {
                root.recordInternalTreeEdges(&mut isTreeEdge);
            }
        }
        isTreeEdge
    }

    pub(crate) fn isIsolatedTree(&self, node: Option<&NodeRef>) -> bool {
        // Go dereferences node (nil sentinel keys would panic here).
        let node = node.expect("nil pointer dereference: tree sentinel");
        if node.b().isContainer {
            return false;
        }
        let roots = self.b().Trees.at(&Some(node.clone()));
        // it is an isolated tree if all edges are to trees
        for edge in node.edges() {
            let mut isToTree = false;
            for root in &roots {
                if root.b().SentinelEdge.as_ref() == Some(&edge) {
                    isToTree = true;
                    break;
                }
            }
            if !isToTree {
                return false;
            }
        }
        true
    }

    pub(crate) fn addIsolatedTreeEdgesToMap(&self, isTreeEdge: &mut Map<EdgeRef, bool>) {
        let trees = self.b().Trees.clone();
        for rootSentinel in self.TreeOrder() {
            if self.isIsolatedTree(rootSentinel.as_ref()) {
                for root in trees.at(&rootSentinel) {
                    if let Some(se) = root.b().SentinelEdge.clone() {
                        isTreeEdge.insert(se, true);
                    }
                }
            }
        }
    }
}

impl TreeRef {
    // tree is either the source or target of its sentinel edge, return true if it is the source
    pub(crate) fn isSentinelEdgeSource(&self) -> bool {
        let t = self.b();
        let se = t.SentinelEdge.as_ref().expect("nil pointer dereference: tree.SentinelEdge");
        let r = se.b().From == t.Node;
        r
    }

    // return the node this tree is connected to via its sentinel edge
    pub(crate) fn sentinelNode(&self) -> NodeRef {
        let se = self.b().SentinelEdge.clone().expect("nil pointer dereference: tree.SentinelEdge");
        if self.isSentinelEdgeSource() {
            return se.to();
        }
        se.from()
    }

    pub(crate) fn root(&self) -> TreeRef {
        let parent = self.b().Parent.clone();
        match parent {
            None => self.clone(),
            Some(p) => p.root(),
        }
    }
}
