// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Copyright 2026 Niklas Krieger
// Go reference oracle for the orthoflow parity tests. Not part of
// terrastruct/d2: scripts/gen-goref.sh copies it into a d2 checkout
// (it uses TALA's internal packages, Copyright 2022 Terrastruct Inc.).

// Command routebref is a randomized differential oracle for the routing
// post-processing stages (twin: crates/routing/tests/replay_fuzz.rs). It perturbs the routes
// of a stage snapshot, writes the perturbed graph, applies one stage and
// writes Go's result, so the port can replay the identical input.
//
//	routebref <seed> <in.json> <outdir>
//	routebref route <seed> <in.json> <outdir>   (perturbed whole-graph EdgeRouting)
//
// For every stage S it writes outdir/<S>.in.json and outdir/<S>.out.json
// (or outdir/<S>.err with the error text).
package main

import (
	"context"
	"fmt"
	"math/rand"
	"os"
	"path/filepath"
	"strconv"

	"github.com/d2lang/d2/d2layouts/d2talalayout/internal/graphjson"
	"github.com/d2lang/d2/d2layouts/d2talalayout/internal/layoutgraph"
	"github.com/d2lang/d2/d2layouts/d2talalayout/internal/routing"
	"github.com/d2lang/d2/lib/geo"
	"github.com/d2lang/d2/lib/label"
)

var stages = []struct {
	name string
	run  func(context.Context, *layoutgraph.Graph) error
}{
	{"Crosshatch", routing.Crosshatch},
	{"SimplifyEdgeRoutes", routing.SimplifyEdgeRoutes},
	{"SwapEdgePorts", routing.SwapAllEdgePorts},
	{"StraightEdgesFallback", routing.StraightEdgesFallback},
	{"BalanceEdgeSegments", routing.BalanceEdgeSegments},
	{"FixClusterEdgeBranching", routing.FixClusterEdgeBranching},
	{"TraceEdgesToShapeBorder", routing.TraceEdgesToShapeBorder},
	{"ReorderDuplicates", routing.ReorderDuplicates},
	{"NudgeEdgeChannels", routing.NudgeEdgeChannels},
	{"ShortcutEdgeRoutes", routing.ShortcutEdgeRoutes},
}

func check(err error) {
	if err != nil {
		fmt.Fprintln(os.Stderr, "error:", err)
		os.Exit(1)
	}
}

func read(ctx context.Context, data []byte) *layoutgraph.Graph {
	var g layoutgraph.Graph
	check(graphjson.Unmarshal(ctx, data, &g))
	return &g
}

func rnd(r *rand.Rand, a, b float64) float64 {
	lo, hi := a, b
	if lo > hi {
		lo, hi = hi, lo
	}
	return float64(int(lo) + r.Intn(int(hi-lo)+1))
}

// staircase builds an orthogonal route from p0 to pN with k interior legs.
func staircase(r *rand.Rand, p0, pN geo.Point, k int, verticalFirst bool) []*geo.Point {
	if verticalFirst {
		p0.X, p0.Y = p0.Y, p0.X
		pN.X, pN.Y = pN.Y, pN.X
	}
	out := []*geo.Point{geo.NewPoint(p0.X, p0.Y)}
	y := p0.Y
	for i := 0; i < k; i++ {
		x := rnd(r, p0.X-40, pN.X+40)
		if i == k-1 {
			x = pN.X
		}
		out = append(out, geo.NewPoint(x, y))
		if i == k-1 {
			break
		}
		y = rnd(r, p0.Y-40, pN.Y+40)
		out = append(out, geo.NewPoint(x, y))
	}
	out = append(out, geo.NewPoint(pN.X, pN.Y))
	if verticalFirst {
		for _, p := range out {
			p.X, p.Y = p.Y, p.X
		}
	}
	return out
}

