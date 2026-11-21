# Architecture

How mini-Sowel is built **today**. Short: the blocks, where they run, how data flows. Only what
exists: the target architecture is decision D1, and what comes next is in the roadmap.

## Overview

```
          crates/core  (Rust library: name, version, banner, memory line;
                        radio/: ASH, EZSP, ZDO, ZCL, the coordinator; no socket, no
                        clock, no random source)
                  │                         │
     crates/host (Rust, Mac)          firmware/ (Rust std on ESP-IDF, ESP32)
     prints the banner; with          writes the banner and its free memory
     --radio, TCP transport ──► Dongle Max's radio (original firmware, port 6638)
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
| Core | The Sowel core: shared by every program, no hardware. `radio/`: the ASH frame layer, the EZSP commands and callbacks, ZDO and ZCL frames, the generic reading of the standard clusters' measures (`measures.rs`), and the coordinator (start or resume the network, pairing, identifying a device, setting up its reporting, enrolling alarm devices, listening to its measures, sending commands and waiting for their acknowledgement in `commands.rs`), as state machines fed with bytes, time and random bytes; `RadioTransport` is the trait a transport implements | `crates/core/` |
| Devices and API | In the core: `devices/` (Sowel's `Device`, `DeviceData`, `DeviceOrder`; the catalog from clusters to data and orders; the registry with its bounds; the `DeviceStore` trait) and `api/` (Sowel's routes as a function from request to response, reused by the box; an order comes back as an order request, which the host's radio loop executes through a queue of at most 2 orders at once (one being sent, one waiting), the core's own orders using the same queue in `orders/`); in the host: `store.rs` (`devices.json`) and `serve.rs` (`tiny_http` on 127.0.0.1) | `crates/core/src/{devices,api,orders}/`, `crates/host/src/{store,serve}.rs` |
| Zones and equipments | In the core: `zones/` (the tree, Sowel's rules, the aggregation) and `equipments/` (bindings by alias, status and stale as Sowel); the API serves them; the host stores `zones.json` and `equipments.json` | `crates/core/src/{zones,equipments}/` |
| Simulator | A simulated EmberZNet radio and simulated devices (temperature-humidity, motion, relay) playing scenario files (`scenarios/`); it implements the core's `RadioTransport`, so the host and the tests run the real protocol code against it; Mac only | `crates/sim/` |
| Host | The program on the Mac; `radio_tcp.rs` is the TCP transport to the Dongle Max's radio; `random.rs` reads the system's random source; `backup.rs` writes the network backup to the data folder (`mini-sowel-data/`, ignored by git) | `crates/host/` |
| Firmware | The program on the Dongle Max's ESP32 | `firmware/` (own toolchain, `firmware/rust-toolchain.toml`) |
| Interface | The web app, Sowel's design system | `ui/` |
| Tools | Node programs run on the Mac, never shipped in the box: `z2m-coverage` loads Zigbee2MQTT's device library (`zigbee-herdsman-converters`, pinned) and writes its coverage report | `tools/` (each tool its own `package.json`) |
| Checks | The verdict and the docs check | `scripts/validate.sh`, `scripts/check-docs.sh`, `.githooks/` |

## Builds

- **Mac workspace** (`Cargo.toml` at the root): `crates/core`, `crates/host`, `crates/sim`; Rust stable pinned
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

The box stores no data. On the Mac, the host keeps `zones.json` and `equipments.json` (written
at once when they change), `devices.json` (Sowel's devices with their
data and orders, and each device's last network address and endpoint; definitions written at once, values every 10 minutes and on stop) and writes
`network.json` (the network's identifiers
and keys, mode 0600) to its data folder after forming a network; the radio keeps the network
itself in its own flash. On the Mac, `tools/z2m-coverage/report.md` is the coverage
report, committed with the library version and date it measures.
