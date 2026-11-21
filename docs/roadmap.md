# Roadmap

What gets built next, and why, toward the true north in [CLAUDE.md](../CLAUDE.md). The place for
the future and for how far the work has got: the docs that describe the tool speak of the present
only. Planned and reviewed with `/solite-roadmap`: briefly after every feature, fully at every
milestone. A new need, at any time: one line in the inbox.

## Current milestone

**E. Solite in the box**. The user can then: see the garage of milestone B run inside the
Dongle Max itself, Mac switched off: the sensors report, the motion switches the relay, the
interface is served by the box.
Why now: the real unknowns are on the target (the ESP32's memory, the serial link to the radio,
Ethernet, storage in flash), and an autonomous box is the true north; the user asked to reach the
target quickly. Done when: the flashed Dongle Max runs the garage with the real devices for a
day, its original firmware saved and restorable.

Hardware: the Dongle Max (being delivered); the SNZB-02, a Sonoff motion sensor and the ZBMINIR2.
Flashing needs the user's explicit yes, once the original firmware is saved (decision D6).

| # | Increment | Done when | Status |
| - | --------- | --------- | ------ |
| E1 | Prepare the target, before the delivery: how the Dongle Max's ESP32 is wired (pins to the radio, the Ethernet chip, the Wi-Fi antenna, the button, the LEDs), how it is flashed and how its original firmware is saved and restored; the whole engine run in the firmware under QEMU with the simulated radio, its memory measured; the memory budget per item written in `docs/`, a test with every bound at its maximum, the placement fixed in `sdkconfig` (the engine in PSRAM, one TLS session at a time), the network settled: one image holds Wi-Fi and Ethernet, only one is active (setting `auto | wifi | ethernet`, `auto` by default: a cable at boot means Ethernet, else Wi-Fi; no switching while running), Wi-Fi built first, the memory budget set by the larger of the two (Wi-Fi), not their sum (first: removes the memory risk and the unknowns of the board without hardware) | A written hardware sheet, and the engine's memory line in QEMU for the realistic home | built (spec 015: hardware sheet, memory budget, worst-case test, partition table, network choice in the core; spec 016: the engine in the firmware under QEMU, peak 312 KB for the realistic home) |
| E2 | The real hardware from the Mac, the day of the delivery, nothing flashed: the pending demos of A3 to A5 and B2 to B8 (radio over TCP, SNZB-02, motion sensor, ZBMINIR2), defects fixed; then the stacked branches merged into `main`, pushed with the user's yes, the first CI run (second: proves EZSP in Rust on the real radio before the box) | The hand in front of the motion sensor switches the ZBMINIR2 from the Mac | demos run 2026-11-21, merged (pull requests 1 and 2); `setValue` defect open |
| E3 | Solite in the box: original firmware saved, then flashed with the user's yes; the radio over the serial link, the network (Wi-Fi first, then Ethernet, as E1 settled), data in flash, the API and the interface served by the box (precompressed); configured by `scripts/demo-garage.sh`; with no cable and no Wi-Fi settings the box opens its own access point `Solite-XXXX` with a page to pick the network (settings can also be written to flash when flashing); a Wi-Fi that stays unreachable is retried, then the access point reopens after a few minutes | The garage runs in the box for a day, Mac off | |

## Next milestone

