// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Copyright 2026 Niklas Krieger
// Go reference oracle for the orthoflow parity tests. Not part of
// terrastruct/d2: scripts/gen-goref.sh copies it into a d2 checkout
// (it uses TALA's internal packages, Copyright 2022 Terrastruct Inc.).

package hierarchy

// rankDAG/build parity cases for crates/hierarchy/src/parity_test.rs. Run with
// ORTHOFLOW_HIER_PARITY_OUT=<dir> go test -run TestZZTSParityGen ./d2layouts/d2talalayout/internal/hierarchy/

import (
	"encoding/json"
	"math/rand"
	"os"
	"path/filepath"
	"testing"

	"github.com/d2lang/d2/d2layouts/d2talalayout/internal/layoutgraph"
)

type zzEdge struct {
	From, To int
	Src, Dst string
	Weight   int
}

type zzCase struct {
	Kind       string
	Nodes      []int
	Edges      []zzEdge
	Force      bool
	Nil        bool
	Err        bool
	Levels     map[int]int
	LevelCount int
}

func zzBuildGraph(c *zzCase) (*layoutgraph.Graph, map[int]*layoutgraph.Node) {
	g := layoutgraph.NewGraph()
	byID := map[int]*layoutgraph.Node{}
	for _, id := range c.Nodes {
		n := layoutgraph.NewNode(layoutgraph.EntityID(id), 10, 10)
		g.AddNewNodeToContainer(nil, n)
		byID[id] = n
	}
	for i, e := range c.Edges {
		edge := g.Connect(byID[e.From], byID[e.To])
		edge.ID = layoutgraph.EntityID(i + 1)
		edge.SourceArrowhead = layoutgraph.Arrowhead(e.Src)
		edge.TargetArrowhead = layoutgraph.Arrowhead(e.Dst)
		if e.Weight > 0 {
			edge.SetHierarchyRankWeight(e.Weight)
		}
	}
	return g, byID
}

func TestZZTSParityGen(t *testing.T) {
	out := os.Getenv("ORTHOFLOW_HIER_PARITY_OUT")
	if out == "" {
		t.Skip()
	}
	var cases []zzCase
	arrows := []string{"", "none", "triangle", "arrow"}
	for seed := int64(0); seed < 400; seed++ {
		r := rand.New(rand.NewSource(seed))
		n := 2 + r.Intn(14)
		c := zzCase{}
		perm := r.Perm(100)
		for i := 0; i < n; i++ {
			c.Nodes = append(c.Nodes, perm[i]+1)
		}
		if seed%2 == 0 {
			// rankDAG on a connected weighted DAG (edges forward in a random order)
			c.Kind = "rank"
			seen := map[[2]int]bool{}
			for i := 1; i < n; i++ {
				j := r.Intn(i)
				seen[[2]int{j, i}] = true
				c.Edges = append(c.Edges, zzEdge{From: c.Nodes[j], To: c.Nodes[i], Src: "none", Dst: "triangle", Weight: 1 + r.Intn(5)})
			}
			extra := r.Intn(2 * n)
			for k := 0; k < extra; k++ {
				a, b := r.Intn(n), r.Intn(n)
				if a == b {
					continue
				}
				if a > b {
					a, b = b, a
				}
				if seen[[2]int{a, b}] {
					continue
				}
				seen[[2]int{a, b}] = true
				c.Edges = append(c.Edges, zzEdge{From: c.Nodes[a], To: c.Nodes[b], Src: "none", Dst: "triangle", Weight: 1 + r.Intn(5)})
			}
			r.Shuffle(len(c.Edges), func(i, j int) { c.Edges[i], c.Edges[j] = c.Edges[j], c.Edges[i] })
			g, _ := zzBuildGraph(&c)
			res, err := rankDAG(t.Context(), g)
			if err != nil {
				c.Err = true
			} else {
				c.Levels = map[int]int{}
				for node, level := range res.nodeToLevel {
					c.Levels[int(node.ID)] = level
				}
				c.LevelCount = res.levelCount
			}
		} else {
			c.Kind = "build"
			c.Force = r.Intn(3) == 0
			for i := 1; i < n; i++ {
				j := r.Intn(i)
				a, b := j, i
				if r.Intn(4) == 0 {
					a, b = b, a
				}
				c.Edges = append(c.Edges, zzEdge{From: c.Nodes[a], To: c.Nodes[b], Src: arrows[r.Intn(4)], Dst: arrows[r.Intn(4)]})
				if r.Intn(3) != 0 {
					c.Edges[len(c.Edges)-1].Src = ""
					c.Edges[len(c.Edges)-1].Dst = "triangle"
				}
			}
			extra := r.Intn(2 * n)
			for k := 0; k < extra; k++ {
				a, b := r.Intn(n), r.Intn(n)
				e := zzEdge{From: c.Nodes[a], To: c.Nodes[b], Src: "", Dst: "triangle"}
				if r.Intn(4) == 0 {
					e.Src = arrows[r.Intn(4)]
					e.Dst = arrows[r.Intn(4)]
				}
				c.Edges = append(c.Edges, e)
			}
			g, _ := zzBuildGraph(&c)
			h, err := build(t.Context(), g, c.Force, Candidates(g), nil)
			if err != nil {
				c.Err = true
			} else if h == nil {
				c.Nil = true
			} else {
				c.Levels = map[int]int{}
				for node, level := range h.Levels() {
					c.Levels[int(node.ID)] = level
				}
				c.LevelCount = h.LevelCount
			}
		}
		cases = append(cases, c)
	}
	data, err := json.Marshal(cases)
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(out, "cases.json"), data, 0o644); err != nil {
		t.Fatal(err)
	}
}