func perturb(r *rand.Rand, g *layoutgraph.Graph) {
	for _, e := range g.Edges {
		if len(e.Points) < 2 || e.IsCurve {
			continue
		}
		p0, pN := *e.Points[0], *e.Points[len(e.Points)-1]
		switch r.Intn(6) {
		case 0, 1:
			e.Points = staircase(r, p0, pN, 1+r.Intn(4), r.Intn(2) == 0)
		case 2:
			// duplicate / collinear points
			var pts []*geo.Point
			for _, p := range e.Points {
				pts = append(pts, p)
				if r.Intn(4) == 0 {
					pts = append(pts, p.Copy())
				}
			}
			e.Points = pts
		case 3:
			// shift interior points orthogonally
			for i := 1; i+1 < len(e.Points); i++ {
				if r.Intn(2) == 0 {
					d := float64(r.Intn(41) - 20)
					a, b := e.Points[i-1], e.Points[i]
					if a.X == b.X {
						a.X += d
						b.X += d
					} else if a.Y == b.Y {
						a.Y += d
						b.Y += d
					}
				}
			}
		}
		if r.Intn(5) == 0 && e.Label == nil {
			e.Label = &layoutgraph.Label{Text: "l" + strconv.Itoa(r.Intn(9)), Position: label.Unset, Width: 10, Height: 10}
		}
	}
	// Shared cluster-side ports exercise Crosshatch.
	for _, vessel := range g.ClusterOrder() {
		cluster := g.Clusters[vessel]
		var shared *geo.Point
		for _, abduction := range cluster.EdgeAbductions {
			e := abduction.Edge
			if e == nil || len(e.Points) < 2 || r.Intn(3) == 0 {
				continue
			}
			for _, n := range cluster.Nodes {
				var port *geo.Point
				if e.From == n {
					port = e.Points[0]
				} else if e.To == n {
					port = e.Points[len(e.Points)-1]
				}
				if port == nil {
					continue
				}
				if shared == nil {
					shared = port.Copy()
				} else {
					port.X, port.Y = shared.X, shared.Y
				}
				break
			}
		}
	}
	// Straight labeled duplicates exercise ReorderDuplicates.
	for i, e := range g.Edges {
		if r.Intn(2) == 0 || e.From == e.To || e.IsCurve {
			continue
		}
		var group []*layoutgraph.Edge
		for _, other := range g.Edges[i:] {
			if other == e || other.IsDuplicateOf(e) {
				group = append(group, other)
			}
		}
		if len(group) < 3 {
			continue
		}
		from, to := e.From, e.To
		vertical := from.TopLeft.Y+from.Height <= to.TopLeft.Y || to.TopLeft.Y+to.Height <= from.TopLeft.Y
		for j, d := range group {
			a, b := d.From, d.To
			if vertical {
				x := rnd(r, from.TopLeft.X+1, from.TopLeft.X+from.Width-1)
				if j == 0 {
					x = from.TopLeft.X + from.Width/2
				}
				ya, yb := a.TopLeft.Y+a.Height, b.TopLeft.Y
				if a.TopLeft.Y > b.TopLeft.Y {
					ya, yb = a.TopLeft.Y, b.TopLeft.Y+b.Height
				}
				d.Points = []*geo.Point{geo.NewPoint(x, ya), geo.NewPoint(x, yb)}
			} else {
				y := rnd(r, from.TopLeft.Y+1, from.TopLeft.Y+from.Height-1)
				xa, xb := a.TopLeft.X+a.Width, b.TopLeft.X
				if a.TopLeft.X > b.TopLeft.X {
					xa, xb = a.TopLeft.X, b.TopLeft.X+b.Width
				}
				d.Points = []*geo.Point{geo.NewPoint(xa, y), geo.NewPoint(xb, y)}
			}
			if r.Intn(2) == 0 {
				d.Label = &layoutgraph.Label{Text: "dup", Position: label.Unset, Width: 10, Height: 10}
			}
		}
	}
}

