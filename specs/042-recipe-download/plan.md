# Plan: Spec 042

## Steps

- [x] 0. The user's review of the spec and answers to its open questions (answered 2026-11-27,
      ten decisions in the spec)
- [x] 1. The signed file: header, checks, the key as a value; the `x.y.z` version and the reserved
      ids in spec 041's load; `--recipe-sign`, `--recipe-verify`
- [x] 2. `Storage`: `read_bytes`, `write_bytes`, `remove`, `list`, `usage` (host, firmware, fake,
      `scripts/` made at mount); the engine's `replace_script`, `remove_script`, `ScriptMeta.source`
- [x] 3. R3.1's mark: the kinds `Recipe` and `Catalogue` in `Releases`, `UpdateStatus`'s recipe flag,
      the backup route, the restart and the factory reset reading it
- [x] 4. Install, update, uninstall, the boot load before `start_all` (refused files, a lone
      `.new`), the safe mode, the factory reset through the saver; R4.1 and R4.2 with `MemoryProbe`
- [x] 5. The catalogue, the setting `recipes.catalogue`, the store and refresh routes, the download
- [x] 6. `GET /system/resources`; the storage figures kept; the firmware's probe
- [x] 7. The firmware: the `recipe` thread (`Reply::RecipeJob`, `Reply::CatalogueRefresh`); the boot
      load replacing `load_built_in` (`radio_engine.rs:387-389`); `build.rs`'s listing,
      `SCRIPT_FILES`, `load_built_in` and `built_in` removed; the script phase `include_str!`s the
      file
- [x] 8. The QEMU step: a file signed at boot with a throwaway key installed through an in-memory
      fetcher on the `recipe` thread, loaded again as at boot; `qemu.sh` and `lib/firmware.sh`
      read its `recipe: thread stack` line; `validate.sh` checks the radio image carries no script
- [x] 9. The keys: `SOLITE_RECIPE_PUBLIC_KEY` in `firmware/build.rs`, the host, `release.sh`,
      `release.yml`, `box-session.sh`; `release.sh` and the workflow refuse without
      `release/recipe-key.pub`; `release/README.md`, `docs/release.md`
- [x] 10. The box session: `--recipe-file`, B11 installing or skipping, with their tests in
      `box_session_script.rs` (the F2 group of R7 is not part of this step: it is built with
      step 13, with the user)
- [x] 11. Interface: the Resources and Recipes cards
- [x] 11c. Revised after the second box demo (2026-11-27): an uninstall deletes the recipe's instances; the Plugins page's confirmation names their count (R3.6)
- [x] 11b. Revised after the box demo (2026-11-27): the Plugins page of Administration replaces the Recipes card (R5.4); an uninstall needs only its thread's stack (R4.2)
- [x] 12. Docs: living spec, sowel-subset, D78, memory budget, architecture, roadmap, design system
- [ ] 13. Box run with the user (R7): the F2 group of `box-session.sh` and its tests in
      `box_session_script.rs` built first, with the user; the user's real recipe key made and its
      public half committed before the run; the internal RAM minimum read with two recipes and eight instances; the flash
      writes counted

## Test plan

