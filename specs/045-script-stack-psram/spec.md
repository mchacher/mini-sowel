# Spec 045: The script thread's stack in PSRAM, the recipe thread made once

Assumes the recommended answers given to the user on 2026-11-27 (scope B, the box check below,
the watchdog watched); the user asked for an autonomous run and allowed the box work, flash
included (2026-11-27). Not merged before the user's yes.

## Context
The roadmap line "Spec 045, after F2" (2026-11-27). Measured on the box: with 2 script recipes the
internal RAM fell to 44 KB at its lowest (floor 64 KB, D27); after a few installs and uninstalls
the largest free internal block fell to 27 KB and an install was refused (`Not enough internal
memory`, 36 KB needed: the `recipe` thread's 20 KB stack, made per job, plus 16 KB for the TLS
session) until a restart. Two causes: the permanent `script` thread's 32 KB stack in internal RAM
(`firmware/src/http_server.rs:174-181`, D74), and a 20 KB block the `recipe` thread needs whole at
each job (`firmware/src/http_server.rs:998`, D78).

## Goals
- The internal RAM 32 KB higher for the whole run (the script thread's stack in PSRAM).
- An install no longer needs a whole 20 KB block at the time of the job.

## Out of scope
- The engine thread's 32 KB stack (it writes the flash: internal, D59).
- The Rust side's small blocks of a script recipe or an instance (`SPIRAM_MALLOC_ALWAYSINTERNAL`,
  the roadmap line: only if the minimum falls under about 80 KB).
- The interrupt-watchdog bug (roadmap inbox): watched during the box check; seen again, this
  spec's PSRAM stack is reconsidered before the merge.

## Requirements

### R1: the `script` thread's stack in PSRAM
- The `script` thread (made once at boot) gets its 32 KB stack in PSRAM
  (`MALLOC_CAP_SPIRAM | MALLOC_CAP_8BIT`), as the API workers' (spec 023).
- Allowed because nothing it runs touches the flash (D59: a flash access, a read too, turns the
  caches and so the PSRAM off, and ESP-IDF stops the chip when the caller's stack is in PSRAM).
  What it runs, checked one by one: the core's router for every non-GET `/api/v1/*` request but
  `/auth/*` (the handlers reach no `Storage`: the saves are written by the engine's saver; the
  streamed routes, update and backup, answer a ticket and run on job threads in internal RAM);
  the recipe jobs' load, swap and unload steps (memory only: the script's bytes are passed in).
  The boot load of the installed scripts reads the flash but runs on the main and engine
  threads, not this one.

### R2: a flash access from a PSRAM stack is refused, not a crash
- The box's storage (`firmware/src/storage.rs`, every read, write, list, remove, rename and
  usage) first checks that the calling task's stack is not in PSRAM
  (`esp_ptr_external_ram` on a local's address); if it is, the call returns an error
  `flash access from a PSRAM stack refused` and logs it once per boot, instead of the chip
  stopping. A guard for a future route, never a path in use.

### R3: the `recipe` thread made once
- The `recipe` thread (20 KB, internal RAM, core 0) is made once at boot, like `script`, and runs
  the recipe jobs and the catalogue refreshes one at a time (the one-job rule, 409, unchanged).
- The internal RAM admission of an install, update or refresh then needs only the TLS session's
  16 KB (`RECIPE_INTERNAL_MARGIN`); an uninstall needs nothing. The text is unchanged
  (`Not enough internal memory: largest free block <n> KB, <m> KB needed`).
- When the thread cannot be made at boot (no 20 KB block), the box logs it and every recipe job
  answers 503 `Busy` (as the script thread's absence does).
- The Mac host and the QEMU phase keep a job thread per job (their admission keeps the 36 KB).

### R4: QEMU
- The QEMU image puts its `script` thread's stack in PSRAM too (`CONFIG_SPIRAM_ALLOW_STACK_EXTERNAL_MEMORY`
  in its config), so the `script thread stack N of 32768 unused` check (at least 2 KB) measures
  the PSRAM stack; its `internal ... (min ... free)` figure is written in the memory budget.

## Living spec delta

### docs/living-spec/README.md
- 965 and the admission paragraph: on the box an install, update or refresh needs a 16 KB block of
  internal RAM (its TLS session); an uninstall needs none; the Mac's needs stay 36 and 20 KB.
- The `script` thread sentence (where the living spec says where scripts run): its stack is in
  PSRAM.

### No longer true
- `docs/memory-budget.md` 90 (the `script` thread's 32 KB in internal RAM, "added"), 94-95 (the
  transient `recipe` thread, the overlap), 106 (the room).
- `firmware/src/http_server.rs:173` comment (`its stack in internal RAM`).
- Decisions: a new D83 replaces D74's "every call into a script runs on a stack in internal RAM"
  (the `script` thread's calls) and D78's per-job `recipe` thread on the box.
- `docs/roadmap.md` 245 (the line goes: done).

## Acceptance criteria
- [ ] Tests (core): the admission with the thread kept (16 KB; uninstall never refused) and without
      it (36 and 20 KB, unchanged).
- [ ] validate green, QEMU included, `script thread stack` at least 2 KB unused in PSRAM.
- [ ] Box: 2 recipes, 8 instances of each, then 5 uninstall-install cycles of one recipe: every
      install accepted, the internal minimum at least 64 KB (B11.6), the garage still running.

## Edge cases
- No 20 KB block at boot for the `recipe` thread: logged, recipe jobs 503 `Busy`.
- A storage call from a PSRAM stack (none today): an error, logged, the chip keeps running.
- Two recipe jobs at once: the second is 409 as today (one job at a time).

## Sowel contract
None: no type, field or route.

## How the user will check it
The box check above, its figures written here; `GET /system/resources` before and after.
