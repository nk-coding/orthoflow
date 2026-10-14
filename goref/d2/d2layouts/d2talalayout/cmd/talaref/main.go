// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Copyright 2026 Niklas Krieger
// Go reference oracle for the orthoflow parity tests. Not part of
// terrastruct/d2: scripts/gen-goref.sh copies it into a d2 checkout
// (it uses TALA's internal packages, Copyright 2022 Terrastruct Inc.).

// Command talaref is the Go reference oracle for the orthoflow
// port. It operates on TALA graphjson fixtures.
//
//	talaref stages -seed S in.json outdir   per-stage graph snapshots
//	talaref layout -seeds 1,2,3 in.json     multi-seed layout + scores (JSON)
//	talaref replay -seed S -stage N snap.json  run one stage on a snapshot
//	talaref score graph.json                quality penalty + area
//	talaref bench -seeds 1,2,3 -iters N f…  timing (ms per full layout)
package main

import (
	"context"
	"encoding/json"
	"flag"
	"fmt"
	"math"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"time"

	"github.com/d2lang/d2/d2layouts/d2talalayout/internal/engine"
	"github.com/d2lang/d2/d2layouts/d2talalayout/internal/graphjson"
	"github.com/d2lang/d2/d2layouts/d2talalayout/internal/layoutgraph"
	"github.com/d2lang/d2/d2layouts/d2talalayout/internal/quality"
)

func readGraph(ctx context.Context, path string) *layoutgraph.Graph {
	data, err := os.ReadFile(path)
	check(err)
	var g layoutgraph.Graph
	check(graphjson.Unmarshal(ctx, data, &g))
	return &g
}

func check(err error) {
	if err != nil {
		fmt.Fprintln(os.Stderr, "error:", err)
		os.Exit(1)
	}
}

func parseSeeds(s string) []int64 {
	var out []int64
	for _, f := range strings.Split(s, ",") {
		v, err := strconv.ParseInt(strings.TrimSpace(f), 10, 64)
		check(err)
		out = append(out, v)
	}
	return out
}

type seedReport struct {
	Seed    int64    `json:"seed"`
	Penalty *float64 `json:"penalty,omitempty"`
	Area    *float64 `json:"area,omitempty"`
	Error   string   `json:"error,omitempty"`
}

type layoutReport struct {
	Seeds    []seedReport    `json:"seeds"`
	BestSeed *int64          `json:"bestSeed"`
	Graph    json.RawMessage `json:"graph"`
}

// layoutBest mirrors d2talalayout's seed selection: lowest exact penalty,
// then area; exact ties favour the later seed.
func layoutBest(ctx context.Context, input *layoutgraph.Graph, seeds []int64) layoutReport {
	var rep layoutReport
	var best *layoutgraph.Graph
	var bestP, bestA float64
	for _, seed := range seeds {
		r := seedReport{Seed: seed}
		g, err := engine.Layout(ctx, input, engine.LayoutOptions{Seed: seed})
		if err == nil {
			var p, a float64
			p, a, err = quality.EvaluateWithArea(ctx, g)
			if err == nil {
				r.Penalty, r.Area = &p, &a
				if best == nil || p < bestP || (p == bestP && (a <= bestA || math.IsNaN(bestA))) {
					best, bestP, bestA = g, p, a
					s := seed
					rep.BestSeed = &s
				}
			}
		}
		if err != nil {
			r.Error = err.Error()
		}
		rep.Seeds = append(rep.Seeds, r)
	}
	if best != nil {
		data, err := graphjson.Marshal(ctx, best)
		check(err)
		rep.Graph = data
	} else {
		rep.Graph = json.RawMessage("null")
	}
	return rep
}

func main() {
	ctx := context.Background()
	if len(os.Args) < 2 {
		fmt.Fprintln(os.Stderr, "usage: talaref stages|layout|bench ...")
		os.Exit(2)
	}
	fs := flag.NewFlagSet(os.Args[1], flag.ExitOnError)
	seed := fs.Int64("seed", 1, "seed")
	seeds := fs.String("seeds", "1,2,3", "seeds")
	iters := fs.Int("iters", 5, "iterations")
	stage := fs.Int("stage", 0, "stage index (replay)")
	check(fs.Parse(os.Args[2:]))
	args := fs.Args()
	switch os.Args[1] {
	case "stages":
		g := readGraph(ctx, args[0])
		out := args[1]
		check(os.MkdirAll(out, 0o755))
		_, err := engine.DebugRunStages(ctx, g, *seed, func(i int, name string, g *layoutgraph.Graph) error {
			data, err := graphjson.Marshal(ctx, g)
			if err != nil {
				return fmt.Errorf("marshal after %s: %w", name, err)
			}
			return os.WriteFile(filepath.Join(out, fmt.Sprintf("%02d_%s.json", i, name)), data, 0o644)
		})
		check(err)
	case "roundtrip":
		// Unmarshal -> Clone -> Marshal: codec and clone parity reference.
		g := readGraph(ctx, args[0])
		c, err := layoutgraph.Clone(ctx, g)
		check(err)
		data, err := graphjson.Marshal(ctx, c)
		check(err)
		os.Stdout.Write(data)
		fmt.Println()
	case "replay":
		// replay -seed S -stage N snapshot.json: run only stage N on the
		// deserialized snapshot and print the resulting graph.
		g := readGraph(ctx, args[0])
		check(engine.DebugReplayStage(ctx, g, *seed, *stage))
		data, err := graphjson.Marshal(ctx, g)
		check(err)
		os.Stdout.Write(data)
		fmt.Println()
	case "score":
		// score graph.json: quality.EvaluateWithArea of a laid-out graph.
		g := readGraph(ctx, args[0])
		p, a, err := quality.EvaluateWithArea(ctx, g)
		check(err)
		data, _ := json.Marshal(map[string]float64{"penalty": p, "area": a})
		fmt.Println(string(data))
	case "layout":
		g := readGraph(ctx, args[0])
		data, err := json.Marshal(layoutBest(ctx, g, parseSeeds(*seeds)))
		check(err)
		os.Stdout.Write(data)
		fmt.Println()
	case "bench":
		ss := parseSeeds(*seeds)
		for _, path := range args {
			g := readGraph(ctx, path)
			layoutBest(ctx, g, ss) // warm-up
			var times []float64
			for range *iters {
				start := time.Now()
				layoutBest(ctx, g, ss)
				times = append(times, float64(time.Since(start).Microseconds())/1000)
			}
			data, _ := json.Marshal(map[string]any{"file": filepath.Base(path), "ms": times})
			fmt.Println(string(data))
		}
	default:
		check(fmt.Errorf("unknown mode %q", os.Args[1]))
	}
}
