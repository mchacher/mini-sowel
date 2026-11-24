# Plan: Spec 025

## Steps

- [x] 1. Living spec updated (the delta, History row 025); D39 finalised and D40 in
      `docs/decisions.md`; roadmap (inbox line removed, box-check line added, Done entry);
      `docs/specs-index.md` row 025.
- [x] 2. Core types: `SaveControl`, `WriteCounter`, `ConsoleCommand::SavesHold` and `SavesFlush`
      (no Sowel type: none mirrored).
- [x] 3. Core logic: the tests of the plan first, then `Saver::tick` (flush, hold, delay),
      `flash_line`, `flushed_line`, `parse_console_line`.
- [x] 4. Adapters: host `--serve` with the delay; firmware: counter in `storage.rs`, the marker
      through it, `SAVES` and the saver's options in `radio_engine.rs`, the console's two lines.
- [x] 5. API or WebSocket: not applicable.
- [x] 6. Interface: not applicable.
- [x] 7. Scripts: `box.sh saves`, `box.sh log --budget`, `fw_flash_writes`; `measure-api.sh`'s help;
      their tests.
- [x] 8. Docs: `docs/architecture.md`, `CLAUDE.md`, `README.md`, the feature skill's sentence.
- [x] 9. `sh scripts/validate.sh` green (four images, QEMU).
- [x] 10. Code review (`code-reviewer`); the demo is the Mac run (the user is away).
- [ ] 11. The box: left for later (roadmap line); nothing flashed in this change.

## Test plan

### Rust, core: the saver (`crates/core/tests/saves.rs`, new)

The clock is the `now` passed to `tick` (a fake clock); the storage is `MemStorage`, which counts
the writes (`attempts`) per call and keeps the text per file.

| Module | Scenario | Expected |
| ------ | -------- | -------- |
| delay | Nominal burst: 33 equipment changes (an equipment, 16 data and 16 order bindings) at `t`, `t+50`, ..., all within 2 s, ticking after each | no write before `first + 2000`; at `first + 2000` exactly one write of `equipments.json`, holding the 33 changes |
| delay | A change 2 s after that write | a new write, 2 s after the change (2 writes in all) |
| delay | A stream of changes every 500 ms for 5 s | writes every 2 s or so (2 or 3), never none until the stream ends |
| delay | A stop during the wait | the file written at once, by the stop's tick |
| delay | A write that fails at the end of the wait | shown on the sink; not tried again before `RETRY_MS`; written at the retry |
| delay | The clock goes back 1 hour while a change waits | written within 2 s of the new time, not an hour later |
| delay | A change while the clock gate is closed, then the gate opens | the wait starts when the gate opens: no write before 2 s after it |
| delay | A delay of 0 | a definition change written at the next tick (the rollback) |
| delay | Values only (no definitions) after 10 minutes | written at once at the interval, as before (the delay applies to definitions only) |
| delay | A device's definitions (paired, renamed) | `devices.json` written after the delay |
| unchanged | A `Saver` without the delay | a definition change written at the next tick (the existing tests of `engine.rs` keep passing) |
| hold | Held: a zone, an equipment, a recipe and a device changed, ticks over 20 minutes | nothing written; every file still due |
| hold | Flush after the hold | one write per changed file (4), none for a file with nothing unsaved; the hold ended; `saves: flushed, 4 files written` from `flushed_line` |
| hold | After the flush, a new change | written by the normal rule (after the delay when set) |
| hold | Flush with nothing unsaved | no write; the hold ended |
| hold | Flush asked while the clock gate is closed | nothing written; once the gate opens, the flush's writes, then the hold ended |
| hold | Stop while held | the files written at once |
| hold | Flush whose zones write fails | the failure on the sink, the other files written, `flushed_line`'s count is 3, the hold ended, the zones tried again after `RETRY_MS` |
| gate | Values changed, the clock gate opens (the saver started in 1970) | `devices.json` not written at the opening, but 10 minutes after it |
| hold | Hold twice, flush twice | one hold; the second flush writes only what changed since the first |
| counter | `WriteCounter::record` counts 1, 2, 3; set near the maximum it stops at `u32::MAX` | as said; `flash_line(3)` is `flash: 3 writes since boot` |

