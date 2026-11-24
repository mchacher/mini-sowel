# Spec 025: flash-write-budget

## Context

The box keeps its home in JSON files on a FAT partition of its flash (`/data`), and every save
rewrites a whole file (a temporary file, synced, then renamed: spec 021). Flash wears with
writes. On the night of 2026-11-22 the tests on the real box caused hundreds of saves (eight runs
of `scripts/measure-api.sh`, 60 zone creations and deletions in a row), and the user asked
(2026-11-23) that the tests never age the flash: **a test on the real box causes at most one flash
write per file it changes, or one write to verify that it works**. Decision D39 was proposed by
spec 024 with that rule and is finalised here.

Today (`crates/core/src/engine/saver.rs`, `crates/core/src/devices/persist.rs`): the values and
`lastSeen` of the devices are written every 10 minutes when they changed; a change of definitions
(a device paired, renamed or deleted, a zone, an equipment, a binding, a recipe instance) is
written at the next turn of the engine's loop, so the 33 requests of `measure-api.sh` give about 33
saves of `equipments.json`. The saver already has a gate (`with_gate`, spec 023 R3.2: nothing is
written until the clock is set; a stop writes whatever the gate says).

The user is away from the box during this run: everything is built and tested on the Mac (host,
`--sim`) and under QEMU; the run on the box is described in "How the user will check it" and left
for later (a roadmap line).

## Goals

1. A test on the box can run with the saves held in RAM, and end with exactly one write per file
   that changed (or none), from the serial cable only.
2. Every flash write of the box is counted and shown, and a script fails when a run went over its
   budget.
3. In normal running, a burst of changes (the interface creating an equipment and its bindings, a
   device's interview) gives one write per file, not one per change; what a power cut can lose
   is stated and bounded.
4. The 33-request measure runs on the Mac against the simulator, never in a loop against the box.
5. The budget rule for box tests is written (D39 finalised) where the people and agents who run
   them read it.

## Out of scope

- A write counter on the Mac host (`FileStore`): the Mac's disk is not the box's flash; the host
  keeps its output as it is.
- Coalescing in the QEMU and measuring images (`sim`, `flash-saves`): the measuring image times
  every save as spec 020 defined it; only the radio image (the box in use) and the host's `--serve`
  coalesce.
- Reducing the size of each write (one file per device, a journal): the roadmap inbox line on
  `devices.json`'s 98 KB save stays.
- Holding the saves from the network (an API route): never, as for pairing (D36).
- Running anything on the box in this change (the user is away).

## Requirements

### R1: Saves held and flushed from the serial console (radio image)

1. The core reads two new console lines (`parse_console_line`, beside `pair`): `saves hold` and
   `saves flush`. `saves` with anything else, or with extra words, is an unknown command (as
   `pair now` is). A `\r` before the `\n` is dropped as for the other lines.
2. `saves hold`: the engine's saver writes nothing from then on, neither definitions nor values;
   every change stays in RAM and stays due. The console logs `saves: held, nothing is written until
   saves flush or a restart`. Holding again changes nothing (same line). The hold lives in RAM only:
   a restart (power, reset, the serial port being opened) starts with saves not held.
3. `saves flush`: the console logs `saves: flush asked`; at its next turn the engine writes, once,
   every file that has unsaved changes (definitions and values, as a stop does: one write per
   changed file, none for a file with nothing to write), ends the hold, and logs `saves: flushed,
   <k> files written` (`k` from 0 to 4: `devices.json`, `zones.json`, `equipments.json`,
   `recipes.json`). Saves then resume their normal rule (R3). A flush without a hold writes what is
   unsaved at once, the same way.
4. The clock gate of spec 023 R3.2 still applies: a flush asked before the clock is set is done at
   the first turn after it is set (so a flush never writes a 1970 date). The hold does not apply to
   a stop: `wifi-set` and `wifi-forget` (which stop the engine, its files saved, then restart) save
   as before, held or not. A restart of the radio inside the engine (a wedged radio, spec 021)
   saves through the same gated tick: held changes stay held; when the engine stops after its
   restarts, they are lost (RAM only). Between the reset and the console's `saves hold` (about 2 s
   into the boot) nothing is held; the clock gate (no SNTP time yet) is what keeps the engine from
   writing in that window.
5. A write that fails during a flush is shown on the log as any failed save and tried again later
   (`RETRY_MS` for zones, equipments and recipes, the 10-minute interval for the devices'
   values), exactly as a failure outside a flush; the hold is ended all the same, and the line
   counts only the files written.
