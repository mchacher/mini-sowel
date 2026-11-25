# Plan: Spec 033

## Steps

- [x] 1. Living spec updated (the delta of `spec.md`)
- [x] 2. Core types: not applicable (no type)
- [x] 3. Core logic: not applicable (a script)
- [x] 4. Adapters: not applicable
- [x] 5. API: not applicable (the script calls the existing routes)
- [x] 6. Interface: not applicable
- [x] 7. `scripts/lib/firmware.sh`: `fw_size_line`
- [x] 8. `scripts/validate.sh`: the image sizes step (R7)
- [x] 9. `scripts/box-session.sh`: options and run folder (R1, R2), steps P1 to P5 and B1 to B10
      (R3), the groups (R4), the seams (R5), the gesture steps (R6)
- [x] 10. `crates/host/tests/box_session_script.rs`: one test per scenario below
- [x] 11. Docs: decisions D55, roadmap, architecture, README, CLAUDE.md, specs-index

## Test plan

All in `crates/host/tests/box_session_script.rs`. The fake `box.sh` (`SOLITE_BOX_SCRIPT`) logs its
arguments to a file and answers from environment variables: `identify` and `flash` succeed or fail,
`log` prints canned lines (`memory-network: internal 120 KB free (largest block 60 KB), psram 1800
KB free of 2048 KB`, `heap: internal 90 KB free (min 80 KB)`, `storage: /data, 40 KB used of 8064
KB`), `saves --budget <n> -- <command>` runs the command with `SOLITE_BOX_ADDRESS` set to the sim
host, prints `box: saves held; serving at http://<address>/`, writes a serial log holding one banner (`solite <version>`) and `network:
resumed` and prints its path as `box: serial log: <path>`, then `box: flash: <m> writes since boot`
with `m` from an environment variable, and fails with `over the budget of <n>` when `m > n`. The
fake builder (`SOLITE_SESSION_BUILDER`) writes a few bytes. `SOLITE_HOST_BIN` is the test's
`solite-host`. The fake `box.sh` mirrors the real one's output per mode, on the real streams:
`log --image radio` prints its selected lines, then `box: serial log: <path>` (the file holds the
canned serial lines) and, with `--budget`, the `flash:` count; `log --raw` prints only the path and
the count; `saves` prints `box: saves held; serving at http://<address>/`, runs the command, then
`box: saves flushed (<k> files written)`, `box: flash: <m> writes since boot`, and `box: serial
log: <path>` last. Runs with `--yes-flash` go through a pseudo-terminal (the `python3` pty helper of
`box_script.rs`), since the flag needs `/dev/tty`. The waits (`SOLITE_SESSION_*_WAIT`,
`SOLITE_SESSION_POLL`, `SOLITE_SESSION_RATE_WAIT`) are set to a second or less. The password file
is `SOLITE_PASSWORD_FILE` in the test's temporary folder.

