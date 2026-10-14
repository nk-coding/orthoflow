// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Copyright 2026 Niklas Krieger
// Go reference oracle for the orthoflow parity tests. Not part of
// terrastruct/d2: scripts/gen-goref.sh copies it into a d2 checkout
// (it uses TALA's internal packages, Copyright 2022 Terrastruct Inc.).

// Command nodeshaperef prints internal/nodeshape behaviour for parity (crates/nodeshape/tests/parity.rs).
package main

import (
	"bufio"
	"fmt"
	"math"
	"os"
	"sort"
	"strings"

	"github.com/d2lang/d2/d2layouts/d2talalayout/internal/nodeshape"
	"github.com/d2lang/d2/lib/geo"
	"github.com/d2lang/d2/lib/label"
)

func h(f float64) string { return fmt.Sprintf("%016x", math.Float64bits(f)) }

var Types = []string{"Square", "RealSquare", "Parallelogram", "Document", "Cylinder", "Queue", "Page", "Package", "Step", "Callout",
	"StoredData", "Person", "C4Person", "Diamond", "Oval", "Circle", "Hexagon", "Cloud", "Table", "Class", "Text", "Code", "Image", "", "weird"}
var Boxes = [][4]float64{{0, 0, 100, 100}, {10, 20, 200, 80}, {-50.5, 33.25, 40, 30}, {7, 9, 25, 300}, {100, 100, 150, 61}, {3.3, 4.4, 59, 47},
	{0, 0, 1, 1}, {12, 34, 834, 523}, {5, 5, 333.3, 111.1}, {-200, -100, 61.044, 20}, {1, 2, 13, 17}, {0, 0, 99, 67}}
var Orients = []geo.Orientation{geo.TopLeft, geo.TopRight, geo.BottomLeft, geo.BottomRight, geo.Top, geo.Right, geo.Bottom, geo.Left, geo.NONE}

func ints(v []int) string {
	s := []string{}
	for _, x := range v {
		s = append(s, fmt.Sprint(x))
	}
	return "[" + strings.Join(s, ",") + "]"
}

func main() {
	w := bufio.NewWriter(os.Stdout)
	defer w.Flush()
	for _, t := range Types {
		for bi, b := range Boxes {
			for cols := 0; cols < 6; cols++ {
				if t != "Table" && cols > 0 {
					break
				}
				box := geo.Box{TopLeft: geo.NewPoint(b[0], b[1]), Width: b[2], Height: b[3]}
				s, kind, ok := nodeshape.New(t, &box)
				fmt.Fprintf(w, "new %q %d %d %d %v\n", t, bi, cols, kind, ok)
				if !ok {
					continue
				}
				nodeshape.SetNumColumns(s, cols)
				fmt.Fprintf(w, "kind %s numcols %d\n", kind.String(), nodeshape.NumColumns(s))
				for gi, g := range s.SnapPointPercentages() {
					parts := []string{}
					for _, p := range g {
						parts = append(parts, h(p.XPercentage)+","+h(p.YPercentage))
					}
					fmt.Fprintf(w, "snap %d %s\n", gi, strings.Join(parts, ";"))
				}
				for _, o := range Orients {
					fmt.Fprintf(w, "ports %d %s %d\n", o, ints(s.PortIndices(o)), s.CenterPortIndex(o))
				}
				m := s.MirroredPortIndices()
				keys := []int{}
				for k := range m {
					keys = append(keys, k)
				}
				sort.Ints(keys)
				mp := []string{}
				for _, k := range keys {
					mp = append(mp, fmt.Sprintf("%d:%d", k, m[k]))
				}
				fmt.Fprintf(w, "mirror %s center %s\n", strings.Join(mp, ","), ints(s.CenterPortIndices()))
				for tier := nodeshape.Good; tier <= nodeshape.Bad+1; tier++ {
					ps := []int{}
					for p := range s.LabelPositionPreferences(tier) {
						ps = append(ps, int(p))
					}
					sort.Ints(ps)
					fmt.Fprintf(w, "lpp %d %s\n", tier, ints(ps))
				}
				if t == "Table" {
					for _, o := range Orients {
						for ci := 0; ci < max(cols, 1); ci++ {
							idx, ok := nodeshape.TablePortIndex(s, o, ci)
							pv, ok2 := nodeshape.TableColumnPortValue(s, o, ci)
							fmt.Fprintf(w, "tableport %d %d %d %v %s,%s %v\n", o, ci, idx, ok, h(pv.X), h(pv.Y), ok2)
						}
					}
				}
			}
		}
	}
	_ = label.Unset
}