**J. The autonomous box**. The user can then: set Solite up from the browser only, on the box:
add a device, make the rooms and equipments, set the recipes, protect it with their own
password, and update it from afar.
Why next: a box that only `curl` can configure is not autonomous. Done when: the box runs a month
in a real annex without intervention (the true north's test).

| # | Increment | Done when | Status |
| - | --------- | --------- | ------ |
| J1 | Add a device from the interface: pairing while serving, the device appears live, named and placed in a room | The ZBMINIR2 is added without a terminal | |
| J2 | Rooms and equipments from the interface (Sowel's screens ported) | The garage and its equipments made with the mouse | |
| J3 | Recipes from the interface: the list, the form built from the slots as Sowel's, enable and disable, the log | motion-light set without `curl` | |
| J4 | A password at commissioning, confirmed by the box's button (EN 18031: no common default password); factory reset | The box asks for a password on first start | |
| J5 | Updates from afar: signed firmware, written to the inactive partition, automatic return to the old one if the new does not start | An update installed from the interface, and one that fails rolled back | |
| J6 | Home settings: the location (sun times, so "inactive during day" works), the time zone; export and import of the configuration | The light no longer switches on in daylight | |

## Built, waiting for the real hardware

Milestones A and B are built and shown on the simulator; their demos on the real devices are E2.

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

One line each, planned when their turn comes, in this order. The letter D is skipped: decisions
are D1, D2...

- **C. Recipe proposals**: pair a sensor, pick the room, accept the proposal; Sowel's modes
  (depends on a prototype in the current Sowel).
- **I. Device recognition beyond the standard clusters**: Zigbee2MQTT definitions export (Tuya
  datapoint interpreter included), Philips' brand library, hand-written definitions for the
  common Legrand, Sonoff and Aqara devices, Philips' functions counted per brand first; equipment
  types beyond D3's six, zone orders, window contacts; alarm devices beyond motion, contact, leak
  and smoke (sirens, keypads, CO, vibration).
- **H. Daily use**: 24 h history (bounded by constants), MQTT export, mini PWA, languages beyond
  French and English, radio firmware update, recipe logs and state kept across a restart.
- **F. WebAssembly recipes**: signed `.wasm` recipes run in `wasmi`, signed files updated without
  restart; a recipe's memory bounded first (about 78 KB each in `wasmi` against 7 KB native).
- **G. Link with Sowel**: the box's equipments in Sowel (depends on `sowel-plugin-solite`, which
  first publishes the box's devices, then its equipments and zones once Sowel lets a plugin
  publish them: Sowel issue, draft ready).
- Beyond the true north, not planned: Matter bridge; product track (decision D5).

## Inbox

New needs not yet placed. Anyone adds a line, at any time; the next review places them. "Note"
is the framing note, `inputs/note-de-cadrage-mini-sowel.md`.

| Date | Need | Came from |
| ---- | ---- | --------- |
| 2026-11-21 | Stream the loading of the home (its files are read whole and parsed: the largest transient of the engine's peak, 312 KB in QEMU); measure the peak again on the box | Spec 016 |
| 2026-11-21 | The 3 `motion-light` instances of the realistic home queue 48 start orders against `MAX_UNATTENDED` = 32: 16 are dropped with a warning at every start; decide whether the queue bound or the start sync of recipes changes | Spec 016 review |
| 2026-11-21 | Optimise the engine back under 300 KB (stream the home's loading first) | D27 |
| 2026-11-21 | The engine saves `devices.json` (94 KB) once per device when its route changes (at the first contact, then rarely); the write is synchronous in the radio loop: measure the stall it causes on the box (E3) | Spec 016 review |

## Sowel issues to open

Needs on the Sowel side, for the user to open as Sowel issues (never changed from here).

| Date | Issue | Came from |
| ---- | ----- | --------- |
| 2026-11-21 | motion-light enters override when a zone event arrives between its ON and the light's echo (stale state read) | Spec 012 |
| 2026-11-21 | motion-light's failsafe leaves override mode set; the button's ON arms no off timer | Spec 012 |
| 2026-11-21 | motion-light validates an order aliased `state` but sends by category first; the `disableWhenDaylight` slot speaks of offsets the code does not read; timers beyond 24.8 days fire at once | Spec 012 |
| 2026-11-21 | `POST /recipe-instances/:id/actions` on an unknown id answers 400 `Instance not running` instead of 404 | Spec 012 |

## Done

| Date | Increment | Spec |
| ---- | --------- | ---- |
| 2026-11-20 | Walking skeleton: the tool starts and shows its name on the Mac, the ESP32 and in the browser | 001 |
| 2026-11-20 | A1: Zigbee2MQTT coverage of the leading brands | 002 |
| 2026-11-20 | A2: the firmware boots in QEMU and shows its memory | 003 |
| 2026-11-21 | E1: the target prepared: hardware sheet, memory budget, partition table, network choice (spec 015); the engine run in the firmware under QEMU, memory measured (spec 016) | 015, 016 |

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
  Dongle Max, motion sensor and ZBMINIR2 pending. A full review with `/solite-roadmap` is due
  (milestone reached, inbox of 18 items, the memory ceiling reached) before milestone C or E.
- 2026-11-21: the engine's memory ceiling raised from 200 KB to 300 KB with the user (decision D20), after
  an estimate of the whole box; its optimisations added to the inbox.
- 2026-11-21: full review with the user after milestone B: the user wants Solite on the target quickly.
  Current milestone E (Solite in the box: E1 prepare the target, E2 the real hardware from the
  Mac, E3 flashed), next J (the autonomous box: configuration screens, password, updates, home
  settings; done when a month in an annex), then C, I, H, F, G. A and B wait for their hardware
  demos (E2). Every inbox item placed; the Sowel issues moved to their own list.
- 2026-11-21: E2 demos run on the real Dongle Max (USB relayed by `socat`), a SNZB-03PR2 and a
  ZBMINIR2: radio, pairing, resume, readings, API, orders, motion-light and interface worked
  (specs 004 to 014 updated). Four defects: battery shown 0 % (not reproduced since: the sensor reports 200, 100 %,
  so the 0 was probably a first report after joining; watch), `setValue` refused by the radio
  (EmberStatus 0x70, one of the two set while forming the network; the start-up one is accepted;
  settle it at the next network forming), `scripts/demo-garage.sh` and the IPv4/IPv6 port clash
  fixed (branch fix/e2-defects). Merge pending.
- 2026-11-21: Wi-Fi decided with the user and placed (inbox line removed): one image with Wi-Fi and
  Ethernet, exclusive, chosen at boot (`auto | wifi | ethernet`), Wi-Fi first; access point
  `Solite-XXXX` to give the network without a screen. E1 and E3 changed; E2's branches merged.
- 2026-11-21: E1's first half built (spec 015). Worst case measured at 987 KB (ceiling 1 MB, D23). The
  budget shows the internal RAM as the tight resource: about 156 KB are left for the engine's small
  allocations (most of its bytes), against about 130 KB for the realistic home; spec 016 measures
  it, and `SPIRAM_MALLOC_ALWAYSINTERNAL` is the knob. The `sdkconfig` cannot carry the partition
  table (esp-idf-sys ignores it, D25): it is given at flashing.
- 2026-11-21: E1's second half built (spec 016), in an autonomous run: the engine's loop is in the
  core, the firmware mounts its FAT partition and, in the QEMU image, runs the realistic home on the
  simulated radio. Peak 312 KB (ceiling 350 KB, D27; upper bound with the simulator inside), internal RAM
  never below 107 KB free; E1 is built. One inbox line: stream the loading of the home (its files are read
  whole: the largest transient of the peak); the figure is measured again on the box in E3.
- 2026-11-21 (review of spec 016): the ceiling of the QEMU run is 350 KB (D27; the Mac test stays at
  300 KB). Inbox: the 94 KB `devices.json` save is synchronous in the radio loop, its stall is measured on
  the box (E3); the 16 dropped start orders of the motion-light instances are a line of their own. A
  save writes `<name>.tmp`, renames it to `<name>.new` once complete, and only `.new` is recovered.
