# Spec 036: The supervised engine

## Context

The true north asks for a box that runs a month in an annex without intervention. Two gaps stand in
the way (inventory of 2026-11-25, section "Resilience over a month"):

1. **The engine can stop for good while the box stays powered.** After 3 restarts of the radio in
   10 minutes, a radio that answers at no speed, or a storage error, the radio image's engine
   thread ends and the ESP32 stays up doing nothing (`firmware/src/radio_engine.rs`, the
   `Restarted::Stopped` and `RunError::Other` branches; "the ESP32 stays up" in the living spec).
   Nobody is there to unplug it.
2. **Nothing watches the engine thread.** ESP-IDF's task watchdog watches only the two idle tasks,
   with a 5 s timeout that only logs (`CONFIG_ESP_TASK_WDT_PANIC` is off); a search for
   `esp_task_wdt_add` finds nothing. An engine stuck in a loop or a wait that never ends is a box
   that looks alive and does nothing.

This spec adds a supervisor: the last rung of the engine's restart ladder is a restart of the
whole box, with the reason kept across the restart; a boot-loop guard bounds those restarts and,
past its budget, starts the box in a **safe mode** that keeps the way in (Wi-Fi, API, update) with
the engine stopped; the engine thread is subscribed to the task watchdog, which now restarts the
box.

Decided in an autonomous run (the user away): every choice below is recorded in D58 for the
user's review.

## Goals

1. An engine that ends on an error restarts the box (after the radio's own restarts), never stays
   idle.
2. An engine thread that stops feeding the task watchdog for 20 s restarts the box.
3. The reason of each restart is read at the next boot, logged and served by the API, without
   writing the flash.
4. A persistent fault never makes the box restart in a loop: at most 3 counted restarts in an hour
   and 6 in a day, then safe mode, retried once a day.
5. In safe mode the API and the update route work, so a bad image can be replaced; a new image
   that cannot run its engine still rolls back (spec 032).
6. The ladder's logic lives in the core, behind an adapter, tested on the Mac.

## Out of scope

- A notification of a restart or of the safe mode (Telegram, web push, Sowel): inbox line, needs a
  notification channel first.
- A Settings card in the interface that shows the supervisor's status: inbox line (the API route
  exists).
- Sowel's `GET /api/v1/health`: inbox line (its anonymous liveness shape, for an uptime monitor).
- Writing the reason to the flash (NVS or a file): the RTC memory is enough for a restart, and a
  power cut is itself the reason (D58).
- The host program (Mac): it has no restart of its own; the API answers its default status.
- The image QEMU runs (feature `sim`) and the box's image without radio: unchanged; only the radio
  image is supervised (it is the one that runs in an annex).
- Changing the panic strategy: the target `xtensa-esp32-espidf` already aborts on a panic (its
  `panic-strategy` is `abort`), and ESP-IDF's panic handler prints and restarts the chip
  (`CONFIG_ESP_SYSTEM_PANIC_PRINT_REBOOT`). That is the policy kept (D58, R7).

## Requirements

### R1: Restart reasons

The core names why the box restarted, as a short code (stable number, kept in RTC memory, and a
text for the log and the API):

