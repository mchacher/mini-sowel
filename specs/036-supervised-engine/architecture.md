# Architecture: Spec 036

## Overview

```
boot (radio image)
  esp_reset_reason ─┐
  RTC record words ─┴─> core supervisor::Supervisor::boot ──> Mode::Normal | Mode::Safe, boot line,
                                                              status -> Home::set_supervision (API)
Normal: engine thread (core 1)
  esp_task_wdt_add ─> load home ─> radio start ─> engine::run (feeds each turn, Saver feeds per file)
                                   │ radio error ─> serial::restart (feeds per step) ... Stopped
                                   └ error ─> save once ─> Supervisor::incident(reason) ─> record ─> esp_restart
Safe: no engine thread; home loaded (or empty) and served; OTA thread and probation as usual
OTA thread's 1 s tick: Supervisor::tick (stored every 10 s) ─> record clock in RTC; safe 24 h ─> retry restart
console restart / restart_after_update / roll_back ─> Supervisor::asked ─> record ─> esp_restart
```

## Data

- `supervisor::Record` (core): `clock_ms: u64`, `safe_since: Option<u64>`, `pending: Option<Reason>`,
  `last: Option<Reason>`, `restarts: VecDeque<u64>` (bound `MAX_KEPT` = 6), `boots: u32`, `safe_entries: u32`.
  Encoded as `[u32; RECORD_WORDS]`, `RECORD_WORDS` = 24 (96 bytes): 0 magic `0x5353_5631`
  ("SSV1"), 1 version (1), 2 pending code (0: none), 3 last code (0: none), 4-5 clock (lo, hi),
  6-7 safe_since (lo, hi; all ones: none), 8 restart count (at most 6), 9-20 six restart times as
  (lo, hi) pairs, 21 boots since the power-on, 22 safe-mode entries since the power-on,
  23 checksum (FNV-1a over words 0 to 22).
- `supervisor::Reason` (core): enum with the codes and texts of spec R1; `counts()`.
- `supervisor::Status` (core, `Serialize`, camelCase): `mode`, `bootCause`, `lastIncident`,
  `restartsLastHour`, `restartsLastDay`. Held by `Home` (`supervision`, default normal/power-on).
- Constants: `RESTARTS_PER_HOUR` 3, `RESTARTS_PER_DAY` 6, `HOUR_MS`, `DAY_MS`,
  `SAFE_RETRIES_MS` = [1 h, 4 h, 24 h], `BARE_AFTER` 3, `TICK_STORE_MS` = 10 000,
  `WATCHDOG_TIMEOUT_MS` 20 000, `LONGEST_STALL_MS` 3 800.
- `supervisor::Mode`: `Normal`, `Safe`, `SafeBare`.
- RTC: one `static` of 24 `AtomicU32` in `.rtc_noinit` (firmware), read and written with plain
  `load` and `store` only, never a read-modify-write (`fetch_*`, `compare_exchange` compile to
  `S32C1I`, which works on internal SRAM only). One `Mutex<Supervisor>` in internal RAM guards
  the writers.

## Files changed

| File | Change |
| ---- | ------ |
| `crates/core/src/supervisor.rs` (new) | `Reason`, `Mode`, `Record` (encode/decode, checksum), `Supervisor` (`boot`, `tick`, `incident`, `asked`, `status`, `mode`, lines), `last_rung(supervisor, adapter, reason, detail, stop, save)`, `SupervisorAdapter` trait, constants |
| `crates/core/src/lib.rs` | `pub mod supervisor;` |
| `crates/core/src/engine/saver.rs` | `with_feed(Box<dyn FnMut()>)`, `feed()`; feed before each file written |
| `crates/core/src/engine/mod.rs` | `run` calls `saver.feed()` at each turn |
| `crates/core/src/radio/serial.rs` | `RestartSteps.feed`: called before each step; `Restarted::Silent` split from `Stopped` (no speed answers); comment "keeps the ESP32 up" corrected |
| `crates/core/src/home/mod.rs` | `supervision` field, `set_supervision`, `supervision()` |
| `crates/core/src/api/system.rs`, `api/mod.rs` | `GET /system/supervisor` (405 otherwise), route list; the safe-mode gate (503 on a non-`GET` outside the listed routes) |
| `docs/sowel-subset.md` | the route's row and its deviation (D58) |
| `crates/core/tests/supervisor.rs` (new) | ladder, budget, safe mode, retry, record, feeds |
| `crates/core/tests/radio_serial.rs`, `engine.rs`, API test | `feed` field in `RestartSteps` callers; feeds counted; route test |
| `firmware/src/supervisor.rs` (new, radio image) | RTC record, adapter (load/store via RTC, feed `esp_task_wdt_reset`, restart `esp_restart`), static `Mutex<Supervisor>`, `boot()`, `asked_restart()`, `tick()` (called by the OTA thread), `subscribe()`/`unsubscribe()` with the subscribed flag |
| `firmware/src/radio_engine.rs` | subscribe, feed in `Link::read` and the ladder's steps, reasons on each error path, `last_rung` called; doc comments "stays up" corrected |
| `firmware/src/main.rs` | supervisor boot before the engine; safe mode path (home loaded or empty, sent to `wifi::run`, `STOPPED` set) |
| `crates/core/src/network.rs` (console parse) | `ConsoleCommand::Panic` for the line `panic`, with its test |
| `firmware/src/console.rs`, `firmware/src/ota.rs` | `asked` before `esp_restart`, before the rollback; the console's `panic` line (R12); the OTA thread ticks the supervisor |
| `firmware/sdkconfig.radio.defaults` | `CONFIG_ESP_TASK_WDT_TIMEOUT_S=20`, `CONFIG_ESP_TASK_WDT_PANIC=y` |
| `scripts/validate.sh` | checks the radio build has the two settings and the box image does not have the panic one |
| `scripts/box-session.sh` + its test | R11, only if small |
| Docs | living spec, memory budget, decisions D58, roadmap, specs-index, architecture |

## Reused

- `radio::serial::ResetReason` and the firmware's `reset_reason()` mapping (boot cause).
- `Restarts` / `restart` ladder (spec 021): unchanged rules, one more field.
- `Saver::with_*` builder pattern (spec 023, 025) for `with_feed`.
- `ota.rs`'s one-second tick, which already runs the probation, for the supervisor's tick.
- `api/system.rs` routing and `json(200, ...)` (spec 032's `version`).