| Module | Scenario | Expected |
| ------ | -------- | -------- |
| options | No `--yes-flash` (fake box) | the fake's log has no `flash` and no `saves`; P2 to P4, B2, B4 to B10 `skipped (needs --yes-flash)`; P1, B1, B3 OK; exit 0 |
| options | `--yes-flash` without `--backup` | exit 2, the usage message, the fake never called |
| options | `--yes-flash --backup <missing file>` | exit 2, `backup not found`, the fake never called |
| options | `--yes-flash --api-only 8080`, an unknown option | exit 2 |
| options | `SOLITE_SESSION_ROOT` inside the repository | exit 2 |
| run | `--yes-flash --backup <file>` (fake box, fake builder, sim host): the nominal pass | B2 calls `flash --image radio --keep-storage --backup <file>`; B4, B6, B7 OK inside `saves` with budgets 3, 5, 4; B5 OK; B8's checks after the 401 `skipped (this build has no update key)`; verdict 0 KO; exit 0 |
| run | A `saves` over its budget (fake count 9) | that step `KO (flash: 9 writes since boot, over the budget of <n>)`; every later box step `skipped (stopped after <id>)`; exit 1 |
| run | The fake `log` fails (B3 after B2) | B3 KO with the reason; the later box steps stopped; exit 1 |
| run | The fake `identify` exits 3 | B1 KO, the box steps stopped, no `flash` in the fake's log |
| run | A builder that fails for `base` | P3 KO; no `flash` in the fake's log; exit 1 |
| gestures | Without `--with-gestures` | H1 to H11 `skipped (needs a person: ...)` |
| gestures | `--with-gestures` with no terminal | H1 to H11 `skipped (no terminal)` |
| report | Any run | the last line is the verdict with the log path; `session.log` exists in a folder of mode 0700 |
| secrets | A nominal pass where the account is set up | the password is in `SOLITE_PASSWORD_FILE` (0600; `$HOME/.config/solite/box-password` by default in a box run), its path on the verdict line, the password itself in no output line; no `update.key` left in the run folder |
| auth group | Sim host with no account (`--api-only`) | status, setup (password file), login timed, 401 without token, `/ws` 4001 and a message with the token, permit-join 401, then 202 `permitJoin: false` with `{"time":0}`, the 429 at attempt 11 of the minute (setup and timed login counted): all OK |
| auth group | Sim host with an account, `SOLITE_PASSWORD` unset | KO `set SOLITE_PASSWORD (the box has an account)` |
| home group | Sim host (`--api-only`) with `SOLITE_HOME_LATITUDE=45.0`, `SOLITE_HOME_LONGITUDE=5.0`, `SOLITE_HOME_TIMEZONE=Europe/Paris` (synthetic) | backup round trip 200, settings PUT 200 and read back, sun times present, time zone `Europe/Paris` `env`, dates OK; busy and stopped reader `skipped (box only)` |
| home group | Sim host, no `SOLITE_HOME_*`, no location in the settings | settings put back 200; sun times `skipped (set SOLITE_HOME_LATITUDE, ...)`; time zone OK `UTC (fallback)` |
| update group | Fake server, run folder holding `location`, settings read back after the restart | check 4 OK; settings without the location: check 4 KO |
| update group | `--api-only`: P2 makes the key, P4 signs 16 bytes and flips them; the key is gone after the run | P2, P4 OK; no `update.key` left |
| home group | Fake server whose aggregation `sunrise` stays `null` | check 3 KO `clock not set within <n> s` (wait set to 1 s) |
| home group | A `mains` device whose `lastSeen` is 20 min old and `offline` | check 5 OK; the same device `online`: KO |
| auth group | Account exists, password file holds a wrong password | check 2 KO; B6 to B10 skipped (after B4: no account) |
| update group | Sim host, no key (`--api-only`) | 401 without token OK; check 2 `skipped (this build has no update key)`; 3 to 6 skipped |
| update group | Fake update server: install 200, version answers the next version, then 200 for the rollback image, then the next version again | all six OK, with the waits set to a second |
| update group | Fake update server whose version never changes | check 4 KO (`the box did not come back with <x.y.z> within <n> s`) |
| update group | Fake server answering 409 `The running image is not confirmed yet` to the rollback image | check 5 KO |
| options | `--factory-reset` alone | exit 2 |
| options | `--yes-flash` with no terminal (no pty) | exit 2, `typed by the user in a terminal` |
| run | `identify` exits 4 | B1 KO, the box steps stopped |
| run | Builder fails for `next` | P4 KO; B8 `skipped (after P4 KO)`, B9 `skipped (after B8 skipped)`; B10 (with `--factory-reset`) still runs |
| run | The version line cannot be found in the copy (builder seam left, a fake tree without it) | P4 KO naming the line |
| run | B4's serial log without `network: resumed` | B5 KO, the run goes on |
| run | `--address` given | the update group uses it, not the address `saves` printed |
| run | SIGTERM during a run | no `update.key` left in the run folder |
| auth group | Fake server answering 403 `confirmRequired` to setup | check 2 KO with the box's text; B6 to B10 `skipped (after B4: no account)` |
| auth group | `SOLITE_PASSWORD` set, host with no account | setup with it; `measure-api.sh` logs in with it |
| home group | Fake slow server (`--group home`, box mode): holds four bodyless requests and answers the 5th 503 `Busy`; a stopped reader | checks 6 and 7 OK; a server that answers 200 to the 5th: check 6 KO |
| home group | `SOLITE_PYTHON` naming a missing program | `/ws`, busy, stopped reader `skipped (no python3)` |
| update group | Fake server: rollback upload 200 but the old token stays valid (no restart) | check 6 KO |
| reset group | Sim host, `--api-only --factory-reset` | 204, `setupRequired: true`, setup again 201 |
| measure | `--api-only` on the garage (relay paired) | B6's equivalent OK (`measure-api.sh` exit 0) |
| validate | `fw_size_line` with a fake image of 3.2 MB and a 4 MB slot | the line ends with the 75 % warning; 1 MB: no warning |
| unchanged | `box.sh` and `measure-api.sh` | not modified; their tests still pass |

## Validation

- [x] Validate command green with `--skip-slow` (the QEMU run, red on main for the heap ceiling, not run here)
- [x] Agent review done, blocking findings fixed (1 blocking, 6 should-fix fixed; re-review 0 blocking)
- [ ] Demo seen and approved by the user (the box run: next session)
