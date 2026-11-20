# Architecture

How mini-Sowel is built **today**. Short: the blocks, where they run, how data flows. Only what
exists: the target architecture is decision D1, and what comes next is in the roadmap.

## Overview

```
                 crates/core  (Rust library: name, version, banner)
                  │                         │
     crates/host (Rust, Mac)          firmware/ (Rust std on ESP-IDF, ESP32)
     prints the banner                writes the banner to the serial log

     ui/ (Preact + TypeScript, browser): Sowel layout, the tool's name
```

The core has no hardware or operating-system dependency: the same code is compiled for the Mac
(`aarch64`/`x86_64`) and for the ESP32 (`xtensa-esp32-espidf`). The interface is a separate web
app, run by Vite's development server; neither the host nor the box serves it.

## Blocks

| Block | Role | Where in the code |
| ----- | ---- | ----------------- |
| Core | The Sowel core: shared by every program, no hardware | `crates/core/` |
| Host | The program on the Mac | `crates/host/` |
| Firmware | The program on the Dongle Max's ESP32 | `firmware/` (own toolchain, `firmware/rust-toolchain.toml`) |
| Interface | The web app, Sowel's design system | `ui/` |
| Checks | The verdict and the docs check | `scripts/validate.sh`, `scripts/check-docs.sh`, `.githooks/` |

## Builds

- **Mac workspace** (`Cargo.toml` at the root): `crates/core`, `crates/host`; Rust stable pinned
  in `rust-toolchain.toml`.
- **Firmware** (`firmware/Cargo.toml`): built with the Xtensa toolchain from `espup`, target
  `xtensa-esp32-espidf`, ESP-IDF version pinned in `firmware/.cargo/config.toml`; the build
  downloads ESP-IDF into `firmware/.embuild/` (ignored by git). Settings of ESP-IDF:
  `firmware/sdkconfig.defaults`.
- **Interface** (`ui/`): Vite builds `ui/dist/`; `ui/scripts/check-bundle-budget.mjs` checks the
  first load against its budget.

## Data

None: the tool stores and exchanges no data.
