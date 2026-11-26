# Spec 042: Script recipes installed, updated and removed without a new firmware

Written during the night run of 2026-11-26, the user away, **for their review**: nothing of it is
built. The choices marked **(to confirm)** are the user's; the spec gives a recommendation for each.

## Context

Spec 041 (F1) runs script recipes, JavaScript in Sowel's script-recipe format, through QuickJS. The
scripts come from two places only: `--recipe-script` on the Mac (unsigned, for development) and the
files of `recipes/scripts/` built into the radio image (D76). A new recipe therefore needs a new
firmware. F2, ranked first by the user (roadmap, 2026-11-26), takes that away: a script recipe is
downloaded, checked against a signature made with a key of its own (not the firmware's), installed,
updated and removed while the box runs.

The box's memory is the limit (spec 041: 224 KB of PSRAM per script runtime, at most 2 script
recipes; the internal RAM read 74 KB at its lowest with one instance). The roadmap's inbox asks for
an install that looks at the memory really free, refuses with a plain message, and a "Resources" view
that shows the user the box's state (2026-11-26).

What is reused: the release chain of specs 032 to 040 (Ed25519 with `ed25519-dalek`'s
`verify_strict`, the signed 128-byte header, the `Fetcher` with its redirects, URL rule, resume and
pacing, the job threads with an internal-RAM stack) and the script
engine of spec 041 (load, bounds, failure and restart).

## Goals

1. A signed script file: the script's source behind a 128-byte header of its own (magic
   `SOLITERS`, not the firmware's `SOLITEUP`), signed with the **recipe key**, whose public half is
   built into the firmware; a file signed with any other key, or with the firmware's key, is refused.
2. A catalogue: a `recipes.json` manifest at a fixed address lists the recipes one can install
   (id, version, name, description, size, SHA-256, address of the signed file). The box and the
   host read it on demand.
3. Install a recipe from the catalogue, or from a signed file the person uploads; update it when the
   catalogue has a newer version; remove it. The instances of a removed recipe are kept, stopped,
   and start again when it is installed again.
4. Admission by memory: an install is refused when the box's PSRAM free now, less the recipe's
   runtime limit, would be under a reserve. The refusal says why in plain words.
5. A "Resources" card in Settings: the internal RAM and the PSRAM (free now, lowest since boot), the
   storage used, and each script recipe with its version, its source (built in or installed), its
   memory held against its limit, its instances and the time of its calls.

## Out of scope

- The rotation of the recipe key (a key that leaks is replaced by a firmware that carries the new
  public key; scripts are then signed again). Its design, like spec 040's bridge for the firmware
  key, is a line of the roadmap's inbox.
- Scripts in the backup archive (D54: an older release refuses an archive with an unknown file).
  After a restore, the instances of an installed recipe come back stopped; installing the recipe
  again starts them. A line of the inbox.
- Writing scripts in the interface, Sowel's AI drafting of scripts, the `draft/observing/active`
  states of Sowel's draft (F3 and Sowel's side).
- More than `MAX_SCRIPT_RECIPES` (2) script recipes, built in and installed together: raising it is
  a decision of the user (D74).
- Unsigned scripts on the box. `--recipe-script` stays the Mac's developer flag.

## Requirements

### R1: The signed script file (`.srs`)

1. Layout: a header of 128 bytes, then the script's source (UTF-8, at most `MAX_SCRIPT_BYTES`,
   16 KB). Header: magic `SOLITERS` (8 bytes); format `1` (u32, little-endian); the recipe's version
   as three u16 (major, minor, patch); 2 reserved bytes at zero; the source's length (u32); its
   SHA-256 (32 bytes); 8 reserved bytes at zero; the Ed25519 signature (64 bytes) over bytes 0 to 63.
2. Checks, in this order, each refusal with its reason: length at least 128 and at most 128 + 16 KB;
   magic; format; reserved bytes; the length field equals the source's; the SHA-256; the signature
   with the built-in recipe key (`verify_strict`; a key of small order refused when the firmware is
   built). Then the script is loaded as in spec 041, and the version it declares in `recipe({...})`
   must equal the header's (a signed header cannot be put in front of another version).
