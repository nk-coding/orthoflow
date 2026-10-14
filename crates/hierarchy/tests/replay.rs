// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Copyright 2026 Niklas Krieger
//
// Stage invocations mirror d2layouts/d2talalayout/internal/engine/pipeline.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).

extern crate orthoflow_hierarchy as hierarchy;
#[test]
#[ignore = "needs Go reference data (ORTHOFLOW_GOREF), see README"]
fn replay_preprocess_hierarchies() {
    testkit::assert_replay("PreprocessHierarchies", |ctx, env| {
        hierarchy::Assign(ctx, &env.graph, None, &hierarchy::Candidates(&env.graph))?;
        hierarchy::Place(ctx, &env.graph, None, Some(&env.hierarchyPlacementRandom))?;
        hierarchy::RemoveIsolatedMemberships(&env.graph);
        Ok(())
    });
}
