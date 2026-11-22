#!/bin/sh
# The verdict, one command: docs, format, lint, types, tests, UI budget, the three ESP32 firmware
# images (the box's, the one QEMU runs, the measuring one) and the engine's run in QEMU.
#
#   sh scripts/validate.sh          everything (before a commit, in CI)
#   sh scripts/validate.sh --fast   the quick checks only (the pre-commit hook): docs, format, lint
#   sh scripts/validate.sh --only rust|ui|firmware
#                                   one third of the verdict, for the CI's parallel jobs: the three
#                                   together are exactly the full run
#   sh scripts/validate.sh --only e2e
#                                   opt-in, in no other run and not in the CI (it needs a browser):
#                                   the simulated path driven in headless Chromium
#
# Green only when every step is. Never weaken a step to get green: fix the cause.
set -e
cd "$(dirname "$0")/.."

# Tools installed in the user's folder (rustup, espup) are not always on the PATH of a hook.
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"

fast=false
only=""
case "$1" in
  --fast) fast=true ;;
  --only)
    only="$2"
    case "$only" in rust | ui | firmware | e2e) ;; *) echo "validate: --only takes rust, ui, firmware or e2e" >&2; exit 2 ;; esac
    ;;
esac
# True when the group is part of this run (all of them, unless --only names one).
want() { [ -z "$only" ] || [ "$only" = "$1" ]; }

step() { printf '\n== %s\n' "$1"; }

if want ui && [ ! -d ui/node_modules ]; then
  echo "validate: ui/node_modules is missing. Run: cd ui && npm ci" >&2
  exit 1
fi

if want ui && [ ! -d tools/z2m-coverage/node_modules ]; then
  echo "validate: tools/z2m-coverage/node_modules is missing. Run: cd tools/z2m-coverage && npm ci" >&2
  exit 1
fi

if want rust; then
  step "Docs"
  sh scripts/check-docs.sh

  step "Rust: format"
  cargo fmt --all --check
  rustfmt --check --edition 2024 firmware/build.rs firmware/src/*.rs

  step "Rust: lint (core, host)"
  cargo clippy --workspace --all-targets --quiet -- -D warnings
fi

if want ui; then
  step "UI and tools: format, lint"
  (cd ui && npm run -s format:check && npm run -s lint)
  (cd tools/z2m-coverage && npm run -s format:check)
fi

if $fast; then
  printf '\nvalidate --fast: OK\n'
  exit 0
fi

if want rust; then
  step "Rust: tests (core, host)"
  cargo test --workspace --quiet
fi

if want ui; then
  step "UI: types, tests, build, budget"
  (cd ui && npm run -s typecheck && npm test --silent && npm run -s build >/dev/null && npm run -s check:bundle)

  step "Tools: types, tests"
  (cd tools/z2m-coverage && npm run -s typecheck && npm test --silent)
fi

if want firmware; then
  step "Firmware: ESP32 builds and lint (the box's image, the QEMU image with feature sim, the measuring image with feature flash-saves)"
  if [ -f "$HOME/export-esp.sh" ]; then
    . "$HOME/export-esp.sh"
  else
    echo "validate: the ESP32 toolchain is not set up (espup install, see README.md)." >&2
    exit 1
  fi
  (
    cd firmware
    cargo build --release --quiet && cargo clippy --release --quiet -- -D warnings
    cargo build --release --features sim --quiet && cargo clippy --release --features sim --quiet -- -D warnings
    cargo build --release --features flash-saves --quiet && cargo clippy --release --features flash-saves --quiet -- -D warnings
  )

  step "Firmware: the engine runs in QEMU (heap peak and internal RAM checked)"
  sh scripts/qemu.sh
fi

if [ "$only" = "e2e" ]; then
  step "End to end: the simulated path in headless Chromium"
  [ -d tools/e2e/node_modules ] || { echo "validate: tools/e2e/node_modules is missing. Run: cd tools/e2e && npm ci" >&2; exit 1; }
  (cd tools/e2e && npm run -s format:check)
  sh scripts/e2e-sim.sh
fi

printf '\nvalidate: OK\n'