3. The recipe's id is the one its script declares, the only one: a catalogue entry whose id differs
   is refused; for an installed recipe it is 1 to 32 characters among `a-z`, `0-9` and `-` (spec 041
   allows 64: the file name `scripts/<id>.srs` stays under the FAT's 64-character names).
4. A file of a newer `format` than the firmware knows is refused with its reason, at install and at
   boot (`format 2 is not supported`).
5. The recipe key: `release/recipe-key.pub` in the repository (the public half only), built into the
   host and the firmware as `SOLITE_RECIPE_PUBLIC_KEY` (as `SOLITE_UPDATE_PUBLIC_KEY`, D52). A build
   without it answers every install with 503 `Recipe installs are not configured`. The private half is
   the user's, never in the repository: on their computer, and as a secret of the recipes' CI
   **(to confirm, R3)**.
4. Tools on the host: `--recipe-sign <file.js> --key-file <key> --out <file.srs>` and
   `--recipe-verify <file.srs>`; the key pair is made with the existing `--update-keygen`.

### R2: The catalogue

1. `recipes.json`, unsigned (each file it points to is signed, and pinned by its SHA-256):
   `{ "format": 1, "recipes": [ { "id", "version", "name", "description", "i18n"?, "icon"?,
   "size", "sha256", "url", "minVersion"? } ] }`, camelCase; at most 32 recipes and 32 KB;
   `minVersion` is the lowest firmware that runs it.
2. Its address: a setting `recipes.catalogue` with a default **(to confirm)**: the latest release of a
   repository of its own, `github.com/mchacher/solite-recipes`
   (`releases/latest/download/recipes.json`). It is not the firmware's repository: its "latest"
   release must stay the firmware's (spec 039). The URL rule of D66 applies (https anywhere, http on
   the home network only).
3. `GET /api/v1/recipes/store` answers the catalogue as the box last read it, each entry with
   `installed` (the version installed, or none) and `builtIn`; `POST /api/v1/recipes/store/refresh`
   reads it again (at most once a minute, as the version check; 429 otherwise). These routes are
   Solite's (D78): Sowel's plugin store (`/plugins/store`, `/plugins/install` with `{ repo }`) serves
   TypeScript packages, not scripts **(to confirm: names under `/recipes/*`, recommended)**.

### R3: Install, update, remove

1. `POST /api/v1/recipes/install` with `{ "id": "<id>" }` installs the catalogue's entry: the file is
   downloaded, its size and SHA-256 checked against the catalogue, the header's version against the
   entry's (a stale or forged catalogue cannot offer an older signed file under a newer number),
   then R1's checks. Today the update (`Updates::claim`, `update/install.rs:122`) and the backup
   (`backup/state.rs:59-66`) each have a guard of their own; F2 puts the three jobs under one guard:
   while one runs, the others answer 409 with the running job's name (`An update is in progress`,
   `A backup is in progress`, `A recipe install is in progress`). With a body of type
   `application/octet-stream`, it installs that signed file instead (the person's upload: any
   signed version of a recipe not installed, since the person chose the file; the update rule of
   R3.4 for one installed).
2. Refused when: the id is a built-in recipe's or a built-in script's (409); the id is installed
   (409, use update); `MAX_SCRIPT_RECIPES` are loaded (409 `Too many script recipes (at most 2)`);
   R4's memory rule; R1's checks (400, the reason); the catalogue does not list it (404); the
   catalogue entry needs a newer firmware (409 with `minVersion`).
3. Installed: the signed file is written to the data folder (`scripts/<id>.srs`, one flash write),
   the recipe is loaded, and the instances kept for that id (stopped since a removal or a restore,
   with spec 012's `Recipe not found: <id>`) start if they are enabled; `201` with the recipe's `RecipeInfo` and its version.
4. `POST /api/v1/recipes/:id/update`: the catalogue's version must be newer than the installed one
   (409 otherwise). The instances stop, the old runtime is dropped, the new file is loaded; when it
   loads, it is written over the old one and the instances start again with their settings (a
   setting the new version refuses leaves that instance stopped with its reason); when it does not
   load, the old file is loaded again, its instances start, and the answer is the reason (400). When
   the write fails, the new runtime is dropped and the old file loaded again (500 `Could not save
   the recipe`). When the old file does not load again either, the recipe is failed as in spec 041
   (instances stopped, the reason in their log, its restarts from the old file) and the answer is
   500 with both reasons. The admission of R4 counts the old runtime's memory as freed. The upload
   form of R3.1 updates too, with the same rules (strictly newer).
5. `POST /api/v1/recipes/:id/uninstall`: built-in scripts cannot be removed (409). The instances
   stop and are kept (`Recipe not found: <id>` in their log, as for an instance restored without its
   recipe), the runtime is dropped, the file removed.
6. At boot, after the built-in scripts and **before the instances start** (`start_all`), the
   installed files are read in the order of their ids and go through R1's checks again (a file
   changed on the flash is refused, logged `recipes: script <id>: <reason>`, left in place), R4's
   rule and `MAX_SCRIPT_RECIPES`; the enabled instances of the installed recipes then start as any
   other. An id that a built-in script also has: the built-in wins, the file is left and logged.
