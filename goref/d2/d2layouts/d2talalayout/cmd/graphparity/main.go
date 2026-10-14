// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Copyright 2026 Niklas Krieger
// Go reference oracle for the orthoflow parity tests. Not part of
// terrastruct/d2: scripts/gen-goref.sh copies it into a d2 checkout
// (it uses TALA's internal packages, Copyright 2022 Terrastruct Inc.).

// Command graphparity prints layoutgraph/graphbounds results for a fixture so
// ports can be compared line by line (Rust twin: rust/crates/graphjson/tests/parity.rs).
package main

import (
	"context"
	"fmt"
	"os"
	"sort"
	"strings"

	"github.com/d2lang/d2/d2layouts/d2talalayout/internal/graphbounds"
	"github.com/d2lang/d2/d2layouts/d2talalayout/internal/graphjson"
	"github.com/d2lang/d2/d2layouts/d2talalayout/internal/layoutgraph"
	"github.com/d2lang/d2/d2layouts/d2talalayout/internal/limits"
	"github.com/d2lang/d2/lib/geo"
	"github.com/d2lang/d2/lib/label"
)

func f(x float64) string { return fmt.Sprintf("%v", x) }

func p(pt *geo.Point) string {
	if pt == nil {
		return "nil"
	}
	return f(pt.X) + "," + f(pt.Y)
}

func pv(pt geo.Point) string { return f(pt.X) + "," + f(pt.Y) }

func id(n *layoutgraph.Node) string {
	if n == nil {
		return "0"
	}
	return fmt.Sprint(n.ID)
}

func ids(ns []*layoutgraph.Node) string {
	out := []string{}
	for _, n := range ns {
		out = append(out, id(n))
	}
	return "[" + strings.Join(out, " ") + "]"
}

func sortedIDs(ns []*layoutgraph.Node) string {
	c := append([]*layoutgraph.Node(nil), ns...)
	sort.Slice(c, func(i, j int) bool { return c[i].ID < c[j].ID })
	return ids(c)
}

func errs(err error) string {
	if err == nil {
		return "ok"
	}
	if layoutgraph.IsCandidateRejection(err) {
		return "reject"
	}
	return "error"
}

func load(ctx context.Context, path string) *layoutgraph.Graph {
	data, err := os.ReadFile(path)
	if err != nil {
		panic(err)
	}
	g := layoutgraph.NewGraph()
	if err := graphjson.Unmarshal(ctx, data, g); err != nil {
		panic(err)
	}
	return g
}

func positions(g *layoutgraph.Graph) string {
	out := []string{}
	for _, n := range g.Nodes {
		out = append(out, id(n)+"@"+p(n.TopLeft)+"/"+f(n.Width)+"x"+f(n.Height))
	}
	return strings.Join(out, " ")
}

