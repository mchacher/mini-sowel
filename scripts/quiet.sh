#!/bin/sh
# Runs a command and keeps its output out of the conversation: the whole output goes to a log
# file, and only the verdict is printed. Use it for anything that prints a lot (tests, builds,
# QEMU, npm, gh), so an agent's context holds one line on success and the useful lines on failure.
#
#   sh scripts/quiet.sh [--tail N] [--] <command> [args...]
#
# Success: one line, `ok: <command> (<seconds> s[, <n> tests])`.
# Failure: `FAILED (exit <code>): <command>`, the lines that name a failure (at most 20), the last
# N lines (15 by default) and the path of the full log. The exit code is the command's own.
# The log folder is $QUIET_LOG_DIR, or a `solite-quiet` folder in $TMPDIR (never in the repository).

tail_lines=15
while [ $# -gt 0 ]; do
  case "$1" in
    --tail) tail_lines="$2"; shift 2 ;;
    --) shift; break ;;
    *) break ;;
  esac
done
if [ $# -eq 0 ]; then
  echo "usage: sh scripts/quiet.sh [--tail N] [--] <command> [args...]" >&2
  exit 2
fi

dir="${QUIET_LOG_DIR:-${TMPDIR:-/tmp}/solite-quiet}"
mkdir -p "$dir" || exit 2
label=$(printf '%s' "$*" | cut -c1-80)
slug=$(printf '%s' "$*" | tr -c 'A-Za-z0-9' '_' | cut -c1-40)
log="$dir/$(date +%Y%m%d-%H%M%S)-$slug.log"

start=$(date +%s)
"$@" >"$log" 2>&1
code=$?
seconds=$(($(date +%s) - start))

if [ "$code" -eq 0 ]; then
  # `cargo test` prints one `test result: ok. N passed` line per test binary: add them up.
  tests=$(sed -n 's/^test result: ok\. \([0-9][0-9]*\) passed.*/\1/p' "$log" | awk '{s+=$1} END {if (NR>0) print s}')
  if [ -n "$tests" ]; then
    echo "ok: $label (${seconds} s, ${tests} tests)"
  else
    echo "ok: $label (${seconds} s)"
  fi
else
  echo "FAILED (exit $code): $label (${seconds} s)"
  grep -E 'FAILED|panicked|^error|^Error|failed|not ok' "$log" | head -n 20
  echo "--- last $tail_lines lines"
  tail -n "$tail_lines" "$log"
  echo "--- full log: $log"
fi
exit "$code"
