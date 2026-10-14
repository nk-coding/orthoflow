#!/usr/bin/env bash
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
#
# Copyright 2026 Niklas Krieger
#
# Generates the Go reference data for orthoflow's parity tests from a
# checkout of terrastruct/d2 (https://github.com/terrastruct/d2).
#
#   scripts/gen-goref.sh <d2-checkout> <out-dir>
#
# The Go oracle sources in goref/d2 are copied into the d2 checkout (they use
# TALA's internal packages, so they must live inside the d2 module). Output:
#
#   <out>/bin/                      Go oracles (talaref, hierlabref, graphparity, routebref, ...)
#   <out>/corpus/inputs/            81 fixtures copied from TALA's internal/testdata
#   <out>/corpus/expected/          talaref layout -seeds 1,2,3      (= tests/fixtures/corpus/expected)
#   <out>/corpus/stages/<f>/        talaref stages -seed 1           (graph after each of the 37 stages)
#   <out>/corpus/replay/<f>/        talaref replay -seed 1 -stage N  (stage N alone on Go's snapshot N-1)
#   <out>/random/{inputs,expected}/ 300 random graphs from TALA's fuzzer generator (= tests/fixtures/random)
#   <out>/base-parity/              gocompat/shape/labelgeom/nodeshape/hierarchy unit fixtures
#   <out>/graph-parity/             layoutgraph/graphbounds report (crates/graphjson/tests/parity.rs)
#   <out>/ovg-parity/               OVG + route-search dumps (crates/routing/src/ovg_parity_test.rs)
#
# Then run the Go-dependent tests with
#   ORTHOFLOW_GOREF=<out> cargo test --release -- --ignored
#
# Env: GO (go binary, default `go`), JOBS (parallel jobs, default nproc),
# STEPS (space-separated subset of: build corpus expected stages replay random
# base graph ovg; default all). The d2 commit orthoflow was verified against
# is 6a7b4837838a3907604a1d9eec57ee98b6f3cd01; other commits will differ.
set -euo pipefail