7. Every route above is the administrator's (the session of spec 029). In the supervisor's safe mode
   (D58: non-`GET` routes refused, the update and the version check excepted) these routes are not
   excepted, the `GET`s answer, and no installed script is loaded at boot.
8. The factory reset (spec 029) removes the installed files; a backup's restore leaves them (they
   are not in the archive).

### R4: Admission by memory

1. A recipe is admitted when `psram_free − SCRIPT_MEMORY_LIMIT ≥ SCRIPT_PSRAM_RESERVE`, where
   `psram_free` is the PSRAM free now (the lowest since boot never comes back up after a passing
   peak, a TLS handshake or a backup, and already holds the scripts loaded: it is shown, not used)
   and `SCRIPT_PSRAM_RESERVE` is 512 KB **(to confirm)**. The reserve is measured against the
   realistic home (memory budget: 990 KB of margin); the budget's worst case (330 KB of margin with
   two scripts at their limit) is under it, so in that case a second script is refused, by design.
   Refused: `Not enough memory for <id>: <n> KB free, <m> KB needed`.
2. Instances: the 8 of D74 stay the bound; a runtime that reaches its limit fails its recipe as in
   spec 041. No rule of its own.
3. The memory is read through an adapter of the core, `MemoryProbe`, over the types
   `crates/core/src/memory.rs` already has (`MemoryStats`, `HeapStats`: free, lowest and largest
   block of each memory), with the storage used and its size added. The firmware reads ESP-IDF's
   heap and the FAT; the host answers the figures it can read and no PSRAM (its admission always
   passes; a test fake covers the refusals).

### R5: The Resources card

1. `GET /api/v1/system/resources` (a Solite extension, administrator): `{ "internal": { "free",
   "minFree", "largestBlock" }, "psram": { "free", "minFree", "total" } | null, "storage": { "used",
   "total" }, "scripts": [ { "id", "version", "source": "builtIn" | "installed", "state": "ready" |
   "failed", "held", "limit", "instances", "calls", "averageUs", "maxUs" } ] }`, bytes and
   microseconds as numbers.
2. Settings gets a "Resources" card (Sowel's `card`, tokens and text sizes): the two memories as a
   bar each (used, lowest free marked), the storage, and a row per script recipe; a "Recipes" card
   beside it lists the catalogue with Install, Update and Remove, and an "Upload a signed recipe"
   button. French and English texts. The interface's budget (170 KB gzip, 77.9 KB after spec 040)
   holds; the added size is written in `docs/design-system.md`.

### R6: Bounds and cost

1. `MAX_SCRIPT_RECIPES` stays 2 (built in and installed together), `MAX_SCRIPT_BYTES` 16 KB, the
   catalogue 32 entries and 32 KB, one download at a time, installed files at most 2 on the flash
   (16 KB + 128 B each).
2. The flash: one write per install, update or removal, none at boot (D39); measured on the box with
   `box.sh saves`.
3. The image: the added code measured in the flash table of `docs/memory-budget.md`, under 75 % of
   the slot.
4. The memory budget gets the catalogue (at most 32 KB, PSRAM), the file read whole to be checked
   (16 KB + 128 B, PSRAM, for the length of an install), the two files on the FAT, and the `script`
   thread's time: a load (up to 1000 ms) runs on the thread of the API's non-`GET` requests (D74),
   which wait meanwhile.

## Living spec delta

### docs/living-spec/README.md

- "Runs script recipes": scripts also come from installs; a script recipe is built in or installed;
  the signed file, the catalogue, install, update, remove, the boot's checks, the memory rule.
- New section "Shows its resources": the route and the card.
- "Serves Sowel's device API": the new routes, as Solite extensions.
- History: spec 042's row.

### docs/sowel-subset.md

The routes of R2.3, R3 and R5.1, as Solite extensions named after Sowel's `/plugins/*`.

### No longer true

- D76's "Downloading, signing ... and removing a script are F2" (a new decision, D78, records what
  F2 does; D76 is not edited).
- The living spec's sentence that scripts come only from the image and the Mac's flag, and the
  ones naming `recipes/scripts/` and `--recipe-script` as the only sources (search `docs/`).
- The memory budget's scripts row (its sources), and the roadmap's F2 line and the inbox line of
  2026-11-26 on admission and the Resources card (done by this spec once built).

## Acceptance criteria

