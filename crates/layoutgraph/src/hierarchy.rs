// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/layoutgraph/hierarchy.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

/// Hierarchy records the level assigned to each member of a directed, layered
/// subgraph together with the discovery facts used by later layout stages.
/// Discovery and placement live in the sibling hierarchy package; this type
/// remains here because nodes own their hierarchy membership as graph state.
#[derive(Clone, Debug, Default)]
pub struct Hierarchy {
    /// Shared map handle: `Levels()` hands out the same map the hierarchy owns.
    pub(crate) level: GoMap<NodeRef, i64>,

    pub LevelCount: i64,
}

gocompat::go_ptr!(HierarchyRef, Hierarchy);

/// NewHierarchy creates an empty hierarchy record.
pub fn NewHierarchy() -> HierarchyRef {
    HierarchyRef::new(Hierarchy { level: GoMap::new(), LevelCount: 0 })
}

impl HierarchyRef {
    /// Levels returns the hierarchy's node-to-level mapping. The hierarchy
    /// algorithms mutate this map while assigning and replacing memberships.
    pub fn Levels(&self) -> GoMap<NodeRef, i64> {
        self.b().level.clone()
    }

    /// ReplaceLevels atomically replaces the node-to-level mapping.
    pub fn ReplaceLevels(&self, levels: GoMap<NodeRef, i64>) {
        self.bm().level = levels;
    }
}

impl NodeRef {
    /// HierarchyLevel returns n's assigned level. Callers must only use it for a
    /// node with non-nil Hierarchy membership.
    pub fn HierarchyLevel(&self) -> i64 {
        let h = self.b().Hierarchy.clone().expect("nil pointer dereference: node.Hierarchy");
        let levels = h.b().level.clone();
        levels.at(self)
    }
}