| Module | Scenario | Expected |
| ------ | -------- | -------- |
| file | A file signed with the recipe key | Accepted, the source and version out |
| file | Signed with the firmware's key; with another key; one byte of the source changed; of the header; the magic `SOLITEUP`; reserved bytes set; length field wrong; 128 bytes; 16 KB + 1 | Refused, each with its reason |
| file | Format 2 | `format 2 is not supported` |
| file | The script declares 1.0.1 behind a header of 1.0.0 | 400 `version differs from the signed one` |
| file | An installed script with no `version`, or `1.0` | 400 `Script recipe invalid: version must be x.y.z` |
| file | An id of 33 characters; an id with `/` | Refused with its reason |
| load | A script with the id `store`; `install` (catalogue, upload, `--recipe-script`) | `Script recipe invalid: id <id> is reserved` |
| key | No recipe key: install, update, refresh | 503 `Recipe installs are not configured` |
| key | A key of small order, a key of 63 characters | `recipes: invalid recipe key, installs refused`, 503 |
| catalogue | A valid list; 33 entries; 32 KB + 1; bad JSON; format 2; an entry `url` in http on the internet | Kept; refused as a whole, the last good list kept, `error` set |
| catalogue | `GET /recipes/store` before any read | `{"recipes": [], "readAt": null, "error": null}` |
| catalogue | The store with one entry installed, one refused, one a native recipe's id | `installed`, `refused`, `builtIn` set |
| catalogue | Two refreshes within a minute | 200 then 429 |
| catalogue | Refresh with the clock not set | 503 with its text |
| catalogue | Refresh with the network down | 200, the last list, `error` |
| setting | `recipes.catalogue` in http on the internet; 513 characters; empty | 400; 400; the default used |
| install | From the catalogue; from an upload | 201, `RecipeInfo` and version; the file written once |
| install | Before any catalogue read | The catalogue read, then 201 |
| install | The catalogue's id or version differs from the file's; size or SHA-256 differs | 400, nothing written |
| install | Network down; a download cut and resumed three times, then cut again | 502 with the reason |
| install | A native id; a `--recipe-script` id; a loaded id; two files on the flash and a third id; an unknown id; `minVersion` above the firmware | 409, 409, 409, 409 `Too many script recipes (at most 2)`, 404, 409 |
| install | Kept stopped instances of that id, enabled and disabled | The enabled ones start |
| install | The write fails | Recipe unloaded, 500 `Could not save the recipe` |
| guard | Each row of R3.1's table (a recipe job during each listed job, and each listed route during a recipe job or a refresh) | 409 with the row's text |
| guard | A refresh during an upload, a backup, a restore | Both run |
| guard | An upload claimed right after a recipe job began (the window) | 409 `Update already in progress` |
| guard | A restart during a recipe job; during a refresh | 409 `Recipe install already in progress`; `Recipe catalogue refresh already in progress` |
| guard | A factory reset during a recipe job | 409 `Recipe install already in progress` |
| memory | Probe with `free − limit` under the reserve; just over | 409 with the figures; admitted |
| memory | Largest internal block under `FETCH_STACK_BYTES` + 16 KB; just over | 503 with the figures; admitted |
| update | Newer version that loads | Instances restarted with their settings, file replaced |
| update | Newer version that does not load | Old one back, instances restarted, 400 with the reason |
| update | Same or older version (catalogue or upload) | 409 |
| update | The write fails after the new version loaded | Old version back, 500 |
| update | The old file does not load again | Recipe failed, instances stopped with the reason, 500 |
| uninstall | Installed recipe | Instances stopped and kept, `Recipe not found: <id>` logged, file removed, 204 |
| uninstall | A native recipe; a `--recipe-script`; no file of that id | 409; 409; 404 |
| uninstall | A refused file | File removed, 204 |
| uninstall | The removal fails | 500 `Could not remove the recipe`, runtime dropped |
| boot | Files `b` and `a`, `a` changed on the flash | `a` refused (logged, `refused`, left in place), `b` loaded; read in id order |
| boot | A format 2 file; a third file; a file over R4.1 | Each refused with its reason |
| boot | A lone `<id>.srs.new` (a fake storage cut between delete and rename); a `.tmp` | `.new` promoted and loaded; `.tmp` ignored |
| boot | An enabled instance of an installed recipe | Running after the restart |
| boot | An installed id a `--recipe-script` also has | The script loaded, the file refused (`built in`) |
| boot | Install or upload over a refused file | It replaces it, 201 |
| safe | Boot in safe mode with a file | Not loaded, `refused` `Safe mode: not loaded` |
| safe | Uninstall; install, update, refresh in safe mode | 204; 503 |
| reset | Factory reset with an installed recipe | Runtime dropped, the saver removes its file |
| resources | The route with a fake probe; with the host's | camelCase JSON, the scripts' rows; `internal`, `psram`, `storage.total` `null` |
| resources | The storage figures after an install and an uninstall | Changed; the `GET` read no file (a counting fake storage) |
| api | The routes without a session | 401 |
| api | `/recipes/install` with JSON, with `application/octet-stream`, with another type | Each handled; 415 |
| contract | `sowel_contract.rs` with the new routes and setting | Green, each in `docs/sowel-subset.md` |
| host | `--recipe-sign` then `--recipe-verify`; verify of a changed file; sign a script without `x.y.z` | Exit 0; exit 1 with the reason; exit 2 |
| firmware | The radio image's ELF | No `id: "motion-light-script"` (`validate.sh`) |
| qemu | The install step on the `recipe` thread and the load as at boot | `recipe: thread stack` at least 2048 bytes unused; the engine thread's margin kept |
| release | `release.sh` without `release/recipe-key.pub` | Refuses, with the reason |
| session | B11 without the recipe on the box, with and without `--recipe-file` | Installs then runs; skipped `not installed: --recipe-file` |
| ui | The Resources card with a fake answer; no memory figures | Bars and rows; the bars hidden |
| ui | The Plugins page: Installed and Store tabs, install, update, remove, upload, a refused file, an error, search; the 429 at opening; the sidebar entry and its update badge | The calls made; the messages shown; the held list; the badge count |
| core | An uninstall with the largest internal block between `FETCH_STACK_BYTES` and `FETCH_STACK_BYTES + RECIPE_INTERNAL_MARGIN` | Passes (204), where an install answers 503 |
| core | An uninstall with the largest block under `FETCH_STACK_BYTES` | 503 with the 20 KB figure |
| flash | Install, update, uninstall on the box (`box.sh saves`) | One write each, none at boot |
| box | R7: two recipes, eight instances, an update | Internal RAM minimum at least 64 KB |

## Validation

- [ ] Validate command green
- [ ] Agent review done, blocking findings fixed
- [ ] Demo seen and approved by the user
