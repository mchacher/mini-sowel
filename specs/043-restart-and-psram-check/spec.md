# Spec 043: A Restart button, and the update's probation checks the PSRAM

Written and built during the night run of 2026-11-26, the user away (their plan: "small features,
each specified, tested, merged on green CI"). Two needs of the roadmap's inbox (2026-11-26). The two
questions the spec review raised were decided in the run and are marked **(decided in the run)**.

## Context

1. The box restarts by software only through its serial console or a change of Wi-Fi: the person in
   front of the interface has no way to restart it (box session of 2026-11-26).
2. Since D77 the PSRAM runs at 80 MHz. `CONFIG_SPIRAM_IGNORE_NOTFOUND` lets a box whose PSRAM fails
   to start boot without it. The update's probation (spec 032: confirm after 60 s healthy, roll back
   at 300 s) calls an image healthy when the engine runs and the API listens
   (`firmware/src/ota.rs:303`). Worse, without PSRAM the OTA thread does not start at all (its chunk
   buffer is PSRAM, `ota.rs:292`; `ota::start()` fails and is only logged, `main.rs:180`): the
   probation never ticks, the image is neither confirmed nor rolled back until the next reset.

## Goals

1. `POST /api/v1/system/restart` restarts the box (the Mac's host answers that it cannot), and
   Settings has a "Restart" card with a confirmation.
2. An image on probation is confirmed only with its PSRAM; without it, it is rolled back.

## Out of scope

- A restart into safe mode, a factory reset (spec 029's), a restart of the host program.

## Requirements

### R1: Restart

1. `POST /api/v1/system/restart`, the administrator's (spec 029's session; 401 without, the gate runs
   first), empty body. The core answers `Reply::Restart` (as `Reply::Update`): the firmware sends
   `202 { "restarting": true }` and then runs the existing `ota::restart_after_update()`
   (`ota.rs:408`): 500 ms for the answer to leave, the engine stopped (it saves as it stops), at most
   10 s, then `supervisor::asked_restart()` (the boot's cause `asked`, D58: not a crash, it clears
   the restart count and the safe mode) and `esp_restart`. The box answers again after its boot.
2. Refused with 409, the existing texts:
   - while an update is being received or installed: `Update already in progress`
     (`update/install.rs:131`); while an update check runs: `Update check already in progress`
     (`update/release.rs:574`); while a backup or restore runs: `Backup already in progress`
     (`backup/state.rs:66`);
   - **while a new image is on probation** (`Flash::pending`, `ota.rs:211`): `The update is being
     checked, restart in a minute` (a reset then makes the bootloader roll the update back)
     **(decided in the run: refuse; the person can wait a minute)**.
   Allowed: when an update is installed and waits for its restart, or a restore waits for its
   restart (the box is about to restart anyway): the same 202. While the pairing window is open: the
   restart closes it **(decided in the run: accept)**.
3. A second request while a restart is under way: 202 again, one restart (a static flag in the
   firmware).
4. The safe mode (D58, `safe_mode_refusal`, `api/mod.rs:416`) lets this route through, beside
   `auth/*`, `system/update` and `system/version/check`: restarting is the way out of it (the cause
   `asked` clears the count, `supervisor.rs:311-314`). In safe mode the engine is already stopped:
   no 10 s wait.
5. The Mac's host maps `Reply::Restart` to `501 { "error": "The host does not restart itself" }`.
6. Settings: a "Restart" card (Sowel's `card`, tokens) with a button; a confirmation ("Restart the
   box? It is back in about a minute."); then a message while the box is away; the page polls
   `GET /api/v1/system/version` every 2 s and reloads once it answers, **a 401 included** (the
   session ends with the restart: the reload shows the login); after 90 s without an answer, a
   message to reload by hand. A 409 shows its text and the button is enabled again. French and
   English. The interface's budget (170 KB gzip) holds.

### R2: The PSRAM in the probation

1. The core's probation takes a `Health { engine, api, psram }` in place of one boolean
   (`Probation::tick`, `probation.rs:33`; `Updates::watch`, `install.rs:256`); healthy is all
   three. The host passes `psram: true` (`crates/host/src/serve.rs:617`): it has no PSRAM to expect.
2. Every Solite image expects a PSRAM: the box is the Dongle Max (an ESP32-D0WDR2-V3, 2 MB in its
   package), and the memory budget does not hold without it (`docs/memory-budget.md`).
3. The OTA thread starts without PSRAM too: its chunk buffer is then absent, every update call
   answers 503 `No memory for an update`, and the probation ticks; the firmware's `healthy()` reads
   the PSRAM's size (`heap_caps_get_total_size(MALLOC_CAP_SPIRAM)`). An image without its PSRAM is
   therefore rolled back at 300 s.
4. The reason of an unhealthy probation is logged once, when it changes, not each second:
   `update: unhealthy: no PSRAM` (or `engine stopped`, `API not listening`), then the existing
   `update: not confirmed in time, rolling back` (`install.rs:269-283`).

## Living spec delta

### docs/living-spec/README.md

- The safe mode's exceptions (lines 430-432): `system/restart` added.
- The update's probation: healthy includes the PSRAM; without it the image is rolled back.
- Settings: the Restart card; the route among the system routes.
- History: spec 043's row.

### docs/sowel-subset.md

The route as a Solite extension (the system routes' table and the safe mode's list).

### docs/decisions.md

D79: the restart route and its place among the safe mode's exceptions (D58's list, extended by
D65, grows by one), refused during a probation.

### No longer true

- README.md 430-432 and D65's list (decisions.md:1281) of the routes the safe mode lets through
  (D65 is not edited: D79 extends it).
- The probation's "healthy = engine and API" wherever written.

## Acceptance criteria

- [ ] Core: `POST /system/restart` answers `Reply::Restart`; 401 without a session; 409 with each text
      of R1.2; allowed in safe mode while other `POST`s get 503; allowed when an update or a restore
      waits for its restart.
- [ ] Host: 501 with its text.
- [ ] Probation: rolls back without PSRAM, confirms with it; the reason logged once (core tests with a
      fake clock).
- [ ] The Restart card asks, calls, waits, reloads on an answer or a 401, says so after 90 s, shows a
      409 (UI tests, French and English).
- [ ] `sh scripts/validate.sh` green; the route in `docs/sowel-subset.md`; D79 written.
- [ ] Box (with the user, their flash): the button restarts the box, the boot's cause reads `asked`.

## Edge cases

| Case | Behaviour |
| ---- | --------- |
| Restart during the 60 s probation of a new image | 409 `The update is being checked, restart in a minute` |
| Two restart requests | 202 twice, one restart |
| Restart in safe mode | 202; no engine to stop; the restart leaves safe mode |
| Saves held (`saves hold`, the box tests' mode) | The engine's stop flushes them, as before an update's restart |
| The box does not come back within 90 s | The card says so, offers a reload |
| The session ends with the restart | The poll's 401 counts as an answer: reload, the login shows |
| PSRAM missing on an image on probation | OTA thread up without its buffer, rolled back at 300 s |
| PSRAM missing at the boot of an image already confirmed | Not the probation's: the `memory:` line shows no PSRAM |

## Sowel contract

No Sowel route restarts the engine; `POST /api/v1/system/restart` is a Solite extension, written in
`docs/sowel-subset.md`.

## How the user will check it

On the Mac: the card says the host does not restart. On the box (with the user, their flash): the
button restarts it, the page comes back by itself to the login; the box's log shows the cause
`asked`.
