# Architecture

How Solite is built **today**. Short: the blocks, where they run, how data flows. Only what
exists: the target architecture is decision D1, and what comes next is in the roadmap.

## Overview

```
          crates/core  (Rust library: name, version, banner, memory line;
                        radio/: ASH, EZSP, ZDO, ZCL, the coordinator;
                        engine/: the radio loop and the saver; no socket, no
                        clock, no random source, no file)
                  │                         │
     crates/host (Rust, Mac)          firmware/ (Rust std on ESP-IDF, ESP32)
     prints the banner; with          banner, memory, data partition (FAT),
     --radio, TCP transport ──► Dongle Max's radio (original firmware, port 6638)
     files in a folder                network setting; feature sim: the engine on
                                      crates/sim; booted on the Mac in
                                      Espressif's QEMU (scripts/qemu.sh)

     ui/ (Preact + TypeScript, browser): the rooms and equipments, live;
         served from ui/dist by the host's --serve, through /api and /ws

     tools/z2m-coverage (Node, Mac only): Zigbee2MQTT's library → report.md
```

The core has no hardware or operating-system dependency: the same code is compiled for the Mac
(`aarch64`/`x86_64`) and for the ESP32 (`xtensa-esp32-espidf`). The interface is a separate web
app, built by Vite into `ui/dist`, which the host's `--serve` serves; the box does not serve it.

## Blocks

