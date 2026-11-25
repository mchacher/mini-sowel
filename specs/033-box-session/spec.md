# Spec 033: box-session

## Context

Every session with the Dongle Max in hand has so far been driven step by step from a
conversation: flash, read the boot log, run one API test inside `box.sh saves`, read the answer,
decide the next command. The roadmap's "Tests to run with the box in hand" has grown to about
twenty items (specs 021, 023, 025 to 029, 031, 032), and running them one by one costs a
conversation's worth of tokens and the user's attention at every step. The user asked (2026-11-25)
for one command that runs the whole list, with no prompt between the steps, so that a box session
is one command, one report and one verdict.

`scripts/box-session.sh` is that command. It does not replace `scripts/box.sh`: it calls it, step
after step, with the budgets of decision D39, and calls the host program for the update key and the
signature. The user typing `--yes-flash` is their single yes of decision D6 for that run (new
decision D55).

## Goals

1. One command runs, in one pass and with no prompt, every item of the roadmap's list that needs no
   hand gesture, and prints one report line per step and one verdict.
2. Without `--yes-flash` it flashes nothing and writes nothing through the API: it runs only the
   read-only steps.
3. The steps that need a person (a button, a power cut, a cable, the Wi-Fi off, a hand in front of
   a sensor, a browser) are reported `skipped (needs a person: ...)`, or asked one by one in an
   optional second phase, `--with-gestures`.
4. The script's own logic (flags, budget, a failed step, the missing backup, a gesture step) is
   tested on the Mac with a fake `box.sh`, and every API check it runs is tested against the host's
   simulator.
5. `scripts/validate.sh` prints the size of each of the four firmware images and its share of the
   4 MB `ota_0` slot, with a warning above 75 %.

## Out of scope

- Running the script against the real box: the user runs it at the next box session (the first run
  is expected to reveal route or timing mismatches, fixed then by `/solite-fix`).
- The items that need a person stay manual: they are listed by the script, not automated.
- The roadmap items with no code yet: the home lock's log line per batch of measures (spec 027),
  E3d, E3e, the box's button.
- The 023 checks of the serial console's long and unknown lines: `box.sh` has no command that sends
  a raw line, and adding one is a change of `box.sh` (inbox line).
- A second run of the whole session in a loop, or any repeat of a writing step: never (D39).

## Requirements

### R1: The command and its options

`sh scripts/box-session.sh [--yes-flash] [--backup <file>] [--factory-reset] [--with-gestures]
[--port <device>] [--address <address>[:<port>]]`

`sh scripts/box-session.sh --api-only <address>[:<port>] [--factory-reset]`

1. `--yes-flash`: the user's yes to flash the radio image once and to write through the API, for
   this run only (D55). Without it, no step that flashes or writes runs: each is reported
   `skipped (needs --yes-flash)`.
2. `--backup <file>` (or the environment variable `SOLITE_BOX_BACKUP`): the original firmware's
   backup. With `--yes-flash` it is required: missing option, or a file that does not exist, is a
   usage error before anything is done (`box-session: --yes-flash needs --backup <file> (the
   original firmware's backup)` or `box-session: backup not found: <file>`, exit 2). It is passed
   to `box.sh flash --backup`, which checks it (16 MB, this box's MAC, a proven row of
   `docs/hardware.md`, D35).
3. The flash is always `box.sh flash --image radio --keep-storage --backup <file>`: the home, the
   Zigbee network and the Wi-Fi settings stay. No option erases `storage`.
4. `--factory-reset`: also runs the factory reset step (R4, step B10). Without it, that step is
   `skipped (erases the home: needs --factory-reset)`. Without `--yes-flash` or `--api-only` it is
   a usage error (`box-session: --factory-reset needs --yes-flash or --api-only`, exit 2).
