// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/trees/types.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

/// Go `type TreeEdgeDirection string`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TreeEdgeDirection {
    Outwards,
    Inwards,
    Bidirectional,
    Undirected,
}
pub use TreeEdgeDirection::{Bidirectional, Inwards, Outwards, Undirected};

impl TreeEdgeDirection {
    pub fn as_str(&self) -> &'static str {
        match self {
            Outwards => "Outwards",
            Inwards => "Inwards",
            Bidirectional => "Bidirectional",
            Undirected => "Undirected",
        }
    }
}

pub(crate) fn newWorkGuard(ctx: &Ctx, location: &str) -> GoResult<WorkGuardRef> {
    limits::NewWorkGuard(ctx, location, limits::MaxEngineWorkUnits)
}

/// treeEdgeDirection determines whether edge points from node toward its
/// sentinel, toward node, in both directions, or in neither direction.
pub(crate) fn treeEdgeDirection(node: &NodeRef, edge: &EdgeRef) -> TreeEdgeDirection {
    if edge.IsBidirectional() {
        Bidirectional
    } else if edge.IsUndirected() {
        Undirected
    } else if edge.to() == *node {
        Inwards
    } else {
        Outwards
    }
}

// Rust-only memory hygiene: trees form Parent <-> Children Rc cycles. Trees that
// Go simply drops (garbage) would leak in Rust, so Preprocess/Place record every
// tree they create and, on exit, unlink the ones no longer reachable from the
// graph. Unreachable trees are unobservable, so this does not change behaviour.
thread_local! {
    static CREATED_TREES: std::cell::RefCell<Option<Vec<TreeRef>>> = const { std::cell::RefCell::new(None) };
}

pub(crate) fn newTree(node: &NodeRef) -> TreeRef {
    let t = layoutgraph::NewTree(node);
    CREATED_TREES.with(|c| {
        if let Some(v) = c.borrow_mut().as_mut() {
            v.push(t.clone());
        }
    });
    t
}

/// Runs f while recording created trees, then unlinks those unreachable from g.
pub(crate) fn withTreeRecording<T>(g: &GraphRef, f: impl FnOnce() -> T) -> T {
    let outer = CREATED_TREES.with(|c| c.borrow_mut().replace(Vec::new()));
    let result = f();
    let created = CREATED_TREES.with(|c| std::mem::replace(&mut *c.borrow_mut(), outer)).unwrap_or_default();
    if !created.is_empty() {
        let mut reachable: Set<TreeRef> = Set::default();
        let mut stack: Vec<TreeRef> = Vec::new();
        for (_, roots) in g.trees().entries() {
            stack.extend(roots);
        }
        stack.extend(g.nodeToTree().values());
        while let Some(t) = stack.pop() {
            if !reachable.insert(t.clone()) {
                continue;
            }
            let tb = t.b();
            stack.extend(tb.Children.iter().cloned());
            stack.extend(tb.Parent.iter().cloned());
        }
        for t in created {
            if !reachable.contains(&t) {
                let mut tb = t.bm();
                tb.Parent = None;
                tb.Children = Vec::new();
            }
        }
    }
    result
}