6. The lines are read on the serial console only (the USB cable), never from the network
   (decision D36's rule for `pair`).

### R2: Every flash write counted (all firmware images)

1. The firmware's storage adapter (`firmware/src/storage.rs`) counts its writes to `/data` since
   boot: each save of a file (whatever its size: the `.tmp`, its sync and its renames are one save),
   each removal and each rename of a file, and the boot marker's write and its removal after 10
   minutes of running (`/data/booting`). A write is counted once attempted, even when it fails (the
   flash may have been written); a removal of a file already absent is not counted. Reads are not
   counted.
2. After each counted write it logs `flash: <n> writes since boot` (`n` from 1). The counter is a
   32-bit number that stops at its maximum (4 294 967 295) instead of wrapping: no memory grows with
   the writes.
3. `sh scripts/box.sh log --budget <n>` (any image, from the port or `--from-file`): after the log's
   other checks, prints the last `flash:` line of the log (`flash: 0 writes since boot` when there
   is none) and fails (exit 1, `box: flash: <m> writes since boot, over the budget of <n>`) when
   `m > n`. `--budget` takes a whole number; anything else is a usage error (exit 2).

### R3: A burst of changes gives one write (radio image and host `--serve`)

1. A change of definitions (devices, zones, equipments, recipes) is written
   `DEFINITION_DELAY_MS` = 2 000 ms after the saver first sees it unsaved, not at once: every change
   made in that window goes into the same write. One wait covers the four files: it starts at the
   first tick (with the clock gate open and saves not held) that finds any definition unsaved,
   and ends with that tick's writes; a write that fails keeps its own retry time (R3.5). The delay is a constant in
   `crates/core/src/engine/saver.rs`; its bound (at most 2 s, plus one turn of the engine's loop, of
   definitions are in RAM only) is written next to it.
2. The delay counts from the first unsaved change, not from the last: a stream of changes that never
   stops is still written every 2 s or so, never deferred without end.
3. A stop (Ctrl-C on the host, the console's restart on the box) writes at once, delay or not: a
   clean stop loses nothing.
4. What a power cut can lose, in normal running: the definitions changed in the last 2 s (plus one
   loop turn), and the devices' values of the last 10 minutes (unchanged). While saves are held
   (R1), everything since the hold.
5. A failed write is retried as before (`RETRY_MS` for zones, equipments and recipes, the 10-minute
   interval for the devices); the delay never makes a retry come sooner and adds nothing beyond it:
   while a failed file waits for its retry, the wait stays over (another change is then written at
   the next turn).
6. The saver's clock is the wall clock it is given. When `now` is before the start of the wait (the
   clock went back), the wait starts again from `now` (at most another 2 s), never a wait as long
   as the jump; a jump forward (the clock set by SNTP) ends the wait at once.
7. Without the delay (the host's `--pair`, the QEMU and measuring images, and the tests that build a
   `Saver` plainly) the saver writes definitions at the next turn, as before.
8. A device paired then cut from power within the 2 s window is in the radio's network but not in
   `devices.json`; it is identified again when it next speaks (accepted: pairing is rare and done
   with someone at the box).
9. A delay of 0 gives back the behaviour before this spec (the rollback, if the delay is ever
   judged too risky).
10. When the clock gate opens (the clock set by SNTP, from 1970), the devices' 10-minute values
   interval starts again from that moment: the radio image's saver is made before the clock is
   set, and without this a boot would write `devices.json` as soon as a device speaks after the
   clock is set (found by the code review: a write per boot, and `box.sh log --budget 1` failing).

### R4: `box.sh saves`: one test, one serial session

Opening the serial port resets the box (living spec, "Pairs a device"), so a hold and its flush
must be sent in the same session of the port: a separate `flush` command would reset the box first
and lose what was held.

1. `sh scripts/box.sh saves [--budget <n>] [--no-flush] [--port <device>] [-- <command> [<args>]]`
   opens the serial port (which resets the box), waits for `console: ready` (12 s at most, as
   `wifi`), sends `saves hold` and reads `saves: held` within 15 s (no answer: the message of
   `wifi`, exit 4). Every line read from the moment the port opens, those before `console: ready`
   included, is kept in the session's log and searched for `flash:` lines.
2. It then waits for the radio image's `serving: http://<address>/` line (60 s at most, the wait of
   `wifi`; none: `box: the box serves nothing within 60 s: the command is not run, the saves stay
   held until the next restart`, exit 1) and prints `box: saves held; serving at http://<address>/`.