| Code | Text | Counted | When |
| ---- | ---- | ------- | ---- |
| 1 | `radio-stopped` | yes | The radio's restart ladder refused a 4th restart in 10 minutes |
| 2 | `radio-silent` | yes | The radio answers at no speed (at the start or in the ladder) |
| 3 | `radio-start` | yes | Any other radio failure before the loop runs (reset, identify, network start) |
| 4 | `engine-error` | yes | The home cannot be loaded, or the loop ended on a storage or data error (`RunError::Other`) |
| 5 | `panic` | yes | The previous run ended on a panic (ESP-IDF's reset reason) |
| 6 | `watchdog` | yes | A watchdog reset the chip (task, interrupt or other watchdog) |
| 7 | `brown-out` | no | The supply dropped (brown-out or power glitch): shown, not counted (a restart does not mend a power supply, and the safe mode would only stop the automations) |
| 8 | `unexpected` | yes | A software or other reset that nothing of Solite asked for |
| 9 | `asked` | no | A restart asked on purpose: the console's `restart`, an installed update, a restore, an update's rollback |
| 10 | `safe-mode-retry` | no | The retry of the safe mode (R5) |

A power-on (power, reset button, USB reset) is not a restart: it is the boot cause `power-on`,
and it clears the record (R3).

### R2: The last rung of the ladder

On the radio image, when the engine thread ends because of an error, the box restarts:

1. the radio's own ladder runs as today first (at most 3 restarts of the radio in 10 minutes); its
   end now says which: `Restarted::Stopped` (the 4th restart refused: `radio-stopped`) or
   `Restarted::Silent` (no speed answers: `radio-silent`);
2. when the ladder ends, when the loop ends on another error (`engine-error`), or when the start
   fails (`radio-start`, `engine-error`, `radio-silent`), the core's `supervisor::last_rung` runs:
   if a stop was asked meanwhile (`STOP`, set by the console or an update), it does nothing (the
   asked restart wins, R6); otherwise it saves what is due once (the same save a stop does: only
   files with unsaved changes; an error is logged and does not stop the restart), writes the reason
   pending into the RTC record, logs `supervisor: restarting the box after <reason>: <detail>`, and
   calls the adapter's `restart()`;
3. the firmware's `restart()` waits 1 s (the log drains), then `esp_restart`.

A stop asked by the console or by an update (`STOP`) is not an error: no restart from the engine
(the caller restarts, R6).

### R3: The record kept across a restart

The record lives in the ESP32's RTC slow memory, in a section that the boot does not initialise
(`.rtc_noinit`): it survives a software restart, a panic and a watchdog reset, and is garbage
after a power-on. It holds 24 words of 32 bits (96 bytes): a magic number, a version, the reason
pending, the last reason, the supervisor clock, the start of the safe mode, the times of
the last counted restarts (at most 6), the number of safe-mode entries and of boots since the
power-on, and a checksum. A record whose magic, version, checksum, reason code or count is wrong
is ignored (a fresh record). **No flash write**: the record never goes to NVS or the data
partition.

The supervisor clock counts the milliseconds the box has run since the last power-on, across
restarts: the record keeps the clock's value, written every 10 s by the supervisor's tick, and
each boot starts from it (so a crash loses at most 10 s of it). The windows of R4 and R5 run on it
(the wall clock is not set at boot).

### R4: The boot-loop guard

At each boot the supervisor reads the record and the reset reason, and decides the boot cause:

- a power-on: cause `power-on`, the record starts fresh (no restart counted, no safe mode);
- a reason pending in the record (written by R2, R5 or a restart asked): that reason;
- otherwise, from ESP-IDF's reset reason: `panic`, `watchdog`, `brown-out`, or `unexpected`.

Every reason but `asked` and `safe-mode-retry` becomes the last incident; a counted reason also
adds the boot's clock to the restart times; an `asked` or `safe-mode-retry` cause clears the
restart times and ends the safe mode. Then:

- with 3 counted restarts or more within the last 60 minutes of the supervisor clock, or 6 or more
  within the last 24 hours, the box enters **safe mode**;
- a box already in safe mode stays in it (a crash in safe mode does not end it); with 3 counted
  restarts or more since the safe mode began, it is the **bare safe mode** (R5): the home's files
  are not even read.

The boot logs one line: `supervisor: boot after <cause>, <n> restarts in the last hour, <m> in the
last day` (plus `, safe mode` or `, bare safe mode`), and, on entering safe mode,
`supervisor: safe mode: the engine stays stopped, the API and updates stay up; retried in <t>`.

### R5: Safe mode

In safe mode the radio image starts its console, its update thread, the Wi-Fi station and the
server as usual, and:

- the engine thread does not start: no radio reset, no radio loop, no saver, no boot marker, no
  pairing window, no recipe started;
- the home is loaded from the data partition and served, so the user can log in and install an
  update; a home that cannot be loaded is replaced by an empty home in RAM
  and the log says it; in the bare safe mode (R4) the empty home is served without reading the
  files (a home whose load crashes cannot loop); the stored files are never touched;
- the API answers every request that is not a `GET` with 503 `{"error": "Safe mode: the engine is
  stopped"}`, except the account's routes (`/auth/*`) and `/system/update`: nothing is accepted
  that would never run or never be saved. A restore (`POST /system/backup`) and a factory reset
  need the presence of spec 029, which only a power-on gives, and a safe mode is never a
  power-on: they are not ways out of it (503). **The update is the way out over the network**;
  otherwise a USB flash. Presence counted from the last real power-on across restarts is an
  inbox line;
- the first-password setup follows the reset reason as today (spec 029): a safe mode is never a
  power-on, so it never opens; an empty home therefore cannot be logged into, and the way out is a
  power-on (which ends the safe mode, but a fault that persists brings it back) or a USB flash;
- the update route works; an image on probation is not healthy (the engine's loop does not run),
  so it rolls back after 5 minutes (spec 032, D54);
- the safe mode is retried: after 1 hour the first time since the power-on, 4 hours the second,
  24 hours from the third on (supervisor clock), the box restarts once with the cause
  `safe-mode-retry`, restart times cleared: the engine gets a new chance, and a fault that persists
  brings the safe mode back after 3 more restarts;
- it ends at a power-on, at a restart asked (an update, a restore, the console's `restart`), or at
  a retry.

Bound: in a day, a persistent fault gives at most 6 counted restarts plus the retries (at most 3
on the first day: 1 h, 4 h, then 24 h).

### R6: Restarts asked on purpose

Before every `esp_restart` that Solite asks for on purpose (the console's `restart`, the restart
after an update or a restore, the rollback of the probation), the firmware writes the pending
reason `asked` into the record. The next boot then counts nothing and ends a safe mode.

### R7: Panics

The policy kept: a panic aborts (the target's strategy), ESP-IDF prints the panic and restarts the
chip; the next boot reads the reset reason `panic` and counts it. No `catch_unwind` (it cannot
catch under `abort`), no change to `firmware/Cargo.toml`'s profile. `CONFIG_ESP_SYSTEM_PANIC_PRINT_REBOOT`
stays ESP-IDF's default. The dead branches that log `engine: the thread panicked` in
`firmware/src/main.rs` (a join that never sees a panic under `abort`) stay, commented as such.

### R8: The engine's watchdog

On the radio image:

- the task watchdog's timeout is **20 s** and a timeout **restarts the chip** (`CONFIG_ESP_TASK_WDT_TIMEOUT_S=20`,
  `CONFIG_ESP_TASK_WDT_PANIC=y`, in `sdkconfig.radio.defaults` only);
- the engine thread subscribes itself (`esp_task_wdt_add`) when it starts and unsubscribes
  (`esp_task_wdt_delete`) before it ends; the adapter's `feed()` does nothing unless the
  subscription succeeded and is still on (a flag), so no error line repeats every 50 ms;
- it is fed at every read of the radio's serial link (the firmware's `Link::read`: the core reads
  in slices of `READ_SLICE_MS` = 50 ms, so every radio wait, whatever its own timeout: the 5 s
  reset, the 5 s answer, the 10 s network start, the probe, a restore's network swap, feeds every
  50 ms), at each turn of the engine's loop, before each file the saver writes, and before each
  step of the radio's restart ladder;
- what blocks without reading the radio is a write to the flash: the longest known is the 3.8 s
  save of a 98 KB `devices.json` (spec 020, 025), one file, which cannot be fed in the middle.
  The core holds `LONGEST_STALL_MS` = 3 800 and `WATCHDOG_TIMEOUT_MS` = 20 000 (over five times
  it), and a test checks the ratio;
- the idle tasks of both cores stay watched with the same timeout and now restart the chip (after
  20 s instead of a log line after 5 s). What runs long on the other threads stays below it: the
  update's slot erase yields during the erase (`CONFIG_SPI_FLASH_YIELD_DURING_ERASE=y`, 20 ms
  slices), the backup's key derivation runs in slices of 5 000 iterations (spec 035), a login's
  hash uses the iterations the box itself wrote (`auth::ITERATIONS`, about 0.5 to 1 s, D47; a
  restore never carries the account, spec 035), and the restore applied at boot runs before the
  engine starts, on the main task, writing files one by one.

### R9: The status in the API

`GET /api/v1/system/supervisor` (Solite's own route, behind the token like the other `/system`
routes; D58) answers 200:

```json
{
  "mode": "normal",
  "bootCause": "radio-stopped",
  "lastIncident": "radio-stopped",
  "restartsLastHour": 1,
  "restartsLastDay": 1
}
```

`mode` is `normal`, `safe` or `safe-bare`; `bootCause` is a text of R1 or `power-on`; `lastIncident` is the
last reason other than `asked` and `safe-mode-retry` since the power-on (a brown-out included), or
`null`. The host program answers the default
(`normal`, `power-on`, `null`, 0, 0). Other methods: 405.

### R10: The adapter

The core defines the trait the firmware implements (the Mac's tests use a fake):

- `load() -> [u32; RECORD_WORDS]`: the words kept across a restart;
- `store(&[u32; RECORD_WORDS])`: writes them (RTC memory: no flash);
- `feed()`: feeds the engine thread's watchdog;
- `restart()`: restarts the whole box (never returns on the box; a fake records it).

### R11: The box session

`scripts/box-session.sh` reads `GET /api/v1/system/supervisor` with the token after its version
check and fails the step when the answer is not 200 or `mode` is not `normal`, only if that is a
few lines and its fake answers it in `crates/host/tests/box_session_script.rs`; otherwise the check
stays in the roadmap's list as a person's (decided when building, written in the plan).

### R12: A fault to check it on the box

The radio image's console takes one more line, `panic`, which panics on purpose (logged first:
`console: panic asked`). The line is parsed in the core with the others (`network::parse_console_line`,
`ConsoleCommand::Panic`); the deliberate `panic!` is the one exception to the rule of no panic
outside tests, recorded in D58. It is the box check's fault: one `panic` line typed on the serial console
restarts the box once with the cause `panic`, shown by the route. A person's check (the line is
typed in a serial terminal, `box.sh` sends no raw line).