### Rust, core: the console lines (`crates/core/tests/network.rs`)

| Module | Scenario | Expected |
| ------ | -------- | -------- |
| console | `saves hold`, `saves flush`, with and without `\r` | `SavesHold`, `SavesFlush` |
| console | `saves`, `saves hold now`, `saves stop`, `saves  hold`, `saves hold ` (trailing space), `SAVES HOLD` | `Unknown` |
| console | `pair` and `wifi-forget` | unchanged |

### Shell, `box.sh` (`crates/host/tests/box_script.rs`, fake console)

| Module | Scenario | Expected |
| ------ | -------- | -------- |
| saves | Nominal: hold answered, `serving:` seen, command, flush answered with `flash:` lines | lines sent `saves hold` then `saves flush`; the command ran with `SOLITE_BOX_ADDRESS=192.168.1.20`; prints `box: saves held; serving at http://192.168.1.20/`, `box: saves flushed (2 files written)`, `box: flash: 3 writes since boot`; exit 0 |
| saves | `--budget 2` with 3 writes | `box: flash: 3 writes since boot, over the budget of 2`, exit 1 (flush still sent) |
| saves | `--budget 3` with 3 writes | exit 0 |
| saves | `--no-flush` | only `saves hold` sent; `box: saves still held: ...`; exit 0 |
| saves | A silent console | `box: no answer from Solite's console on ...`, exit 4 |
| saves | No `serving:` line | `box: the box serves nothing within ...`, exit 1; the command not run; no flush sent |
| saves | The command fails (exit 3) | flush still sent; `box: the command failed (exit 3)`, exit 1 |
| saves | A `flash: 1 writes since boot` line before `saves: held` and none after | counted: `box: flash: 1 writes since boot` |
| saves | A second `console: ready` after the hold | `box: the box restarted during the session ...`, exit 1, no flush sent |
| saves | SIGTERM to the script while the command runs | the command stopped, `saves flush` sent, exit 130 |
| saves | A command that cannot start | `box: cannot run the command`, the flush sent, `exit 127`, exit 1 |
| saves | `flush asked` but no `flushed` | `box: the console took the line but the engine flushed nothing within ...`, exit 1 |
| saves | No command and no terminal | exit 2, `box: saves needs a command after -- or a terminal`; the console never started |
| log | `--budget 4` on a log whose last `flash:` line says 4 | the line printed, exit 0 |
| log | `--budget 3` on the same log | `box: flash: 4 writes since boot, over the budget of 3`, exit 1 |
| log | `--budget 0` on a log with no `flash:` line | `flash: 0 writes since boot`, exit 0 |
| log | `--budget x` | usage error, exit 2 |
| unchanged | `log` without `--budget`, `pair`, `wifi` | as before (their tests keep passing) |

### Shell, `measure-api.sh` (`crates/host/tests/measure_script.rs`)

| Module | Scenario | Expected |
| ------ | -------- | -------- |
| measure | Against the host (existing test) | 33 answered 201, nothing left behind (unchanged, now with the delay in `--serve`) |
| measure | No argument | the usage line names the Mac and `box.sh saves` |

### Host (`crates/host/tests/serve.rs`)

| Module | Scenario | Expected |
| ------ | -------- | -------- |
| unchanged | What reads the data folder shortly after a request, with the 2 s delay in `--serve`: `serve.rs` "a rename reaches the file" (polls 10 s), "zones.json on disk" (polls 10 s), the tests that read the files after Ctrl-C (a stop writes at once); `sim.rs`'s file list (after the host ends); `scripts/demo-garage.sh` and `tools/e2e` read the API, never the files | as before |

## Validation

- [x] Validate command green (Rust and interface here; firmware images and QEMU in CI)
- [x] Agent review done, blocking findings fixed
- [x] Demo: the Mac run of "How the user will check it" (the user is away)
