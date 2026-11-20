# Roadmap

What gets built next, and why, toward the true north in [CLAUDE.md](../CLAUDE.md). The place for
the future and for how far the work has got: the docs that describe the tool speak of the present
only. Planned and reviewed with `/mini-sowel-roadmap`: briefly after every feature, fully at every
milestone. A new need, at any time: one line in the inbox.

## Current milestone

Not planned yet: run `/mini-sowel-roadmap` right after the setup.

<!-- Once planned:
**<milestone name>**. The user can then: ... Done when: ...

| # | Increment | Done when | Status |
| - | --------- | --------- | ------ |
| 1 | ...       | ...       | next   |
-->

## Later milestones

One line each, planned when their turn comes.

## Inbox

New needs not yet placed. Anyone adds a line, at any time; the next review places them. "Note"
is the framing note, `inputs/note-de-cadrage-mini-sowel.md`.

| Date | Need | Came from |
| ---- | ---- | --------- |
| 2026-11-20 | Radio spike: from the Mac, in Rust, form a Zigbee network through the Dongle Max (radio over TCP, EZSP), have a real sensor join, read its temperature | Note, Steps 1; Risks (EZSP in Rust) |
| 2026-11-20 | Measure the share of Zigbee2MQTT converters (`zigbee-herdsman-converters`) exportable automatically | Note, Steps 1; Key functions |
| 2026-11-20 | Zigbee2MQTT definitions export: a Node tool in CI exports, signs and publishes the declarative converters; the box interprets them without JavaScript; hand-written definitions for common missing devices | Note, Key functions; Risks |
| 2026-11-20 | Simulated radio on the Mac replaying scenarios (sensor joining, temperatures, motion) | Note, Development and tests 1 |
| 2026-11-20 | Engine on the Mac: devices, equipments (light, shutter, sensor, switch, thermostat, button), zones with Sowel's aggregation, modes, API and WebSocket in Sowel's format | Note, Steps 2; Scope |
| 2026-11-20 | `motion-light` recipe lighting a real bulb, same name, parameters and behaviour as Sowel's | Note, Steps 2 |
| 2026-11-20 | Recipe proposals: pair a sensor, pick the room, the box proposes the valid recipes; one tap creates the instance (prototype in the current Sowel first, judged on production) | Note, Key functions; Steps 3 |
| 2026-11-20 | The engine on the box: firmware with the real adapters (EZSP over UART, flash storage, HTTP/WebSocket), Ethernet first and Wi-Fi as fallback | Note, Steps 4; Architecture |
| 2026-11-20 | Interface built into the firmware: pre-compressed hashed assets, `Cache-Control: immutable`, chunked from flash, a bounded number of WebSocket clients (about 4); the interface served by the host on the Mac too | Note, Interface |
| 2026-11-20 | OTA updates of the ESP32 firmware: manifest, inactive partition, sha256 then signature check, health check and automatic rollback | Note, Security and updates |
| 2026-11-20 | Commissioning: the user creates their own password, confirmed by a press on the button (EN 18031: no common default password) | Note, Security and updates (Compliance) |
| 2026-11-20 | Memory ceilings in the tests (for example a 200 KB peak for 50 devices), binary size tracked, firmware booted under Espressif's QEMU in CI | Note, Development and tests (Guardrails) |
| 2026-11-20 | WebAssembly recipes: `.wasm` recipes downloaded from a registry, signature checked, run in `wasmi` with access to data, orders and timers only; first ports: motion-light, motion-light-dimmable, switch-light, schedule-on-off, presence-thermostat, vmc-humidity | Note, Key functions; Steps 5 |
| 2026-11-20 | Signed recipe and device-definition files updated without restart (Ed25519) | Note, Security and updates |
| 2026-11-20 | Radio firmware update: `.gbl` file pushed to the EFR32 Gecko bootloader over UART, checked by the ESP32 first | Note, Security and updates |
| 2026-11-20 | Short history in the box (24 h, low resolution) and optional MQTT export | Note, Scope |
| 2026-11-20 | Mini PWA: manifest and icons, add to home screen; full-screen on iPhone to check | Note, Interface |
| 2026-11-20 | Interface languages: the active language only in the bundle | Note, Interface |
| 2026-11-20 | Link with Sowel: `sowel-plugin-mini` (in the Sowel ecosystem) shows the box's equipments in Sowel; the box keeps running if the link drops; the box's page through Sowel's HTTPS for a real PWA with notifications | Note, Key functions; Steps 6; Interface |
| 2026-11-20 | Matter bridge: equipments and zones exposed as Matter devices (Apple Home, Google Home); check the maturity of `rs-matter` | Note, Key functions; Risks |
| 2026-11-20 | Product track: ESP32-S3 board, secure boot, backup keys, factory without secrets, OEM, CRA (vulnerability reporting, SBOM from Cargo) | Note, Steps 7; Hardware; Security |
| 2026-11-20 | Choose the project's license (Sowel is AGPL-3.0) before the first public release | Setup, decision D7 |
| 2026-11-20 | CI on the remote: `sh scripts/validate.sh` on every pull request, with the ESP32 toolchain | Setup, decision D2 |

## Done

| Date | Increment | Spec |
| ---- | --------- | ---- |
| 2026-11-20 | Walking skeleton: the tool starts and shows its name on the Mac, the ESP32 and in the browser | 001 |

## Log

Every change to this roadmap: date, what changed, why.

- 2026-11-20: roadmap created at setup; inbox filled with the needs of the framing note and of the
  setup.
