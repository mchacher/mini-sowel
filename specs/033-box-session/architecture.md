# Architecture: Spec 033

## Overview

```
box-session.sh (outer run)
  P1..P5 on the Mac ──► solite-host --update-keygen / --update-sign, firmware builds
                        (or $SOLITE_SESSION_BUILDER), fw_elf2image
  B1..B10 on the box ─► box.sh identify | flash | log | saves -- box-session.sh --group <g>
                                                               (SOLITE_BOX_ADDRESS from saves)
                     ─► box-session.sh --group update (straight to the address: the box restarts)
  H1..H11 ───────────► skipped, or asked on /dev/tty (--with-gestures)
  report lines ──────► stdout; every tool's output ──► <run folder>/session.log
```

`--api-only <address>` runs the groups straight against an address (the Mac host), no box.

One shell script, POSIX `sh` as the others (`set -u`, no `set -e`: a step's failure is a report
line, not an exit). Each step is a function that returns its outcome through a small `report`
helper (`report <id> <name> OK|KO|skipped <detail>`), which prints the line, appends it to the
log and counts it. The stop rule is a variable (`STOPPED=<id>`) that every later box step reads
first.

## Data

| Name | Type | Where |
| ---- | ---- | ----- |
| Run folder | directory, 0700 | `$SOLITE_SESSION_ROOT/<date>` (`$TMPDIR/solite-box-session/<date>`) |
| `session.log` | text | run folder |
| password file | 24 hex characters, 0600 (folder 0700) | `$SOLITE_PASSWORD_FILE`: `$HOME/.config/solite/box-password` in a box run, the run folder in `--api-only`; only when the run created it, never overwritten |
| `location` | the latitude, longitude and time zone set, 0600 | run folder, for the persistence check |
| `update.key` | 64 hex characters, 0600 | run folder (box run and `--api-only`), removed by the `EXIT` trap |
| `next.sup`, `rollback.sup`, `bad.sup` | signed update files | run folder |
| `address` | the last `serving at` address | run folder |
| Throwaway tree and its target | source copy, build folder | `$SOLITE_SESSION_ROOT/tree`, `$SOLITE_SESSION_ROOT/target` (kept between runs) |

Nothing is stored in the repository; no address, password or key is written into a tracked file.

## Files changed

| File | Change |
| ---- | ------ |
| `scripts/box-session.sh` | New: the session (R1 to R6) |
| `scripts/validate.sh` | Firmware group: the image sizes step (R7) |
| `scripts/lib/firmware.sh` | A helper `fw_size_line <name> <bin>` (KB, %, warning) used by `validate.sh` and `box-session.sh` P5 |
| `crates/host/tests/box_session_script.rs` | New: the script's tests (fake `box.sh`, fake builder, the sim host, a fake update server) |
| `docs/living-spec/README.md` | The delta |
| `docs/decisions.md` | D55 |
| `docs/roadmap.md` | The box list: which step runs each item; inbox: `box.sh` raw console line |
| `docs/architecture.md` | Box row |
| `README.md`, `CLAUDE.md` | The command line; the D6 sentence |
| `docs/specs-index.md` | Row 033 |

## Reused

- `scripts/box.sh` for every touch of the box (identify, flash, log, saves): the session adds no
  serial or esptool code.
- `scripts/lib/firmware.sh`: `fw_find_tools`, `fw_build_table`, `fw_elf2image`, `fw_version`.
- `scripts/measure-api.sh` for spec 025's 33 requests, as the roadmap runs it.
- The login and setup of `measure-api.sh` and `demo-garage.sh` (`/auth/status`, setup or login,
  the bearer on every request): the same `curl` pattern.
- The tests follow `crates/host/tests/measure_script.rs` (a sim host serving on a free port, the
  script run as a child process) and `crates/host/tests/box_script.rs` (fake tools written to a
  temporary folder, each logging its arguments).
