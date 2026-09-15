#!/usr/bin/env bash
# Builds the engine for wasm, drives it in a JavaScript runtime, and checks it plays the same
# story the native build plays.
#
# Usage:
#   tools/wasm-smoke.sh                     # the standard example
#   tools/wasm-smoke.sh examples/hello      # any project
#
# Why this exists
# ---------------
# `BUILD_AND_ASSETS.md §5` makes web a first-class target, and "first class" has to mean more
# than "it compiles": the module has to load, run a story, and produce the commands a page would
# draw — which is what this checks. Node rather than a browser because the engine boundary is
# `Command` values, so the same run under a browser would exercise the canvas, not the engine;
# and because a CI runner has Node and may not have a browser.
#
# The diff against `vela run --headless` is the real assertion: one engine, two platforms, one
# command stream. Anything else would mean the browser build had drifted, and every determinism
# claim in `RUNTIME.md §4` would end at the platform boundary.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

project="${1:-examples/standard}"
target="wasm32-unknown-unknown"
work="$(mktemp -d)"
trap 'rm -rf "${work}"' EXIT

for tool in cargo node wasm-bindgen; do
    if ! command -v "${tool}" >/dev/null 2>&1; then
        echo "tools/wasm-smoke.sh needs ${tool}" >&2
        if [ "${tool}" = "wasm-bindgen" ]; then
            echo "  cargo install wasm-bindgen-cli --version 0.2.128" >&2
        fi
        exit 1
    fi
done

rustup target add "${target}" >/dev/null 2>&1 || true

echo "building the engine for ${target}"
cargo build --release --target "${target}" -p vela-web >/dev/null

wasm="target/${target}/release/vela_web.wasm"
echo "generating the JavaScript glue"
wasm-bindgen --target nodejs --out-dir "${work}/glue" "${wasm}" >/dev/null

echo "building the project"
cargo run -q -p vela-cli -- build "${project}" --out "${work}/dist" >/dev/null
module="${work}/dist/scripts/main.velac"

echo "playing it in wasm"
node tools/wasm-smoke.cjs "${work}/glue" "${module}" >"${work}/wasm.txt"

echo "playing it natively"
cargo run -q -p vela-cli -- run "${project}" --headless >"${work}/native.txt"

if ! diff -u "${work}/native.txt" "${work}/wasm.txt"; then
    echo "the wasm engine and the native engine played different stories" >&2
    exit 1
fi

commands="$(wc -l <"${work}/wasm.txt")"
size="$(stat -c%s "${wasm}" 2>/dev/null || stat -f%z "${wasm}")"
echo "wasm smoke: ${commands} command(s), identical to the native run"
echo "wasm size:  ${size} bytes (${wasm})"
