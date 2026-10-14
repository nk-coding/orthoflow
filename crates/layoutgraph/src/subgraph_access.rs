// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/layoutgraph/subgraph_access.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

/// SharedWorkStepper is the narrow accounting boundary used when a sibling
/// layout domain supplies the aggregate work budget for graph traversal.
/// (`*limits.WorkGuard` implements it; so do `&dyn` adapters of other domains.)
pub trait SharedWorkStepper {
    fn Step(&self) -> GoResult<()>;
    fn Finish(&self) -> GoResult<()>;
}

/// NodeGraphOwnershipJournal records the graph pointer owned by each node
/// before a subgraph split temporarily redirects it.
#[derive(Clone, Default)]
pub struct NodeGraphOwnershipJournal {
    pub(crate) original: nodeGraphOwnershipJournal,
}

impl NodeGraphOwnershipJournal {
    /// Restore restores every graph pointer captured by the split.
    pub fn Restore(&self) {
        restoreOwnershipJournal(&self.original);
    }

    /// OriginalGraph reports the graph pointer captured for node by the split.
    pub fn OriginalGraph(&self, node: &NodeRef) -> (Option<GraphRef>, bool) {
        match self.original.get(node) {
            Some(g) => (g.clone(), true),
            None => (None, false),
        }
    }
}

impl GraphRef {
    /// SplitSubgraphsTracked splits a graph while returning the exact ownership
    /// journal needed by callers that keep the temporary subgraphs alive during a
    /// larger atomic operation.
    pub fn SplitSubgraphsTracked(
        &self,
        ctx: &Ctx,
        options: SplitOptions,
        work: Option<&dyn SharedWorkStepper>,
    ) -> (GoResult<Vec<GraphRef>>, NodeGraphOwnershipJournal) {
        let mut internal: nodeGraphOwnershipJournal = Map::default();
        let graphs = self.splitSubgraphsWithOwnership(ctx, options, Some(&mut internal), work);
        (graphs, NodeGraphOwnershipJournal { original: internal })
    }
}
