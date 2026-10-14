// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Copyright 2026 Niklas Krieger
// Go reference oracle for the orthoflow parity tests. Not part of
// terrastruct/d2: scripts/gen-goref.sh copies it into a d2 checkout
// (it uses TALA's internal packages, Copyright 2022 Terrastruct Inc.).

// Prints lib/shape behaviour for parity with the Rust port (crates/shape/tests/parity.rs).
package main

import (
	"bufio"
	"fmt"
	"math"
	"os"
	"strings"

	"github.com/d2lang/d2/lib/geo"
	"github.com/d2lang/d2/lib/shape"
)

func h(f float64) string { return fmt.Sprintf("%016x", math.Float64bits(f)) }
func pt(p *geo.Point) string {
	if p == nil {
		return "nil"
	}
	return h(p.X) + "," + h(p.Y)
}
func bx(b *geo.Box) string {
	if b == nil {
		return "nil"
	}
	return pt(b.TopLeft) + "," + h(b.Width) + "," + h(b.Height)
}

var Types = []string{"Square", "RealSquare", "Parallelogram", "Document", "Cylinder", "Queue", "Page", "Package", "Step", "Callout",
	"StoredData", "Person", "C4Person", "Diamond", "Oval", "Circle", "Hexagon", "Cloud", "Table", "Class", "Text", "Code", "Image", "", "weird"}

var Boxes = [][4]float64{{0, 0, 100, 100}, {10, 20, 200, 80}, {-50.5, 33.25, 40, 30}, {7, 9, 25, 300}, {100, 100, 150, 61}, {3.3, 4.4, 59, 47},
	{0, 0, 1, 1}, {12, 34, 834, 523}, {5, 5, 333.3, 111.1}, {-200, -100, 61.044, 20}}

func main() {
	w := bufio.NewWriter(os.Stdout)
	defer w.Flush()
	for _, t := range Types {
		for bi, b := range Boxes {
			box := geo.NewBox(geo.NewPoint(b[0], b[1]), b[2], b[3])
			s := shape.NewShape(t, box)
			fmt.Fprintf(w, "shape %q %d\n", t, bi)
			fmt.Fprintf(w, "meta %v %v %q %v %v\n", s.IsRectangular(), s.AspectRatio1(), s.GetType(), s.Is(""), s.Is(t))
			fmt.Fprintf(w, "box %s\n", bx(s.GetBox()))
			fmt.Fprintf(w, "inner %s\n", bx(s.GetInnerBox()))
			px, py := s.GetDefaultPadding()
			fmt.Fprintf(w, "pad %s %s\n", h(px), h(py))
			for _, a := range [][4]float64{{10, 10, 0, 0}, {50, 20, 40, 40}, {20, 50, 10, 30}, {0.5, 300, 3, 7}, {123.4, 56.7, 40, 20}} {
				p := s.GetInsidePlacement(a[0], a[1], a[2], a[3])
				dw, dh := s.GetDimensionsToFit(a[0], a[1], a[2], a[3])
				fmt.Fprintf(w, "place %s dims %s %s content %s\n", pt(&p), h(dw), h(dh), bx(s.GetInnerBoxForContent(a[0], a[1])))
			}
			per := s.Perimeter()
			fmt.Fprintf(w, "perimeter %d\n", len(per))
			cx, cy := b[0]+b[2]/2, b[1]+b[3]/2
			segs := []geo.Segment{
				{Start: geo.NewPoint(cx, cy), End: geo.NewPoint(cx, b[1]-50)},
				{Start: geo.NewPoint(cx, cy), End: geo.NewPoint(b[0]+b[2]+50, cy)},
				{Start: geo.NewPoint(b[0]-10, b[1]-10), End: geo.NewPoint(b[0]+b[2]+10, b[1]+b[3]+10)},
				{Start: geo.NewPoint(cx+3.7, cy+2.1), End: geo.NewPoint(b[0]-30, b[1]+b[3]*0.8)},
			}
			for i, ps := range per {
				for j, sg := range segs {
					pts := ps.Intersections(sg)
					strs := []string{}
					for _, p := range pts {
						strs = append(strs, pt(p))
					}
					fmt.Fprintf(w, "isect %d %d [%s]\n", i, j, strings.Join(strs, ";"))
				}
			}
			trace := [][4]float64{{cx, b[1], cx, b[1] - 40}, {b[0], cy, b[0] - 30, cy}, {b[0] + b[2], b[1] + b[3]*0.3, b[0] + b[2] + 20, b[1] + b[3]*0.3},
				{b[0] + b[2]*0.25, b[1] + b[3], b[0] + b[2]*0.25, b[1] + b[3] + 30}, {b[0] + b[2]*0.1, b[1], b[0] - 10, b[1] - 33}}
			for _, tr := range trace {
				rp := geo.NewPoint(tr[0], tr[1])
				pp := geo.NewPoint(tr[2], tr[3])
				r := shape.TraceToShapeBorder(s, rp, pp)
				fmt.Fprintf(w, "trace %s rect %s\n", pt(r), pt(rp))
			}
			fmt.Fprintf(w, "svg %q\n", strings.Join(s.GetSVGPathData(), "|"))
			cmds := shape.GetPathCommands(s)
			if cmds == nil {
				fmt.Fprintf(w, "cmds nil\n")
			} else {
				for _, path := range cmds {
					parts := []string{}
					for _, c := range path {
						parts = append(parts, fmt.Sprintf("%d:%s:%s:%s", c.Kind, pt(&c.Control1), pt(&c.Control2), pt(&c.End)))
					}
					fmt.Fprintf(w, "cmds %s\n", strings.Join(parts, " "))
				}
			}
			if t == "Cloud" {
				for _, ar := range []float64{0.5, 1, 1.25, 3} {
					s.SetInnerBoxAspectRatio(ar)
					fmt.Fprintf(w, "cloudar %s\n", bx(s.GetInnerBox()))
				}
			}
		}
	}
	for _, a := range [][3]float64{{10, 10, 1.5}, {100, 10, 3}, {10, 100, 1.5}, {33, 7, 2}} {
		x, y := shape.LimitAR(a[0], a[1], a[2])
		fmt.Fprintf(w, "limitar %s %s\n", h(x), h(y))
	}
}