3. With a command, it runs it with `SOLITE_BOX_ADDRESS=<address>` in its environment (its output
   shown as it comes), and keeps reading the serial log meanwhile. Without a command, it asks on
   the terminal (`/dev/tty`) `box: saves held: run the test, then press Enter to flush` and waits for
   Enter; with neither a command nor a terminal it does nothing and exits 2 (`box: saves needs a
   command after -- or a terminal`), before opening the port.
4. Then, unless `--no-flush`, it sends `saves flush`, reads `saves: flushed, <k> files written`
   within 30 s and prints `box: saves flushed (<k> files written)`; the console's `saves: flush
   asked` with no `flushed` after it: `box: the console took the line but the engine flushed
   nothing within 30 s: is the engine running, and the clock set?`, exit 1. With `--no-flush` it prints `box: saves
   still held: the changes stay in RAM and are lost at the next restart` and sends nothing (without
   a command it still waits for Enter first).
5. It prints the last `flash:` line read during the session (`box: flash: <m> writes since boot`,
   0 when none, `m` the last count of the whole session), and, with `--budget <n>`, fails when
   `m > n` (exit 1, as R2.3). The serial log of
   the session is kept in `$TMPDIR/solite-box/` and its path printed.
6. The exit code: 1 when the command failed (`box: the command failed (exit <c>)`, after the flush
   and the count, which still happen), or when the budget is exceeded; 0 otherwise.
7. A reset of the box during the session (a second `console: ready` after the hold: a panic, a
   watchdog, power) means the hold is gone and the counter restarted: `box: the box restarted
   during the session: the hold was lost and the count is not the session's`, exit 1, after the
   command ends; no flush is sent.
8. Ctrl-C at the terminal (or a SIGTERM to the Python helper) from the hold on: the command is
   stopped, the flush is sent as at the end (unless `--no-flush`), and the script exits 130: the
   box is never left held by an interrupted test. A SIGTERM to `box.sh` alone ends the script; the
   helper goes on, and flushes once the command ends. A command that cannot start (not found, not
   executable) is a failed command (`box: cannot run the command: <reason>`, exit 127): the flush
   still goes. The command runs from the caller's folder.
9. It asks nothing else and needs no backup. A run of it writes, on the box: the boot marker (1,
   the port resets the box), the files the flush writes (0 to 4), and nothing else unless the
   command does something that writes by itself (`wifi-set`, a pairing that forms a network),
and the marker's removal when the session lasts over 10 minutes. A flow that changes the four files
therefore stays within a budget of 5 (6 past 10 minutes).

### R5: The 33-request measure on the Mac

1. `scripts/measure-api.sh` runs unchanged against the host (`cargo run -p solite-host -- --sim
   scenarios/garage.json --serve`, then `sh scripts/measure-api.sh 8080`): its 33-request contract
   is kept (same requests, same output line, same exit codes).
2. Its help text (the comment at its top and its usage line) says: run it on the Mac against the
   simulator; against the box, at most once per milestone, and inside `box.sh saves` (so its 33
   requests and the two deletions give one write per file).
3. No script loops it.

### R6: The rule, written (decision D39 finalised)

