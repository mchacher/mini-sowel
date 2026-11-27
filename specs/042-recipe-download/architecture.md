# Architecture: Spec 042

## Overview

```
 catalogue recipes.json (https, D66)            a signed file uploaded (.srs, at most 16 KB,
        │ Fetcher (release.rs: redirects,          an ordinary body: no streamed route)
        │ resume, pacing)                                  │
        ▼                                                  ▼
 crates/core/src/recipes/install/   (feature script-recipes)
   file.rs       the 128-byte SOLITERS header: parse, checks, signature (recipe key, as a value)
   catalogue.rs  recipes.json: parse, bounds, URL rule, the store's answer
   mod.rs        the recipe job (install / update / uninstall), the refresh, the boot load;
                 the two memory rules (MemoryProbe); the storage figures kept
        │ RecipeEngine: add_script (exists), replace_script and remove_script (new)
        ▼
 crates/core/src/recipes/mod.rs   ScriptMeta gets `source: BuiltIn | Installed { version }`;
                                  reserved ids; instances of an absent recipe kept stopped
 Storage: scripts/<id>.srs as bytes (read_bytes, write_bytes, remove, list: added to the trait)
```

On the box, one install from the catalogue (the request on the `script` thread, D74):

```
 script thread (32 KB, internal)          recipe thread (FETCH_STACK_BYTES, internal)
 gate, route, R3.1 mark, R4.2 check  ──►  catalogue (if none held), download, size, SHA-256,
                                          header, signature                       (job_thread::run)
 lock home, load (R4.1), unload old  ◄──  bytes (PSRAM)
                                     ──►  write scripts/<id>.srs, read the storage figures
 on failure: unload, load old again  ◄──  Ok / Err
 answer
```

An upload is the same without the download. An uninstall: the unload on the `script` thread, the
removal on the `recipe` thread. The boot load runs on the engine thread, replacing
`load_built_in` (`radio_engine.rs:387-389`, before `start_recipes` at line 390). The factory reset
drops the runtimes under the lock and gives the saver the files to remove (engine thread). No
step touches the flash from a PSRAM stack.

## Data

| Name | Type | Where |
| ---- | ---- | ----- |
| Installed script | the signed file, at most 16 KB | `/data/scripts/<id>.srs` (box, the folder made at mount), `<data>/scripts/` (Mac); never in the backup |
| Catalogue | `recipes.json`, at most 32 KB, and its `readAt`, `error` | RAM (PSRAM on the box), the last good one; not written to flash |
| Recipe key | 32-byte Ed25519 public key | built in (`SOLITE_RECIPE_PUBLIC_KEY`), `release/recipe-key.pub` |
| Setting | `recipes.catalogue`, a URL of at most 512 characters | `settings.json` (an older release ignores the key: D54 holds) |
| Storage figures | used, total | the home, read at boot and after each write or removal |
| Files to remove | at most `MAX_SCRIPT_RECIPES` + `MAX_REFUSED` (10) names | the home, for the saver (factory reset) |

## The one-job mark (R3.1)

`Releases` (`update/release.rs:488`) gets two kinds, `Recipe` and `Catalogue`, with their texts in
`busy_message`, so the check, the pull and `Home::restart_refusal` (`home/release_ops.rs:26-35`)
refuse them with no new code path. A recipe job is begun under the home's lock, after reading
`UpdateStatus` (installing, installed) and `BackupState` (running, pending). The backup route
(`backup_reply`, under the lock) and the factory reset read the mark. An upload's `Updates::claim`
(`update/install.rs:161`) runs after the lock is let go, so `UpdateStatus` gets a recipe flag in its
own mutex, set and cleared with the mark: `claim` reads it and the recipe job's begin reads
`State::Installing`, with no window between the two.

## Files changed