## Living spec delta

### docs/living-spec/README.md

- "Talks to the radio": Changed: "No answer at any speed ... the engine does not start and the
  ESP32 stays up" → "... the engine does not start and the box restarts (`radio-silent`, see
  Supervises its engine)". "At most 3 restarts in 10 minutes; past that the image logs `radio:
  stopped after ...` and stays up. A storage error logs and stops the loop without resetting the
  radio." → "... past that it logs the line and restarts the box (`radio-stopped`). A storage
  error stops the loop without resetting the radio and restarts the box (`engine-error`)."
- New section "Supervises its engine" (after "Updates itself from a signed file"): R1 to R9 in
  business words: the reasons, the last rung, the record in RTC memory (no flash), the boot-loop
  guard (3 an hour, 6 a day), the safe mode and its daily retry, restarts asked, panics, the
  watchdog (20 s, fed by the loop, the saves, the ladder), `GET /api/v1/system/supervisor`.
- "Talks to the radio" (the console): New: the `panic` line (R12).
- "Serves Sowel's API from the box": New: in safe mode, every request but a `GET`, the account's
  and the update's answers 503 (R5).
- "Updates itself from a signed file": New: a supervised restart before the confirmation rolls the
  image back like any other restart; an image in safe mode is not healthy and rolls back.
