#!/usr/bin/env bash
# Installs the exact native tools the build and the checks need, verified by SHA-256:
#   wasm-bindgen (must match the wasm-bindgen crate version in Cargo.lock)
#   wasm-opt (binaryen)
#   gitleaks (secret scanner)
#   cargo-deny (Rust dependency licenses and advisories)
#   zizmor (GitHub Actions workflow linter)
# Usage: scripts/install-tools.sh                  # all of them, into ~/.local
#        scripts/install-tools.sh gitleaks         # only the ones named
#        TOOLS_BIN=/some/bin scripts/install-tools.sh
# Supported: macOS arm64, Linux x86_64. No sudo needed.
set -euo pipefail

WASM_BINDGEN_VERSION=0.2.130
BINARYEN_VERSION=133
GITLEAKS_VERSION=8.30.1
CARGO_DENY_VERSION=0.22.0
ZIZMOR_VERSION=1.32.0

ALL_TOOLS=(wasm-bindgen wasm-opt gitleaks cargo-deny zizmor)
if [[ $# -eq 0 ]]; then
  set -- "${ALL_TOOLS[@]}"
fi
for tool in "$@"; do
  case " ${ALL_TOOLS[*]} " in
    *" $tool "*) ;;
    *)
      echo "Unknown tool '$tool'. Known: ${ALL_TOOLS[*]}" >&2
      exit 1
      ;;
  esac
done
SELECTED=" $* "
wanted() { [[ "$SELECTED" == *" $1 "* ]]; }

TOOLS_HOME="${TOOLS_HOME:-$HOME/.local/share/transit-braid-tools}"
TOOLS_BIN="${TOOLS_BIN:-$HOME/.local/bin}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"

case "$(uname -s)-$(uname -m)" in
  Darwin-arm64)
    WB_TRIPLE=aarch64-apple-darwin
    WB_SHA=b90ddb9357f584b1af897d58ad77e4cce2b48fc17e2a967bc249a87f72809444
    BY_PLATFORM=arm64-macos
    BY_SHA=ad66da82ac13f163e424b1643f16c6dfcccc98b5966296b43e52d3cab04f84a8
    GL_PLATFORM=darwin_arm64
    GL_SHA=b40ab0ae55c505963e365f271a8d3846efbc170aa17f2607f13df610a9aeb6a5
    CD_TRIPLE=aarch64-apple-darwin
    CD_SHA=aa23e2f32eaf0b92efb36de3fc925f3e1b841fdafcc555ddab43d4d146cec251
    ZZ_TRIPLE=aarch64-apple-darwin
    ZZ_SHA=6efed18c4539a663fd3c851a2be9533fd8456c4e646d9416206da059154a0d3c
    ;;
  Linux-x86_64)
    WB_TRIPLE=x86_64-unknown-linux-musl
    WB_SHA=9687f3bb551b9f66f37ed32a40d7913a00f7962db653054e56857bc306c3227f
    BY_PLATFORM=x86_64-linux
    BY_SHA=2dc9c7813f5375db93d96ead4b78222fcc3e2677bbb832297af4797782a37489
    GL_PLATFORM=linux_x64
    GL_SHA=551f6fc83ea457d62a0d98237cbad105af8d557003051f41f3e7ca7b3f2470eb
    CD_TRIPLE=x86_64-unknown-linux-musl
    CD_SHA=98c56e6a408bfa5c3c60744b6015aaa6aa08f743fdeb88386d06fca035bb1786
    ZZ_TRIPLE=x86_64-unknown-linux-gnu
    ZZ_SHA=bb110cc765eca9cd7efb2e284a956be5974c60027b0970f16397d86208a9957b
    ;;
  *)
    echo "Unsupported platform $(uname -s)-$(uname -m). Install wasm-bindgen $WASM_BINDGEN_VERSION," >&2
    echo "binaryen version_$BINARYEN_VERSION, gitleaks $GITLEAKS_VERSION, cargo-deny $CARGO_DENY_VERSION and" >&2
    echo "zizmor $ZIZMOR_VERSION by hand." >&2
    exit 1
    ;;
esac

# wasm-bindgen's CLI and crate versions must match exactly, or the generated glue breaks.
LOCKED=$(awk '/^name = "wasm-bindgen"$/{getline; gsub(/version = |"/,""); print; exit}' "$ROOT/Cargo.lock" 2>/dev/null || true)
if wanted wasm-bindgen && [[ -n "$LOCKED" && "$LOCKED" != "$WASM_BINDGEN_VERSION" ]]; then
  echo "Cargo.lock has wasm-bindgen $LOCKED but this script pins $WASM_BINDGEN_VERSION." >&2
  echo "Update WASM_BINDGEN_VERSION and its SHA-256 values in $0." >&2
  exit 1
fi

mkdir -p "$TOOLS_HOME" "$TOOLS_BIN"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

