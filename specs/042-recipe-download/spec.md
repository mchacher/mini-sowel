# Spec 042: Script recipes installed, updated and removed without a new firmware

Written during the night run of 2026-11-26, the user away; revised on 2026-11-27 after an
independent review and the user's ten decisions (see "Decided by the user").

## Context

Spec 041 (F1) runs script recipes, JavaScript in Sowel's script-recipe format, through QuickJS. The
scripts come from two places only: `--recipe-script` on the Mac (unsigned, for development) and the
files of `recipes/scripts/` built into the radio image (D76). A new recipe therefore needs a new
firmware. F2, ranked first by the user (roadmap, 2026-11-26), takes that away: a script recipe is
downloaded, checked against a signature made with a key of its own (not the firmware's), installed,
updated and removed while the box runs.

The box's memory is the limit (spec 041: 224 KB of PSRAM per script runtime, at most 2 script
recipes; the internal RAM read 74 KB at its lowest with one instance, the floor being 64 KB). F2
makes two script recipes on the box possible for the first time. The `script` thread's 32 KB stack
stays in internal RAM: moving it to PSRAM is spec 045, built after F2 and paused by the watchdog bug
of the roadmap's inbox (a PSRAM-stacked task meeting a flash access). F2 therefore adds no flash
access to any task whose stack is in PSRAM, and no new streamed route.

What is reused: the release chain of specs 032 to 040 (Ed25519 with `ed25519-dalek`'s
`verify_strict`, the signed 128-byte header, the `Fetcher` with its redirects, URL rule, resume and
pacing, the job threads with an internal-RAM stack, `Releases`' one-job mark) and the script engine
of spec 041 (load, bounds, failure and restart).

## Goals

1. A signed script file: the script's source behind a 128-byte header of its own (magic
   `SOLITERS`, not the firmware's `SOLITEUP`), signed with the **recipe key**, whose public half is
   built into the firmware; a file signed with any other key, or with the firmware's key, is refused.
2. A catalogue: a `recipes.json` manifest at a fixed address lists the recipes one can install. The
   box and the host read it on demand and hold it in RAM.
3. Install a recipe from the catalogue, or from a signed file the person uploads; update it when the
   catalogue has a newer version; remove it. The instances of a removed recipe are kept, stopped,
   and start again when it is installed again.
4. Admission by memory: an install is refused when the PSRAM free now, less the recipe's runtime
   limit, would be under a reserve, or when the internal RAM has no block for the job's thread. The
   refusal says why in plain words.
5. A "Resources" card in Settings: the internal RAM and the PSRAM (free now, lowest since boot), the
   storage used, and each script recipe with its version, its source, its memory held against its
   limit, its instances and the time of its calls.
6. `motion-light-script` leaves the radio image: every script recipe on the box is installed.

## Out of scope

- The rotation of the recipe key (a key that leaks is replaced by a firmware that carries the new
  public key; scripts are then signed again). Its design, like spec 040's bridge for the firmware
  key, is a line of the roadmap's inbox.
- Scripts in the backup archive (D54: an older release refuses an archive with an unknown file). A
  restore on the same box leaves the files: the installed recipes load at the restart and their
  restored instances start. On another box, the restored instances stay stopped (`Failed to restore:
  Recipe not found: <id>`) until the recipe is installed there (R3.4). A line of the inbox.
- Writing scripts in the interface, Sowel's AI drafting of scripts, the `draft/observing/active`
  states of Sowel's draft (F3 and Sowel's side).
- More than `MAX_SCRIPT_RECIPES` (2) script recipes: raising it is a decision of the user (D74).
- Unsigned scripts on the box. `--recipe-script` stays the Mac's developer flag.
- The `script` thread's stack in PSRAM (spec 045, after F2) and the watchdog bug it waits for.
- Removing the native `motion-light` recipe (a separate spec right after F2, an inbox line); the
  recipe SDK stays.
- Making the recipe key, the `mchacher/solite-recipes` repository, its CI and its secret: the user's.

## Requirements

### R1: The signed script file (`.srs`)