if [ $# -ne 2 ]; then
  sed -n '7,32p' "$0" | sed 's/^# \{0,1\}//'
  exit 2
fi
REPO="$(cd "$(dirname "$0")/.." && pwd)"
D2="$(cd "$1" && pwd)"
mkdir -p "$2"
OUT="$(cd "$2" && pwd)"
GO="${GO:-go}"
JOBS="${JOBS:-$(nproc 2>/dev/null || echo 4)}"
STEPS="${STEPS:-build corpus expected stages replay random base graph ovg}"
TALA="$D2/d2layouts/d2talalayout"
VERIFIED=6a7b4837838a3907604a1d9eec57ee98b6f3cd01

step() { [[ " $STEPS " == *" $1 "* ]]; }
log() { echo "== $*" >&2; }

if [ ! -d "$TALA/internal/engine" ]; then
  echo "$D2 does not look like a terrastruct/d2 checkout (missing d2layouts/d2talalayout)" >&2
  exit 1
fi
if commit=$(git -C "$D2" rev-parse HEAD 2>/dev/null) && [ "$commit" != "$VERIFIED" ]; then
  echo "warning: d2 checkout is at $commit, orthoflow was verified against $VERIFIED" >&2
fi

export OUT GO D2 TALA

if step build; then
  log "copying goref/d2 into $D2 and building the oracles"
  cp -R "$REPO/goref/d2/." "$D2/"
  mkdir -p "$OUT/bin"
  for cmd in talaref hierlabref graphparity routebref basegen shapesref labelsref nodeshaperef; do
    (cd "$D2" && "$GO" build -o "$OUT/bin/$cmd" "./d2layouts/d2talalayout/cmd/$cmd")
  done
  (cd "$D2" && "$GO" test -c -o "$OUT/bin/routing.test" ./d2layouts/d2talalayout/internal/routing)
fi

if step corpus; then
  log "corpus inputs from $TALA/internal/testdata"
  rm -rf "$OUT/corpus/inputs"; mkdir -p "$OUT/corpus/inputs"
  td="$TALA/internal/testdata"
  for kind in layout ovg; do
    for f in "$td/$kind"/*/graph.input.json; do
      cp "$f" "$OUT/corpus/inputs/${kind}_$(basename "$(dirname "$f")").json"
    done
  done
  for f in "$td/regression"/*.json; do cp "$f" "$OUT/corpus/inputs/regression_$(basename "$f")"; done
  for f in "$td/benchmark"/*.json; do cp "$f" "$OUT/corpus/inputs/bench_$(basename "$f")"; done
fi

layout_all() { # <inputs dir> <expected dir>
  mkdir -p "$2"
  ls "$1"/*.json | xargs -P "$JOBS" -I{} sh -c '"$0" layout -seeds 1,2,3 "$1" > "$2/$(basename "$1")"' "$OUT/bin/talaref" {} "$2"
}

if step expected; then
  log "corpus/expected (talaref layout -seeds 1,2,3)"
  rm -rf "$OUT/corpus/expected"; layout_all "$OUT/corpus/inputs" "$OUT/corpus/expected"
fi

if step stages; then
  log "corpus/stages (talaref stages -seed 1)"
  rm -rf "$OUT/corpus/stages"; mkdir -p "$OUT/corpus/stages"
  ls "$OUT/corpus/inputs"/*.json | xargs -P "$JOBS" -I{} sh -c \
    'n=$(basename "$1" .json); "$0" stages -seed 1 "$1" "$2/$n" > /dev/null' "$OUT/bin/talaref" {} "$OUT/corpus/stages"
fi

if step replay; then
  log "corpus/replay (talaref replay -seed 1 -stage N on Go's snapshot N-1)"
  rm -rf "$OUT/corpus/replay"; mkdir -p "$OUT/corpus/replay"
  replay_one() {
    n=$1; mkdir -p "$OUT/corpus/replay/$n"
    prev="$OUT/corpus/inputs/$n.json"; i=0
    for cur in "$OUT/corpus/stages/$n"/*.json; do
      s=$(basename "$cur")
      "$OUT/bin/talaref" replay -seed 1 -stage $i "$prev" > "$OUT/corpus/replay/$n/$s" 2>/dev/null || echo "replay failed $n $s" >&2
      prev="$cur"; i=$((i+1))
    done
  }
  export -f replay_one
  ls "$OUT/corpus/stages" | xargs -P "$JOBS" -I{} bash -c 'replay_one {}'
fi

if step random; then
  log "random graphs (TALA fuzzer generator) + talaref layout"
  rm -rf "$OUT/random"; mkdir -p "$OUT/random/inputs"
  (cd "$D2" && ORTHOFLOW_RANDGRAPH_OUT="$OUT/random/inputs" ORTHOFLOW_RANDGRAPH_N=300 \
    "$GO" test -count=1 -run TestPortDumpRandomGraphs ./d2layouts/d2talalayout/internal/engine > /dev/null)
  layout_all "$OUT/random/inputs" "$OUT/random/expected"
fi

if step base; then
  log "base-parity fixtures"
  mkdir -p "$OUT/base-parity"
  "$OUT/bin/basegen" "$OUT/base-parity"
  "$OUT/bin/shapesref" > "$OUT/base-parity/shapes.txt"
  "$OUT/bin/labelsref" > "$OUT/base-parity/labels.txt"
  "$OUT/bin/nodeshaperef" > "$OUT/base-parity/nodeshape.txt"
  tmp=$(mktemp -d)
  (cd "$D2" && ORTHOFLOW_HIER_PARITY_OUT="$tmp" "$GO" test -count=1 -run TestZZTSParityGen ./d2layouts/d2talalayout/internal/hierarchy > /dev/null)
  mv "$tmp/cases.json" "$OUT/base-parity/hierarchy-cases.json"; rmdir "$tmp"
fi

if step graph; then
  log "graph-parity (graphparity on inputs and stages 05/07/21/33/36)"
  mkdir -p "$OUT/graph-parity"
  (cd "$OUT" && { ls corpus/inputs/*.json; ls corpus/stages/*/{05,07,21,33,36}_*.json; } > graph-parity/files.txt)
  (cd "$OUT" && xargs "$OUT/bin/graphparity" < graph-parity/files.txt > graph-parity/expected.txt)
fi

if step ovg; then
  log "ovg-parity (OVG + route search dumps of stage 20_Rescale)"
  rm -rf "$OUT/ovg-parity"; mkdir -p "$OUT/ovg-parity"
  for d in "$OUT/corpus/stages"/*/; do
    n=$(basename "$d")
    [ -f "$d/20_Rescale.json" ] || continue
    ORTHOFLOW_OVG_DUMP_IN="$d/20_Rescale.json" ORTHOFLOW_OVG_DUMP_OUT="$OUT/ovg-parity/$n.json" \
      "$OUT/bin/routing.test" -test.run TestOVGPortParityDump > /dev/null
  done
fi

log "done. Differences against the fixtures shipped in tests/fixtures (expected: none):"
for pair in "corpus/inputs" "corpus/expected" "random/inputs" "random/expected"; do
  [ -d "$OUT/$pair" ] && { diff -rq "$REPO/tests/fixtures/$pair" "$OUT/$pair" || true; }
done
for f in shapes.txt labels.txt nodeshape.txt rand.txt hierarchy-cases.json; do
  [ -f "$OUT/base-parity/$f" ] && { cmp -s "$REPO/tests/fixtures/go-unit/$f" "$OUT/base-parity/$f" || echo "differs: go-unit/$f"; }
done
exit 0
