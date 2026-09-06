#!/bin/sh
# Rebuild the wasm bindings into src/wasm (wasm-pack --target web).
# Prefers a toolchain that has the wasm32-unknown-unknown std installed;
# on this machine the rustup-managed "stable" toolchain has it while the
# Homebrew cargo on PATH does not.
# Note: wasm-pack resolves --out-dir relative to the crate path, so this
# runs from the repo root and emits into playground/src/wasm.
set -e

cd "$(dirname "$0")/../.."

if rustc --print target-libdir --target wasm32-unknown-unknown >/dev/null 2>&1 \
  && [ -d "$(rustc --print target-libdir --target wasm32-unknown-unknown)" ]; then
  exec wasm-pack build --release --target web --out-dir playground/src/wasm .
fi

for tc in "$HOME"/.rustup/toolchains/*/; do
  if [ -d "${tc}lib/rustlib/wasm32-unknown-unknown" ]; then
    export PATH="${tc}bin:$PATH"
    break
  fi
done

exec wasm-pack build --release --target web --out-dir playground/src/wasm .