// perturbForRouting moves free top-level leaf nodes into empty space and
// shuffles arrowheads so whole-graph routing sees new geometry.
func perturbForRouting(r *rand.Rand, g *layoutgraph.Graph) {
	arrowheads := []layoutgraph.Arrowhead{layoutgraph.NoArrowhead, layoutgraph.TriangleArrowhead, "arrow", "diamond"}
	free := func(n *layoutgraph.Node) bool {
		if n.Container != nil || n.IsContainer() || n.Cluster != nil || n.Sequence != nil || n.Hierarchy != nil || n.TopLeft == nil {
			return false
		}
		if _, ok := g.NodeToTree[n]; ok {
			return false
		}
		return true
	}
	const pad = 30.0
	overlaps := func(n *layoutgraph.Node, x, y float64) bool {
		for _, o := range g.Nodes {
			if o == n || o.TopLeft == nil {
				continue
			}
			if x-pad < o.TopLeft.X+o.Width && o.TopLeft.X < x+n.Width+pad && y-pad < o.TopLeft.Y+o.Height && o.TopLeft.Y < y+n.Height+pad {
				return true
			}
		}
		return false
	}
	for _, n := range g.Nodes {
		if !free(n) || r.Intn(2) == 0 {
			continue
		}
		for try := 0; try < 4; try++ {
			x := n.TopLeft.X + float64(r.Intn(121)-60)
			y := n.TopLeft.Y + float64(r.Intn(121)-60)
			if !overlaps(n, x, y) {
				n.TopLeft.X, n.TopLeft.Y = x, y
				break
			}
		}
	}
	for _, e := range g.Edges {
		if e.From == e.To || e.From.Hierarchy != nil || e.To.Hierarchy != nil {
			continue
		}
		if _, ok := g.NodeToTree[e.From]; ok {
			continue
		}
		if _, ok := g.NodeToTree[e.To]; ok {
			continue
		}
		if r.Intn(3) == 0 {
			e.SourceArrowhead = arrowheads[r.Intn(len(arrowheads))]
			e.TargetArrowhead = arrowheads[r.Intn(len(arrowheads))]
		}
	}
}

func mainRoute(seed int64, data []byte, outdir string) {
	ctx, _, err := layoutgraph.EnsureTransactionWorkGuard(context.Background(), "AutolayoutTransactions")
	check(err)
	g := read(ctx, data)
	perturbForRouting(rand.New(rand.NewSource(seed)), g)
	in, err := graphjson.Marshal(ctx, g)
	check(err)
	check(os.WriteFile(filepath.Join(outdir, "EdgeRouting.in.json"), in, 0o644))
	g = read(ctx, in)
	if _, err := routing.RouteGraph(ctx, g, routing.GraphRouteOptions{}); err != nil {
		check(os.WriteFile(filepath.Join(outdir, "EdgeRouting.err"), []byte(err.Error()), 0o644))
		return
	}
	out, err := graphjson.Marshal(ctx, g)
	check(err)
	check(os.WriteFile(filepath.Join(outdir, "EdgeRouting.out.json"), out, 0o644))
}

func main() {
	if len(os.Args) == 5 && os.Args[1] == "route" {
		seed, err := strconv.ParseInt(os.Args[2], 10, 64)
		check(err)
		data, err := os.ReadFile(os.Args[3])
		check(err)
		check(os.MkdirAll(os.Args[4], 0o755))
		mainRoute(seed, data, os.Args[4])
		return
	}
	if len(os.Args) != 4 {
		fmt.Fprintln(os.Stderr, "usage: routebref <seed> <in.json> <outdir>")
		os.Exit(2)
	}
	seed, err := strconv.ParseInt(os.Args[1], 10, 64)
	check(err)
	data, err := os.ReadFile(os.Args[2])
	check(err)
	outdir := os.Args[3]
	check(os.MkdirAll(outdir, 0o755))
	ctx, _, err := layoutgraph.EnsureTransactionWorkGuard(context.Background(), "AutolayoutTransactions")
	check(err)
	for index, stage := range stages {
		g := read(ctx, data)
		perturb(rand.New(rand.NewSource(seed*100+int64(index))), g)
		in, err := graphjson.Marshal(ctx, g)
		if err != nil {
			continue // perturbation produced an unserializable graph; skip
		}
		check(os.WriteFile(filepath.Join(outdir, stage.name+".in.json"), in, 0o644))
		g = read(ctx, in)
		if err := stage.run(ctx, g); err != nil {
			check(os.WriteFile(filepath.Join(outdir, stage.name+".err"), []byte(err.Error()), 0o644))
			continue
		}
		out, err := graphjson.Marshal(ctx, g)
		check(err)
		check(os.WriteFile(filepath.Join(outdir, stage.name+".out.json"), out, 0o644))
	}
}
