// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Copyright 2026 Niklas Krieger
// Go reference oracle for the orthoflow parity tests. Not part of
// terrastruct/d2: scripts/gen-goref.sh copies it into a d2 checkout
// (it uses TALA's internal packages, Copyright 2022 Terrastruct Inc.).

package engine

// Port harness (not upstream): dumps random graphs from the fuzzer's own
// generator for end-to-end Go-vs-port differential testing. Runs only when
// ORTHOFLOW_RANDGRAPH_OUT is set:
//   ORTHOFLOW_RANDGRAPH_OUT=dir ORTHOFLOW_RANDGRAPH_N=300 go test -run TestPortDumpRandomGraphs ./internal/engine

import (
	"fmt"
	"math/rand"
	"os"
	"path/filepath"
	"strconv"
	"testing"

	"github.com/d2lang/d2/d2layouts/d2talalayout/internal/graphjson"
)

func TestPortDumpRandomGraphs(t *testing.T) {
	out := os.Getenv("ORTHOFLOW_RANDGRAPH_OUT")
	if out == "" {
		t.Skip("ORTHOFLOW_RANDGRAPH_OUT not set")
	}
	n, _ := strconv.Atoi(os.Getenv("ORTHOFLOW_RANDGRAPH_N"))
	if n <= 0 {
		n = 100
	}
	if err := os.MkdirAll(out, 0o755); err != nil {
		t.Fatal(err)
	}
	// nolint: gosec
	master := rand.New(rand.NewSource(20261014))
	for i := 0; i < n; i++ {
		config := normalizeFuzzerConfig(master.Int63(), uint8(master.Intn(256)), uint8(master.Intn(256)),
			uint8(master.Intn(256)), uint8(master.Intn(256)), uint8(master.Intn(60)), uint8(40+master.Intn(60)))
		g := createRandomGraph(t.Context(), config)
		data, err := graphjson.Marshal(t.Context(), g)
		if err != nil {
			t.Logf("graph %d: marshal: %v", i, err)
			continue
		}
		name := fmt.Sprintf("rand_%03d_n%d.json", i, len(g.Nodes))
		if err := os.WriteFile(filepath.Join(out, name), data, 0o644); err != nil {
			t.Fatal(err)
		}
	}
}
