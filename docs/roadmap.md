# Roadmap

What gets built next, and why, toward the true north in [CLAUDE.md](../CLAUDE.md). The place for
the future and for how far the work has got: the docs that describe the tool speak of the present
only. Planned and reviewed with `/mini-sowel-roadmap`: briefly after every feature, fully at every
milestone. A new need, at any time: one line in the inbox.

## Current milestone

**A. The radio speaks Rust**. The user can then: from the Mac, `mini-sowel-host` forms its own
Zigbee network through the Dongle Max, a real sensor (Sonoff SNZB-02) joins it, and its
temperature arrives live, still after a restart, without pairing again. The firmware boots in an
emulated ESP32 (Espressif's QEMU) with its free memory shown, and the share of Zigbee2MQTT
definitions the box can reuse is known for the leading brands.
Why first: it removes the three main risks of the framing note (EZSP in Rust, device coverage,
the ESP32's memory) before anything is built on them. Done when: the SNZB-02, paired from the Mac,
reports its temperature for a day, restart included; the coverage report and the QEMU memory
figures exist.

Hardware: the Dongle Max and a Sonoff SNZB-02, both arriving 2026-11-21. A1 and A2 need none;
A3's code is written and tested without hardware, its demo needs the Dongle Max. Nothing is needed
on the Sowel side.

| # | Increment | Done when | Status |
| - | --------- | --------- | ------ |
| A1 | Zigbee2MQTT coverage of the leading brands: a tool in `tools/` reports, per brand, the share of definitions that are declarative, Tuya datapoint tables, or code, and the share a generic reading of the standard ZCL clusters would catch (first: no hardware, measures the device-coverage risk) | The report exists, with the SNZB-02's line | next |
| A2 | The firmware boots in Espressif's QEMU on the Mac and shows its banner and free memory (internal RAM, PSRAM); part of `sh scripts/validate.sh` (second: no hardware, measures the memory risk from day one) | The banner and the memory figures show on the Mac | |
| A3 | Talk to the radio: ASH and EZSP in Rust, in the core, tested against Silicon Labs' reference frames; the host connects to the Dongle Max over TCP and shows the EmberZNet version and the network state | The version shows, with the real Dongle Max | |
| A4 | Form a network and let the SNZB-02 join: network key and channel kept in a file; the host shows the joining device's address, manufacturer and model | The SNZB-02 shows with its name after a press on its button | |
| A5 | Read its temperature and survive a restart: temperature, humidity and battery shown at each report, through a generic reading of the standard clusters; after a restart of the host the network resumes and the sensor keeps reporting | Readings scroll, then come back after a restart | |

## Later milestones

One line each, planned when their turn comes. The letter D is skipped: decisions are D1, D2...

- **B. Engine on the Mac**: devices, equipments, zones, modes, API and WebSocket in Sowel's format,
  simulated radio, generic reading of the standard ZCL clusters, Zigbee2MQTT definitions export
  (Tuya datapoint interpreter included), memory ceilings in the tests, `motion-light` lighting a
  real bulb.
- **C. Recipe proposals**: pair a sensor, pick the room, accept the proposal (depends on a
  prototype in the current Sowel).
- **E. On the box**: firmware with the real adapters, the interface built in, OTA with rollback,
  commissioning password; first exercised in QEMU (emulated Ethernet serving the interface, OTA
  rollback, the emulated UART bridged to the Dongle Max's TCP radio).
- **F. WebAssembly recipes**: signed `.wasm` recipes run in `wasmi`, signed files updated without
  restart.
- **G. Link with Sowel**: the box's equipments in Sowel (depends on `sowel-plugin-mini`).
- **H. Daily use**: 24 h history, MQTT export, mini PWA, interface languages, radio firmware
  update.
- Beyond the true north, not planned: Matter bridge; product track (decision D5).

## Inbox

New needs not yet placed. Anyone adds a line, at any time; the next review places them. "Note"
is the framing note, `inputs/note-de-cadrage-mini-sowel.md`.

| Date | Need | Came from |
| ---- | ---- | --------- |

## Done

| Date | Increment | Spec |
| ---- | --------- | ---- |
| 2026-11-20 | Walking skeleton: the tool starts and shows its name on the Mac, the ESP32 and in the browser | 001 |

## Log

Every change to this roadmap: date, what changed, why.

- 2026-11-20: roadmap created at setup; inbox filled with the needs of the framing note and of the
  setup.
- 2026-11-20: license chosen (AGPL-3.0-only, decision D8): its inbox item removed.
- 2026-11-20: remote and CI connected (decision D9): the CI inbox item removed.
- 2026-11-20: milestone A planned; every inbox item placed (A1 to A5, B to H, or beyond the true
  north; none dropped). The coverage measure is limited to the leading Zigbee brands (the user's
  wish, widened to the leaders beyond their first list: Philips Hue, Tuya and its white labels,
  Aqara, IKEA, Innr, Sonoff, Schneider, Legrand, Ledvance, Heiman, Lexman...). QEMU added (A2)
  so the memory risk is measured before the hardware arrives. CI on the remote is not an
  increment: it comes with `/add-remote`.
