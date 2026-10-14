// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Copyright 2026 Niklas Krieger
// Go reference oracle for the orthoflow parity tests. Not part of
// terrastruct/d2: scripts/gen-goref.sh copies it into a d2 checkout
// (it uses TALA's internal packages, Copyright 2022 Terrastruct Inc.).

package routing

// Port-parity dump for crates/routing/src/ovg_parity_test.rs. Runs only when
// ORTHOFLOW_OVG_DUMP_IN is set:
//   ORTHOFLOW_OVG_DUMP_IN=graph.json ORTHOFLOW_OVG_DUMP_OUT=out.json go test -run TestOVGPortParityDump ./d2layouts/d2talalayout/internal/routing/

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"sort"
	"testing"

	"github.com/d2lang/d2/d2layouts/d2talalayout/internal/graphjson"
	"github.com/d2lang/d2/d2layouts/d2talalayout/internal/layoutgraph"
)

type parityOVGNode struct {
	X         float64     `json:"x"`
	Y         float64     `json:"y"`
	Center    bool        `json:"center"`
	Tunnel    bool        `json:"tunnel"`
	Container string      `json:"container"`
	Near      []string    `json:"near"`
	Owners    [][3]string `json:"owners"`
	Adj       [][3]float64 `json:"adj"`
}

type parityRoute struct {
	Edge     string       `json:"edge"`
	Err      string       `json:"err"`
	Distance float64      `json:"distance"`
	Points   [][2]float64 `json:"points"`
	Work     uint64       `json:"work"`
}

type parityDump struct {
	Err        string          `json:"err"`
	Nodes      []parityOVGNode `json:"nodes"`
	Ports      [][]any         `json:"ports"`
	Centers    [][]any         `json:"centers"`
	EdgeCount  int             `json:"edgeCount"`
	GuardWork  uint64          `json:"guardWork"`
	GuardNodes uint64          `json:"guardNodes"`
	GuardEdges uint64          `json:"guardEdges"`
	GuardCand  uint64          `json:"guardCandidates"`
	Routes     []parityRoute   `json:"routes"`
}

func parityID(n *layoutgraph.Node) string {
	if n == nil {
		return ""
	}
	return fmt.Sprintf("%d", n.ID)
}

func TestOVGPortParityDump(t *testing.T) {
	in := os.Getenv("ORTHOFLOW_OVG_DUMP_IN")
	if in == "" {
		t.Skip("ORTHOFLOW_OVG_DUMP_IN not set")
	}
	ctx := context.Background()
	data, err := os.ReadFile(in)
	if err != nil {
		t.Fatal(err)
	}
	var g layoutgraph.Graph
	if err := graphjson.Unmarshal(ctx, data, &g); err != nil {
		t.Fatal(err)
	}
	dump := parityDump{}
	ovg, err := buildOVGFromGraphWithLimits(ctx, &g, nil, defaultOVGBuildLimits())
	if err != nil {
		dump.Err = err.Error()
	} else {
		parityFillOVG(&dump, ovg)
		dump.Routes = parityRoutes(ctx, ovg, &g)
	}
	out, err := json.Marshal(dump)
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(os.Getenv("ORTHOFLOW_OVG_DUMP_OUT"), out, 0o644); err != nil {
		t.Fatal(err)
	}
}

func parityFillOVG(dump *parityDump, ovg *OVG) {
	for _, n := range ovg.Nodes {
		pn := parityOVGNode{X: n.X, Y: n.Y, Center: n.IsNodeCenter, Tunnel: n.IsTunnel, Container: parityID(n.Container)}
		for owner := range n.IsNearPort {
			pn.Near = append(pn.Near, parityID(owner))
		}
		sort.Strings(pn.Near)
		for owner, md := range n.portOwners() {
			pn.Owners = append(pn.Owners, [3]string{parityID(owner), fmt.Sprintf("%d", md.directions), fmt.Sprintf("%t", md.isCenterPort)})
		}
		sort.Slice(pn.Owners, func(i, j int) bool { return pn.Owners[i][0] < pn.Owners[j][0] })
		for _, e := range n.Edges {
			adj := n.adjacent(e)
			pn.Adj = append(pn.Adj, [3]float64{adj.X, adj.Y, e.Distance})
		}
		dump.Nodes = append(dump.Nodes, pn)
	}
	sort.SliceStable(dump.Nodes, func(i, j int) bool {
		a, b := dump.Nodes[i], dump.Nodes[j]
		if a.X != b.X {
			return a.X < b.X
		}
		if a.Y != b.Y {
			return a.Y < b.Y
		}
		if a.Center != b.Center {
			return !a.Center
		}
		return len(a.Adj) < len(b.Adj)
	})
	for n, ports := range ovg.Ports {
		var ps [][2]float64
		for _, p := range ports {
			ps = append(ps, [2]float64{p.X, p.Y})
		}
		dump.Ports = append(dump.Ports, []any{parityID(n), ps})
	}
	sort.Slice(dump.Ports, func(i, j int) bool { return dump.Ports[i][0].(string) < dump.Ports[j][0].(string) })
	for n, c := range ovg.Centers {
		dump.Centers = append(dump.Centers, []any{parityID(n), c.X, c.Y})
	}
	sort.Slice(dump.Centers, func(i, j int) bool { return dump.Centers[i][0].(string) < dump.Centers[j][0].(string) })
	dump.EdgeCount = len(ovg.Edges)
	dump.GuardWork = ovg.buildGuard.work
	dump.GuardNodes = ovg.buildGuard.nodes
	dump.GuardEdges = ovg.buildGuard.edges
	dump.GuardCand = ovg.buildGuard.candidates
}

// parityRoutes routes every non-loop edge with plain OVG search (no
// slingshot), in graph order, adding each route before the next search.
func parityRoutes(ctx context.Context, ovg *OVG, g *layoutgraph.Graph) []parityRoute {
	var out []parityRoute
	router, err := newOVGEdgeRouterWithWorkLimit(ctx, Default, ovg, g, nil, g.Edges, maxRouteSearchWorkUnits)
	if err != nil {
		return []parityRoute{{Err: err.Error()}}
	}
	for _, e := range g.Edges {
		if e.From == e.To {
			continue
		}
		pr := parityRoute{Edge: e.DebugID()}
		nodes, dist, err := router.search(ctx, e)
		pr.Work = router.work.used
		if err != nil {
			pr.Err = err.Error()
			out = append(out, pr)
			continue
		}
		pr.Distance = dist
		for _, n := range nodes {
			pr.Points = append(pr.Points, [2]float64{n.X, n.Y})
		}
		out = append(out, pr)
		if len(nodes) < 2 {
			continue
		}
		if err := router.addRoute(&Route{GEdge: e, OVGNodes: nodes, FromPort: *nodes[1].Point, ToPort: *nodes[len(nodes)-2].Point}); err != nil {
			out = append(out, parityRoute{Edge: e.DebugID(), Err: "addRoute: " + err.Error()})
			break
		}
	}
	return out
}
