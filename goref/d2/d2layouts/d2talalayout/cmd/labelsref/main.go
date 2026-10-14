// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Copyright 2026 Niklas Krieger
// Go reference oracle for the orthoflow parity tests. Not part of
// terrastruct/d2: scripts/gen-goref.sh copies it into a d2 checkout
// (it uses TALA's internal packages, Copyright 2022 Terrastruct Inc.).

// Prints lib/label + internal/labelgeom + geo behaviour for parity (crates/labelgeom/tests/parity.rs).
package main

import (
	"bufio"
	"fmt"
	"math"
	"math/rand"
	"os"
	"strings"

	"github.com/d2lang/d2/d2target"
	"github.com/d2lang/d2/lib/geo"
	"github.com/d2lang/d2/lib/label"
)

func h(f float64) string { return fmt.Sprintf("%016x", math.Float64bits(f)) }
func pt(p *geo.Point) string {
	if p == nil {
		return "nil"
	}
	return h(p.X) + "," + h(p.Y)
}
func route(r geo.Route) string {
	s := []string{}
	for _, p := range r {
		s = append(s, pt(p))
	}
	return strings.Join(s, ";")
}

var arrows = []string{"none", "arrow", "unfilled-triangle", "triangle", "diamond", "filled-diamond", "circle", "filled-circle", "cross", "box", "filled-box", "line", "cf-one", "cf-many", "cf-one-required", "cf-many-required", "bogus"}

func randRoute(r *rand.Rand) geo.Route {
	n := 2 + r.Intn(5)
	rt := geo.Route{}
	x, y := float64(r.Intn(400)-200), float64(r.Intn(400)-200)
	if r.Intn(3) == 0 {
		x += r.Float64()
		y += r.Float64() * 3
	}
	rt = append(rt, geo.NewPoint(x, y))
	for i := 1; i < n; i++ {
		switch r.Intn(3) {
		case 0:
			x += float64(r.Intn(300) - 150)
		case 1:
			y += float64(r.Intn(300) - 150)
		default:
			x += r.Float64()*200 - 100
			y += r.Float64()*200 - 100
		}
		rt = append(rt, geo.NewPoint(x, y))
	}
	return rt
}

func main() {
	w := bufio.NewWriter(os.Stdout)
	defer w.Flush()
	r := rand.New(rand.NewSource(5))
	for i := 0; i < 3000; i++ {
		rt := randRoute(r)
		isT := r.Intn(2) == 0
		sa, ta := arrows[r.Intn(len(arrows))], arrows[r.Intn(len(arrows))]
		wd, ht := r.Float64()*80, r.Float64()*30
		if i%5 == 0 {
			wd, ht = float64(r.Intn(60)), float64(r.Intn(20))
		}
		got := arrowheadTopLeft(rt, isT, sa, ta, wd, ht)
		fmt.Fprintf(w, "arrow %s %v %s %s %s %s => %s\n", route(rt), isT, sa, ta, h(wd), h(ht), pt(got))
	}
	for i := 0; i < 1500; i++ {
		rt := randRoute(r)
		pos := label.Position(r.Intn(41))
		sw, pct, wd, ht := float64(r.Intn(5)), r.Float64(), r.Float64()*80, r.Float64()*30
		if i%7 == 0 {
			pct = -0.3 + r.Float64()*1.6
		}
		p, idx := pos.GetPointOnRoute(rt, sw, pct, wd, ht)
		fmt.Fprintf(w, "onroute %s %d %s %s %s %s => %s %d\n", route(rt), pos, h(sw), h(pct), h(wd), h(ht), pt(p), idx)
		b := geo.NewBox(geo.NewPoint(r.Float64()*100, r.Float64()*100), r.Float64()*200, r.Float64()*200)
		pad := float64(r.Intn(10))
		q := pos.GetPointOnBox(b, pad, wd, ht)
		fmt.Fprintf(w, "onbox %s %s %s %s %d %s %s %s => %s\n", pt(b.TopLeft), h(b.Width), h(b.Height), h(pad), pos, h(wd), h(ht), pos.String(), pt(q))
		fmt.Fprintf(w, "posmeta %d %v %v %v %v %v %v %d %d\n", pos, pos.IsShapePosition(), pos.IsEdgePosition(), pos.IsOutside(), pos.IsUnlocked(), pos.IsBorder(), pos.IsOnEdge(), pos.Mirrored(), label.FromString(pos.String()))
	}
	// geo bits: route length/bbox, IntersectionPoint, DistanceToLine, bezier At / intersections
	for i := 0; i < 2000; i++ {
		rt := randRoute(r)
		tl, br := rt.GetBoundingBox()
		d := r.Float64() * rt.Length() * 1.2
		p, idx := rt.GetPointAtDistance(d)
		fmt.Fprintf(w, "route %s %s => %s %s %s %s %d %s\n", route(rt), h(d), h(rt.Length()), pt(tl), pt(br), pt(p), idx, pt(geo.Points(rt).GetMedian()))
		a, b, c, e := rt[0], rt[1], geo.NewPoint(r.Float64()*300-150, r.Float64()*300-150), geo.NewPoint(r.Float64()*300-150, r.Float64()*300-150)
		ip := geo.IntersectionPoint(a, b, c, e)
		fmt.Fprintf(w, "geo %s %s %s %s => %s %s %d %d %s %s\n", pt(a), pt(b), pt(c), pt(e), pt(ip), h(c.DistanceToLine(a, b)), a.GetOrientation(b), a.Compare(c), h(geo.TruncateDecimals(c.X)), h(geo.Vector{c.X, c.Y}.Radians()))
		bc := geo.NewBezierCurve([]*geo.Point{a, b, c, e})
		t := r.Float64()
		seg := geo.Segment{Start: geo.NewPoint(r.Float64()*300-150, r.Float64()*300-150), End: geo.NewPoint(r.Float64()*300-150, r.Float64()*300-150)}
		if i%4 == 0 {
			seg.End.X = seg.Start.X
		}
		is := []string{}
		for _, q := range bc.Intersections(seg) {
			is = append(is, pt(q))
		}
		fmt.Fprintf(w, "bezier %s %s %s => %s [%s]\n", route(geo.Route{a, b, c, e}), h(t), pt(seg.Start)+";"+pt(seg.End), pt(bc.At(t)), strings.Join(is, "|"))
	}
}

func arrowheadTopLeft(route geo.Route, isTarget bool, sourceArrowhead, targetArrowhead string, width, height float64) *geo.Point {
	connection := d2target.BaseConnection()
	connection.Route = route
	connection.SrcArrow = d2target.Arrowhead(sourceArrowhead)
	connection.DstArrow = d2target.Arrowhead(targetArrowhead)
	text := &d2target.Text{LabelWidth: int(width), LabelHeight: int(height)}
	if isTarget {
		connection.DstLabel = text
	} else {
		connection.SrcLabel = text
	}
	return connection.GetArrowheadLabelPosition(isTarget)
}