- "Runs a box session in one command": the `task_wdt` line stays a failure of the log check
  (unchanged); the check of the route if the script adds it (R11).

### docs/memory-budget.md

- New row: the supervisor's record in RTC slow memory, 96 bytes (no heap, no flash); no new
  thread (the OTA thread's one-second tick runs the supervisor's tick).

### No longer true

- `docs/living-spec/README.md`: "the engine does not start and the ESP32 stays up" and "stays up.
  A storage error logs and stops the loop" → rewritten as above.
- `firmware/src/radio_engine.rs` doc comments: "the ESP32 stays up" (several) → the box restarts.
- `crates/core/src/radio/serial.rs` (`Restarted::Stopped`'s comment): "the caller stops the engine
  and keeps the ESP32 up" → the caller restarts the box.
- `docs/roadmap.md` inbox: none resolved by name; the two-core line mentions the idle-task
  watchdog on core 1, still true.
- Other names searched: `stays up`, `task_wdt`, `watchdog`, `esp_restart`, `restart ladder`,
  `STOPPED`; corrections listed in the plan's step 1 as found.

## Acceptance criteria

- [x] A test drives the ladder: a radio stopped, a silent radio, a start failure and an engine
      error each give one `restart()` of the adapter with its reason in the record.
- [x] A test shows 3 counted restarts in an hour enter safe mode, 2 do not; 6 in a day enter it,
      5 spread over the day do not; a power-on clears it; a restart asked ends it.
- [x] A test shows the daily retry: 24 h in safe mode give one `safe-mode-retry` restart, restart
      times cleared, not counted.
- [x] A test shows the record round trip (every field) and that a corrupt record (magic, version,
      checksum, out-of-range reason) is ignored.
- [x] A test shows the saver feeds before each file, the engine's loop at each turn and the
      ladder before each step; a test checks `WATCHDOG_TIMEOUT_MS` ≥ 5 × `LONGEST_STALL_MS`.
- [x] A test shows `last_rung` saves once, writes the reason and restarts, still restarts when the
      save fails, and does nothing when a stop was asked.
- [x] A test shows a brown-out is shown as the last incident and not counted; 3 crashes in safe
      mode give the bare safe mode; the retries come after 1 h, 4 h, then 24 h.
- [x] A test shows the API in safe mode: a `POST` of a zone answers 503, a `GET` and a login
      answer as usual.
- [x] A test shows the console line `panic` parses to `ConsoleCommand::Panic`.
- [x] Spec 032's probation tests pass unchanged.
- [x] `GET /api/v1/system/supervisor` answers the status's JSON; 405 on another method; 401
      without a token.
- [x] The radio image builds with the watchdog settings; the other images keep theirs (checked by
      `validate.sh`); the QEMU run stays green under its ceiling (D27 unchanged).

## Edge cases

- The RTC memory is garbage after a power-on: the record is ignored, whatever it holds.
- A crash during a supervised restart's save: the next boot reads `panic` or `watchdog` and counts
  it.
- A stop asked (console, update) at the moment the engine ends on an error: `last_rung` sees
  `STOP` and does nothing; the asked restart writes `asked`.
- A crash while the safe mode runs: the safe mode stays; 3 of them make it the bare safe mode.
- A safe mode whose home cannot be loaded: an empty home served, the account absent; the update
  route then needs the first-password setup, which only opens after a power-on (spec 029): the
  user unplugs the box (a power-on ends the safe mode) or flashes it over USB. Logged.
- An update being written when a supervised restart happens: the slot is not marked for boot
  (`esp_ota_end` never ran); the old image boots.
- The watchdog fires while a supervised restart's save runs (a save longer than 20 s): the next
  boot reads `watchdog` (the pending reason was not written yet, it is written after the save) and
  counts it.
- The OTA thread does not start (its own error): the supervisor is not ticked, its clock does not
  advance in this run, and a safe mode is not retried until a power-on; logged.
- An image on probation that restarts for any reason rolls back at that boot (the bootloader,
  spec 032); the old image then reads the record (or ignores it if it predates this spec).
- The supervisor clock wraps: never in practice (u64 milliseconds).
- More than 6 counted restarts in a day: only the last 6 times are kept (bound), which is all the
  guard needs.
- The watchdog cannot be subscribed (ESP-IDF error): logged, the engine runs unwatched.

## Sowel contract

None mirrored. `GET /api/v1/system/supervisor` is Solite's own route (like
`POST /system/factory-reset`, D48): Sowel has no supervisor. Sowel's `GET /api/v1/health` is not
taken (inbox). `docs/sowel-subset.md` gets one row in its route table and one in its
"Deviations" (citing D58), as `crates/core/tests/sowel_contract.rs` compares the routes. The 503
of the safe mode is Solite's too (D58).