- [ ] A script signed with the recipe key installs from the catalogue and from an upload; one signed
      with the firmware's key, or any other, or changed by one byte, is refused with its reason.
- [ ] Update replaces the runtime and restarts the instances; a new version that does not load leaves
      the old one running; uninstall keeps the instances stopped, and a new install starts them.
- [ ] An install that would take the PSRAM under the reserve is refused with the figures (a fake
      probe on the Mac); a third script recipe is refused.
- [ ] At boot, installed scripts are checked again and loaded before the instances start (an
      enabled instance of an installed recipe runs after a restart); a changed file is refused and
      logged.
- [ ] `GET /api/v1/system/resources` answers the internal RAM, `psram: null` and the scripts' rows on
      the Mac (the PSRAM figures on the box); the card shows them.
- [ ] The new routes are in `docs/sowel-subset.md` and in the contract test's list of Solite's own
      routes; `crates/core/tests/sowel_contract.rs` is green.
- [ ] Box check (the user present, after their yes to flash, D6; one pass, D39): install `motion-light-script` from a test
      catalogue on the home network, see it in the Resources card, update it, remove it; the flash
      writes counted.
- [ ] D78 written; `sh scripts/validate.sh` green.

## Edge cases

| Case | Behaviour |
| ---- | --------- |
| No network, catalogue unreachable | `GET /recipes/store` answers the last list read, or an empty one with `error`; install 502 with the reason |
| Catalogue over 32 KB or 32 entries, bad JSON | Refused as a whole, the last good list kept |
| Download cut | Resumed as an update's (D72), at most three times; then 502 |
| Size or SHA-256 differs from the catalogue | 400, nothing written |
| Two installs at once, or during a firmware update or a backup | The second 409 `Busy` |
| The flash write fails | The recipe is dropped again, 500 `Could not save the recipe` |
| Power lost during the write | The save's atomic rename (`.tmp`, `.new`) keeps the old file or the new one, never half |
| Uninstall while an instance's call runs | The home lock serialises: the call ends, then the removal |
| Installed id equals a later built-in script's id (a newer firmware ships it) | The built-in wins at boot, the installed file is left and logged; uninstall removes it |
| Script declares another version than its header | 400 `version differs from the signed one` |
| Catalogue entry's version differs from the file's header | 400, nothing written |
| Update: the write fails after the new version loaded | New runtime dropped, old file loaded again, 500 |
| Update: the old file does not load again | Recipe failed (spec 041), instances stopped with the reason, 500 |
| A rollback to an older firmware | It ignores `recipes.catalogue` and the `scripts/` files (it never reads them); a newer firmware finds them again |
| Factory reset | Installed files removed |
| Catalogue URL setting empty or refused by D66's rule | 400 at the setting's save, as the update URL |

## Sowel contract

No Sowel type changes: `RecipeInfo` is served as in spec 041. Sowel has no script distribution yet
(its draft keeps scripts in its database). Sowel's plugin store (`docs/technical/api-reference.md`,
`/plugins/store`, `/plugins/install` with `{ repo, confirmed?, expectedSha256? }`, entries with a
`tier`) serves TypeScript packages; Solite's routes are its own, under `/recipes/*` with `{ id }`, a
deviation written in D78 and in `docs/sowel-subset.md`'s Solite extensions. A Sowel issue
(inbox): the signed script file and the catalogue as a common format, so Sowel and Solite share one.

## How the user will check it

1. On the Mac: `cargo run -p solite-host -- --update-keygen ...` once for a test recipe key, then
   `--recipe-sign recipes/scripts/motion-light-script.js`, a local `recipes.json` served from the Mac,
   the host built with that public key: install, update to a version signed 1.0.1, uninstall, in the
   interface; a file signed with another key refused.
2. The Resources card on the Mac (no PSRAM: the internal figures only).
3. On the box (with the user, their flash): the same through the Wi-Fi from a catalogue on the Mac,
   the PSRAM figures in the card, the flash writes of `box.sh saves`.

## Open questions for the user

1. Where the catalogue lives: a repository of its own `mchacher/solite-recipes` with a CI that signs
   with the recipe key (recommended), or files you publish by hand.
2. The PSRAM reserve: 512 KB recommended.
3. Keep `motion-light-script` built into the image (it holds one of the 2 script places, and your
   garage uses it), or move it to the catalogue (then installed like any other). Recommended: move it
   to the catalogue, so both places are free for installs, and install it on your box once F2 runs.
4. Route names under `/recipes/*` (recommended) or Sowel's `/plugins/*`.
5. The factory reset removes the installed recipes (recommended).
