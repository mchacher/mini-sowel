# Architecture: Spec 025

## Overview

```
serial console line ──> core parse_console_line ──> SavesHold | SavesFlush
        (radio image: console.rs)                          │
                                                           v
                                         static SaveControl (radio_engine.rs)
                                                           │ read at each tick
engine loop ──> Saver::tick(home, now, stopping)  <────────┘
                  1. clock gate closed and not stopping: nothing
                  2. flush asked (taken once): forced write, hold ended, "saves: flushed, k files"
                  3. held and not stopping: nothing
                  4. definitions unsaved: wait DEFINITION_DELAY_MS from the first sight (if set)
                  5. write what is due (WritePolicy for the devices, flags for the rest)
                          │
                          v
                Storage (firmware FatStorage) ──> counted: "flash: n writes since boot"

box.sh saves: one port session: "saves hold" → wait "serving:" → command (SOLITE_BOX_ADDRESS)
              → "saves flush" → last "flash:" line → budget check
box.sh log --budget n: last "flash:" line of the log against n
```

The logic (when to write, hold, flush, delay, the lines) is in the core and tested on the Mac;
the firmware holds the statics and the counting at the storage adapter; the scripts parse lines.

## Data

| Name | Type | Where | Bound |
| ---- | ---- | ----- | ----- |
| `SaveControl { held, flush }` | two `AtomicBool` | core type; one `static` in `firmware/src/radio_engine.rs` | fixed size, RAM only, reset at boot |
| `WriteCounter` | `AtomicU32`, saturating | core type; one `static` in `firmware/src/storage.rs` | 4 bytes, stops at `u32::MAX` |
| `Saver::delay_ms` | `u64` | `crates/core/src/engine/saver.rs` | `DEFINITION_DELAY_MS` = 2 000 ms when set, 0 otherwise |
| `Saver::pending_since` | `Option<u64>` (ms, wall clock) | same | one value |
| `Saver::control` | `Option<&'static SaveControl>` | same | one reference |

Nothing new is stored on `/data`; the files keep their format.

`Saver::tick` separates two things a stop used to mean together: **forced** (write every file with
unsaved changes, values included, ignoring the hold, the delay and the retry time: a stop or a
flush) and **stopping** (a failure is the returned error, not a retry: a stop only). A flush is
forced but not stopping: its failures go to the sink and the retry path. The tick counts the files
it wrote for `flushed_line`. The delay reads the unsaved flags without taking them, through the
peeks that exist (`Home::changes()` for zones and equipments, `Home::recipes_changed()`,
`Devices::changes()` for the devices' definitions): no new method on `Home`.

## Files changed

| File | Change |
| ---- | ------ |
| `crates/core/src/engine/saver.rs` | `DEFINITION_DELAY_MS` (with its bound), `SaveControl`, `flushed_line`, `Saver::with_definition_delay`, `Saver::with_control`; `tick` gains the flush, the hold and the delay; the module comment |
| `crates/core/src/engine/storage.rs` | `WriteCounter` and `flash_line` |
| `crates/core/src/engine/mod.rs` | re-exports |
| `crates/core/src/devices/persist.rs` | `due_stream`'s `stopping` read as "forced" (name only); the module comment |
| `crates/core/src/network.rs` | `ConsoleCommand::SavesHold`, `SavesFlush`; `parse_console_line`; `HOLD_LINE`, `FLUSH_ASKED_LINE` |
| `crates/core/tests/engine.rs` (or a new `crates/core/tests/saves.rs`) | the saver's scenarios (fake clock = the `now` passed, counting storage = `MemStorage::attempts`) |
| `crates/core/tests/network.rs` | the console lines |
| `firmware/src/storage.rs` | `static WRITES: WriteCounter`; counted in `FatStorage::write`, `rename`, `remove`, `touch` (new, the marker) |
| `firmware/src/radio_engine.rs` | `static SAVES: SaveControl`; the saver `with_control(&SAVES)` and `with_definition_delay(DEFINITION_DELAY_MS)`; the marker through `storage::touch` and `storage::remove` |
| `firmware/src/console.rs` | the two lines acted on; the module comment |
| `firmware/Cargo.toml` | an empty `[workspace]` table: the firmware is its own workspace also in an agent's worktree nested in the repository (Cargo took the outer workspace there, so `validate.sh` and the commit hook could not build the images); nothing changes in the main checkout, where the root workspace already excludes it |
| `crates/host/src/serve.rs` | the saver `with_definition_delay(DEFINITION_DELAY_MS)` |
| `scripts/box.sh` | `saves` sub-command (console mode `saves`, `--` command), `--budget`, `--no-flush`; `log --budget`; header and usage |
| `scripts/lib/firmware.sh` | `fw_flash_writes <log>`: the last `flash:` count, 0 when none |
| `crates/host/tests/box_script.rs` | a fake console that answers line by line; `saves` and `--budget` tests |
| `scripts/measure-api.sh` | header comment and usage line (R5.2) |
| `crates/host/tests/measure_script.rs` | the usage line names the Mac rule |
| `docs/living-spec/README.md`, `docs/decisions.md` (D39 finalised, D40), `docs/roadmap.md`, `docs/specs-index.md`, `docs/architecture.md`, `CLAUDE.md`, `README.md`, `.claude/skills/solite-feature/SKILL.md` | as the spec says |

## What is reused

- The saver's gate (`with_gate`, spec 023) and its early return: the hold is the same idea, kept
  apart from the clock gate so that a flush still waits for the clock.
- `WritePolicy::due_stream(.., stopping)`: a flush asks it as a stop does (values included).
- The console path of `pair` (spec 023, D36): a line parsed in the core, a static flag set by
  `console.rs`, taken by the engine's thread.
- `box.sh`'s `console_exchange` (Python over pyserial, fake console through
  `SOLITE_SERIAL_CONSOLE`, `console: ready` wait, log prefix stripped) and its tests' `Env`.
- `MemStorage` (`crates/core/tests/mem_storage`) counts the writes (`attempts`).
