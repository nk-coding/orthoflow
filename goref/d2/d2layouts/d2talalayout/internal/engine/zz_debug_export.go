// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Copyright 2026 Niklas Krieger
// Go reference oracle for the orthoflow parity tests. Not part of
// terrastruct/d2: scripts/gen-goref.sh copies it into a d2 checkout
// (it uses TALA's internal packages, Copyright 2022 Terrastruct Inc.).

package engine

// Port-harness hook (not upstream): runs every pipeline stage and reports the
// graph after each one so language ports can be diffed stage by stage.

import (
	"context"
	"fmt"

	"github.com/d2lang/d2/d2layouts/d2talalayout/internal/grouping"
	"github.com/d2lang/d2/d2layouts/d2talalayout/internal/layoutgraph"
)

// StageNames lists the default pipeline stage names in order.
func StageNames() []string {
	names := make([]string, len(defaultPipelineStages))
	for i, s := range defaultPipelineStages {
		names[i] = s.name
	}
	return names
}

// DebugRunStages clones graph and runs the default pipeline, invoking after
// each stage with the stage index, name and the live workspace graph.
func DebugRunStages(ctx context.Context, graph *layoutgraph.Graph, seed int64, after func(int, string, *layoutgraph.Graph) error) (*layoutgraph.Graph, error) {
	workspace, err := layoutgraph.Clone(ctx, graph)
	if err != nil {
		return nil, err
	}
	if len(workspace.Nodes) == 0 {
		return workspace, nil
	}
	p := newPipeline(workspace, seed, false)
	ctx, _, err = layoutgraph.EnsureTransactionWorkGuard(ctx, "AutolayoutTransactions")
	if err != nil {
		return nil, err
	}
	for i := range defaultPipelineStages {
		stage := &defaultPipelineStages[i]
		if i == 0 {
			grouping.ResetClusters(p.graph)
			p.resetFullLayoutRouteState()
		}
		if err := stage.run(p, ctx); err != nil {
			return nil, fmt.Errorf("%s: %w", stage.name, err)
		}
		if err := after(i, stage.name, p.graph); err != nil {
			return nil, err
		}
	}
	return workspace, nil
}

// DebugReplayStage runs only stage index on graph (in place) using a fresh
// pipeline seeded with seed. Port harness: given the same input snapshot, a
// port's replay must produce an identical graph.
func DebugReplayStage(ctx context.Context, graph *layoutgraph.Graph, seed int64, index int) error {
	if index < 0 || index >= len(defaultPipelineStages) {
		return fmt.Errorf("stage index %d out of range", index)
	}
	p := newPipeline(graph, seed, false)
	ctx, _, err := layoutgraph.EnsureTransactionWorkGuard(ctx, "AutolayoutTransactions")
	if err != nil {
		return err
	}
	stage := &defaultPipelineStages[index]
	if index == 0 {
		grouping.ResetClusters(p.graph)
		p.resetFullLayoutRouteState()
	}
	if err := stage.run(p, ctx); err != nil {
		return fmt.Errorf("%s: %w", stage.name, err)
	}
	return nil
}