1. D39 in `docs/decisions.md` loses its "proposed": a test on the real box is one pass of the flow
   being shown, run inside `box.sh saves` with a `--budget` (the boot marker plus one write per file
   the flow changes, plus the marker's removal past 10 minutes), or one write and a restart to
   check that it persists; loops, measures and stress run on the Mac or in QEMU; each
   spec that runs on the box writes in its result the `flash:` count of the run.
2. A new decision D40 records the means: the hold and flush on the serial console in one session,
   the write counter, the 2 s coalescing and what a power cut can lose. It adjusts D13's
   "definitions at once (the user's choice)": definitions are written within 2 s; D13 is not
   edited, D40 says it replaces that clause.
3. The feature skill's reference does not list box steps; the skill (Phase 6, ESP32) gets one
   sentence pointing to D39 for a run on the box. `CLAUDE.md`'s command list gets `box.sh saves`.

## Living spec delta

### docs/living-spec/README.md

- **Starts and shows its name** (the radio image paragraph): New: the serial console also reads
  `saves hold` and `saves flush` (R1); every firmware image logs `flash: <n> writes since boot`
  after each write to `/data` (R2); the radio image writes a change of definitions 2 s after it
  first sees it, so a burst gives one write; what a power cut can lose (R3.4).
- **Keeps devices, in Sowel's format**: Changed: "a device paired, renamed or deleted is written at
  once" → "is written within 2 seconds under `--serve` (a burst of changes gives one write), at
  once with `--pair`"; a clean stop still writes at once.
- **Organises the home in zones and equipments**: Changed: "written at once when they change" →
  "written within 2 seconds of a change (one write for a burst)".
- **Runs the motion-light recipe** (What survives a restart): Changed: "written at once" →
  "written within 2 seconds".
- **Runs without the dongle**: New: `measure-api.sh` is run on the Mac against the simulator; on
  the box at most once per milestone, inside `box.sh saves`.
- **Flashes the box and brings it back**: New: `log --budget <n>` (R2.3); `saves` (R4).
- **History**: row 025.

### No longer true

Searched (`grep -rn -i` in `docs/` but roadmap and decisions, `README.md`, `CLAUDE.md`,
`inputs/README.md`, `.claude/`) for "at once", "written at once", "measure-api", "box.sh",
"saves", "flash write", "D39":

- `docs/living-spec/README.md`, "Keeps devices": "a device paired, renamed or deleted is written at
  once" → as in the delta.
- `docs/living-spec/README.md`, "Organises the home": "written at once when they change" → as in
  the delta.
- `docs/living-spec/README.md`, "Runs the motion-light recipe": "`recipes.json` ..., written at
  once" → as in the delta.
- `docs/living-spec/README.md`, `pair` bullet: "it writes nothing to the flash" → "it writes nothing
  to the flash but the boot marker of the reset it causes".
- `crates/core/src/engine/saver.rs` and `crates/core/src/devices/persist.rs` module comments: "at
  once for zones, equipments and recipes" / "a change of definitions at once" → "after the
  definition delay when one is set".
- `docs/architecture.md`, Box row: lists `box.sh`'s commands → adds `saves` and the budget.
- `docs/architecture.md`, storage paragraph: "`zones.json`, `equipments.json` and `recipes.json`
  (written at once when they change)", "definitions written at once" → "within 2 seconds under
  `--serve`".
- `docs/decisions.md`, D13: "definitions at once (the user's choice)": history, not edited; D40
  replaces that clause.
- `crates/host/src/store.rs` test comments ("written at the next tick"): true for a plain `Saver`
  (no delay), kept.
- `docs/roadmap.md` (looks ahead, not "no longer true", but updated): the inbox line of 2026-11-23
  on the budget rule is done and removed; a line for the box checks of this spec is added.

## Acceptance criteria

- [x] `parse_console_line` reads `saves hold` and `saves flush` (with or without `\r`); `saves`,
      `saves hold now`, `saves stop` are unknown.
- [x] With saves held, a change of each kind and 20 minutes of values write nothing; after `flush`,
      one write per changed file, the hold ended, then the normal rule again.
- [x] A flush asked while the clock gate is closed writes nothing until the gate opens.
- [x] A stop while held writes the files at once.
- [x] A burst of 33 changes within 2 s gives one write of `equipments.json` (core test, fake clock,
      counting storage); a change 2 s after the write gives a new one; a stop during the wait
      writes at once; a failed write is retried after `RETRY_MS`, not sooner.
- [x] The write counter stops at its maximum; `flash_line(n)` is `flash: <n> writes since boot`.
- [x] `box.sh log --budget` passes at and under the budget, fails over it, counts 0 with no line.
- [x] `box.sh saves -- <command>` with the fake console: sends `saves hold` then `saves flush`, runs
      the command with `SOLITE_BOX_ADDRESS`, prints the flush and the count, fails over the budget,
      sends no flush with `--no-flush`, reports a silent console (exit 4), a console that never
      serves (exit 1, no command run, no flush) and a failed command (exit 1, flush still sent).
- [x] `measure-api.sh`'s tests against the host still pass; its help names the Mac and the
      once-per-milestone rule.
- [x] The four firmware images build; QEMU's run is green (its log shows the `flash:` lines of the
      seed).
- [x] D39 finalised, D40 written, the skill's sentence and `CLAUDE.md`'s command line added.

## Edge cases

