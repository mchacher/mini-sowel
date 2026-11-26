# Architecture: Spec 042

## Overview

```
 catalogue recipes.json (https, D66)        a signed file uploaded (.srs)
        │ Fetcher (release.rs: redirects, resume, pacing)  │ streamed body (api, like update)
        ▼                                                  ▼
 crates/core/src/recipes/install/   (feature script-recipes)
   file.rs       the 128-byte SOLITERS header: parse, checks, signature (recipe key)
   catalogue.rs  recipes.json: parse, bounds, the store's answer
   mod.rs        install / update / uninstall / boot load; the memory rule (MemoryProbe);
                 one guard for the update, the backup and the install (new: today two)
        │ RecipeEngine: add_script (exists), replace_script and remove_script (new)
        ▼
 crates/core/src/recipes/mod.rs   ScriptMeta gets `source: BuiltIn | Installed { version }`;
                                  instances of an absent recipe kept stopped, started on install
 Storage: scripts/<id>.srs (read, write, remove, list: two methods added to the trait)
```

## Data

| Name | Type | Where |
| ---- | ---- | ----- |
| Installed script | the signed file, 128 B + at most 16 KB | `/data/scripts/<id>.srs` (box), the host's data folder (Mac); never in the backup (R out of scope) |
| Catalogue | `recipes.json`, at most 32 KB | RAM (PSRAM on the box), the last good one; not written to flash |
| Recipe key | 32-byte Ed25519 public key | built in (`SOLITE_RECIPE_PUBLIC_KEY`), `release/recipe-key.pub` |
| Setting | `recipes.catalogue`, a URL | `settings.json` (an older release ignores the key: D54 holds) |

## Files changed

| File | Change |
| ---- | ------ |
| `crates/core/src/update/mod.rs` | the Ed25519 and SHA-256 checks made shared (a function over a header's magic), no change of the firmware header |
| `crates/core/src/recipes/install/` (new) | `file.rs`, `catalogue.rs`, `mod.rs` |
| `crates/core/src/recipes/mod.rs` | `replace_script`, `remove_script`; `ScriptMeta.source`; start of kept instances on install |
| `crates/core/src/engine/storage.rs` | `remove(name)` and `list(prefix)` on `Storage`, the host's and the firmware's adapters, the test fake |
| `crates/core/src/memory.rs` | `MemoryProbe` over its `MemoryStats` and `HeapStats`, the storage's use added; `Resources` (serde, camelCase) |
| `crates/core/src/update/install.rs`, `crates/core/src/backup/state.rs` | their two guards become one, shared with the install |
| `crates/core/src/api/` | the routes of R2.3, R3, R5.1; the upload as a streamed body (as `/system/update`) |
| `crates/host/src/main.rs`, `serve.rs` | `--recipe-sign`, `--recipe-verify`; the key built in; the probe (no PSRAM) |
| `firmware/src/` | the key built in; the probe over `heap_caps_*` and the FAT's use; the download on the `update-pull` thread pattern, the load and write on the `script` thread (internal RAM: the flash write and the JavaScript stack) |
| `ui/src/components/settings/` | `ResourcesCard.tsx`, `RecipesCard.tsx` (new files: `SettingsPage.tsx` is 32 KB), their tests; `ui/src/api/` |
| `release/recipe-key.pub`, `release/README.md` | the public half; how the key is made and kept |
| `docs/` | living spec, sowel-subset, decisions D78, memory budget (flash, the catalogue's and files' bytes), roadmap |

## Reused

- Specs 032 to 040: `verify_strict`, the header's check pattern, the `Fetcher`, D66's URL rule, D72's
  resume, the pacing, the job threads (D59, D60, D68).
- Spec 041: `load_script`, the failure and restart of a script recipe, the bounds.
- Spec 035: the streamed body and the atomic save.

## Decisions

- **D78**: script recipes are installed from a catalogue or an upload, as files signed with a recipe
  key of their own (magic `SOLITERS`, not the firmware's), checked at install and at every boot,
  admitted by the PSRAM's lowest free since boot against a reserve; not in the backup.