| Block | Role | Where in the code |
| ----- | ---- | ----------------- |
| Core | The Sowel core: shared by every program, no hardware. `radio/`: the ASH frame layer, the EZSP commands and callbacks, ZDO and ZCL frames, the generic reading of the standard clusters' measures (`measures.rs`), and the coordinator (start or resume the network, pairing, identifying a device, setting up its reporting, enrolling alarm devices, listening to its measures, sending commands and waiting for their acknowledgement in `commands.rs`), as state machines fed with bytes, time and random bytes; `RadioTransport` is the trait a transport implements; `engine/`: the radio loop (poll the radio, apply activity, routes and measures, recompute, tick, run the recipes and the orders, save what is due), the `Saver` that writes the files, and the `Storage` trait (the engine's four named files, written chunk by chunk) that the host and the firmware implement; the clocks, the random source and the output are closures given to it; `network.rs`: the network setting (`auto`, `wifi`, `ethernet`) read from its JSON and the link chosen at boot from it and the cable (decision D24) | `crates/core/` |
| Devices and API | In the core: `devices/` (Sowel's `Device`, `DeviceData`, `DeviceOrder`; the catalog from clusters to data and orders; the registry with its bounds; the `DeviceStore` trait) and `api/` (Sowel's routes as a function from request to response, reused by the box; an order comes back as an order request, which the host's radio loop executes through the queue in `orders/`: at most 2 orders from the API at once (one being sent, one waiting), sent before the recipes' orders, which have their own bound of 32); `home/pairing.rs` (the pairing window: the request of the API, the radio loop's state, the addresses being set up) and `api/pairing.rs` (the `integrations/solite/permit-join` route), which `engine::run` serves between two polls; in the host: `store.rs` (the Mac's `Storage`: `devices.json` and the other files in a folder) and `serve.rs` (`tiny_http` on a loopback port of its own, behind the host's front door, `front.rs`, which owns the public port on 127.0.0.1; locks are always taken in the order home, orders' queue, live hub, and the WebSocket threads take only the hub) | `crates/core/src/{devices,api,orders}/`, `crates/host/src/{store,serve}.rs` |
| Zones and equipments | In the core: `zones/` (the tree, Sowel's rules, the aggregation) and `equipments/` (bindings by alias, status and stale as Sowel); the API serves them; the host stores `zones.json` and `equipments.json` | `crates/core/src/{zones,equipments}/` |
| Recipes | In the core: `recipes/` (the engine: instances, their state and log, timers checked on each loop turn; `motion_light/`, Sowel's recipe in Rust: settings, validation, state machine) and `home/events.rs` (the compact events recipes listen to, filtered and bounded); a recipe's orders go through the order queue after the API's; the API serves Sowel's recipe routes; the radio loop of the core's `engine` runs the engine, and the host stores `recipes.json` | `crates/core/src/{recipes,home}/`, `crates/core/src/engine/`, `crates/host/src/{serve,store}.rs` |
| Live events | In the core: `live/` (Sowel's `EngineEvent`s, their topics and dedup keys, the clients' subscriptions and bounded queues, built only for a topic a client listens to) and `equipments/status_tracker.rs`; in the host: `front.rs` (the public port: `/ws` kept on sockets the host owns, any other request passed to `tiny_http` on a loopback port) and `ws.rs` (one thread per client, the 200 ms batches written as fragments, pings) | `crates/core/src/live/`, `crates/host/src/{front,ws}.rs` |
| Simulator | A simulated EmberZNet radio and simulated devices (temperature-humidity, motion, relay) playing scenario files (`scenarios/`); it implements the core's `RadioTransport`, so the host, the firmware image QEMU runs and the tests run the real protocol code against it; the Mac and that image, never the box's | `crates/sim/` |
| Host | The program on the Mac; `radio_tcp.rs` is the TCP transport to the Dongle Max's radio; `random.rs` reads the system's random source; `backup.rs` writes the network backup to the data folder (`solite-data/`, ignored by git) | `crates/host/` |
| Firmware | The program on the Dongle Max's ESP32: banner, memory line, the data partition mounted at `/data` (FAT with wear levelling, `storage.rs`; formatted when it does not mount), the network setting read from `/data/network.json`; the image of the box then logs `radio: none`; with the cargo feature `sim` (the image QEMU runs) it seeds `/data` with the realistic home of `firmware/fixtures/` and runs the engine on the simulated radio in a thread of 32 KB of stack (`simulated.rs`), then logs the `engine:` line with the memory it measured (`measure.rs`, ESP-IDF's local-minimum monitor of the free heap) | `firmware/` (own toolchain, `firmware/rust-toolchain.toml`) |
| Interface | The web app, Sowel's design system: Sowel's components and stores ported (`store/`, `components/`, `lib/`, `api/`), its own page, live store (`useLive`) and `t()`; the core answers `/` and `/assets/*` with the file to send (`api/static_files.rs`), the host streams it from `--ui` (`static_files.rs`); the Administration section (`components/layout/`): Discovery per integration (`components/discovery/`, `store/usePairing.ts`) pairs, Devices (`components/devices/`) lists them with their equipments, Topology and Equipments (`components/zones/`, `components/equipments/`) make the rooms and the equipments, bound from a plan computed in the browser (`components/equipments/bindingUtils.ts`, `lib/binding-candidates.ts`) | `ui/`, `crates/core/src/api/static_files.rs`, `crates/host/src/static_files.rs` |
| Tools | Node programs run on the Mac, never shipped in the box: `z2m-coverage` loads Zigbee2MQTT's device library (`zigbee-herdsman-converters`, pinned) and writes its coverage report | `tools/` (each tool its own `package.json`) |
| Checks | The verdict and the docs check | `scripts/validate.sh`, `scripts/check-docs.sh`, `.githooks/` |

## Builds

- **Mac workspace** (`Cargo.toml` at the root): `crates/core`, `crates/host`, `crates/sim`; Rust stable pinned
  in `rust-toolchain.toml`.
- **Firmware** (`firmware/Cargo.toml`): built with the Xtensa toolchain from `espup`, target
  `xtensa-esp32-espidf`, ESP-IDF version pinned in `firmware/.cargo/config.toml`; the build
  downloads ESP-IDF into `firmware/.embuild/` (ignored by git). Settings of ESP-IDF:
  `firmware/sdkconfig.defaults` (16 MB flash, PSRAM in the heap with allocations above 4 KB
  placed there and 64 KB of internal RAM kept for the network and DMA, mbedTLS buffers in PSRAM,
  chip revision 3 and later, long file names on the FAT partition). The cargo feature `sim`
  builds the image QEMU runs (the simulator and the fixtures inside); without it, the image is
  the box's. The partition table is `firmware/partitions.csv` (settings, two
  application slots of 4 MB, a data partition; decision D25), given to the box at flashing, not
  through `sdkconfig.defaults`. `sh scripts/qemu.sh` builds that table, assembles a flash image
  with ESP-IDF's `esptool` with the application at the offset of the first slot, as the table
  gives it, and boots it in Espressif's QEMU (`qemu-system-xtensa`, installed into
  `firmware/.embuild/` by ESP-IDF's tool installer), with 2 MB of PSRAM, the data partition blank
  at every run; it fails on a panic, a reboot, a missing line, a heap peak of the engine above
  350 KB, a device of the home with no measure, a dropped radio callback or an internal RAM minimum free below 64 KB (`--box` boots the box's image instead). The wiring, the risks and the backup procedure of the box are
  in [hardware.md](hardware.md); the memory shared out item by item is in
  [memory-budget.md](memory-budget.md).
- **Unsafe code**: the core and the host forbid it; the firmware denies it, except in the small
  functions that call ESP-IDF's C functions, each with a `SAFETY` comment (decision D11).
- **Tools** (`tools/<name>/`): TypeScript run directly by Node 24, no build step; type-checked by
  `tsc`, tested with `node:test`.
- **Interface** (`ui/`): Vite builds `ui/dist/`, which the host serves; `npm run dev` proxies
  `/api` and `/ws` to a host on port 8080; `ui/scripts/check-bundle-budget.mjs` checks the
  first load against its budget.

## Data

The box stores no data. On the Mac, the host keeps `zones.json`, `equipments.json` and
`recipes.json` (written
at once when they change), `devices.json` (Sowel's devices with their
data and orders, and each device's last network address and endpoint; definitions written at once, values every 10 minutes and on stop) and writes
`network.json` (the network's identifiers
and keys, mode 0600) to its data folder after forming a network; the radio keeps the network
itself in its own flash. On the Mac, `tools/z2m-coverage/report.md` is the coverage
report, committed with the library version and date it measures.
