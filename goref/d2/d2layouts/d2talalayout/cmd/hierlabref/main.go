// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Copyright 2026 Niklas Krieger
// Go reference oracle for the orthoflow parity tests. Not part of
// terrastruct/d2: scripts/gen-goref.sh copies it into a d2 checkout
// (it uses TALA's internal packages, Copyright 2022 Terrastruct Inc.).

// Command hierlabref is a unit-parity oracle for the
// hierarchy, labeling, loops and quality packages (twin: crates/quality/tests/hierlabref.rs).
//
//	hierlabref inspect g.json    quality.Inspect metrics
//	hierlabref compound g.json   hierarchy.PlaceCompound(ctx, g, rand(0)) + graph
//	hierlabref newedges g.json   labeling.PlaceNewEdges(ctx, g, odd-indexed edges) + graph
//	hierlabref loops g.json      loops.Route on every node, then loops.ComputeOffsets + graph
package main

import (
	"context"
	"encoding/json"
	"fmt"
	"math/rand"
	"os"

	"github.com/d2lang/d2/d2layouts/d2talalayout/internal/graphjson"
	"github.com/d2lang/d2/d2layouts/d2talalayout/internal/hierarchy"
	"github.com/d2lang/d2/d2layouts/d2talalayout/internal/labeling"
	"github.com/d2lang/d2/d2layouts/d2talalayout/internal/layoutgraph"
	"github.com/d2lang/d2/d2layouts/d2talalayout/internal/loops"
	"github.com/d2lang/d2/d2layouts/d2talalayout/internal/quality"
	"github.com/d2lang/d2/lib/geo"
)

func check(err error) {
	if err != nil {
		fmt.Fprintln(os.Stderr, "error:", err)
		os.Exit(1)
	}
}

func errString(err error) string {
	if err != nil {
		return "error"
	}
	return ""
}

func main() {
	ctx := context.Background()
	data, err := os.ReadFile(os.Args[2])
	check(err)
	var g layoutgraph.Graph
	check(graphjson.Unmarshal(ctx, data, &g))
	out := map[string]any{}
	switch os.Args[1] {
	case "inspect":
		m, err := quality.Inspect(ctx, &g)
		out["error"] = errString(err)
		out["metrics"] = m
	case "compound":
		changed, err := hierarchy.PlaceCompound(ctx, &g, rand.New(rand.NewSource(0)))
		out["changed"] = changed
		out["error"] = errString(err)
	case "newedges":
		var edges []*layoutgraph.Edge
		for i, e := range g.Edges {
			if i%2 == 1 {
				edges = append(edges, e)
			}
		}
		out["error"] = errString(labeling.PlaceNewEdges(ctx, &g, edges))
	case "loops":
		var routes [][][]float64
		for _, n := range g.Nodes {
			for _, e := range loops.Route(n) {
				var pts [][]float64
				for _, p := range e.Points {
					pts = append(pts, []float64{p.X, p.Y})
				}
				routes = append(routes, pts)
			}
		}
		out["routes"] = routes
		loops.ComputeOffsets(&g)
		var offsets [][]float64
		for _, n := range g.Nodes {
			var o []float64
			for _, k := range []geo.Orientation{geo.TopLeft, geo.Top, geo.TopRight, geo.Right, geo.BottomRight, geo.Bottom, geo.BottomLeft, geo.Left} {
				v, ok := n.LoopOffsets[k]
				if ok {
					o = append(o, v)
				} else {
					o = append(o, -1)
				}
			}
			offsets = append(offsets, o)
		}
		out["offsets"] = offsets
	}
	gd, err := graphjson.Marshal(ctx, &g)
	check(err)
	out["graph"] = json.RawMessage(gd)
	res, err := json.Marshal(out)
	check(err)
	os.Stdout.Write(res)
	fmt.Println()
}
