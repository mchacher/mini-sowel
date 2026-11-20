# Architecture

How mini-Sowel is built **today**. Short: the blocks, where they run, how data flows. Only what
exists: the target architecture is decision D1, and what comes next is in the roadmap.

## Overview

```
                 crates/core  (Rust library: name, version, banner)
                  │                         │
     crates/host (Rust, Mac)          firmware/ (Rust std on ESP-IDF, ESP32)
     prints the banner                writes the banner and its free memory
                                      to the serial log; booted on the Mac
                                      in Espressif's QEMU (scripts/qemu.sh)

     ui/ (Preact + TypeScript, browser): Sowel layout, the tool's name

     tools/z2m-coverage (Node, Mac only): Zigbee2MQTT's library → report.md
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
| Tools | Node programs run on the Mac, never shipped in the box: `z2m-coverage` loads Zigbee2MQTT's device library (`zigbee-herdsman-converters`, pinned) and writes its coverage report | `tools/` (each tool its own `package.json`) |
| Checks | The verdict and the docs check | `scripts/validate.sh`, `scripts/check-docs.sh`, `.githooks/` |

## Builds

- **Mac workspace** (`Cargo.toml` at the root): `crates/core`, `crates/host`; Rust stable pinned
  in `rust-toolchain.toml`.
- **Firmware** (`firmware/Cargo.toml`): built with the Xtensa toolchain from `espup`, target
  `xtensa-esp32-espidf`, ESP-IDF version pinned in `firmware/.cargo/config.toml`; the build
  downloads ESP-IDF into `firmware/.embuild/` (ignored by git). Settings of ESP-IDF:
  `firmware/sdkconfig.defaults` (16 MB flash, PSRAM in the heap, chip revision 3 and later).
  `sh scripts/qemu.sh` assembles a flash image with ESP-IDF's `esptool` and boots it in
  Espressif's QEMU (`qemu-system-xtensa`, installed into `firmware/.embuild/` by ESP-IDF's tool
  installer), with 2 MB of PSRAM; it fails on a panic, a reboot or a missing memory line.
- **Unsafe code**: the core and the host forbid it; the firmware denies it, except in the small
  functions that call ESP-IDF's C functions, each with a `SAFETY` comment (decision D11).
- **Tools** (`tools/<name>/`): TypeScript run directly by Node 24, no build step; type-checked by
  `tsc`, tested with `node:test`.
- **Interface** (`ui/`): Vite builds `ui/dist/`; `ui/scripts/check-bundle-budget.mjs` checks the
  first load against its budget.

## Data

The box stores and exchanges no data. On the Mac, `tools/z2m-coverage/report.md` is the coverage
report, committed with the library version and date it measures.
