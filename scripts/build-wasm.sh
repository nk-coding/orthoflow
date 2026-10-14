#!/usr/bin/env bash
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
#
# Copyright 2026 Niklas Krieger
#
# Builds the orthoflow-wasm crate for wasm32-unknown-unknown (cargo profile
# `wasm` = release + panic=abort) and runs wasm-bindgen on it.
#
#   scripts/build-wasm.sh <out-dir> [<wasm-bindgen target>]
#
# The target defaults to `web` (ES module with an async init function); `nodejs`,
# `bundler`, `deno`, `no-modules` and `experimental-nodejs-module` are passed
# through. Output: <out-dir>/orthoflow_wasm.js, orthoflow_wasm_bg.wasm and
# the .d.ts files.
#
# Requirements: the wasm32-unknown-unknown Rust target
# (`rustup target add wasm32-unknown-unknown`) and wasm-bindgen-cli with exactly
# the version pinned in crates/orthoflow-wasm/Cargo.toml
# (`cargo install wasm-bindgen-cli --version 0.2.129`).
# Env: CARGO_TARGET_DIR (default <repo>/target), WASM_BINDGEN (default
# `wasm-bindgen`), JOBS (cargo -j, default: cargo's default).
set -euo pipefail
if [ $# -lt 1 ] || [ $# -gt 2 ]; then
  sed -n '7,22p' "$0" | sed 's/^# \{0,1\}//' >&2
  exit 2
fi
REPO="$(cd "$(dirname "$0")/.." && pwd)"
mkdir -p "$1"
OUT="$(cd "$1" && pwd)"
TARGET="${2:-web}"
WASM_BINDGEN="${WASM_BINDGEN:-wasm-bindgen}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$REPO/target}"

want=$(sed -n 's/^wasm-bindgen = .*"=\([0-9.]*\)".*/\1/p' "$REPO/crates/orthoflow-wasm/Cargo.toml")
have=$("$WASM_BINDGEN" --version | awk '{print $2}')
if [ "$want" != "$have" ]; then
  echo "wasm-bindgen $have found, but the crate pins wasm-bindgen =$want;" >&2
  echo "install the matching CLI: cargo install wasm-bindgen-cli --version $want" >&2
  exit 1
fi

jobs=()
[ -n "${JOBS:-}" ] && jobs=(-j "$JOBS")
(cd "$REPO" && cargo build "${jobs[@]}" --locked --profile wasm --target wasm32-unknown-unknown -p orthoflow-wasm)
"$WASM_BINDGEN" --target "$TARGET" --out-dir "$OUT" "$CARGO_TARGET_DIR/wasm32-unknown-unknown/wasm/orthoflow_wasm.wasm"
ls -l "$OUT"
