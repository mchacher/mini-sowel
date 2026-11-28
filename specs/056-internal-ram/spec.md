# Spec 056: the internal RAM back over its floor with 4 script recipes (engine study, part 1)

Written in an autonomous run (user, 2026-11-28: "ok pour faire l'analyse au niveau moteur", "on
avance et on lâche rien"). From the study of 2026-11-28 (its figures below).

## Context
On the Dongle Max (0.3.0) with 4 script recipes, the internal RAM is about 85 KB free at rest, but
its lowest point fell to 28,348 bytes (largest block 27,648) across recipe installs: under the
64 KB floor (`docs/memory-budget.md`). QuickJS (`firmware/src/script_alloc.rs`) and mbedTLS
(`CONFIG_MBEDTLS_EXTERNAL_MEM_ALLOC`) already allocate in PSRAM. What stays internal is the Rust
heap's small blocks: ESP-IDF keeps every `malloc` of at most `CONFIG_SPIRAM_MALLOC_ALWAYSINTERNAL`
(4096) bytes in internal RAM: the engine's data, each recipe's Rust side, the catalogue read as a
`serde_json::Value` (`crates/core/src/recipes/install/catalogue.rs:82`, read again at each request,
`install/mod.rs:137-158`).

## Goals
- The internal RAM's lowest point stays over 64 KB through installs, updates and removals of 4
  script recipes, on the box.
- Each step of an install says the internal RAM it leaves, so the next drop is found, not guessed.

## Out of scope
- One QuickJS runtime shared by the recipes (part 2: a context's cost measured first).
- More than 4 script recipes (after this part, from its figures).

## Requirements
### R1: the steps of an install measured
- The radio image logs, at each step of a recipe install, update and removal (catalogue read,
  download, check, load, store), the internal RAM's free and lowest figures; one line a step,
  `recipe install: <step> internal <free>/<lowest>`.
### R2: small blocks in PSRAM
- `CONFIG_SPIRAM_MALLOC_ALWAYSINTERNAL` lowered to 512 bytes (256 if 512 is not enough), after a
  review of every buffer that must stay internal (DMA, interrupt, flash access: D59), allocated
  with an explicit internal capability where it is not already (`firmware/src/radio_uart.rs` and
  the others the review finds).
### R3: the catalogue read in typed form
- The catalogue (`recipes.json`) parsed into typed structures holding only what the box uses, once
  per read, not again at each request; same answers on the API.

## Living spec delta
### docs/living-spec/README.md
- The install's log lines (R1).
### docs/memory-budget.md
- The internal RAM figures with 4 recipes: at rest, lowest across installs; the threshold.
### docs/decisions.md
- D86: the always-internal threshold, and why (the internal RAM floor with script recipes).

## Acceptance criteria
- [ ] The install's step lines in the radio image's log (seen on the box).
- [ ] Every buffer that must be internal allocated as such (review in the result).
- [ ] The catalogue's answers unchanged (existing tests), its parse typed (a test of its shape).
- [ ] On the box, 4 recipes installed, updated and removed in turn: the internal RAM's lowest
      point over 64 KB, written in the result with the before figure.
- [ ] The radio still pairs and reports (a device's data arrive) with the new threshold.
- [ ] validate green (QEMU included).

## Edge cases
- PSRAM slower than internal RAM: the engine's 200 ms a call and a request's time, measured before
  and after on the box.
- A buffer a driver gives to DMA from `malloc`: found by the review, else a boot or radio failure
  on the box (tested by pairing and reading).

## How the user will check it
The box's Resources card (internal RAM, lowest) after installing and removing recipes.
