# orthoflow

An orthogonal graph layout engine for diagrams: nodes (with shapes, labels and
nested containers) are placed and edges are routed with right-angle polylines.

orthoflow is an **unofficial, independent port of the TALA layout engine from
Terrastruct's D2** (<https://github.com/terrastruct/d2>) to Rust, with a
WebAssembly build (npm package [`orthoflow`](https://github.com/nk-coding/orthoflow-js)).
It is **not affiliated with or endorsed by Terrastruct**. "TALA" and "D2" are
names of Terrastruct Inc. and are used here only to describe where the code
comes from.

The port is based on terrastruct/d2 at commit
[`6a7b4837838a3907604a1d9eec57ee98b6f3cd01`](https://github.com/terrastruct/d2/tree/6a7b4837838a3907604a1d9eec57ee98b6f3cd01)
(2026-10-11): `d2layouts/d2talalayout` plus the parts of `lib/geo`,
`lib/label`, `lib/shape`, `lib/svg` and `d2target` that it uses.

## Exactness

The goal of the port is bit-identical results: for the same input graph and
seeds, orthoflow produces the same layout as the Go engine, with every
coordinate equal as an IEEE-754 double. Go standard library behaviour that
affects results is reproduced exactly as well (`math` as executed on
linux/amd64, including the assembly implementations of `Exp`, `Log` and
`Hypot`, the `sort`/`slices` pdqsort variants, and `math/rand`).

How this was verified (native and WebAssembly builds):

| check | cases |
|---|---|
| graph after each of the 37 pipeline stages, against Go | 81 fixtures × 37 stages |
| each stage replayed alone on Go's snapshot of the previous stage | 81 fixtures × 37 stages |
| full 3-seed layout (scores, selected seed, final graph) | 81 fixtures + 300 random graphs from TALA's fuzzer generator |
| routing post-processing on randomly perturbed routes | ~37,000 cases |
| unit-level parity (math/sort/rand, shapes, labels, OVG construction, ranking, ...) | millions of values |

The full-layout checks run as part of `cargo test` (fixtures and Go's expected
output are in `tests/fixtures`). The stage-level checks need Go reference data
that is generated from a d2 checkout (see [Tests](#tests)).

## Scope and known deviations

`orthoflow::layout` runs what the Go reference oracle `talaref layout` runs:
one full engine layout per seed (`engine.Layout`), scored with
`quality.EvaluateWithArea`; the best seed is the one with the lowest penalty,
then the lowest area, and exact ties go to the later seed. The default seeds
are 1, 2, 3, as in D2.

Not included: D2's adapter between D2 graphs and the engine graph
(`d2layouts/d2talalayout/adapter.go`, so you produce the graphjson input
yourself), and the optional post-selection refinement that D2's
`d2talalayout.Layout` applies (`engine.CompoundCandidate` /
`engine.PreserveCompoundRoutes`; they are ported in the `engine` crate but not
called by `layout`). D2 also runs seeds concurrently; orthoflow runs them one
after another (with the same result).

Known differences (details in the source comments):

- **Go map iteration order.** In a few places the Go engine iterates over Go
  maps, whose order is random: the enqueue order in `ConnectedNodes` for a
  node that is both a container child and a container, the concatenation
  order of cluster/sequence edge abductions (matters only with several equal
  matches), the visit order of tree sentinels when decoding graphjson, a few
  tie cases in the placement refinement (`alignAxes`, `BalanceSymmetry`), and
  the grouping of isolated tree roots in `trees.buildPlacementTrees`. orthoflow
  uses a fixed order. Where Go's output depends on these, Go itself is
  nondeterministic, so exact parity is impossible by construction. No corpus
  fixture showed run-to-run variation in Go.
- **NaN payloads.** NaN sign/payload bits are not reproduced in general (they
  do not occur in valid layouts).
- **Error messages** are close to Go's but not guaranteed identical; graphjson
  decoding is case-sensitive (Go's decoder is not).
- **Capacity-based limits.** Where Go charges slice capacity against a work or
  size budget, orthoflow uses the length. This only matters near the hard
  limits (about 1M references or route points).
- **Number formatting.** The graph JSON in the output is written by
  `serde_json`, which prints integral floats as `354.0` where Go prints `354`.
  The values are identical.
- Concurrency, `sync.Pool` reuse, developer snapshots and timing
  instrumentation of the Go engine are not ported (no observable effect).

## Input and output format

The input is TALA's *graphjson* (`internal/graphjson/representation.go`).
A small example (`examples/small.json`): a container `1` holding `2` and `3`,
next to node `4`:

```json
{
  "nodes": [
    { "id": 1, "width": 260, "height": 200, "topLeft": null, "shapeType": "Square",
      "label": { "Text": "backend", "Position": "", "Width": 70, "Height": 21 } },
    { "id": 2, "width": 108, "height": 66, "topLeft": null, "shapeType": "Square",
      "label": { "Text": "api", "Position": "", "Width": 30, "Height": 21 } },
    { "id": 3, "width": 108, "height": 90, "topLeft": null, "shapeType": "Cylinder",
      "label": { "Text": "db", "Position": "", "Width": 25, "Height": 21 } },
    { "id": 4, "width": 120, "height": 66, "topLeft": null, "shapeType": "Oval",
      "label": { "Text": "client", "Position": "", "Width": 50, "Height": 21 } }
  ],
  "edges": [
    { "id": 10, "from": 4, "to": 2, "points": [], "sourceArrowhead": "", "targetArrowhead": "triangle",
      "label": { "Text": "HTTPS", "Position": "", "Width": 45, "Height": 21 } },
    { "id": 11, "from": 2, "to": 3, "points": [], "sourceArrowhead": "", "targetArrowhead": "triangle" }
  ],
  "containers": { "0": [1, 4], "1": [2, 3] },
  "CellSize": 0, "crossingCost": 0, "turnCost": 0, "nonCenterPortCost": 0
}
```

Graph fields:

| field | meaning |
|---|---|
| `nodes`, `edges` | the graph |
| `containers` | map from container node ID to its child node IDs; key `"0"` is the root |
| `CellSize`, `crossingCost`, `turnCost`, `nonCenterPortCost` | engine state; use `0` in inputs (computed by the layout) |
| `clusters`, `sequences`, `clusterVessels`, `sequenceVessels`, `trees`, `hubs`, `hierarchies`, `directions`, `isRootHierarchy` | engine state written by the pipeline; omit in inputs |

Node fields:

| field | meaning |
|---|---|
| `id` | signed 64-bit integer, unique among nodes |
| `width`, `height` | node size; the output size can differ (containers are sized around their children, nodes may grow to fit their edge ports) |
| `topLeft` | `null` in the input; position in the output |
| `fixedTopLeft` | optional fixed position |
| `shapeType` | D2 shape: `Square`, `RealSquare`, `Parallelogram`, `Document`, `Cylinder`, `Queue`, `Page`, `Package`, `Step`, `Callout`, `StoredData`, `Person`, `C4Person`, `Diamond`, `Oval`, `Circle`, `Hexagon`, `Cloud`, `Table`, `Class`, `Text`, `Code`, `Image` (`""` = `Square`) |
| `label` | `{ "Text", "Position", "Width", "Height" }`; `Position` `""` lets the layout choose |
| `icon` | `{ "Position" }` |
| `d2id`, `fontSize`, `desiredWidth`, `desiredHeight`, `nears`, `numColumns`, `is3D`, `isMultiple`, `isInvisible`, `forceHierarchy` | optional D2 attributes |

Edge fields:

| field | meaning |
|---|---|
| `id`, `from`, `to` | edge ID and endpoint node IDs |
| `points` | `[]` in the input; the route in the output |
| `sourceArrowhead`, `targetArrowhead` | D2 arrowhead names (`""` or `none`, `triangle`, `arrow`, `diamond`, `circle`, `box`, `cf-one`, `cf-many`, ...) |
| `label`, `sourceArrowheadLabel`, `targetArrowheadLabel` | labels, as for nodes |
| `minWidth`, `minHeight`, `labelPercentage` | label geometry (output) |
| `fromTableColumnIndex`, `toTableColumnIndex` | SQL-table row ports |
| `style` | D2 style values (`{ "strokeWidth": { "value": "2" }, ... }`) |
| `d2id`, `originalId`, `isInvisible` | optional |

The layout result (`orthoflow::LayoutReport`, also what the CLI and the WASM
build return) is:

```json
{
  "seeds": [ { "seed": 1, "penalty": 0, "area": 134820 }, ... ],
  "bestSeed": 3,
  "graph": { "nodes": [ ... ], "edges": [ ... ], ... }
}
```

A seed that fails has an `"error"` string instead of `penalty`/`area`;
`bestSeed` and `graph` are `null` if every seed fails. In `graph`, a consumer
needs:

- node `topLeft` (`{x, y}`), `width` and `height`; containers are ordinary
  nodes, so their final box is in the node list too;
- edge `points`: the orthogonal route from the source node border to the
  target node border;
- label positions: `label.Position` of nodes and edges is a D2 label position
  name (`INSIDE_MIDDLE_CENTER`, `OUTSIDE_TOP_CENTER`, `OUTSIDE_BOTTOM_RIGHT`,
  `UNLOCKED_BOTTOM`, ...). Node label positions are relative to the node box;
  edge labels are placed along the route at `labelPercentage`. The
  `orthoflow-label` crate (`label::Position::GetPointOnBox` /
  `GetPointOnRoute`, ported from D2's `lib/label`) converts them to
  coordinates exactly like D2.

IDs are signed 64-bit integers. D2 uses 63-bit hash IDs, so JavaScript
consumers must not parse the JSON with plain `JSON.parse` if IDs can exceed
2^53 (the npm package handles this, see its README).

## Rust API

```toml
[dependencies]
orthoflow = { git = "https://github.com/nk-coding/orthoflow" }
```

```rust
let input = std::fs::read_to_string("examples/small.json")?;
let report = orthoflow::layout(&input, &[1, 2, 3])?; // &[] = default seeds 1, 2, 3
println!("best seed: {:?}", report.best_seed);
let graph_json: &str = report.graph.as_deref().expect("a seed succeeded");
let report_json: String = report.to_json(); // same format as the CLI
```

`orthoflow::layout_with_seed(&input, seed)` runs a single seed. The
`orthoflow-*` crates are implementation details (one crate per Go package)
and have no stable API.

## CLI

```sh
cargo build --release
target/release/orthoflow layout -seeds 1,2,3 examples/small.json   # layout report (JSON)
target/release/orthoflow score graph.json                          # {"area":..,"penalty":..} of a laid-out graph
target/release/orthoflow bench -seeds 1,2,3 -iters 5 tests/fixtures/corpus/inputs/*.json
target/release/orthoflow roundtrip in.json                         # decode + clone + encode
target/release/orthoflow stages -seed 1 in.json outdir             # graph after every pipeline stage
target/release/orthoflow replay -seed 1 -stage 7 snapshot.json     # run one stage on a snapshot
```

The modes and output formats are the same as those of the Go reference oracle
`talaref` (`goref/`), so outputs of the two can be compared directly.

## WebAssembly

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.129
scripts/build-wasm.sh out/web            # wasm-bindgen --target web
scripts/build-wasm.sh out/node nodejs    # or any other wasm-bindgen target
```

The module (about 3.7 MB, cargo profile `wasm` = release + `panic = "abort"`)
exports `layout(graphJson, seeds: BigInt64Array)` and
`layoutWithSeed(graphJson, seed: bigint)`, both returning the report JSON as a
string. The npm package `orthoflow` wraps it with a typed API.

## Benchmarks

Geometric mean of the median time per fixture over the 81 corpus fixtures,
full 3-seed layout, relative to single-threaded native Go (lower is faster):

| implementation | relative time | module size |
|---|---|---|
| Go, native, 1 CPU (baseline) | 1.00× | |
| orthoflow, native | 0.84× | |
| orthoflow → WASM, Node | 1.50× | 3.7 MB |
| Go → WASM, Node | 4.75× | 17.5 MB |

One pass over all 81 fixtures takes about 75 s natively (Go: 80 s).

## Tests

```sh
cargo test --release
```

runs the unit tests (including ported Go unit tests) and
`crates/orthoflow/tests/fixtures_parity.rs`, which lays out all 381 fixtures
in `tests/fixtures/{corpus,random}` with seeds 1, 2, 3 and compares the result
exactly with Go's output (`ORTHOFLOW_FILTER=<substring>` selects fixtures).

The stage-level parity tests are `#[ignore]`d because they need Go reference
data (about 270 MB) that is not shipped. To generate it you need Go and a
checkout of terrastruct/d2 at the commit above:

```sh
git clone https://github.com/terrastruct/d2 && git -C d2 checkout 6a7b4837838a3907604a1d9eec57ee98b6f3cd01
scripts/gen-goref.sh d2 /tmp/goref                # about 2 minutes
ORTHOFLOW_GOREF=/tmp/goref cargo test --release -- --ignored
```

`scripts/gen-goref.sh` copies the Go reference oracles from `goref/d2` into the
d2 checkout (they use TALA's internal packages), builds them, and writes the
stage snapshots (`talaref stages`), single-stage replays (`talaref replay`),
the expected layouts and random graphs (identical to `tests/fixtures`; the
script reports any difference), and the unit-level fixtures. A few extra data
sets (other seeds, routing perturbations) are described in the doc comments of
`crates/routing/tests/replay_{seeds,fuzz}.rs` and
`crates/placement/tests/replay_node_placement.rs`.

## Repository layout

```
crates/orthoflow        public API (graphjson in, layout report out)
crates/orthoflow-cli    the `orthoflow` binary
crates/orthoflow-wasm   wasm-bindgen bindings
crates/gocompat          Go standard library semantics (math, sort, rand, context, errors, ...)
crates/<package>         one crate per Go package: geo, label, shape, d2target (lib/ and d2target),
                         layoutgraph, graphjson, routing, placement, hierarchy, ... (internal/)
crates/testkit           test helpers (not published)
tests/fixtures           fixtures and Go's expected output
goref/                   Go reference oracles (copied into a d2 checkout by scripts/gen-goref.sh)
scripts/                 build-wasm.sh, gen-goref.sh, gen_nodeshape.py
```

Each Rust module is the translation of the Go file with the same name and
starts with a header naming it. Go identifiers are kept verbatim.

## License

orthoflow is licensed under the [Mozilla Public License 2.0](LICENSE), like D2.
The Go standard library ports in `crates/gocompat` are licensed under the
BSD-3-Clause license of Go, with the notices of the FreeBSD msun (Sun
Microsystems) and Cephes code that Go's `math` package is based on, and the
FxHasher there is adapted from rustc-hash (MIT OR Apache-2.0). See
[LICENSES](LICENSES) for every third-party origin and the files it applies to.

## Trademarks

"TALA", "D2" and "Terrastruct" are names (and possibly trademarks) of
Terrastruct Inc. They are used in this repository only to describe the origin
of the code. orthoflow is not affiliated with, sponsored by or endorsed by
Terrastruct Inc.
