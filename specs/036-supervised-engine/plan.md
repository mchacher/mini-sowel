# Plan: Spec 036

## Steps

- [x] 1. Living spec delta applied (inbox lines also: presence counted from the last power-on across restarts, so a restore is a way out of the safe mode) (`docs/living-spec/README.md`: "Talks to the radio" sentences,
      new "Supervises its engine" section, the probation note); `docs/memory-budget.md` row;
      decision D58; `docs/architecture.md` (the supervisor module, its tick on the OTA thread); roadmap
      (inbox lines: notification of a restart, a supervisor card, Sowel's `/health`; the box test
      in "Tests to run with the box in hand"; a log line); specs-index row; sentences made false
      searched (`stays up`, `watchdog`, `task_wdt`, `esp_restart`, `restart ladder`) and corrected.
- [x] 2. Core: `supervisor.rs` (reasons, record codec, supervisor, adapter trait, constants,
      status JSON); `Home::supervision`.
- [x] 3. Core: `Saver::with_feed` and feeds; `engine::run` feeds each turn; `RestartSteps.feed`.
- [x] 4. Core: `GET /api/v1/system/supervisor` in `api/system.rs` and the route list.
- [x] 5. Host: nothing but the `feed` field where it builds `RestartSteps` (if it does); the API
      answers the default status.
- [x] 6. Firmware (radio image): `supervisor.rs` (RTC record in `.rtc_noinit`, adapter, ticked by the OTA thread, `asked_restart`, watchdog subscribe/feed/unsubscribe); `radio_engine.rs`
      (reasons, last rung, feeds); `main.rs` (boot decision, safe mode); `console.rs` and `ota.rs`
      (`asked` before each restart and the rollback; the console's `panic` line); `sdkconfig.radio.defaults`; feed in `Link::read`.
- [x] 7. `scripts/validate.sh`: the radio build has `CONFIG_ESP_TASK_WDT_PANIC=y` and
      `CONFIG_ESP_TASK_WDT_TIMEOUT_S=20`; the box image (`target/xtensa...`) has not the panic one.
- [x] 8. `scripts/box-session.sh` R11, only if a few lines with its fake; else the roadmap item says
      it stays a person's. Decision: skipped. A `GET /system/supervisor` check would add a numbered
      check to a group and a route to the script's fake in `box_session_script.rs`, which changes
      the expected output of several tests: more than a few lines. The roadmap item stays a
      person's, done with the box in hand.
- [x] 9. `sh scripts/validate.sh` green, QEMU included.

## Test plan

| Module | Scenario | Expected |
| ------ | -------- | -------- |
| supervisor (`crates/core/tests/supervisor.rs`, fake adapter) | Power-on boot with garbage in the record | Cause `power-on`, mode normal, record fresh (0 restarts), stored |
| supervisor | Incident `radio-stopped` then boot (reset reason software) | `restart()` called once; next boot cause `radio-stopped`, last incident `radio-stopped`, 1 restart in the hour |
| supervisor | Each incident reason (radio-stopped, radio-silent, radio-start, engine-error) | Pending code in the stored words; one restart each; the line names the reason and the detail |
| supervisor | Boot with no pending reason, reset reason panic / watchdog / software / other | Cause `panic` / `watchdog` / `unexpected` / `unexpected`, counted |
| supervisor | Boot after a brown-out, 3 times in an hour | Cause and last incident `brown-out`, not counted, mode normal |
| supervisor | 3 counted crashes while in safe mode | Bare safe mode; 2: safe mode |
| supervisor (`last_rung`) | Error, save succeeds | Save called once, reason pending in the words, `restart()` once, the line |
| supervisor (`last_rung`) | Error, save fails | The save's error logged, `restart()` still called once |
| supervisor (`last_rung`) | `STOP` set | No save, no write, no restart |
| supervisor | 2 counted restarts within an hour | Normal |
| supervisor | 3 counted restarts within an hour | Safe mode; the safe-mode line |
| supervisor | 3 counted restarts spread over 3 hours (one per 61 min) | Normal (hour window), 3 in the day |
| supervisor | 6 counted restarts in a day, spread (one per 2 h) | Safe at the 6th; 5 spread: normal |
| supervisor | Restart times older than 24 h | Pruned; at most 6 kept (bound: a 7th drops the oldest) |
| supervisor | Crash (panic) while in safe mode | Stays safe |
| supervisor | First safe mode, tick at 1 h - 1 ms then at 1 h | No action, then one restart with pending `safe-mode-retry`; next boot normal, 0 restarts, not counted |
| supervisor | Second and third safe-mode entries since the power-on | Retried after 4 h, then 24 h (and 24 h for every later one) |
| supervisor | Restart asked (`asked`) in safe mode | Next boot normal, restart times cleared |
| supervisor | Power-on after safe mode | Normal, fresh |
| supervisor | Tick every 10 s | Clock stored at most every `TICK_STORE_MS`; next boot's clock continues from the stored value |
| supervisor | Record round trip with every field set (6 times, safe_since, pending, last, boots) | Decoded equal |
| supervisor | Corrupt record: magic, version, checksum, unknown reason code, count > 6 | Ignored: fresh record |
| supervisor | Status JSON | `{"mode","bootCause","lastIncident","restartsLastHour","restartsLastDay"}`, camelCase, `null` when none |
| saver (`crates/core/tests/supervisor.rs` or `engine.rs`) | Every file dirty, with a feed | One feed before each file written (feeds = files written) |
| saver | No feed set | Saves as before (existing tests unchanged) |
| engine (`engine.rs`) | `engine::run` on the simulated radio for N turns with a feed | Feeds ≥ turns |
| radio serial (`radio_serial.rs`) | Restart ladder | A feed before each step (save, reset, probe, listen); existing ladder tests unchanged |
| radio serial | No speed answers in the ladder / 4th restart refused | `Restarted::Silent` / `Restarted::Stopped` |
| figures | `WATCHDOG_TIMEOUT_MS` ≥ 5 × `LONGEST_STALL_MS`, and > the radio's `READ_SLICE_MS` | Holds |
| API (`crates/core/tests/` API tests) | `GET /api/v1/system/supervisor` with token | 200 and the default status; after `set_supervision`, that status |
| API | `POST` / `PUT` on it | 405 |
| API | Without a token | 401 |
| API | Safe mode: `POST /api/v1/zones`, `PUT /api/v1/settings` | 503 `Safe mode: the engine is stopped` |
| API | Safe mode: `GET /api/v1/zones`, `POST /api/v1/auth/login` | As in normal mode |
| API | Safe mode: `POST /api/v1/system/factory-reset` | 503 |
| console parse (core network tests) | Line `panic` | `ConsoleCommand::Panic`; other lines unchanged |
| contract (`sowel_contract.rs`) | The new route | Listed in `docs/sowel-subset.md` as Solite's own |
| update (`crates/core/tests/` update tests) | Spec 032's probation and rollback | Unchanged, passing |

## Validation

- [x] Validate command green (QEMU included)
- [x] Agent review done, blocking findings fixed
- [x] Demo: tests and the route on the Mac; the box run is in the roadmap's list (D39)
