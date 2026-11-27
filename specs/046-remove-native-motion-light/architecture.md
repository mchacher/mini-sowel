# Architecture, spec 046

## Overview
A removal and a rename: the native crate and its registration go; the script becomes Sowel's
`motion-light` 1.1.2 (id, info, validation texts); every fixture, test and tool that built native
instances uses the script; the memory ceilings load the script before they measure.

## Data
`recipes.json` instances keep `recipeId: "motion-light"`: unchanged on disk. The signed file
(`SOLITERS` header) carries id `motion-light`, version 1.1.2; the box stores
`scripts/motion-light.srs`.

## Files changed
| File | Change |
| --- | --- |
| `Cargo.toml:7`, `crates/core/Cargo.toml:23`, `crates/recipes/motion-light/` (with `tests/motion_light.rs`, `tests/recipe_info.json`) | removed |
| `crates/core/src/recipes/mod.rs:191,572-580` | `new()` with no native, `placeholder()` merged; `Native::new` `#[cfg(test)]` |
| `crates/core/src/home/install_ops.rs:155-158` | feature-off `is_built_in_recipe` returns `false` |
| `crates/core/src/recipes/script/slots.rs:316` and the host's settings check | the script's `validate` runs before the generic check (R3) |
| `recipes/scripts/motion-light-script.js` → `motion-light.js` | id, version, Sowel's info, i18n, texts (R2, R3) |
| paths of the script: `CLAUDE.md:57`, `README.md:55`, `scripts/qemu.sh`, `scripts/lib/firmware.sh:237-299`, `scripts/validate.sh:136`, `scripts/e2e-sim.sh:55`, `firmware/src/script_phase.rs:67`, `crates/core/tests/{lock_time.rs:142,script_memory.rs,memory.rs,script_parity.rs:30}`, `crates/host/src/serve.rs:1560`, `crates/host/tests/recipe_script.rs:12` | new path and id |
| `crates/core/tests/script_parity.rs` → `motion_light.rs`; old `motion_light.rs` deleted | R7 |
| `crates/core/tests/{memory_ceiling,memory_worst_case,lock_time,memory}.rs` | script instances, header bounds, 16-instance case gone (R8) |
| core tests creating a motion-light instance, all `required-features = ["script-recipes"]` and loading the script: events, fixtures (its `UPDATE_FIXTURES` command gains `--features script-recipes`), live, native_host, orders, recipe_json_bytes, recipe_params_memory, recipe_settle, recipes, sowel_contract, aggregate, coordinator, script_recipes (ids at :178, :277, :329), and all users of `support::Rig::create_instance` (`support/mod.rs:415-419`) and `realistic::build` | R1, R9 |
| `crates/host/tests/{ws.rs:676,740, serve.rs, sim.rs:289-301,317, demo_script.rs:71,169, recipe_script.rs:72, box_session_script.rs (13 refs, :2407-2413, :2803)}` | `--recipe-script`, the new id, B11 |
| `firmware/src/script_phase.rs:1-15,65-67,123-131,329-405,432-443` | load the script, then `start_recipe_instances` on the kept 3; version read from the script; no swap (R8) |
| `firmware/fixtures/recipes.json` | unchanged (`motion-light`) |
| `scripts/demo-garage.sh:155`, `scripts/box-session.sh:838-996,1713-1733`, `tools/e2e/src/e2e.mjs:383` | R9 |
| `ui/src/test-utils.ts:417-478`, a zone page test with `GET /recipes` empty | fixture id unchanged; new UI test |
| docs: living spec, architecture, memory-budget, sowel-subset, roadmap, recipe-sdk comments, D81 | as the spec's delta |

Unchanged: `crates/host/src/store.rs:412` (a storage round trip).

## Reused
F2's boot load, the start of kept instances at install, the `Recipe not found` restore path; the
parity scenarios; `lock_time.rs`'s script variant (:102-185); the native's `params.rs` texts as the
reference for R3.
