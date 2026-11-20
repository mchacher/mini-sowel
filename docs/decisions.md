# Decisions

Every choice that shapes the project, with its reasons, so whoever comes next understands it.
Newest last. A decision is never edited: a new one replaces it and says so.

## D1: Stack

- **Date**: 2026-11-20
- **Choice**:
  - **Core and host program**: Rust stable (pinned in `rust-toolchain.toml`), a Cargo workspace
    (`crates/core`, `crates/host`). Checks: `cargo fmt`, `cargo clippy -D warnings`, `cargo test`;
    the compiler is the type check.
  - **ESP32 firmware**: Rust with its standard library (`std`) on ESP-IDF, through `esp-idf-svc`;
    the Xtensa toolchain installed by `espup` (`firmware/rust-toolchain.toml`, channel `esp`);
    ESP-IDF pinned in `firmware/.cargo/config.toml` (v5.5.5 at setup). A Cargo project of its own
    in `firmware/`, outside the Mac workspace, using the same `crates/core`.
  - **Interface**: Preact (with `preact/compat`, so React components of Sowel work unchanged),
    TypeScript strict, Vite, Tailwind 4. Checks: Prettier, ESLint (typescript-eslint and Sowel's
    design rules), `tsc`, Vitest, a first-load size budget.
  - **One command**: `sh scripts/validate.sh` (all), `--fast` for the pre-commit hook.
  - **Tests**: Rust integration tests in `crates/<crate>/tests/`, unit tests in-module; UI tests
    beside the components.
- **Target architecture** (from the framing note, `inputs/note-de-cadrage-mini-sowel.md`):

  ```
  ┌──────────────────────────── ESP32 (Rust) ────────────────────────────┐
  │  Sowel core (no hardware dependency): devices, equipments, zones,    │
  │  modes, WebAssembly recipes, recipe proposals                        │
  │      │              │               │                │               │
  │  Radio adapter  Storage adapter  HTTP / WS adapter  OTA adapter      │
  └──────┼──────────────────────────────┼────────────────────────────────┘
         │ UART (EZSP)                  │ Ethernet PoE / Wi-Fi
  ┌──────┴───────────┐          ┌───────┴────────────────────────────┐
  │ EFR32MG24        │          │ Mini PWA, sowel-plugin-mini,       │
  │ EmberZNet (NCP)  │          │ signed updates                     │
  └──────┬───────────┘          └────────────────────────────────────┘
         │ Zigbee: sensors, lights, plugs, shutters
  ```

  The core reaches the hardware only through the four adapters. On the Mac, `crates/host` gives
  it a simulated radio (replayed scenarios) or the real radio over TCP (the Dongle Max's original
  firmware) or USB (an EFR32 dongle); on the box, `firmware/` gives it the real ones. The
  interface is built into the firmware and served by the box. Recipes run in `wasmi`; device
  definitions come from an export of Zigbee2MQTT's converters (a Node tool in `tools/`, run in CI).
- **Why**: ESP-IDF already provides what the true north needs to run a month without
  intervention: OTA on two partitions with automatic rollback, the Dongle Max's Ethernet, Wi-Fi,
  an HTTP and WebSocket server, flash storage. With `std`, Rust is written the same way on the Mac
  and on the ESP32, which matters for a user new to Rust, and keeps about 90 % of the work on the
  Mac. The interface stays in TypeScript, which the user knows, and can reuse Sowel's components.
- **Alternative considered**: Rust `no_std` with `esp-hal` and Embassy, the path Espressif now
  backs: pure Rust, smaller binaries, finer memory control; but OTA with rollback and the HTTP
  server must be assembled, Ethernet on the classic ESP32 is marked "unstable" in `esp-hal` 1.2,
  and `no_std` is harder for a Rust beginner. The cost of the choice: `esp-idf-svc` is maintained
  by the esp-rs community with little paid time from Espressif, and the first build downloads
  ESP-IDF (a few GB). Because the core has no ESP-IDF dependency, moving to `esp-hal` would change
  `firmware/` only.

## D2: Git and hosting

- **Date**: 2026-11-20
- **Choice**: local git, branch `main`; remote: none yet, `/add-remote` connects one later (GitHub,
  beside `mchacher/sowel`, with GitHub Actions running `sh scripts/validate.sh`).
- **Why**: the project starts on one computer; the remote and its CI come when the user wants to
  publish. Commits use the same identity as Sowel, set for this repository only.

## D3: Sowel contract

- **Date**: 2026-11-20
- **Choice**: everything mini-Sowel exposes takes the names, types and JSON of Sowel's API; the
  reference is Sowel's `src/shared/types.ts`, which wins over Sowel's docs where they disagree
  (for example `zone.data.changed` carries `{zoneId, aggregatedData}`, as in `types.ts`, not
  `{zoneId, key, value}` as in `api-reference.md`). Equipment types are limited to light, shutter,
  sensor, switch (a plug is a `switch`), thermostat and button. mini-Sowel is a repository separate
  from Sowel; what it needs on the Sowel side is an ordinary Sowel issue.
- **Why**: the same contract is what lets `sowel-plugin-mini` show the box's equipments in Sowel,
  and lets Sowel's recipes and components be ported without translation.

## D4: Design system

- **Date**: 2026-11-20
- **Choice**: Sowel's design system (tokens, Tailwind theme, design rules, components), copied
  from the Sowel repository and adapted to the box: system fonts instead of Inter and JetBrains
  Mono, Preact instead of React, Lucide icons imported by name only, a first-load budget of
  170 KB gzip. The starter kit's design system is not used. Details: `docs/design-system.md`.
- **Why**: mini-Sowel is part of the Sowel family and becomes part of Sowel's interface when
  linked; the kit's look is unrelated. The box serves its pages from flash: the budget and the
  fonts follow from the framing note (100 to 170 KB gzip, system fonts).

## D5: Trajectory and hardware

- **Date**: 2026-11-20
- **Choice**: a personal open-source project with the door to a product left open. The hardware
  is the Sonoff Dongle Max (ESP32-D0WDR2-V3, EFR32MG24, 16 MB flash, 2 MB PSRAM, Ethernet PoE,
  Wi-Fi, USB); only its firmware is replaced. The product track (ESP32-S3 board, secure boot,
  backup keys, OEM, CRA) is out of the current trajectory.
- **Why**: the framing note left "personal or product from the start" open; the user chose
  personal first. The classic ESP32 holds a single key digest (no backup keys): accepted for a
  personal project, a known limit for a product.

## D6: Development hardware stays on its original firmware

- **Date**: 2026-11-20
- **Choice**: the firmware is built for the ESP32 from the start, but the Dongle Max is not
  flashed until the user decides it; its original firmware exposes the radio over TCP, which the
  host program uses for development.
- **Why**: flashing the dongle removes the TCP radio the first development steps rely on.

## D7: License

- **Date**: 2026-11-20
- **Choice**: none chosen at setup; the Cargo manifests carry no `license` field. The choice is in
  the roadmap inbox (Sowel is AGPL-3.0).
- **Why**: the license is the author's decision, and it weighs on the product door left open in
  D5.

## D8: License (replaces D7)

- **Date**: 2026-11-20
- **Choice**: GNU Affero General Public License v3.0 only (`AGPL-3.0-only`), the same as Sowel:
  `LICENSE` at the root, `license` in every Cargo manifest and in `ui/package.json`.
- **Why**: mini-Sowel is part of the Sowel family and reuses Sowel's code (design system,
  components, recipes); one license keeps that reuse simple. As the author holds the copyright,
  the product door of D5 stays open (a separate license can be granted for a product).