| File | Change |
| ---- | ------ |
| `crates/core/src/update/mod.rs` | the Ed25519 and SHA-256 checks made shared (a function over a header's magic), no change of the firmware header |
| `crates/core/src/update/release.rs`, `update/install.rs`, `home/release_ops.rs` | the kinds `Recipe` and `Catalogue`; `UpdateStatus`'s recipe flag; the restart refusal |
| `crates/core/src/recipes/install/` (new) | `file.rs`, `catalogue.rs`, `mod.rs` |
| `crates/core/src/recipes/mod.rs`, `recipes/script/load.rs` | `replace_script`, `remove_script`; `ScriptMeta.source`; the reserved ids; the `x.y.z` version of an installed script; start of kept instances on install |
| `crates/core/src/engine/storage.rs` | `read_bytes`, `write_bytes` (the same atomic save, D15), `remove`, `list(prefix)` (`.tmp` ignored, a lone `.new` promoted by `recover_interrupted_save`), `usage`; the host's and the firmware's adapters (`firmware/src/storage.rs` has `remove` and `usage` already), the test fake |
| `crates/core/src/engine/saver.rs` | the removals the factory reset asks; the storage figures after its writes |
| `crates/core/src/memory.rs` | `MemoryProbe` over `MemoryStats` and `HeapStats`; `Resources` (serde, camelCase) |
| `crates/core/src/home/auth_ops.rs` | `factory_reset` drops the installed runtimes, hands the files to the saver, refuses during a recipe job |
| `crates/core/src/api/` | the routes of R2.3, R2.4, R3, R5.1 in `ROUTES`; `/recipes/install` takes `application/octet-stream` as well as JSON; the safe mode lets the uninstall through (`safe_mode_refusal`); `Reply::RecipeJob`, `Reply::CatalogueRefresh` |
| `crates/core/src/settings/mod.rs` | `RECIPES_CATALOGUE` in `KEYS`, its rule (`url_allowed`, 512 characters) |
| `crates/host/src/main.rs`, `serve.rs`, `update_cli.rs` | `--recipe-sign`, `--recipe-verify`; the key built in (`option_env!`); the probe (no memory figures); the jobs on a thread |
| `firmware/build.rs` | `list_scripts` and the `scripts/*.js` listing go; `rerun-if-env-changed=SOLITE_RECIPE_PUBLIC_KEY` beside the update key's |
| `firmware/src/scripts.rs` | `SCRIPT_FILES`, `load_built_in`, `built_in` go; the boot load of `scripts/` |
| `firmware/src/radio_engine.rs` | the boot load replaces `load_built_in` (lines 387-389) |
| `firmware/src/script_phase.rs` | `include_str!`s `recipes/scripts/motion-light-script.js` (as `DEPTH_SCRIPT`, line 53); a step that installs a file signed at boot with a throwaway key through an in-memory fetcher on the `recipe` thread, then loads it again as at boot, logging `recipe: thread stack <n> of 20480 bytes never used` |
| `firmware/src/http_server.rs`, `job_thread.rs`, `memory.rs` | `Reply::RecipeJob` and `Reply::CatalogueRefresh` on a `recipe` thread (`FETCH_STACK_BYTES`); the probe over `heap_caps_*` |
| `firmware/Cargo.toml` | the feature `scripts`' comment (no script built in) |
| `scripts/validate.sh`, `scripts/lib/firmware.sh`, `scripts/qemu.sh` | the radio image's ELF must not carry `id: "motion-light-script"` (beside `fw_check_quickjs`, `validate.sh:137`, `lib/firmware.sh:230`); the `recipe: thread stack` line read, failing under 2048 bytes |
| `scripts/release.sh`, `.github/workflows/release.yml`, `scripts/box-session.sh` | `SOLITE_RECIPE_PUBLIC_KEY` wherever `SOLITE_UPDATE_PUBLIC_KEY` is (`release.sh:537`, `box-session.sh:1323, 1347`); `release.sh` and the workflow refuse without `release/recipe-key.pub`; the session's `--recipe-file` and B11 (`box-session.sh:925-999, 1668-1697`); the F2 group of R7 comes with the box run (plan step 13) |
| `crates/host/tests/box_session_script.rs` | B11 with and without `--recipe-file` (the F2 group's tests come with the group, plan step 13) |
| `ui/src/components/settings/` | `ResourcesCard.tsx`, `RecipesCard.tsx` (new files: `SettingsPage.tsx` is 32 KB), their tests; `ui/src/api/` |
| `release/recipe-key.pub` (the user's), `release/README.md` | the public half; how the key is made and kept |
| `docs/` | as the spec's living spec delta |

Unchanged, since they read `recipes/scripts/motion-light-script.js` as a file: the Mac's tests
(`script_parity`, `script_memory`, `memory_worst_case`, `lock_time`, the host's serve and
recipe-script tests) and the QEMU checks (`scripts/lib/firmware.sh:237-271`, `scripts/qemu.sh:155`).
The realistic home (`firmware/fixtures`) has no script instance.

## Reused

- Specs 032 to 040: `verify_strict`, `parse_public_key`, the header's check pattern, the `Fetcher`,
  `url_allowed` (D66), D72's resume, the pacing, `Releases`' mark, the job threads (D59, D60, D68),
  `CLOCK_SET_AFTER_MS`.
- Spec 041: `load_script`, the failure and restart of a script recipe, the bounds, `Failed to
  restore: Recipe not found: <id>` (`recipes/mod.rs:1672-1676`).
- Spec 035: the atomic save.

## Decisions

- **D78** (refines D76): script recipes are installed from a catalogue (`mchacher/solite-recipes`)
  or an upload, as files of at most 16 KB signed with a recipe key of their own (magic `SOLITERS`,
  not the firmware's), checked at install and at every boot, admitted by the PSRAM free now against
  a 512 KB reserve and by the internal RAM's largest block; one job at a time with the release jobs
  (one TLS session); no flash access from a PSRAM stack; the radio image builds in no script; not in
  the backup; the uninstall allowed in safe mode; routes of Solite's own under `/recipes/*`, the ids
  `store` and `install` reserved (Sowel's plugin store serves packages, not scripts).