1. Layout: a header of 128 bytes, then the script's source (UTF-8). The whole file is at most
   16 KB (16,384 bytes, the API's body limit `MAX_BODY_BYTES`), so an installed source is at most
   16,256 bytes. Header: magic `SOLITERS` (8 bytes); format `1` (u32, little-endian); the recipe's
   version as three u16 (major, minor, patch); 2 reserved bytes at zero; the source's length (u32);
   its SHA-256 (32 bytes); 8 reserved bytes at zero; the Ed25519 signature (64 bytes) over bytes 0
   to 63.
2. Checks, in this order, each refusal 400 with its reason: length at least 129 and at most 16 KB;
   magic; format (a newer one: `format 2 is not supported`); reserved bytes; the length field equals
   the source's; the SHA-256; the signature with the built-in recipe key (`verify_strict`). Then the
   script is loaded as in spec 041.
3. Version: an installed script must declare `version` in `recipe({...})` as `x.y.z` (three numbers,
   each 0 to 65535), else refused `Script recipe invalid: version must be x.y.z`; it must equal the
   header's (`version differs from the signed one`: a signed header cannot be put in front of
   another version). Scripts given by `--recipe-script` keep spec 041's optional free `version`.
4. Id: the one the script declares, the only one: a catalogue entry whose id differs is refused. For
   an installed recipe it is 1 to 32 characters among `a-z`, `0-9` and `-` (the file name
   `scripts/<id>.srs` stays under the FAT's 64-character names). The ids `store` and `install` are
   reserved for every script (spec 041's load refuses them: `Script recipe invalid: id <id> is
   reserved`), so `/recipes/store` and `/recipes/install` never meet Sowel's `/recipes/:recipeId`.
5. The recipe key: the user's own pair (decided 2026-11-27): its public half committed as
   `release/recipe-key.pub`, built into the host and the firmware as `SOLITE_RECIPE_PUBLIC_KEY`
   (as `SOLITE_UPDATE_PUBLIC_KEY`, D52); its private half on the user's computer and as a secret of
   `mchacher/solite-recipes`' CI, never in this repository. `scripts/release.sh` and the release
   workflow refuse to build without `release/recipe-key.pub` (no release until the user has made the
   key); `box-session.sh` builds with it when the file exists. A build without the key answers every
   install, update and refresh with 503 `Recipe installs are not configured`. A key that is not 64
   hex characters or of small order (`parse_public_key`, `update/mod.rs:188-200`) is refused at
   start: logged `recipes: invalid recipe key, installs refused`, the same 503.
6. Tools on the host: `--recipe-sign <file.js> --key <file> --out <file.srs>` (the version and id
   read from the script; exit 2 when it declares no `x.y.z` version) and `--recipe-verify
   <file.srs>` (the checks of R1.2 and R1.3 with the built-in key; exit 1 with the reason); the key
   pair is made with the existing `--update-keygen`.

### R2: The catalogue

1. `recipes.json`, unsigned (each file it points to is signed, and pinned by its SHA-256):
   `{ "format": 1, "recipes": [ { "id", "version", "name", "description", "i18n"?, "icon"?,
   "size", "sha256", "url", "minVersion"? } ] }`, camelCase; at most 32 recipes and 32 KB; `size`
   at most 16 KB; `minVersion` is the lowest firmware that runs it. Refused as a whole (the last good
   list kept) when over a bound, not JSON, of another `format`, or when an entry's `url` breaks
   D66's URL rule (`url_allowed`, `update/release.rs:187`).
2. Its address: the setting `recipes.catalogue` (a Solite setting, as `update.manifestUrl`, D67):
   empty or unset means the default,
   `https://github.com/mchacher/solite-recipes/releases/latest/download/recipes.json` (a repository
   of its own: the firmware repository's "latest" release must stay the firmware's, spec 039). A
   value over 512 characters or breaking D66's rule is refused 400 at the setting's save, as the
   update URL.
3. `GET /api/v1/recipes/store` answers what the box holds, never the network: `{ "recipes": [ the
   entry, plus "installed": "<x.y.z>" | null (a loaded file's version), "refused": "<reason>" |
   null (a file of that id refused, R3.6), "builtIn": true | false (a native recipe or a script given
   by `--recipe-script` has that id) ], "readAt": "<ISO 8601>" | null, "error": "<reason>" | null }`;
   before any read, `{"recipes": [], "readAt": null, "error": null}`. The list is in RAM only.
4. `POST /api/v1/recipes/store/refresh` reads it again and answers as R2.3: at most once a minute
   (429 otherwise, as the version check); 503 while the clock is not set (`The clock is not set yet:
   reading the recipe catalogue needs the date`, as `release_ticket`); R3.1's 409s; R4.2's 503; a
   read that fails keeps the last list and answers it with `error` (200). The Recipes card asks it
   when it opens (a 429 then shows the list held).
5. The routes are Solite's, under `/recipes/*` (decided 2026-11-27): Sowel's plugin store
   (`/plugins/store`, `/plugins/install` with `{ repo }`) serves TypeScript packages, not scripts.

### R3: Install, update, remove

1. One job at a time. A **recipe job** is an install, an update or an uninstall; a **refresh** is
   R2.4. Both take `Releases`' one-job mark (`update/release.rs:488`, one TLS session at a time),
   under the home's lock. Each existing route keeps its 409 text; the new routes answer the text of
   what runs:

   | Asked | Refused with 409 while | Text |
   | ----- | ---------------------- | ---- |
   | A recipe job | a recipe job, a refresh, an update check (asked or automatic), a pull, an update received or installed, an update installed waiting for the restart, a backup or a restore running, a restore waiting for the restart | what runs: `Recipe install already in progress`, `Recipe catalogue refresh already in progress`, `Update check already in progress`, `Update already in progress`, `An update is installed and waits for the restart`, `Backup already in progress`, `A restore waits for the restart` |
   | A refresh | a recipe job, a refresh, an update check, a pull | what runs, as above |
   | `POST /system/version/check`, the automatic check | a recipe job, a refresh | `Update check already in progress` (the automatic check postponed as when busy) |
   | A pull (`POST /system/update`, empty body) | a recipe job, a refresh | `Update already in progress` |
   | An upload (`POST /system/update`, a body) | a recipe job | `Update already in progress` |
   | A backup or a restore | a recipe job | `Backup already in progress` |
   | `POST /system/restart` (D79) | a recipe job, a refresh | `Recipe install already in progress`, `Recipe catalogue refresh already in progress` |
   | `POST /system/factory-reset` | a recipe job | `Recipe install already in progress` |

   A refresh does not refuse an upload, a backup or a restore, nor they it.
2. `POST /api/v1/recipes/install` with `{ "id": "<id>" }` installs the catalogue's entry: when no
   catalogue is held, the job reads it first (as R2.4, the minute's limit aside); the file is
   downloaded (resumed as an update's, D72, at most three times, then 502), its size and SHA-256
   checked against the entry, the header's version against the entry's (a stale or forged
   catalogue cannot offer an older signed file under a newer number), then R1's checks. With a body
   of type `application/octet-stream` (at most 16 KB, an ordinary body), it installs that signed file
   instead: any signed version of a recipe not installed; for an installed one, R3.5's rule.
3. Refused, in this order: R1.5's 503; R3.1's 409; R4.2's 503; the id is a native recipe's or a
   `--recipe-script`'s (409 `<id> is built in`); a loaded file has that id (409 `<id> is installed:
   update it`); `MAX_SCRIPT_RECIPES` files (loaded or refused) are on the flash and none has that id
   (409 `Too many script recipes (at most 2)`); the catalogue does not list it (404 `Recipe not in
   the catalogue: <id>`); the entry needs a newer firmware (409 `<id> <x.y.z> needs Solite
   <minVersion> or later`); the catalogue unreachable or the download failed (502 with the reason);
   size, SHA-256 or version differs from the entry (400); R1's checks (400); R4.1 (409).
4. Installed: the recipe is loaded, the signed file written to the data folder (`scripts/<id>.srs`,
   one flash write, replacing a refused file of that id), and the instances kept for that id
   (stopped since a removal, a restore or this image's first boot, with `Failed to restore: Recipe
   not found: <id>`) start if they are enabled; 201 with the recipe's `RecipeInfo` and its
   `version`. The write fails: the recipe is unloaded again, 500 `Could not save the recipe`.
5. `POST /api/v1/recipes/:id/update`: the catalogue's version must be newer than the installed one
   (409 `<id> <x.y.z> is not newer than <x.y.z>`). The instances stop, the old runtime is dropped,
   the new file is loaded; when it loads, it is written over the old one and the instances start
   again with their settings (a setting the new version refuses leaves that instance stopped with
   its reason); when it does not load, the old file is loaded again, its instances start, and the
   answer is the reason (400). When the write fails, the new runtime is dropped and the old file
   loaded again (500 `Could not save the recipe`). When the old file does not load again either,
   the recipe is failed as in spec 041 (instances stopped, the reason in their log, its restarts
   from the old file) and the answer is 500 with both reasons. R4.1 counts the old runtime's memory
   as freed. The upload form of R3.2 updates too, with the same rules (strictly newer).
6. `POST /api/v1/recipes/:id/uninstall`: a native recipe or a `--recipe-script` (409 `<id> is built
   in`); no file of that id (404). The instances stop and are kept (`Recipe not found: <id>` in
   their log), the runtime is dropped, the file removed (a refused file too); 204. The removal fails:
   500 `Could not remove the recipe` (the runtime stays dropped; the file loads again at the next
   boot).
7. At boot, after any `--recipe-script` and **before the instances start** (`start_all`), the
   files of `scripts/` are read in the order of their ids (a lone `<id>.srs.new` left by a power cut
   first promoted, `.tmp` ignored) and go through R1's checks again, `MAX_SCRIPT_RECIPES` and R4.1;
   the enabled instances of the loaded recipes then start as any other. A file refused is left in
   place, logged `recipes: script <id>: <reason>`, shown `refused` with its reason (R2.3, R5.1); an
   uninstall removes it, an install or an upload of that id replaces it. An id a native recipe or a
   `--recipe-script` also has: the built-in one wins, the file is refused (`built in`).
8. Every route above is the administrator's (the session of spec 029). In the supervisor's safe
   mode (D58) no file is loaded at boot (each shown `refused`, `Safe mode: not loaded`), the `GET`s
   answer, `POST /recipes/:id/uninstall` is let through (a new exception, as D79's restart: the way
   out of a script that breaks the engine), and install, update and refresh answer 503.
9. The factory reset (spec 029) removes the installed recipes (decided 2026-11-27): their runtimes
   are dropped under the home's lock with the rest, and the saver removes their files on the engine
   thread with its other writes (retried as its writes are when one fails).
10. `motion-light-script` leaves the radio image (decided 2026-11-27): the image builds in no script
    recipe, so both script places are free for installs. Its source stays in this repository
    (`recipes/scripts/motion-light-script.js`, read as a file by the Mac's tests, `--recipe-script`
    and the QEMU image's script phase); `mchacher/solite-recipes`' CI signs it from a pinned commit
    of this repository, the catalogue's first entry. A box updated to this image keeps its instances
    of it, stopped and logging `Failed to restore: Recipe not found: motion-light-script` at every
    boot, until the recipe is installed: they then start again (R3.4). The user's garage runs such an
    instance today and accepts that it stops from the F2 flash until the key, the repository and the
    install exist (decided 2026-11-27).

### R4: Admission by memory

1. PSRAM: a recipe is admitted when `psram_free − SCRIPT_MEMORY_LIMIT ≥ SCRIPT_PSRAM_RESERVE`, where
   `psram_free` is the PSRAM free now (the lowest since boot never comes back up after a passing
   peak, a TLS handshake or a backup, and already holds the scripts loaded: it is shown, not used)
   and `SCRIPT_PSRAM_RESERVE` is 512 KB (decided 2026-11-27). The reserve is measured against the
   realistic home (memory budget: 990 KB of margin); the budget's worst case (330 KB of margin with
   two scripts at their limit) is under it, so in that case a second script is refused, by design.
   Refused 409 `Not enough memory for <id>: <n> KB free, <m> KB needed`. Checked at install, update
   and boot.
2. Internal RAM: before a recipe job or a refresh starts its thread, the largest free block of
   internal RAM must be at least `FETCH_STACK_BYTES` (20 KB, the job's thread) plus
   `RECIPE_INTERNAL_MARGIN` (16 KB: the HTTP client's 5 KB and mbedTLS's 8 KB of small internal
   allocations, rounded up), else 503 `Not enough internal memory: largest free block <n> KB, <m> KB
   needed`.
3. Instances: the 8 of D74 stay the bound; a runtime that reaches its limit fails its recipe as in
   spec 041. No rule of its own.
4. The memory is read through an adapter of the core, `MemoryProbe`, over the types
   `crates/core/src/memory.rs` already has (`MemoryStats`, `HeapStats`: free, lowest and largest
   block of each memory). The firmware reads ESP-IDF's heap; the host reads none (`null`: its
   admission always passes; a test fake covers the refusals).

### R5: The Resources card

1. `GET /api/v1/system/resources` (a Solite extension, administrator): `{ "internal": { "free",
   "minFree", "largestBlock" } | null, "psram": { "free", "minFree", "total", "reserve" } | null,
   "storage": { "used", "total" | null }, "scripts": [ { "id", "version" | null, "source": "builtIn"
   | "installed", "state": "ready" | "failed" | "refused", "reason" | null, "held", "limit",
   "instances", "calls", "averageUs", "maxUs" } ] }`, bytes and microseconds as numbers. `builtIn`
   is a script given by `--recipe-script` (the Mac, and the QEMU image's phase); the box has only
   `installed`. A refused file has `held` 0. The Mac answers `internal` and `psram` `null` and
   `storage.total` `null` (`used`: its data folder's files).
2. The storage figures are never read by the `GET`: the API's workers run on PSRAM stacks, which
   must not touch the flash (`firmware/src/http_server.rs:57-59`). They are read at boot and after
   each write or removal, on the thread that wrote (the saver's engine thread, the recipe job's
   thread), and kept.
3. Settings gets a "Resources" card (Sowel's `card`, tokens and text sizes): the two memories as a
   bar each (used, lowest free and the PSRAM reserve marked), the storage, and a row per script
   recipe; a "Recipes" card beside it lists the catalogue with Install, Update and Remove, the
   refused files with their reason and Remove, the error of the last read, and an "Upload a signed
   recipe" button. A row hidden when its figure is `null`. French and English texts. The interface's
   budget (170 KB gzip) holds; the added size is written in `docs/design-system.md`.

### R6: Bounds and cost

1. `MAX_SCRIPT_RECIPES` stays 2 (files on the flash, loaded or refused, and `--recipe-script`
   together), the signed file 16 KB, the catalogue 32 entries and 32 KB, one recipe job or refresh at
   a time.
2. The flash: one write per install, update or removal, none at boot (D39); measured on the box with
   `box.sh saves`.
3. No flash access from a task whose stack is in PSRAM: the download, the checks, the write and the
   removal run on a `recipe` job thread in internal RAM (`FETCH_STACK_BYTES`); the load and the
   unload on the `script` thread (the JavaScript stack, no flash); the boot load and the factory
   reset's removals on the engine thread.
4. The image: the added code measured in the flash table of `docs/memory-budget.md`, under 75 % of
   the slot (the radio image loses `motion-light-script`'s 15 KB of source).
5. The memory budget gets the catalogue (32 KB, PSRAM), the signed file held whole to be checked
   (16 KB, PSRAM, for a job's length), the two files on the FAT, the `recipe` thread (20 KB internal,
   one job at a time, excluded by R3.1 from a pull, an upload and a backup: the worst overlap of the
   job threads stays 51 KB), and the `script` thread's time: a load (up to 1000 ms) runs on the
   thread of the API's non-`GET` requests (D74), which wait meanwhile.

### R7: The box check

One pass (D39), the user present, after their yes to flash (D6, their own `--yes-flash`):

1. Before it: the user has made the recipe key (`--update-keygen`, outside the repository),
   committed `release/recipe-key.pub`, and signed three files with it (`--recipe-sign`, in a
   temporary folder, never committed): `motion-light-script`, and `motion-light-check` (the same
   script with that id) in 1.0.0 and 1.0.1. The image is built with that public key
   (`box-session.sh` reads `release/recipe-key.pub`). No throwaway key: a file signed with one would
   be refused at boot by the next real release.
2. From the flash until the install, the garage's `motion-light-script` instance is stopped and logs
   `Failed to restore: Recipe not found: motion-light-script` at each boot (R3.10, accepted).
3. A test catalogue served from the Mac on the home network (http, D66) with `motion-light-script`
   and `motion-light-check` 1.0.0, then 1.0.1; the setting `recipes.catalogue` pointed at it, put
   back at the end.
4. Install both; the garage's instance starts again; eight enabled instances (four of each) on the
   garage's zone; update `motion-light-check` to 1.0.1 with the instances running; the `heap:`
   line's internal RAM minimum stays at least 64 KB (`INTERNAL_MIN_KB`) through it; the Resources
   card shows the PSRAM, the two recipes and the storage.
5. The check's instances deleted, `motion-light-check` uninstalled, `motion-light-script` kept (the
   garage uses it); the flash writes counted by `box.sh saves` (one per install, update and removal).
6. B11 (spec 041's script group): when `motion-light-script` is not on the box, it uploads the
   file given by `--recipe-file <file.srs>`, or reports B11 skipped (`not installed:
   --recipe-file`) without it.

## Living spec delta

### docs/living-spec/README.md

- "Runs script recipes": where scripts come from (`--recipe-script` on the Mac, installed files on
  both; none built in), the signed file, the catalogue, install, update, uninstall, the boot's
  checks, the two memory rules, the reserved ids, the `x.y.z` version of an installed script.
- "Runs the motion-light recipe": the box runs `motion-light-script` once installed.
- "Serves Sowel's device API" (the recipe routes): the new routes, as Solite extensions.
- "Backs up and restores the whole box": scripts are not in the archive; a restore leaves them (the
  out-of-scope sentence's two cases); a backup or restore is refused during a recipe job.
- "Protects the home with a password": the factory reset removes the installed recipes and is
  refused during a recipe job; the safe mode's routes let through gain the uninstall.
- "Updates itself from a signed file": "One at a time" and "Restarts on request" gain R3.1's rows.
- "Supervises its engine": in safe mode no installed script is loaded, the uninstall goes through.
- "Shows the home in the browser": the Resources and Recipes cards.
- "Publishing a release": the image carries the recipe key; `release.sh` and the release workflow
  refuse to build without `release/recipe-key.pub`.
- "Runs a box session in one command": B11 with `--recipe-file`; the F2 group (R7).
- "Stays within a memory budget": the `recipe` thread and the internal RAM rule.
- New section "Shows its resources": the route and the card.
- History: spec 042's row.

### docs/sowel-subset.md

The routes of R2.3, R2.4, R3.2, R3.5, R3.6 and R5.1 and the setting `recipes.catalogue` in the
table of Solite's names (D78); the safe mode's paragraph gains the uninstall; the restart's
paragraph gains the recipe job and the refresh; "Script recipes" gains the reserved ids and the
`x.y.z` version of an installed script.

### Other docs

`docs/architecture.md` (the script recipes' and firmware's rows, the recipe key beside the update
key, the `recipe` thread), `docs/memory-budget.md` (R6.5, the flash table), `docs/release.md` and
`release/README.md` (the recipe key's custody, as D64's), `docs/design-system.md` (the size),
`docs/decisions.md` (D78), `docs/roadmap.md` (F2 done; the inbox lines of 2026-11-26 on admission
and the Resources card done; "Before F2" reworded as spec 045 after F2).

### No longer true

- D76's "Downloading, signing ... and removing a script are F2", and its "every release carries
  ... `motion-light-script`" (D78 records what F2 does; D76 is not edited).
- Living spec, "Runs the motion-light recipe" (around line 747): "The host and the box also run ...
  `motion-light-script`"; "Runs script recipes" (around line 840): "The box's radio image carries
  the scripts of `recipes/scripts/` and loads them at start. A script is not downloaded or signed
  yet."
- `docs/architecture.md`: line 47 ("the radio image and the QEMU image have them built in
  (`firmware/build.rs` lists them)"), line 55 (`scripts.rs`, "the scripts built in"), line 92
  (`firmware/build.rs` lists `recipes/scripts/*.js`).
- `docs/memory-budget.md`: line 59 ("the release check and pull": recipe downloads and refreshes
  share the one session), line 62 ("in flash for the scripts built in"), line 128's image row
  (`motion-light-script` in the image) gains F2's row, lines 149-150 (the sources of
  `recipes/scripts/` in flash).
- The roadmap's inbox line "Before F2: move the script recipes' internal RAM to PSRAM" (the user put
  F2 first on 2026-11-27).

## Acceptance criteria

- [ ] A script signed with the recipe key installs from the catalogue and from an upload; one signed
      with the firmware's key, or any other, or changed by one byte, or of 16 KB + 1, is refused
      with its reason.
- [ ] Update replaces the runtime and restarts the instances; a new version that does not load leaves
      the old one running; uninstall keeps the instances stopped, and a new install starts them.
- [ ] Every row of R3.1's table is a test, with its text.
- [ ] An install that would take the PSRAM under the reserve, or find the internal RAM's largest
      block too small, is refused with the figures (a fake probe on the Mac); a third script recipe
      is refused.
- [ ] At boot, installed files are checked again and loaded before the instances start; a changed
      file, a format 2, a third file and a lone `.new` behave as R3.7 says.
- [ ] `GET /api/v1/system/resources` answers on the Mac (`internal` and `psram` `null`, the scripts'
      rows) and the card shows it; on the box the figures, the storage read without a flash access
      from the worker.
- [ ] The new routes are in `api::ROUTES` and `docs/sowel-subset.md`, `recipes.catalogue` in
      `settings::KEYS` and its table; `crates/core/tests/sowel_contract.rs` is green.
- [ ] The radio image carries no script (`scripts/validate.sh` searches its ELF); the QEMU image's
      step installs a signed file and its `recipe: thread stack` line passes.
- [ ] Box check (R7) passed with the user's real key.
- [ ] D78 written; `sh scripts/validate.sh` green.

## Edge cases

| Case | Behaviour |
| ---- | --------- |
| No network, catalogue unreachable | Refresh 200 with the last list and `error`; install 502 with the reason |
| Catalogue over 32 KB or 32 entries, bad JSON, a `url` breaking D66 | Refused as a whole, the last good list kept, `error` set |
| Install before any catalogue read | The job reads it first |
| Clock not set | Refresh, install and update from the catalogue 503 |
| Download cut | Resumed as an update's (D72), at most three times; then 502 |
| Size, SHA-256 or version differs from the catalogue | 400, nothing written |
| Script declares another version than its header, or no `x.y.z` | 400 with the reason |
| An upload of 16 KB + 1 | 413, as every body over the limit |
| A script with the id `store` or `install` | Refused at load, by every source |
| The flash write fails | The recipe is dropped again, 500 `Could not save the recipe` |
| Power lost during the write | The save's atomic rename keeps the old file or the new one, never half; a lone `.new` is promoted at boot |
| Uninstall while an instance's call runs | The home lock serialises: the call ends, then the removal |
| A file refused at boot | Shown `refused` with its reason; uninstall removes it; install replaces it |
| An installed id a native recipe or `--recipe-script` also has | The built-in one wins at boot, the file refused (`built in`) |
| Update: the write fails after the new version loaded | New runtime dropped, old file loaded again, 500 |
| Update: the old file does not load again | Recipe failed (spec 041), instances stopped with the reason, 500 |
| Safe mode | No file loaded; uninstall allowed; install, update, refresh 503 |
| A rollback to an older firmware | It ignores `recipes.catalogue` and the `scripts/` files (it never reads them); a newer firmware finds them again |
| Factory reset | Runtimes dropped, files removed by the saver; refused 409 during a recipe job |
| A build without the recipe key | Install, update, refresh 503 `Recipe installs are not configured` |
| `recipes.catalogue` over 512 characters or breaking D66 | 400 at the setting's save; empty means the default |

## Sowel contract

No Sowel type changes: `RecipeInfo` is served as in spec 041. Sowel has no script distribution yet
(its draft keeps scripts in its database). Sowel's plugin store (`docs/technical/api-reference.md`,
`/plugins/store`, `/plugins/install` with `{ repo, confirmed?, expectedSha256? }`, entries with a
`tier`) serves TypeScript packages; Solite's routes are its own, under `/recipes/*` with `{ id }`, a
deviation written in D78 and in `docs/sowel-subset.md`'s table. `/recipes/store` and
`/recipes/install` sit beside Sowel's `GET /api/v1/recipes/:recipeId`
(`../sowel/src/api/routes/recipes.ts:45`); the reserved ids keep them apart. The setting
`recipes.catalogue` is Solite's (as `update.manifestUrl`, D67). A Sowel issue (inbox): the signed
script file and the catalogue as a common format, so Sowel and Solite share one.

## How the user will check it

1. On the Mac: `cargo run -p solite-host -- --update-keygen <file>` once for a test recipe key (the
   Mac only), `--recipe-sign recipes/scripts/motion-light-script.js --key <file> --out <file.srs>`,
   a local `recipes.json` served from the Mac, the host built with that public key
   (`SOLITE_RECIPE_PUBLIC_KEY`): install, update to a copy whose script says `version: "1.0.1"`,
   uninstall, in the interface; a file signed with another key refused.
2. The Resources card on the Mac (no memory figures: the storage and the scripts' rows).
3. On the box: R7.

## Decided by the user (2026-11-27)

1. The catalogue: a repository of its own, `mchacher/solite-recipes`, whose CI signs with the recipe
   key. Making that repository, its CI and its secret is the user's (an outward action, like spec
   038's release secret); this spec builds the box's side, the signing tool and the format.
2. The PSRAM reserve: 512 KB.
3. `motion-light-script` leaves the radio image (R3.10); its source stays in `recipes/scripts/` (the
   tests read it), `mchacher/solite-recipes`' CI signs it from a pinned commit.
4. The routes under `/recipes/*`, the ids `store` and `install` reserved.
5. The factory reset removes the installed recipes, through the saver and by dropping the runtimes,
   never a flash access from a PSRAM stack.
6. F2 is built now; spec 045 (the `script` thread's stack in PSRAM) comes after. So F2 adds no flash
   access from a PSRAM-stacked task (writes and removals on an internal-RAM job thread or the saver;
   the storage figures read at boot and after writes, kept), no new streamed route (the signed file
   at most 16 KB, an ordinary body), the internal RAM rule of R4.2, and R7's check with two recipes
   and eight instances.
7. The box check uses the user's real recipe key (made before the run, never in the repository; its
   public half committed as `release/recipe-key.pub`); `release.sh` refuses to build without it; no
   release with recipes until the key exists.
8. `POST /api/v1/recipes/:id/uninstall` is allowed in safe mode (a new exception, as D79's restart).
9. The garage's `motion-light-script` instance stops from the F2 flash until the key, the repository
   and the install exist (R3.10, R7.2), then starts again.
10. Removing the native `motion-light` is a separate spec right after F2; the recipe SDK stays.

## Decided in the run (2026-11-27, the user away, recorded for their review)

1. R7's check holds **eight instances of each recipe** (16 in all, `MAX_INSTANCES`): the worst case
   the bounds allow, the one whose internal RAM minimum matters.
2. `motion-light-check`, a second test recipe signed with the user's real key for R7, is accepted:
   it is harmless, never in the catalogue, and removed at the check's end.
3. Without `release/recipe-key.pub`, `release.sh` refuses every release, not only one "with
   recipes": since R3.10 no release carries a script recipe, so one without the key could never
   install any. No release until the user makes the key.
