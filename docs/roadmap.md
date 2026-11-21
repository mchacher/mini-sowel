# Roadmap

What gets built next, and why, toward the true north in [CLAUDE.md](../CLAUDE.md). The place for
the future and for how far the work has got: the docs that describe the tool speak of the present
only. Planned and reviewed with `/solite-roadmap`: briefly after every feature, fully at every
milestone. A new need, at any time: one line in the inbox.

## Current milestone

**A. The radio speaks Rust**. The user can then: from the Mac, `solite-host` forms its own
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
| A1 | Zigbee2MQTT coverage of the leading brands: a tool in `tools/` reports, per brand, the share of definitions that are declarative, Tuya datapoint tables, or code, and the share a generic reading of the standard ZCL clusters would catch (first: no hardware, measures the device-coverage risk) | The report exists, with the SNZB-02's line | done (spec 002) |
| A2 | The firmware boots in Espressif's QEMU on the Mac and shows its banner and free memory (internal RAM, PSRAM); part of `sh scripts/validate.sh` (second: no hardware, measures the memory risk from day one) | The banner and the memory figures show on the Mac | done (spec 003) |
| A3 | Talk to the radio: ASH and EZSP in Rust, in the core, tested against Silicon Labs' reference frames; the host connects to the Dongle Max over TCP and shows the EmberZNet version and the network state | The version shows, with the real Dongle Max | built (spec 004), demo pending |
| A4 | Form a network and let the SNZB-02 join: network key and channel kept in a file; the host shows the joining device's address, manufacturer and model | The SNZB-02 shows with its name after a press on its button | built (spec 005), demo pending |
| A5 | Read its temperature and survive a restart: temperature, humidity and battery shown at each report, through a generic reading of the standard clusters; after a restart of the host the network resumes and the sensor keeps reporting | Readings scroll, then come back after a restart | built (spec 006), demo pending |

## Next milestone

**B. The engine on the Mac**. The user can then: on the Mac, with the Dongle Max, pair sensors
and a relay, put them in rooms, and see a motion sensor switch the light on (`motion-light`, as in
Sowel); everything readable through an API in Sowel's format, and live in the interface.
Why next: it proves the heart of the true north, a minimal Sowel automating on its own, before it
moves into the box (E). Done when: a real Sonoff motion sensor switches a real Sonoff ZBMINIR2
relay through `motion-light`, each room's state shows in the interface, and all of it survives a
restart.

Hardware: the Dongle Max, the SNZB-02, a Sonoff motion sensor and a Sonoff ZBMINIR2 relay (the
user has them). Approved by the user on 2026-11-21, to be built in an autonomous run as far as
possible without hardware.

| # | Increment | Done when | Status |
| - | --------- | --------- | ------ |
| B1 | Simulated radio: the host replays a scenario (a sensor joins, temperatures, motion) without the dongle (first: makes everything after it testable without hardware, in CI) | `--sim <scenario>` shows the same lines as the real radio | done (spec 007), on the branch of A3 to A5 |
| B2 | Devices in Sowel's format: a paired device becomes a Sowel `Device` with its data read generically (temperature, humidity, battery, motion, on/off, level), stored on disk, read with `GET /api/v1/devices`; memory ceilings tested from here (second: Sowel's contract, D3, is the base of the rest) | The JSON matches Sowel's, for the real SNZB-02 | built (spec 008), real SNZB-02 pending |
| B3 | Alarm sensors: IAS zone devices (the Sonoff motion sensor) are enrolled; their motion, contact, leak or smoke becomes device data | The motion sensor's `occupancy` turns true in the API | built (spec 009), real sensor pending |
| B4 | Equipments and zones: create a room, put an equipment in it (sensor, light, switch), with Sowel's zone aggregation (average temperature, motion in the room) | The API shows "Garage: 19.5 °C, motion" | built (spec 010) |
| B5 | Orders: switch a light or relay on and off through the API, on the real ZBMINIR2 | The relay obeys | built (spec 011), real ZBMINIR2 pending |
| B6 | Recipe `motion-light`, native Rust (WebAssembly is F), same name, parameters and behaviour as Sowel's | Motion switches the relay on, then off after the delay | built (spec 012), real Dongle Max pending |
| B7 | Live events: WebSocket in Sowel's format (`/ws`, topics, 200 ms batches) | A client sees the temperature, the motion and the light change as they happen | built (spec 013) |
| B8 | Interface: served by the host, shows the rooms and their equipments live | The temperature changes in the browser | built (spec 014), real Dongle Max pending |