# fetch <url> <expected sha256> <file>
fetch() {
  curl -fsSL --proto '=https' --tlsv1.2 -o "$TMP/$3" "$1"
  local actual
  actual=$(shasum -a 256 "$TMP/$3" | cut -d' ' -f1)
  if [[ "$actual" != "$2" ]]; then
    echo "Checksum mismatch for $3: expected $2, got $actual" >&2
    exit 1
  fi
}

have() { command -v "$1" >/dev/null 2>&1 && "$1" --version 2>/dev/null | grep -q "$2"; }

if ! wanted wasm-bindgen; then
  :
elif have wasm-bindgen "$WASM_BINDGEN_VERSION"; then
  echo "wasm-bindgen $WASM_BINDGEN_VERSION already installed"
else
  name="wasm-bindgen-$WASM_BINDGEN_VERSION-$WB_TRIPLE"
  fetch "https://github.com/wasm-bindgen/wasm-bindgen/releases/download/$WASM_BINDGEN_VERSION/$name.tar.gz" "$WB_SHA" wb.tar.gz
  rm -rf "${TOOLS_HOME:?}/$name" && tar -xzf "$TMP/wb.tar.gz" -C "$TOOLS_HOME"
  ln -sf "$TOOLS_HOME/$name/wasm-bindgen" "$TOOLS_BIN/wasm-bindgen"
  echo "installed wasm-bindgen $WASM_BINDGEN_VERSION"
fi

if ! wanted wasm-opt; then
  :
elif have wasm-opt "version $BINARYEN_VERSION"; then
  echo "wasm-opt (binaryen $BINARYEN_VERSION) already installed"
else
  fetch "https://github.com/WebAssembly/binaryen/releases/download/version_$BINARYEN_VERSION/binaryen-version_$BINARYEN_VERSION-$BY_PLATFORM.tar.gz" "$BY_SHA" by.tar.gz
  rm -rf "${TOOLS_HOME:?}/binaryen-version_$BINARYEN_VERSION" && tar -xzf "$TMP/by.tar.gz" -C "$TOOLS_HOME"
  ln -sf "$TOOLS_HOME/binaryen-version_$BINARYEN_VERSION/bin/wasm-opt" "$TOOLS_BIN/wasm-opt"
  echo "installed wasm-opt (binaryen $BINARYEN_VERSION)"
fi

if ! wanted gitleaks; then
  :
elif have gitleaks "$GITLEAKS_VERSION"; then
  echo "gitleaks $GITLEAKS_VERSION already installed"
else
  fetch "https://github.com/gitleaks/gitleaks/releases/download/v$GITLEAKS_VERSION/gitleaks_${GITLEAKS_VERSION}_$GL_PLATFORM.tar.gz" "$GL_SHA" gl.tar.gz
  mkdir -p "$TOOLS_HOME/gitleaks-$GITLEAKS_VERSION" && tar -xzf "$TMP/gl.tar.gz" -C "$TOOLS_HOME/gitleaks-$GITLEAKS_VERSION"
  ln -sf "$TOOLS_HOME/gitleaks-$GITLEAKS_VERSION/gitleaks" "$TOOLS_BIN/gitleaks"
  echo "installed gitleaks $GITLEAKS_VERSION"
fi

if ! wanted cargo-deny; then
  :
elif have cargo-deny "$CARGO_DENY_VERSION"; then
  echo "cargo-deny $CARGO_DENY_VERSION already installed"
else
  name="cargo-deny-$CARGO_DENY_VERSION-$CD_TRIPLE"
  fetch "https://github.com/EmbarkStudios/cargo-deny/releases/download/$CARGO_DENY_VERSION/$name.tar.gz" "$CD_SHA" cd.tar.gz
  rm -rf "${TOOLS_HOME:?}/$name" && tar -xzf "$TMP/cd.tar.gz" -C "$TOOLS_HOME"
  ln -sf "$TOOLS_HOME/$name/cargo-deny" "$TOOLS_BIN/cargo-deny"
  echo "installed cargo-deny $CARGO_DENY_VERSION"
fi

if ! wanted zizmor; then
  :
elif have zizmor "$ZIZMOR_VERSION"; then
  echo "zizmor $ZIZMOR_VERSION already installed"
else
  fetch "https://github.com/zizmorcore/zizmor/releases/download/v$ZIZMOR_VERSION/zizmor-$ZZ_TRIPLE.tar.gz" "$ZZ_SHA" zz.tar.gz
  mkdir -p "$TOOLS_HOME/zizmor-$ZIZMOR_VERSION" && tar -xzf "$TMP/zz.tar.gz" -C "$TOOLS_HOME/zizmor-$ZIZMOR_VERSION"
  ln -sf "$TOOLS_HOME/zizmor-$ZIZMOR_VERSION/zizmor" "$TOOLS_BIN/zizmor"
  echo "installed zizmor $ZIZMOR_VERSION"
fi

case ":$PATH:" in
  *":$TOOLS_BIN:"*) ;;
  *) echo "Note: add $TOOLS_BIN to your PATH." ;;
esac
