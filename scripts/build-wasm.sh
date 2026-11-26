#!/usr/bin/env bash
# Builds the WebAssembly module and its JS/TS glue into packages/transit-braid/wasm/:
#   1. cargo: Rust -> .wasm (SIMD on, speed-optimized profile)
#   2. wasm-bindgen: generates the JS loader and .d.ts types (--target web works in
#      browsers, Node, Deno and Bun), with __wbg_reset_state(), which swaps a crashed instance
#      for a fresh one (crash recovery); the .wasm is the same with or without it
#   3. wasm-opt: shrinks and speeds up the .wasm, then strips metadata
#   4. size budget: fails if the gzipped .wasm is over WASM_BUDGET_KB (default 300)
# Needs: scripts/install-tools.sh run once.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="$ROOT/packages/transit-braid/wasm"
BUDGET_KB="${WASM_BUDGET_KB:-300}"
RAW="$ROOT/target/wasm32-unknown-unknown/wasm-release/transit_braid_wasm.wasm"

for tool in cargo wasm-bindgen wasm-opt; do
  command -v "$tool" >/dev/null || { echo "Missing $tool. Run scripts/install-tools.sh" >&2; exit 1; }
done

cd "$ROOT"
# --locked: exactly what Cargo.lock says (fails if it is out of date, never rewrites it).
cargo build --quiet --locked -p transit-braid-wasm --target wasm32-unknown-unknown \
  --profile wasm-release --target-dir "$ROOT/target"

rm -rf "$OUT"
wasm-bindgen --target web --experimental-reset-state-function --out-dir "$OUT" \
  --out-name transit_braid_wasm "$RAW"

WASM="$OUT/transit_braid_wasm_bg.wasm"
# wasm-opt reads the enabled features (SIMD etc.) from the module, then strips metadata.
wasm-opt -O3 --strip-debug --strip-producers --strip-target-features "$WASM" -o "$WASM"

bytes=$(wc -c <"$WASM" | tr -d ' ')
gz=$(gzip -9 -c "$WASM" | wc -c | tr -d ' ')
echo "transit_braid_wasm_bg.wasm: $((bytes / 1024)) KB raw, $((gz / 1024)) KB gzipped (budget ${BUDGET_KB} KB gzipped)"
if ((gz > BUDGET_KB * 1024)); then
  echo "WASM size budget exceeded." >&2
  exit 1
fi