5. `--with-gestures`: after the automatic steps, asks the gesture steps one by one (R6).
6. `--port <device>`: passed to every `box.sh` call.
7. `--address <address>[:<port>]` (or `SOLITE_BOX_ADDRESS`): the box's address for the update
   steps; without it, the address the last `box.sh saves` printed (`serving at http://<address>/`)
   is used.
8. `--api-only <address>[:<port>]`: no box, no `box.sh`, no firmware build: runs the API groups
   (R4) straight against that address (the Mac host's `--serve`), each step that needs the box
   `skipped (box only)`. A number alone is a port on `127.0.0.1`, as `measure-api.sh`. Any host
   other than `127.0.0.1`, `localhost` or `::1` is refused (`box-session: --api-only is for the Mac
   host (127.0.0.1, localhost, ::1): <address> refused`, exit 2): pointed at the box it would write
   with no hold, no budget and no `--yes-flash` (found by the code review).
9. `--yes-flash` with `--api-only`, or an unknown option: usage error, exit 2.
10. **Who types `--yes-flash`** (D55): the user, in their own terminal, as `box.sh backup`,
    `restore` and `flash`. Claude never adds the flag to a command it runs, and no agent message is
    the yes. The script refuses `--yes-flash` when `/dev/tty` cannot be opened (`box-session:
    --yes-flash is typed by the user in a terminal`, exit 2): a run with no terminal is not the
    user's.
11. Secrets: the box's password comes from `SOLITE_PASSWORD` (user `SOLITE_USER`, `admin` when
    unset), never from an argument, never printed, never written to the repository. When
    `SOLITE_PASSWORD` is unset, the password file is read: `$SOLITE_PASSWORD_FILE`, by default
    `$HOME/.config/solite/box-password` for a box run and `<run folder>/password` for
    `--api-only`. When the box has no account, the auth step sets one up with `SOLITE_PASSWORD`
    when it is set, else with the password file's when it exists, else with a password generated
    at run time (24 hexadecimal characters from `/dev/urandom`) written to the password file
    (mode 0600, its folder 0700, never overwritten), named by path in the report and in the
    verdict line. The box run keeps it in the user's folder, not in `$TMPDIR` (which macOS
    purges): a lost password on a box whose button is not wired is only recovered by a flash that
    erases `storage`. Every later step, `measure-api.sh` included, gets the same password
    (`SOLITE_PASSWORD` exported from it).

### R2: The run folder, the output and the verdict