func main() {
	ctx := context.Background()
	w := os.Stdout
	for _, path := range os.Args[1:] {
		g := load(ctx, path)
		fmt.Fprintln(w, "== fixture", path)
		placed := true
		for _, n := range g.Nodes {
			if n.TopLeft == nil {
				placed = false
			}
		}
		guard, _ := limits.NewWorkGuard(ctx, "parity", limits.MaxEngineWorkUnits)
		if placed {
			tl, br := g.BoundingBox()
			fmt.Fprintln(w, "graph.BoundingBox", p(tl), p(br))
			fmt.Fprintln(w, "graph.Area", f(g.Area()))
			ntl, nbr := layoutgraph.Nodes(g.Nodes).BoundingBox()
			fmt.Fprintln(w, "nodes.BoundingBox", p(ntl), p(nbr))
			ftl, fbr := layoutgraph.Nodes(g.Nodes).FixedBoundingBox()
			fmt.Fprintln(w, "nodes.FixedBoundingBox", p(ftl), p(fbr))
			fmt.Fprintln(w, "nodes.Center", p(layoutgraph.Nodes(g.Nodes).Center()))
			btl, bbr, berr := graphbounds.FixedBoundingBox(g.Nodes, guard)
			fmt.Fprintln(w, "graphbounds.FixedBoundingBox", p(btl), p(bbr), errs(berr))
		}
		fmt.Fprintln(w, "ClusterOrder", ids(g.ClusterOrder()))
		fmt.Fprintln(w, "SequenceOrder", ids(g.SequenceOrder()))
		fmt.Fprintln(w, "TreeOrder", ids(g.TreeOrder()))
		fmt.Fprintln(w, "HubOrder", ids(g.HubOrder()))
		fmt.Fprintln(w, "ContainerRDFSOrder", ids(g.ContainerRDFSOrderUnbounded(nil)))
		fmt.Fprintln(w, "ClusterRDFSOrder", ids(g.ClusterRDFSOrder()))
		fmt.Fprintln(w, "AllDescendantNodes", ids(g.AllDescendantNodes(nil, true)), ids(g.AllDescendantNodes(nil, false)))
		fmt.Fprintln(w, "HasFixedNode", g.HasFixedNode())

		for _, n := range g.Nodes {
			pre := "node " + id(n)
			fmt.Fprintln(w, pre, "DebugID", n.DebugID(), "Level", n.Level(), "ContainerLevel", n.ContainerLevel(),
				"Owning", id(n.OwningContainer()), "IsContainer", n.IsContainer(), "Leaky", n.HasLeakyEdge(), "ShapeType", n.ShapeType())
			if n.TopLeft == nil {
				continue
			}
			btl, bbr := n.BoundingBox(g.Nodes)
			btl2, bbr2 := n.BoundingBox(nil)
			fmt.Fprintln(w, pre, "BoundingBox", p(btl), p(bbr), p(btl2), p(bbr2))
			gtl, gbr, gerr := graphbounds.NodeBoundingBox(n, g.Nodes, guard)
			fmt.Fprintln(w, pre, "graphbounds.NodeBoundingBox", p(gtl), p(gbr), errs(gerr))
			fmt.Fprintln(w, pre, "Center", p(n.Center()), "Area", f(n.Area()))
			ports := []string{}
			for _, pt := range n.Ports() {
				ports = append(ports, pv(pt))
			}
			fmt.Fprintln(w, pre, "Ports", strings.Join(ports, " "))
			cps := []string{}
			for _, pt := range n.CenterPorts() {
				cps = append(cps, pv(pt))
			}
			fmt.Fprintln(w, pre, "CenterPorts", strings.Join(cps, " "))
			for _, o := range []geo.Orientation{geo.Top, geo.Right, geo.Bottom, geo.Left} {
				ps := []string{}
				for _, pt := range n.PortsByOrientation(o) {
					ps = append(ps, pv(pt)+":"+fmt.Sprint(int(n.PortOrientation(&pt))))
				}
				fmt.Fprintln(w, pre, "PortsByOrientation", int(o), strings.Join(ps, " "))
			}
			ib := n.InnerBox()
			fmt.Fprintln(w, pre, "InnerBox", p(ib.TopLeft), f(ib.Width), f(ib.Height))
			for _, pos := range []label.Position{label.InsideMiddleCenter, label.OutsideTopCenter, label.InsideTopLeft, label.OutsideBottomRight} {
				fmt.Fprintln(w, pre, "IconSize", int(pos), f(n.IconSize(pos)), "LabelTopLeft", p(n.LabelTopLeft(pos, 30, 12)))
			}
			ip := n.InsidePlacement(40, 30, layoutgraph.UniformSpacing(10))
			fmt.Fprintln(w, pre, "InsidePlacement", pv(ip))
			if n.IsContainer() {
				sp := g.ContainerPadding(n, true)
				sp2 := g.ContainerPadding(n, false)
				fmt.Fprintln(w, pre, "ContainerPadding", f(sp.Top()), f(sp.Right()), f(sp.Bottom()), f(sp.Left()), f(sp2.Top()), f(sp2.Right()), f(sp2.Bottom()), f(sp2.Left()))
			}
			fmt.Fprintln(w, pre, "PointPastFixedOrigin", n.PointPastFixedOrigin(n.TopLeft.X, n.TopLeft.Y, true), n.PointPastFixedOrigin(1, 1, false))
			mods := []string{}
			for _, mp := range []geo.Point{*n.Center(), {X: n.TopLeft.X - 25, Y: n.TopLeft.Y - 25}} {
				mods = append(mods, fmt.Sprint(n.ContainsPoint(&mp, 5), n.IsPointNear(&mp), n.ContainsPointOnBox(&mp)))
			}
			fmt.Fprintln(w, pre, "Points", strings.Join(mods, " "))
		}
		// pairwise checks on the first nodes
		lim := len(g.Nodes)
		if lim > 12 {
			lim = 12
		}
		for i := 0; i < lim; i++ {
			for j := 0; j < lim; j++ {
				a, b := g.Nodes[i], g.Nodes[j]
				if i == j || a.TopLeft == nil || b.TopLeft == nil {
					continue
				}
				ox, oy := a.OrthogonalDistanceTo(b)
				fmt.Fprintln(w, "pair", id(a), id(b), "DeltaTo", a.DeltaTo(b, a.TopLeft), "Overlap", a.DoesOverlap(b), a.DoesOverlapExact(b),
					"Dist", f(a.DistanceTo(b, true)), f(a.DistanceTo(b, false)), "Orient", int(a.Orientation(b)),
					"Adj", a.IsAdjacentTo(b, true), a.IsAdjacentTo(b, false), "Covers", a.Covers(b), "Surrounds", a.Surrounds(b, 0),
					"Ortho", f(ox), f(oy), "Desc", a.IsDescendantOf(b), "NSA", id(a.NearestSharedAncestor(b)),
					"Along", a.OverlapsAlongDimension(b, true, true), a.OverlapsAlongDimension(b, false, false),
					"Vis", a.VisibilityGraphCandidate(true, true, true, b, 10), a.VisibilityGraphCandidate(false, false, true, b, 10),
					"Spill", a.SpillsOutOf(b), "Conn", a.ConnectionTo(b) != nil)
			}
		}
		segments := []layoutgraph.CrossingSegment{}
		for _, e := range g.Edges {
			pre := "edge " + fmt.Sprint(e.ID)
			etl, ebr := e.BoundingBoxValues()
			fmt.Fprintln(w, pre, "DebugID", e.DebugID(), "BBox", pv(etl), pv(ebr), "Length", f(e.Length()), "Segs", e.NumSegments(),
				"Directed", e.IsDirected(), e.IsBidirectional(), e.IsUndirected(), "Loop", e.IsLoop(), "Tree", e.IsTreeEdge())
			if e.From.TopLeft != nil && e.To.TopLeft != nil {
				fmt.Fprintln(w, pre, "AxisAligned", e.IsAxisAligned(), "Euclid", f(e.EuclideanDistance()))
				fp, tp, hf, ht, o := e.FacingTablePortValues(nil, nil)
				fmt.Fprintln(w, pre, "Facing", pv(fp), pv(tp), hf, ht, int(o))
			}
			if len(e.Points) >= 2 {
				fmt.Fprintln(w, pre, "Straight", e.IsStraight(), "OverlappingEnd", e.HasOverlappingEnd(), "Intersects", layoutgraph.Nodes(g.Nodes).IntersectsNode(e))
				for i := 0; i+1 < len(e.Points); i++ {
					segments = append(segments, layoutgraph.CrossingSegment{Start: *e.Points[i], End: *e.Points[i+1]})
				}
				if e.Label != nil {
					fmt.Fprintln(w, pre, "LabelTopLeft", p(e.LabelTopLeft(e.Label.Position, e.Label.Width, e.Label.Height)))
				}
			}
		}
		fmt.Fprintln(w, "CountSegmentCrossings", layoutgraph.CountSegmentCrossings(segments))

		// costs on a clone (they memoize)
		c, err := layoutgraph.Clone(ctx, g)
		if err != nil {
			panic(err)
		}
		fmt.Fprintln(w, "costs", f(c.CrossingCost()), f(c.TurnCost()), f(c.NonCenterPortCost()))
		c.ComputeCellSize()
		fmt.Fprintln(w, "CellSize", f(c.CellSize))

		for _, opt := range []layoutgraph.SplitOptions{{}, {IncludeContainers: true}, {IncludeContainers: true, IncludeNears: true, TraverseTrees: true}} {
			c, _ := layoutgraph.Clone(ctx, g)
			subs, err := c.SplitSubgraphs(ctx, opt)
			fmt.Fprintln(w, "split", opt.IncludeContainers, opt.IncludeNears, opt.TraverseTrees, errs(err), len(subs))
			for _, s := range subs {
				eids := []string{}
				for _, e := range s.Edges {
					eids = append(eids, fmt.Sprint(e.ID))
				}
				owned := 0
				for _, n := range s.Nodes {
					if n.Graph == s {
						owned++
					}
				}
				fmt.Fprintln(w, "  sub", ids(s.Nodes), "edges", strings.Join(eids, " "), "owned", owned)
			}
		}

		if len(g.Nodes) > 0 && g.Nodes[0].TopLeft != nil {
			fmt.Fprintln(w, "ConnectedNodes", sortedIDs(g.Nodes[0].ConnectedNodes(nil, g)), sortedIDs(g.Nodes[0].ConnectedNodeSet(nil, g)))
		}

		// geometry transactions: move each of the first nodes, commit, rollback
		for _, opts := range []layoutgraph.TransactionOptions{{}, {AffectContainers: true}} {
			c, _ := layoutgraph.Clone(ctx, g)
			if len(c.Nodes) == 0 {
				break
			}
			txn, err := c.NewRequestTransaction(ctx, opts)
			if err != nil {
				fmt.Fprintln(w, "txn new", errs(err))
				continue
			}
			before := positions(c)
			lim := len(c.Nodes)
			if lim > 6 {
				lim = 6
			}
			for i := 0; i < lim; i++ {
				n := c.Nodes[i]
				if n.TopLeft == nil {
					continue
				}
				for _, d := range [][2]float64{{7, 3}, {-200, 0}, {0, 61}} {
					d := d
					txn.Clear()
					txn.AddOp(func() error { n.MoveWithChildren(d[0], d[1]); return nil })
					err := txn.Commit(ctx)
					fmt.Fprintln(w, "txn", opts.AffectContainers, id(n), f(d[0]), f(d[1]), errs(err), positions(c))
					if err == nil {
						txn.Rollback()
						fmt.Fprintln(w, "  rollback restored", positions(c) == before)
					}
				}
			}
		}

		// topology snapshot round trip
		{
			c, _ := layoutgraph.Clone(ctx, g)
			orig, _ := graphjson.Marshal(ctx, c)
			state := layoutgraph.NewGraphStateSnapshot(layoutgraph.GraphStateSnapshotOptions{CaptureTopology: true, CaptureEdgeRoutes: true})
			guard, _ := limits.NewWorkGuard(ctx, "parity", limits.MaxTransactionWorkUnits)
			err := state.UpdateWithWorkGuard(c, guard)
			fmt.Fprintln(w, "snapshot", errs(err))
			if err == nil && len(c.Nodes) > 1 {
				if placed {
					c.Nodes[0].MoveWithChildren(13, 17)
				}
				if len(c.Edges) > 0 {
					c.Disconnect(c.Edges[0])
				}
				c.RemoveNode(c.Nodes[1])
				layoutgraph.RestoreGraphState(c, state)
				after, _ := graphjson.Marshal(ctx, c)
				fmt.Fprintln(w, "snapshot restored", string(orig) == string(after))
			}
		}
		// wrap containers in a clone (mutating geometry helpers)
		if placed {
			c, _ := layoutgraph.Clone(ctx, g)
			for _, n := range c.ContainerRDFSOrderUnbounded(nil) {
				if n.TopLeft == nil {
					continue
				}
				n.WrapChildren()
				fmt.Fprintln(w, "wrap", id(n), p(n.TopLeft), f(n.Width), f(n.Height))
			}
			c.SyncNestedGeometry()
			c.ComputeNodeSpacing()
			fmt.Fprintln(w, "synced", positions(c))
		}
	}
}