- A reset of the box during a `box.sh saves` session (R4.7); Ctrl-C during the command (R4.8).
- A `flash:` line read before `console: ready` (the boot marker): counted (R4.1).
- The engine's radio restart while held: held changes stay held (R1.4).
- `saves hold` twice: one hold, the same line twice; `saves flush` twice: the second writes only
  what changed since the first (often nothing: `0 files written`).
- `saves flush` with nothing unsaved: no write, `saves: flushed, 0 files written`.
- The engine not running (no radio answer, stopped after its restarts): `saves: held` is still
  answered by the console (the hold is a flag), but no `flushed` comes: `box.sh saves` says so
  (R4.4).
- The serial port opened while held: the box resets, the hold is gone, the held changes are lost
  (RAM only, by design; R1.2).
- A command given to `box.sh saves` that fails: the flush and the count still happen (the box is
  not left held), exit 1.
- `--budget` not a number, or `--budget` without a value: usage error, exit 2.
- A burst that lasts longer than 2 s: one write every 2 s or so during it (R3.2).
- The clock going back while a change waits (R3.6).
- A write failure during the delay's write: retried as before (R3.5).
- The counter at its maximum: stays there (R2.2).
- A log with no `flash:` line (an old image, or no write since boot): count 0.

## Memory, size and requests

- The hold: two atomic booleans, static, in the radio image. The counter: one 32-bit atomic,
  static, in every image. The saver gains a delay and an optional "first seen" time (16 bytes). No
  collection grows.
- No change to the interface, so no change to its budget.
- Fewer writes: a burst within 2 s gives one save per file instead of one per request; inside
  `box.sh saves`, one per file for the whole test.

## Sowel contract

None: the console lines, the log lines and the scripts are Solite's own; no API type, field or
event changes. The files on `/data` keep their format.

## How the user will check it

**On the Mac** (this run): `cargo run -p solite-host -- --sim scenarios/garage.json --serve 8080
--speed 10`, then `sh scripts/demo-garage.sh 8080` and `sh scripts/measure-api.sh 8080`: both pass
as before; the files in the data folder are rewritten a few times, not once per request. The core
tests show the burst, the hold and the flush with a counting storage; `box.sh`'s tests show
`saves` and `--budget` with a fake console; `sh scripts/qemu.sh` shows the `flash:` lines of the
seed.

**On the box** (later, the user present; one pass, D39): flash the radio image with
`--keep-storage` (one image write, the user's yes, D6), then
`sh scripts/box.sh saves --budget 5 -- sh -c 'sh scripts/measure-api.sh "$SOLITE_BOX_ADDRESS"'`:
the 33 requests pass; `saves: flushed, <k> files written` with `k` at most 3 (`zones.json` and
`equipments.json`, changed even though the measure deletes what it made, and `devices.json` when
values came in); `flash: <m> writes since boot` with `m` at most 5 (the boot marker, the flush).
Before and after, `curl http://<address>/api/v1/equipments` and `/zones`. Then one
`sh scripts/box.sh log --image radio --seconds 60 --budget 1` (a restart): the boot reads the files
the flush wrote with no `cannot read` line, the two lists answer the same as before the restart
(persistence of what the flush wrote), and the only write is the marker's. Result: to be written here after the run (the box is out of reach in this run).

### Mac run

2026-11-24, `--sim scenarios/garage.json --pair 30`, then `--serve 18080 --speed 10`:
`demo-garage.sh 18080` set the garage up; `measure-api.sh 18080`: `33 requests, 33 answered 201,
0 Busy, total 0.057 s`, so the whole measure falls inside one 2 s wait (one write per file in
`--serve`, core test `a_burst_of_33_changes_within_2_s_is_one_write`); Ctrl-C wrote the files at
once. `validate.sh` in this worktree: docs, format, lint, Rust and interface tests green; the
firmware images do not build in a worktree nested in the repository (Cargo finds the parent
workspace), so the four images and QEMU are checked by CI's run of `validate.sh`.

### Review

`spec-reviewer`: twelve findings fixed in the spec (forced and stopping apart, the session log from
the port's opening, the marker's removal at 10 minutes in the budget, a reset during the session,
Ctrl-C, D13's clause, the persistence check). `code-reviewer` (Opus): one blocking finding fixed
(the values' interval restarted when the clock is set, R3.10), three should-fix fixed (a command
that cannot start, Ctrl-C before `serving:`, the recovery's renames counted; the SIGTERM sentence
narrowed), minor ones fixed but one: a `saves hold` sent between a `saves flush` and the engine's
next turn is ended by that flush (`box.sh saves` never sends them so).
