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

## Result (steps 1 to 4; the box, step 5, comes after)
- **R1**: `RecipeTicket::step` (`crates/core/src/recipes/install/job.rs`) logs `recipe install:
  <step> internal <free>/<lowest>` through the memory probe; seen in QEMU (catalogue downloaded,
  catalogue read, download, check, load, store, removal unloaded, removal stored).
- **R3**: `CatalogueState` holds `Vec<Entry>`, typed (`i18n` as `LangPack`/`SlotText`, unknown
  fields dropped, so the store no longer serves them); tests in `catalogue.rs`
  (`a_catalogue_of_32_entries_is_read_typed_with_its_i18n`, `unknown_fields_are_ignored_and_not_served`),
  the API answers checked by `recipe_install.rs` unchanged. A catalogue whose `i18n` is not that shape
  is now refused as a whole (it was kept as any JSON).
- **R2, QEMU** (sim image with scripts, 50 devices, 3 recipes): internal free after the engine's
  load 233 KB (min 135 KB) at 4096, 276 KB (min 166 KB) at 512; lowest during the install and the
  removal 186,427 / 170,667 B at 4096, 237,115 / 231,955 B at 512. Boot free 347 KB both.
- **Review of what stays internal** (ESP-IDF v5.5.5 sources read):
  1. UART (`radio_uart.rs`, `console.rs`): the driver's object, FIFO buffer, rx/tx rings and
     semaphores use `UART_MALLOC_CAPS` = internal 8-bit when `CONFIG_UART_ISR_IN_IRAM=y` (the radio
     image, checked by `validate.sh`); the Rust side hands only stack or `Vec` memory to
     `uart_read_bytes`/`uart_write_bytes`, which copy. No change needed.
  2. Wi-Fi: its OS adapter allocates with explicit caps (internal for queues, `prefer` PSRAM for
     the rest with `SPIRAM_TRY_ALLOCATE_WIFI_LWIP`); lwIP `mem_clib_malloc` prefers PSRAM already.
  3. Flash (`storage.rs`, `ota.rs`, backup): `esp_flash_read`/`esp_flash_write` copy a buffer outside
     internal DRAM through a bounce buffer; the FAT layer's `ff_memalloc` prefers PSRAM; the wear
     levelling temp buffer (4096, via `malloc`) moves to PSRAM, which is safe by the same path. The
     stacks of threads that touch the flash stay internal (`MallocCap::Internal`, D59).
  4. FreeRTOS objects (`pvPortMalloc`) and pthread stacks: explicit internal. mbedTLS: PSRAM already.
  5. The firmware's own `heap_caps_malloc` (OTA chunk, HTTP body buffers): PSRAM on purpose.
  No buffer needed a new explicit capability; the only unverifiable-here item is Ethernet (none
  in this firmware) and the real Wi-Fi/radio behaviour, for the box run.

