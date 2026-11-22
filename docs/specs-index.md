# Specs index

Every change ever specified. One row per folder in `specs/`.

| Spec | Name | What it does | Date | Status |
| ---- | ---- | ------------ | ---- | ------ |
| 001 | walking-skeleton | The tool starts and shows its name on the Mac, on the ESP32 and in the browser; the whole chain of checks works | 2026-11-20 | done |
| 002 | z2m-coverage | Measures, per leading Zigbee brand, the share of Zigbee2MQTT definitions the box can reuse by export, and the share a generic reading of standard clusters handles | 2026-11-20 | done |
| 003 | firmware-in-qemu | The firmware boots in Espressif's QEMU on the Mac and shows its free memory; a broken boot fails the checks | 2026-11-20 | done |
| 004 | talk-to-the-radio | ASH and EZSP in Rust in the core; the host identifies the Dongle Max's radio over TCP | 2026-11-21 | in progress |
| 005 | pair-a-device | The radio runs a Zigbee network; the host opens pairing and shows the device that joins (manufacturer, model) | 2026-11-21 | in progress |
| 006 | read-the-measures | Measures of the standard clusters read generically: reporting set up on pairing, shown live with `--listen`, run after run | 2026-11-21 | in progress |
| 007 | simulated-radio | A simulated radio and devices, driven by scenario files, replace the dongle in every host command and in the tests | 2026-11-21 | in progress |
| 008 | devices-in-sowel-format | Paired devices become Sowel devices with data and orders, kept on disk and served by Sowel's device API | 2026-11-21 | in progress |
| 009 | alarm-sensors | IAS zone devices (motion, contact, leak, smoke) enrolled at pairing; their alarms become device data | 2026-11-21 | in progress |
| 010 | equipments-and-zones | Zones (tree under Sowel's root), equipments bound to device data and orders by alias, and Sowel's zone aggregation, with Sowel's routes | 2026-11-21 | in progress |
| 011 | orders | Equipment orders (on, off, toggle, brightness) sent to the Zigbee devices through the radio loop, with Sowel's route, texts and value rules | 2026-11-21 | in progress |
| 012 | motion-light | Sowel's `motion-light` recipe in native Rust, with Sowel's recipe routes, its log and `recipes.json` | 2026-11-21 | in progress |
| 013 | live-events | Sowel's WebSocket (`/ws`): topics, 200 ms batches, dedup, the events of devices, zones, equipments, orders and recipes | 2026-11-21 | in progress |
| 014 | interface | The interface served by the host: rooms, their state and equipments, live, with a light's switch; Sowel's components ported | 2026-11-21 | in progress |
| 015 | target-sheet-and-budget | The Dongle Max's hardware sheet, the memory budget and its worst-case test, the partition table and memory placement, the network choice at boot | 2026-11-21 | in progress |
| 016 | engine-in-the-firmware | The engine loop moved to the core and run in the ESP32 firmware under QEMU, with the simulated radio, a realistic home and its memory checked | 2026-11-21 | in progress |
| 017 | add-device-from-interface | Pairing opened while serving, from the API and the interface; new devices announced live and named, from a devices page (its room select superseded by spec 019) | 2026-11-22 | in progress |
| 018 | rooms-and-equipments-from-interface | Rooms and equipments made from the interface: Sowel's Topology and Equipments pages and forms ported, the equipment form on a room's page | 2026-11-22 | in progress |
| 019 | administration-ui | Sowel's Administration section (Devices, Equipments, Topology, Discovery per integration); a device has no room; equipments made from a device's row with at least one device, bound as Sowel's interface binds | 2026-11-22 | in progress |
| 020 | box-first-flash | `scripts/box.sh`: the Dongle Max identified, its original firmware saved and restored, Solite's images flashed with the user's yes and their boot checked; memory, storage and the save stall measured on the box | 2026-11-22 | done |
| 021 | radio-over-uart | The radio image (a development image) drives the EFR32 over the UART (pins reset in a fixed order, baud probed) and runs the engine on it; pairing opens at a power-on boot, bounded by a marker; a wedged radio is reset and resumed; measures on the serial log, the serial buffers measured on the box | 2026-11-22 | done |
| 022 | serve-core-and-ui-embed | The rules for serving on the local network in the core (Wi-Fi settings, console lines, `Host` rule, slots, live session rules), the built interface packed as gzip for the flash (`crates/ui-embed`), `box.sh wifi` and `flash --keep-storage`, `demo-garage.sh` on an address, `measure-api.sh` (the 33 requests); all checked on the Mac | 2026-11-22 | done |
| 023 | box-serves-over-wifi | The radio image joins Wi-Fi (settings sent through its serial console) and serves Sowel's API, its live events and the gzip interface embedded in its flash on the local network; the 33 requests and the internal RAM with Wi-Fi measured on the box; depends on spec 022 | 2026-11-22 | planned |
