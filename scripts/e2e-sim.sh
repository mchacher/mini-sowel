#!/bin/sh
# End-to-end test of the simulated path (spec 019, J1 and J2), with no human: starts the host on
# the demo garage in the simulator with an empty data folder, then drives the interface in a
# headless Chromium (Playwright, tools/e2e): zone, Discovery, two equipments, Devices, the garage
# page, the API, a reload, the equipment page (enable, bindings), the device
# page, screenshots of the pages in the dark and Hybrid themes.
# Prints one verdict line; the screenshots of each step are kept in
# $E2E_SHOTS (a folder in $TMPDIR by default, never in the repository).
#
#   sh scripts/e2e-sim.sh [--port N]
#
# Needs: `cd tools/e2e && npm ci`, the Chromium of Playwright in its cache (not downloaded here),
# and `cd ui && npm ci` (the interface is built only when ui/dist is missing or older than ui/src).
# Not part of the default `validate.sh` nor of the CI (no browser there): `validate.sh --only e2e`.
set -u
cd "$(dirname "$0")/.."
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"

port=""
if [ "${1:-}" = "--port" ]; then port="${2:-}"; fi
if [ -z "$port" ]; then
  port=$(python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])')
fi

fail() { echo "e2e-sim: FAIL: $1"; exit 1; }

[ -d tools/e2e/node_modules ] || fail "tools/e2e/node_modules is missing. Run: cd tools/e2e && npm ci"
[ -d ui/node_modules ] || fail "ui/node_modules is missing. Run: cd ui && npm ci"

work=$(mktemp -d "${TMPDIR:-/tmp}/solite-e2e.XXXXXX") || fail "no temporary folder"
data="$work/data"
log="$work/host.log"
mkdir -p "$data"
export E2E_SHOTS="${E2E_SHOTS:-${TMPDIR:-/tmp}/solite-e2e-shots}"
host_pid=""

cleanup() {
  [ -n "$host_pid" ] && kill "$host_pid" 2>/dev/null && wait "$host_pid" 2>/dev/null
  # The log is kept next to the screenshots when the run failed.
  if [ "${failed:-1}" -ne 0 ] && [ -f "$log" ]; then
    mkdir -p "$E2E_SHOTS" && cp "$log" "$E2E_SHOTS/host.log"
  fi
  rm -rf "$work"
}
failed=1
trap cleanup EXIT
trap 'exit 1' INT TERM

# The interface, only when it is missing or older than its sources.
if [ ! -f ui/dist/index.html ] || [ -n "$(find ui/src ui/index.html ui/package.json -newer ui/dist/index.html -type f 2>/dev/null | head -n 1)" ]; then
  sh scripts/quiet.sh sh -c 'cd ui && npm run -s build' || exit 1
fi
sh scripts/quiet.sh cargo build -p solite-host || exit 1

# The `motion-light` recipe is a script (spec 046): the e2e test makes an instance of it.
./target/debug/solite-host --sim scenarios/garage.json --serve "$port" --speed 10 --recipe-script recipes/scripts/motion-light.js --data "$data" >"$log" 2>&1 &
host_pid=$!

# Wait until it answers (at most 60 s): the status of the account needs no token (spec 029); the
# browser test goes through the first-start screen itself.
tries=0
until curl -sf "http://127.0.0.1:$port/api/v1/auth/status" >/dev/null 2>&1; do
  kill -0 "$host_pid" 2>/dev/null || { cat "$log" | tail -n 15; fail "the host stopped at start"; }
  tries=$((tries + 1))
  [ "$tries" -le 600 ] || fail "the host does not answer on port $port"
  python3 -c 'import time; time.sleep(0.1)'
done

E2E_URL="http://127.0.0.1:$port" node tools/e2e/src/e2e.mjs
code=$?
failed=$code
if [ "$code" -eq 0 ]; then echo "e2e-sim: OK (screenshots: $E2E_SHOTS)"; else echo "e2e-sim: FAILED (screenshots: $E2E_SHOTS)"; fi
exit "$code"
