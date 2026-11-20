#!/bin/sh
# The verdict, one command: docs, format, lint, types, tests, UI budget, ESP32 firmware build
# and boot in QEMU.
#
#   sh scripts/validate.sh          everything (before a commit, in CI)
#   sh scripts/validate.sh --fast   the quick checks only (the pre-commit hook): docs, format, lint
#
# Green only when every step is. Never weaken a step to get green: fix the cause.
set -e
cd "$(dirname "$0")/.."

# Tools installed in the user's folder (rustup, espup) are not always on the PATH of a hook.
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"

fast=false
[ "$1" = "--fast" ] && fast=true

step() { printf '\n== %s\n' "$1"; }

if [ ! -d ui/node_modules ]; then
  echo "validate: ui/node_modules is missing. Run: cd ui && npm ci" >&2
  exit 1
fi

if [ ! -d tools/z2m-coverage/node_modules ]; then
  echo "validate: tools/z2m-coverage/node_modules is missing. Run: cd tools/z2m-coverage && npm ci" >&2
  exit 1
fi

step "Docs"
sh scripts/check-docs.sh

step "Rust: format"
cargo fmt --all --check
rustfmt --check --edition 2024 firmware/build.rs firmware/src/*.rs

step "Rust: lint (core, host)"
cargo clippy --workspace --all-targets --quiet -- -D warnings

step "UI and tools: format, lint"
(cd ui && npm run -s format:check && npm run -s lint)
(cd tools/z2m-coverage && npm run -s format:check)

if $fast; then
  printf '\nvalidate --fast: OK\n'
  exit 0
fi

step "Rust: tests (core, host)"
cargo test --workspace --quiet

step "UI: types, tests, build, budget"
(cd ui && npm run -s typecheck && npm test --silent && npm run -s build >/dev/null && npm run -s check:bundle)

step "Tools: types, tests"
(cd tools/z2m-coverage && npm run -s typecheck && npm test --silent)

step "Firmware: ESP32 build and lint"
if [ -f "$HOME/export-esp.sh" ]; then
  . "$HOME/export-esp.sh"
else
  echo "validate: the ESP32 toolchain is not set up (espup install, see README.md)." >&2
  exit 1
fi
(cd firmware && cargo build --release --quiet && cargo clippy --release --quiet -- -D warnings)

step "Firmware: boots in QEMU"
sh scripts/qemu.sh

printf '\nvalidate: OK\n'