## How the user will check it

On the Mac: `cargo test -p solite-core --test supervisor` (the ladder, the budget, the safe mode,
the record, the feeds), and `GET /api/v1/system/supervisor` on `--sim ... --serve` answering the
default status. On the box (roadmap, "Tests to run with the box in hand", one pass, D39), on the
radio image of this spec, flashed only with the user's yes (D6, D55): the route after the flash
shows `power-on`; one `panic` line typed on the serial console (R12) restarts the box once, and
the route then shows `bootCause` `panic` and one restart in the hour; the flash writes of that
incident are at most one save of the files due plus the boot marker written at the next boot
(`box.sh log`). The safe mode and the watchdog itself are not forced on the box (they would take
three restarts or a build that hangs): they are the Mac's tests.

## Review notes

Spec review round 1 (2026-11-25): two blocking items fixed (the watchdog is fed at every radio
read, so the radio's own waits never starve it, and the stall figure is the flash save's; the box
check's fault is the new console `panic` line, on an image flashed with the user's yes), and the
minor ones applied (`Restarted::Silent`, the contract rows, `last_rung` in the core with `STOP`,
the API in safe mode, the bare safe mode, the marker write per boot, the idle tasks' evidence).
Its questions for the user, decided in the autonomous run (D58, for the user's review):

- a brown-out is shown, not counted toward the safe mode;
- the safe mode is retried after 1 h, then 4 h, then 24 h;
- the safe mode is not shown in the interface in this spec (inbox line: a supervisor card or a
  banner); the route and the serial log show it.

Spec review round 2: one item fixed (a restore and a factory reset need a power-on's presence, so
they are not ways out of the safe mode: 503, the update is the way out; presence across restarts
goes to the inbox), minors applied (`lastIncident`, the feed flag, the console parse in the core
and D58's exception for the deliberate panic, plain loads and stores on the RTC words, the tick on
the OTA thread instead of a new thread, the index row's wording).

Code review (2026-11-25, opus): nothing blocking. Fixed: the served status is refreshed every
60 s by the supervisor's tick (the hour and day counts follow the clock); an engine thread that
cannot be started restarts the box (`engine-error`); a refused rollback takes back its `asked`
(`Supervisor::cancel_asked`), so a later crash still counts; `last_rung` checks the stop again
after its save. Left as notes: the safe-mode retry can cut an update being uploaded (the old image
boots, accepted in the edge cases); a reset in the microseconds of a record store makes the
checksum fail and the record starts fresh; a crash while the storage mounts or a pending restore
is applied happens before the supervisor boots (inbox line); `validate.sh`'s negative sdkconfig
checks pass when no file matches, like the existing ones (the same run builds those images).