1. Every run has its folder, `$SOLITE_SESSION_ROOT/<YYYYmmdd-HHMMSS>` (default root
   `$TMPDIR/solite-box-session`), mode 0700; a root inside the repository is refused (exit 2).
   It holds `session.log` (every command's full output, in order), the password file in `--api-only` when one is
   generated, the signed update files and the images built.
2. The standard output is the report only: one line per step, in a fixed order (R3), each
   `box-session: <id> <name>: OK[ (<detail>)]`, `KO (<reason>)` or `skipped (<reason>)`; a group's
   checks get their own lines, `<id>.<n>`. The last line is the verdict: `box-session: <k> OK, <m>
   KO, <s> skipped; log: <path of session.log>[; password: <path of the password file>]` (the
   path when this run created it).
3. Exit 0 when no step is KO, 1 when one is, 2 for a usage error (nothing run).
4. Run through `sh scripts/quiet.sh --tail 60 sh scripts/box-session.sh ...`, the conversation sees
   one verdict line on success and the report on failure.
5. The update key (R4, P2) is created in the run folder and removed when the script ends, whatever
   the way it ends (an `EXIT` trap): no key outlives the run, and none is ever in the repository.

### R3: The steps, in order

| Id | Step | Runs | Depends on | Writes |
| --- | --- | --- | --- | --- |
| P1 | preflight: options, `box.sh` present, the backup, `curl`, `python3` | always | | nothing |
| P2 | update key: `solite-host --update-keygen <run folder>/update.key` | `--yes-flash` or `--api-only` | P1 | the run folder |
| P3 | radio image built with the key (`SOLITE_UPDATE_PUBLIC_KEY`), in `firmware/target/radio` | `--yes-flash` | P2 | the build folder |
| P4 | the update images: the next version (patch + 1) and a rollback image (patch + 2, panics at start), built in a throwaway copy of `HEAD` outside the repository, signed with `--update-sign`; a copy of the first with one signature byte flipped. In `--api-only`: no build, 16 bytes signed as the next version (`--update-sign` accepts bytes with no descriptor), then flipped | `--yes-flash` or `--api-only` | P2 | the run folder |
| P5 | image size: the radio image's application image against the `ota_0` slot (KB, %, KO above the slot, a warning above 75 %) | always, when the radio ELF exists | | nothing |
| B1 | identify (`box.sh identify`) | always | | nothing |
| B2 | flash (`box.sh flash --image radio --keep-storage --backup <file>`) | `--yes-flash` | B1, P3 | the flash, once |
| B3 | boot and memory (`box.sh log --image radio --budget 1`): from the serial log file it names, the internal RAM free and its largest block and the PSRAM free (`memory-network:` line), the internal minimum (`heap:` line), the storage used (`storage: /data, <n> KB used of <m> KB`) | always | B1 (and B2 with `--yes-flash`) | the boot marker |
| B4 | auth, J4 (`box.sh saves --budget 3 -- sh <repository>/scripts/box-session.sh --group auth`) | `--yes-flash` | B3 | `auth.json` once, `devices.json` |
| B5 | second boot, spec 021: the serial log of B4's session (`box: serial log: <path>`) shows `network: resumed` and one banner; and no token and no password in it (029) | `--yes-flash` | B4 ran | nothing more |
| B6 | the 33 requests, spec 025 (`box.sh saves --budget 5 -- sh -c 'sh <repository>/scripts/measure-api.sh "$SOLITE_BOX_ADDRESS"'`, the script's absolute path, `SOLITE_PASSWORD` exported) | `--yes-flash` | an account usable (auth check 2 OK) | zones, equipments |
| B7 | home, specs 026, 027, 028, 023 (`box.sh saves --budget 4 -- sh <repository>/scripts/box-session.sh --group home`) | `--yes-flash` | an account usable | settings, zones, `devices.json` |
| B8 | update, J5 (`sh <repository>/scripts/box-session.sh --group update`, not inside `saves`: the box restarts) | `--yes-flash` | an account usable, P4 | the inactive slot twice; on `/data`, three boot markers and at most two final saves |
| B9 | memory of the updated image, a fresh boot (`box.sh log --raw --seconds 300 --budget 1`): the `heap:` line, read from the serial log file it names | `--yes-flash` | B8 OK | the boot marker |
| B10 | factory reset, J4 (`box.sh saves --budget 7 -- sh <repository>/scripts/box-session.sh --group reset`) | `--factory-reset` | an account usable | the home's files |
| H1... | the gesture steps (R6) | `--with-gestures` | | per step |

1. A step whose dependency was not OK is `skipped (after <id> <outcome>)`. "An account usable"
   is the auth group's check 2 (the account set up or the login done), written by the group to
   the run folder; a later KO inside the auth group (a slow login, the rate limit) does not skip
   B6 to B10.
2. **Stops**: B1 refusing the box (exit 3 or 4), B2 failing, B3 failing after B2, a failed
   rollback (update check 6: the box may be looping on the panicking image; the report names the
   way back, `box.sh flash --image radio --keep-storage` over USB), or any
   `box.sh saves` whose count is over its budget (`over the budget` in its output) stops the box
   steps: every box step after it is `skipped (stopped after <id>)`. A budget exceeded is reported
   `KO (flash: <m> writes since boot, over the budget of <n>)`.
3. Any other KO (a check that fails inside a group, B5, B9) does not stop the run: the next steps
   run.
4. The budgets are decision D39's formula, 1 (the boot marker) plus one per file the step changes:
   B4 3 (`auth.json`, and `devices.json`, which any device message during the hold changes),
   B6 5 (spec 025's figure: zones, equipments, and the margin it measured), B7 4 (settings, zones,
   `devices.json`), B9 1, B10 7 (devices, zones, equipments, recipes, settings,
   `auth.json`). B8 runs outside `saves` (the box restarts three times): its cost on `/data` is
   three boot markers and at most two final saves of the engine before a restart, bounded, one
   pass, not checked by a count.
5. No step is repeated, and no step runs a loop of writes against the box (D39). The update steps
   poll `GET /api/v1/system/version` (a read) every 5 s while the box restarts.

### R4: The API groups

`<self> --group auth|home|update|reset` runs one group against `SOLITE_BOX_ADDRESS` (what `box.sh
saves` sets), with the run folder in `SOLITE_SESSION_DIR`, prints its checks' lines and exits 1
when one is KO. It is how the outer run calls itself inside `saves`, and how the tests call one
group. Every check is one request or a short fixed sequence of them; each prints its line.

Every group, and B6 before `measure-api.sh`, first sends `POST
/api/v1/integrations/solite/permit-join` `{"time":0}` with its token (once it has one): the reset
that opens each `saves` is a power-on boot, which opens a pairing window, and a device joining
during a check would write `devices.json` over the budget.

**auth (J4, spec 029)**:

1. `GET /api/v1/auth/status` answers 200 with `setupRequired`.
2. Account: when `setupRequired` is true, `POST /api/v1/auth/setup` with the password chosen as R1.11 says
   (`SOLITE_PASSWORD`, else the password file's, else a generated one) answers 201 (`account set up; password in <path>`); a 403 with `confirmRequired` is KO
   with the box's text (the 10 minutes after a power-on). When false, `SOLITE_PASSWORD` (or the
   run's password file) is needed: none is KO `set SOLITE_PASSWORD (the box has an account)`.
   When the account exists, check 2 logs in with that password (200 needed: a 401 is KO `the
   password does not open the box's account`), so "an account usable" means a token was obtained.
3. One login timed: `POST /api/v1/auth/login` answers 200; detail `login in <s> s`; KO above 2 s
   (spec 029 expects 0.5 to 1 s on the box).
4. The gate: `GET /api/v1/devices` without a token answers 401.
5. `/ws` without a token is closed with code 4001; with the token offered as the subprotocols
   `bearer.none, bearer.<token>`, the first message arrives (a WebSocket client in `python3`, no
   dependency).
6. Pairing behind the token: `POST /api/v1/integrations/solite/permit-join` without a token answers
   401; with it and the body `{"time":0}`, 202 with `permitJoin: false` (the gate is proven and the
   power-on window is closed).
7. The rate limit (setup and login count together, 10 per minute for the whole box): logins with
   a wrong password until the first 429, at most 12 attempts; OK when the 429 comes at the 11th
   attempt of the minute counting the attempts the group already made (the setup or check 2's
   login, then the timed login) (detail: `429 at attempt
   <k>`). Last of the group. On the box the next `saves` restarts it, which clears the limit; in
   `--api-only` the group then waits `SOLITE_SESSION_RATE_WAIT` seconds (61) so the next groups
   can log in; in a box run it waits only when `SOLITE_SESSION_RATE_WAIT` is set in the
   environment (the tests, whose fake `saves` does not restart the host).

**home (specs 026, 027, 028, 023)**:

1. Backup, 026: `GET /api/v1/backup`, then `POST` of the same document answers 200 (first: an
   import replaces the settings whole).
2. Location and time zone, 026 and 027: with `SOLITE_HOME_LATITUDE`, `SOLITE_HOME_LONGITUDE` and
   `SOLITE_HOME_TIMEZONE` set (the user's real home, from the environment, never in the
   repository), `PUT /api/v1/settings` with `home.latitude`, `home.longitude` and `home.timezone`
   answers 200 and `GET /api/v1/settings` reads them back; the values are written to the run
   folder (`location`, 0600) for the persistence check (update check 4). Without them: `GET`,
   then `PUT` of the same values answers 200, and check 3's sun times are `skipped (set
   SOLITE_HOME_LATITUDE, SOLITE_HOME_LONGITUDE and SOLITE_HOME_TIMEZONE)` when the settings hold
   no location (its clock wait still runs).
3. Sun times, 026: first the clock (the box serves before its clock is set, up to about 20 s):
   `GET /api/v1/zones/aggregation` is polled every 2 s, at most `SOLITE_SESSION_CLOCK_WAIT`
   seconds (30), until the root zone's `sunrise` is not `null` (with a location) or `GET
   /api/v1/devices` shows no `lastSeen` in 1970 (without one); never: KO `clock not set within
   <n> s`, and check 5 skipped. Then the root zone holds `sunrise` and `sunset` (`HH:MM` or
   `--:--`) and `isDaylight`.
4. Time zone, 027: `GET /api/v1/system/timezone` answers 200 with `tz` = the settings'
   `home.timezone` and `source` `env`; with `home.timezone` unset, `{"tz":"UTC","source":
   "fallback"}`.
5. Dates, 028: `GET /api/v1/devices`: no date (`lastSeen`, `createdAt`, `updatedAt`) in 1970 or
   more than a minute in the future, and each `status` agrees with the age of its `lastSeen`
   (`online` within 10 minutes for a `mains` device, 25 hours otherwise; `offline` beyond; a
   margin of one minute around each limit, since the status is updated from time to time; a
   device never heard may be `unknown`). Detail: the count checked; no device: `skipped (no
   device)`.
6. Busy, 023 (box only): four requests whose body never comes keep the four workers, a fifth
   request answers 503 `Busy`; the four end with 408 within 10 s.
7. A client that stops reading, 023 (box only): a client asks the interface's script and reads
   nothing; `GET /api/v1/auth/status` from another connection still answers 200 within 5 s.

**update (J5, spec 032)**, with the files of P4 in the run folder:

1. Without a token, `POST /api/v1/system/update` with an empty body answers 401 (`curl -H 'Expect:'`).
2. The flipped file's 128-byte header alone (the signature is checked on the header, before any
   image byte) answers 400 `Invalid signature`. A 503 (a build with no key) is `skipped (this
   build has no update key)`, and so are the next checks.
3. The next version answers 200 `Installed <x.y.z>, restarting`.
4. Within 180 s, `GET /api/v1/system/version` (logging in again: a restart ends the sessions)
   answers `current` = the next version. When the run folder holds `location` (home check 2), `GET
   /api/v1/settings` still reads it: the settings survived the restart (026, persistence).
5. 75 s after it answered, the rollback image's upload answers 200 (so the next version was
   confirmed: a 409 `The running image is not confirmed yet` is KO).
6. The rollback: first a restart is proven (the token used for the upload answers 401 `Invalid
   or expired token`, a restart ends every session; polled within the rollback wait), then, within
   300 s, `current` answers the next version again: the bootloader rolled the panicking image back.
   No restart seen, or another version: KO, and a stop (R3.2).
7. In `--api-only`, checks 3 to 6 are `skipped (box only: the host does not restart)`; check 2 uses
   P4's flipped file (16 bytes signed with the run's key, a key the host does not hold), so a host
   with a key answers `Invalid signature` too.
8. The waits come from `SOLITE_SESSION_BOOT_WAIT` (180), `SOLITE_SESSION_CONFIRM_WAIT` (75),
   `SOLITE_SESSION_ROLLBACK_WAIT` (300), `SOLITE_SESSION_POLL` (5), in seconds, so the tests run in
   seconds.

**reset (J4, spec 029)**, only with `--factory-reset`:

0. `POST /api/v1/integrations/solite/permit-join` `{"time":0}` answers 202 (a window open since
   the boot would make the reset 409).
1. `POST /api/v1/system/factory-reset` answers 204.
2. `GET /api/v1/auth/status` answers `setupRequired: true`.
3. Setup again, with `SOLITE_PASSWORD` when set, else the run's password: 201.

### R5: The tools and the seams the tests use

1. `box.sh` is `$SOLITE_BOX_SCRIPT` when set (the tests point it at a fake), else `scripts/box.sh`.
2. The host program is `$SOLITE_HOST_BIN` when set, else `cargo run -q -p solite-host --`.
3. The firmware builds (P3, P4) are `$SOLITE_SESSION_BUILDER <kind> <version> <public key hex>
   <out .bin>` when set (kinds `base`, `next`, `rollback`), else the script's own builder: the
   ESP32 toolchain (`$HOME/export-esp.sh`), the radio build of the README with the key in
   `SOLITE_UPDATE_PUBLIC_KEY`, the application image by `fw_elf2image` of `scripts/lib/firmware.sh`.
   `next` and `rollback` are built in a copy of `HEAD` (`git archive`) in
   `$SOLITE_SESSION_ROOT/tree`, with its own target folder `$SOLITE_SESSION_ROOT/target` (kept
   between runs, so a later run rebuilds little), `firmware/.embuild` linked to the repository's
   (ESP-IDF is not downloaded twice), `SOLITE_UI_DIST` pointing at the repository's `ui/dist`;
   the copy's two `version` lines (`Cargo.toml`'s `[workspace.package]` and
   `firmware/Cargo.toml`) are set to the version asked (the application descriptor then carries
   it, which `--update-sign` checks); for `rollback`, the first statement of the firmware's `main`
   is `if std::hint::black_box(true) { panic!("box-session: rollback image") }` (no unreachable
   code). A change that cannot be applied (the line is not found) is a KO
   of P4, never a silent build of the wrong image. Nothing in the repository's working tree changes
   but `firmware/target/radio` (P3).
4. `curl` and `python3` (for `/ws`, the busy and the stopped-reader checks) are taken from the
   `PATH` (`$SOLITE_PYTHON` when set, so a test can name a missing one); without it those checks
   are `skipped (no python3)`.
5. `fw_elf2image` and `fw_build_table` end the caller through `fw_fail`: the script calls them in
   a subshell, so a failure is a KO of P4 or P5, never the end of the run.
6. `box.sh log` prints `box: serial log: <path>`; the memory lines are read from that file, not
   from the standard output.

### R6: The gesture steps

Without `--with-gestures` each is printed `skipped (needs a person: <what>)`. With it, after the
automatic steps, each is shown on the terminal (`/dev/tty`) with what to do and what to look at;
the user answers `y` (seen as expected: OK), `n` (KO) or `s` (skipped). No terminal: `skipped (no
terminal)`. The list (from the roadmap):

| Id | Spec | What the person does, and what they check |
| --- | --- | --- |
| H1 | 021 | Unplug the box for 10 s, plug it back: it comes back, the network resumed (a power cut) |
| H2 | 021 | The ZBMINIR2 powered: it answers an order from the interface |
| H3 | 026 | By day, with "Inactive during day" set: a hand in front of the motion sensor does not switch the light on |
| H4 | 023 | The access point off for 60 s, then on: the box answers again (the lost link) |
| H5 | 023 | A boot with the access point off: the date is not 1970 once the link is back |
| H6 | 023 | The relay unplugged: an order answers 502 in the interface |
| H7 | 029 | In the browser: the login screen, `/ws` live with the token, Discovery's "Add a device" opens pairing |
| H8 | 029 | A software restart (the interface's restart, or `box.sh wifi` again) asks to log in again and keeps the account |
| H9 | 032 | Unplug the box during the minute after an update: the old image comes back |
| H10 | 032 | The Firmware update card shows the error when the box refuses a file |
| H11 | 031 | A real colour bulb (if the user has one): paired, its colour orders applied and read back |

Not listed, and why: a wrong Wi-Fi password and `wifi --forget` (023) cut the box off the network
for the rest of the session; the forming of a new network (021) needs `storage` erased, which the
session never does (R1.3); the button (J4) is not wired.

### R7: The image sizes in `scripts/validate.sh`

In the firmware group, after the four images are built, the step "Firmware: image sizes" prints one
line per image, `<image>: <n> KB, <p> % of the <s> KB ota_0 slot` (box, sim, flash-saves, radio),
from the application image `fw_elf2image` makes of each ELF right after its build, into a temporary
folder; the slot is read from `firmware/partitions.csv` by `fw_build_table`. Above 75 % the line
ends `WARNING: above 75 % of the slot`; the step does not fail on a warning (an image larger than
the slot already fails `fw_elf2image`). It adds no build: about a second per image.

## Living spec delta

### docs/living-spec/README.md

- New, under "Flashes the box and brings it back": a paragraph **Runs a box session in one
  command**: `scripts/box-session.sh`, its options (R1), the steps and what each checks (R3, R4),
  the budgets, the stops, the run folder, the report and its verdict (R2), the gesture steps (R6),
  `--api-only` against the Mac host, the update key made and removed in the run.
- New, under "Stays within a memory budget" (or where `validate.sh`'s firmware checks are
  described): `validate.sh` prints each firmware image's size and share of the `ota_0` slot, with a
  warning above 75 % (R7).

### No longer true

- `CLAUDE.md`, commands: the table has no line for the box session → a line `sh
  scripts/box-session.sh --yes-flash --backup <file>` (the user runs it: it flashes) and
  `--api-only <port>` (the Mac).
- `README.md`, commands: same line.
- `docs/roadmap.md`, "Tests to run with the box in hand": "One session, the user present, after
  their yes to flash (D6)" → the session is `box-session.sh`; each item says whether the script
  runs it (and which step) or it stays a person's.
- `docs/architecture.md`, Box row: names `box.sh`, `demo-garage.sh`, `measure-api.sh` → adds
  `box-session.sh`.
- `CLAUDE.md` "Never flash the Dongle Max without the user's explicit yes": stays true; the yes can
  now be the `--yes-flash` the user types (D55). The sentence gets "(their `--yes-flash` on
  `box-session.sh` is one, for that run)".
- Searched for `box.sh saves`, `Tests to run with the box`, `D6`, `D39`, `measure-api`: the
  feature and fix skills' demo text (one pass inside `box.sh saves`) stays true.

## Acceptance criteria

- [x] Without `--yes-flash`, the fake `box.sh` sees no `flash` and no `saves`; the writing steps
      are `skipped (needs --yes-flash)`; exit 0 when the read-only steps pass.
- [x] `--yes-flash` without `--backup`, or with a missing file: exit 2, the fake `box.sh` never
      called.
- [x] A `saves` over its budget: that step KO with the count, the later box steps `skipped
      (stopped after <id>)`, exit 1.
- [x] A failing step (the fake `log` fails) is KO with its reason, the run goes on, exit 1.
- [x] The gesture steps are `skipped (needs a person: ...)` without `--with-gestures`.
- [x] `--api-only` against `solite-host --sim scenarios/garage.json --serve`: auth, home, the
      33 requests, the update refusals and (with `--factory-reset`) reset are OK; box-only checks
      skipped.
- [x] The update group against a fake server (install, restart, confirmation, rollback) passes;
      a box that never comes back with the next version is KO.
- [x] No key file remains in the run folder after a run; no password is printed.
- [x] `validate.sh` prints the four images' sizes and their share of the slot.
- [x] The roadmap's list says, item by item, which step runs it or that it stays a person's.

## Edge cases

- No `--yes-flash`: read-only steps only (P1, P5, B1, B3).
- `--backup` missing or not found with `--yes-flash`: exit 2, nothing run.
- The box has an account and `SOLITE_PASSWORD` is unset: auth KO with the reason; B6 to B10
  `skipped (after B4: no account)`.
- `confirmRequired` at setup (the box was not powered on within 10 minutes): auth KO with the
  box's text.
- A `saves` over its budget: stop.
- The box refused by `identify` (exit 3) or silent (exit 4): stop.
- A build fails (P3 or P4): P3 KO means B2 never runs (B1's read-only `identify` still does; no
  flash of an image without the key). P3 builds the working tree, P4 builds `HEAD`: P1 warns
  (`uncommitted changes: the update images are built from HEAD`) when `git status --porcelain`
  shows changes under `crates/` or `firmware/`, or when the working tree's version differs from
  `HEAD`'s; P4 KO skips B8 only.
- The version cannot be read or bumped (P4): KO, B8 skipped.
- A run interrupted (Ctrl-C): the key is removed (trap); `box.sh saves` flushes on its own.
- A host build with no update key (Mac): update checks 2 to 6 skipped with the reason.
- No `python3`: the `/ws` and busy checks skipped.
- `SOLITE_SESSION_ROOT` inside the repository: exit 2.
- `--factory-reset` alone: exit 2.
- `--yes-flash` with no terminal: exit 2.
- The panicking image never restarts the box (the old token still valid): update check 6 KO.
- A failed rollback: stop, the way back named.
- B9 reads a fresh boot (opening the port resets the box): the stack high-water marks after an
  install stay a person's check (roadmap).

## Sowel contract

None: the script calls Solite's API, which already mirrors Sowel's routes; it adds no type, field
or event.

## How the user will check it

On the Mac: `cargo run -p solite-host -- --sim scenarios/garage.json --serve --speed 10`, then
`SOLITE_PASSWORD=<a password> sh scripts/box-session.sh --api-only 8080` prints the report, every
API check OK and the box-only ones skipped; `sh scripts/box-session.sh` with no box plugged shows
P1 OK, B1 KO (no box) and every writing step skipped. At the next box session: `sh scripts/quiet.sh
--tail 60 sh scripts/box-session.sh --yes-flash --backup <the backup>` once, the report read in the
log, its result (date, `flash:` counts, the KO steps) written here.

## Decisions taken in this spec

- D55 (new): the `--yes-flash` the user types is their D6 yes for that run (refines D6 and D39).
- The factory reset is opt-in (`--factory-reset`): it erases the user's home on the box (the
  equipments, recipes and devices), which no other step of the session does.
- The session requires the backup that `box.sh flash` leaves optional (D35): one command that runs
  unattended flashes only a box whose way back is proven.
- The update key lives only for the run: a later update of that box needs a USB flash first (the
  next session's B2 does it).
- The update steps run outside `saves`: the box restarts during them, which `saves` reports as a
  lost hold; besides the inactive slot they write three boot markers and at most two final saves
  to `/data` (R3.4).
- The home group sets the user's real location and time zone from `SOLITE_HOME_LATITUDE`,
  `SOLITE_HOME_LONGITUDE` and `SOLITE_HOME_TIMEZONE` (decided by the main session, 2026-11-25), so
  026 and 027 are tested end to end: the settings, the sun times, the time zone, and their
  persistence across the update's restart. The behaviour by day (motion does not switch the light
  on) stays a person's check (H3).
- A generated password is kept in `$HOME/.config/solite/box-password`, mode 0600, outside the
  repository; `SOLITE_PASSWORD` overrides it (decided by the main session, 2026-11-25).

## Open questions

None: the three questions of the review were decided by the main session on 2026-11-25 (the
password file, the location from the environment, `--yes-flash` typed only by the user and
refused without a terminal), recorded above and in D55. The spec review was capped after four
rounds (the last passed).