## Later milestones

One line each, planned when their turn comes. The letter D is skipped: decisions are D1, D2...

- **C. Recipe proposals**: pair a sensor, pick the room, accept the proposal; Sowel's modes
  (depends on a prototype in the current Sowel).
- **I. Device recognition beyond the standard clusters**: Zigbee2MQTT definitions export (Tuya
  datapoint interpreter included), Philips' brand library, hand-written definitions for the
  common Legrand, Sonoff and Aqara devices (the A1 report: the generic reading already covers
  71 % of the library).
- **E. On the box**: firmware with the real adapters, the interface built in, OTA with rollback,
  commissioning password; first exercised in QEMU (emulated Ethernet serving the interface, OTA
  rollback, the emulated UART bridged to the Dongle Max's TCP radio).
- **F. WebAssembly recipes**: signed `.wasm` recipes run in `wasmi`, signed files updated without
  restart.
- **G. Link with Sowel**: the box's equipments in Sowel (depends on `sowel-plugin-mini`).
- **H. Daily use**: 24 h history, MQTT export, mini PWA, languages beyond French and English, radio firmware
  update.
- Beyond the true north, not planned: Matter bridge; product track (decision D5).

## Inbox

New needs not yet placed. Anyone adds a line, at any time; the next review places them. "Note"
is the framing note, `inputs/note-de-cadrage-mini-sowel.md`.

| Date | Need | Came from |
| ---- | ---- | --------- |
| 2026-11-21 | Interface: recipes, devices and pairing screens; editing zones and equipments | Spec 014 |
| 2026-11-21 | The box serves the interface precompressed (`.gz`) | Spec 014 |
| 2026-11-21 | Pairing while serving (`--pair` and `--serve` together), with `device.discovered` on the WebSocket | Spec 013 |
| 2026-11-21 | Sun times: sunrise and sunset from a location, so `isDaylight` and motion-light's "inactive during day" work | Spec 012 |
| 2026-11-21 | Keep recipe logs and state across a restart, if the box's flash allows it | Spec 012 |
| 2026-11-21 | Sowel issue: motion-light enters override when a zone event arrives between its ON and the light's echo (stale state read) | Spec 012 |
| 2026-11-21 | Sowel issue: motion-light's failsafe leaves override mode set; the button's ON arms no off timer | Spec 012 |
| 2026-11-21 | Sowel issue: motion-light validates an order aliased `state` but sends by category first; the `disableWhenDaylight` slot speaks of offsets the code does not read; timers beyond 24.8 days fire at once | Spec 012 |
| 2026-11-21 | Sowel issue: `POST /recipe-instances/:id/actions` on an unknown id answers 400 `Instance not running` instead of 404 | Spec 012 |
| 2026-11-21 | G: `sowel-plugin-mini` first publishes the box's devices only (Sowel plugins can publish nothing else today); then its equipments and zones, once Sowel lets a plugin publish the equipments and zones it owns (Sowel issue to open, draft ready) | Conversation on the link with Sowel |
| 2026-11-21 | Memory: a test with every bound at its maximum (64 devices, 31 zones, 64 equipments, 16 instances, 4 live clients; estimated 400 KB on the Mac), with its own ceiling | Memory estimate, 2026-11-21 |
| 2026-11-21 | Memory: allocations of 16 KB or less go to the internal SRAM first (`SPIRAM_MALLOC_ALWAYSINTERNAL=16384`): lower it (about 512) so the engine lives in PSRAM; Wi-Fi enabled beside Ethernet; mbedTLS forced internal (about 40 KB a session): one TLS session at a time | Memory estimate, 2026-11-21 |
| 2026-11-21 | Memory: a WebAssembly recipe takes about 78 KB (a 64 KB page and stacks) against about 7 KB native: fewer instances or a smaller memory before F; history bounded by constants before H | Memory estimate, 2026-11-21 |
| 2026-11-21 | Memory optimisations to keep in view: recipe logs stored compactly (message templates), fewer log lines, smaller per-client WebSocket buffers, the model's 32-bit size measured on the box | Memory estimate, 2026-11-21 |
| 2026-11-21 | A memory budget of the box per item (engine, HTTP, WebSocket clients, WebAssembly recipes, TLS, OTA buffers, history), written in `docs/` and checked by tests, before milestone E | Spec 010, discussion with the user |
| 2026-11-21 | Equipment types beyond D3's six, zone orders (all lights off), zone reorder, window contacts (a contact device as `contact_window`) | Spec 010 review |
| 2026-11-21 | Alarm devices beyond motion, contact, leak and smoke: sirens and keypads (IAS WD, ACE), CO and vibration zones, IAS on a second endpoint | Spec 009 review |
| 2026-11-20 | Count the distinct brand functions per brand (Philips' 584 lights rely on one brand library: porting it once may cover them) | Spec 002 report |
| 2026-11-20 | The first CI run after the push proves QEMU starts on the Ubuntu runner with its five system libraries (not run locally) | Spec 003 review |

## Done

| Date | Increment | Spec |
| ---- | --------- | ---- |
| 2026-11-20 | Walking skeleton: the tool starts and shows its name on the Mac, the ESP32 and in the browser | 001 |
| 2026-11-20 | A1: Zigbee2MQTT coverage of the leading brands | 002 |
| 2026-11-20 | A2: the firmware boots in QEMU and shows its memory | 003 |

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
- 2026-11-20: A1 done (spec 002), in an autonomous run approved by the user; three needs from its
  report added to the inbox (for milestone B).
- 2026-11-20: A2 done (spec 003), autonomous run: firmware revision 3 with PSRAM, 362 KB internal
  and 2045 KB PSRAM free at start in QEMU; one inbox line (first CI run with QEMU).
- 2026-11-21: milestone B planned with the user (six increments, B1 to B6); the ZBMINIR2 relay
  and a Sonoff motion sensor replace a bulb; modes moved to C; the definitions export and the
  hand-written definitions grouped in a new milestone I (after C), as the A1 report shows the
  generic reading covers more; two inbox lines from the A1 report placed (B2, I).
- 2026-11-21: B3 inserted (alarm sensors' enrollment, split out of B2 by the user); the next
  increments renumbered B4 to B7. A1 to A5 and B1 built in an autonomous run (A3 to A5 waiting for
  their hardware demos).
- 2026-11-21: B7 split into the live events (B7, spec 013) and the interface (B8), decided in
  the autonomous run: each is a demo of its own. B5 and B6 built (specs 011, 012). The memory
  ceiling test of the realistic home reaches 198 KB of 200 KB in the worst case with B7: the inbox item
  "memory budget of the box" is due before more features land in the engine.
- 2026-11-21: B8 built (spec 014): milestone B is built on the simulator, its demos on the real
  Dongle Max, motion sensor and ZBMINIR2 pending. A full review with `/mini-sowel-roadmap` is due
  (milestone reached, inbox of 18 items, the memory ceiling reached) before milestone C or E.
- 2026-11-21: the engine's memory ceiling raised from 200 KB to 300 KB with the user (decision D20), after
  an estimate of the whole box; its optimisations added to the inbox.
