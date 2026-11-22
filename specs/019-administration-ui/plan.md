# Plan: Spec 019

## Steps

- [ ] 1. Living spec updated (the delta, History row 019, row 017 corrected); D30 written; roadmap
      J1, J2, inbox and log; `docs/specs-index.md` row 017; the notes at the top of specs 017 and
      018; `docs/architecture.md`.
- [ ] 2. Core: not applicable (no change in `src/`); the new tests of
      `crates/core/tests/interface_flow.rs` written first, green against today's API.
- [ ] 3. Adapters (host, firmware): not applicable.
- [ ] 4. API or WebSocket: not applicable (routes and events of specs 008, 010, 013, 017).
- [ ] 5. Interface, logic: `lib/binding-candidates.ts`, the plan in `bindingUtils.ts`,
      `isCompatible` and `deduceType` in `equipment-type-meta.tsx`, `deviceEquipments.ts`,
      `api/equipments.ts`, `api/devices.ts`, `store/useDevices.ts`, `store/useEquipments.ts`;
      their tests first (the cases below), then the code.
- [ ] 6. Interface, screens: `SidebarItem`, `admin-nav-items`, `Sidebar` (section);
      `DiscoveryPage`, `ZigbeeDiscovery`, `integrations.ts`; `DeviceList`, `DevicesPage`
      (`DeviceRow` removed); `DeviceSelector`, `EquipmentForm`, `NewEquipmentForm`; routes in
      `App.tsx`; messages; tests beside each file. Screenshots of the section, Discovery, Devices
      and the form (Hybrid and Dark), for the demo.
- [ ] 7. `docs/design-system.md` (port table, layout, size); `npm run check:bundle`: the first
      load must read **at most 60 KB gzip** (this spec's gate; 170 KB is the hard budget the
      script enforces); over 60 KB, stop and review the spec before going on.
- [ ] 8. `sh scripts/validate.sh` green (bundle budget, QEMU run and memory gates included).
- [ ] 9. Code review (`code-reviewer`), demo on the simulator, then the real-hardware script below.

## Test plan

The plan's fixtures are Solite's real device shapes (`catalog.rs`): the ZBMINIR2 (data `state`
light_state boolean; order `state` light_toggle boolean ON/OFF), the TH01 (temperature, humidity,
battery, voltage), the MS01 (occupancy motion, battery, voltage), a dimmable bulb (state,
brightness light_brightness; orders state light_toggle, brightness set_brightness), a device with
only `voltage`.

### Rust, core (`crates/core/tests/interface_flow.rs`)

| Module | Scenario | Expected |
| ------ | -------- | -------- |
| API, the form's sequence | `POST /equipments {name, type: light_onoff, zoneId}` without `deviceIds`, then `POST .../data-bindings {deviceDataId: <relay state>, alias: "state"}`, `POST .../order-bindings {deviceOrderId: <relay state order>, alias: "state"}` | 201 each; the equipment has exactly those two bindings, status follows the relay; `POST .../orders/state {"value": true}` is accepted (the light switches on the simulated relay) |
| API, the form's sequence | A sensor from TH01 and MS01: data bindings `temperature`, `humidity`, `battery`, `occupancy`, `battery_2` | 201 each; the zone aggregation shows the temperature (alias `temperature`) and the motion sensor; `battery_2` is accepted as an alias |
| API, rollback | A data binding to a device data deleted meanwhile (the device deleted after step 1) | 404 `DeviceData not found: <id>`; then `DELETE /equipments/:id` 204; the equipment list is back as before |
| API, unchanged | The device's `zoneId` after the whole sequence | still `null`: nothing in the sequence writes it |
| API, unchanged | The Equipment column's source: `GET /equipments` after the sequence | each data binding carries the `deviceId` of its device |
| API, unchanged | `equipments_are_made_from_devices_with_the_keys_as_aliases`, `an_equipment_with_no_device_or_a_deleted_one_has_no_binding` | unchanged and green: `deviceIds` keeps Sowel's every-key binding, zero devices still allowed by the API |
| API, sequence | One creation at its largest: `POST /equipments` (thermostat), then 16 data-binding and 16 order-binding `POST`s one after another (the five devices of `bindings_stop_at_16_of_each_kind_and_65_device_ids_are_refused`) | 33 answers 201, none 503; the equipment holds 16 and 16 (the core's side only: the ESP32's time and connections are the hardware demo's and E3's) |
| API, bound | 16 data bindings added one by one, a 17th | 400 `Too many bindings (16)` (spec 010): the plan never sends a 17th |
| API, bound | An alias of 33 characters | 400 `Invalid alias`: the plan never sends one |

### Interface, logic (Vitest)

| Module | Scenario | Expected |
| ------ | -------- | -------- |
| `lib/binding-candidates` | ZBMINIR2 for `light_onoff`, `switch` | one candidate: orders `[state]`, data `[state]` |
| `lib/binding-candidates` | Dimmable bulb for `light_dimmable` | one candidate: orders `[state, brightness]`, data `[state, brightness]` |
| `lib/binding-candidates` | ZBMINIR2 for `light_dimmable` | one candidate (on/off only), as Sowel |
| `lib/binding-candidates` | TH01 for `light_onoff` | no candidate |
| `lib/binding-candidates` | An enum order `["ON","OFF"]` with no category | an on/off order (Sowel's `isOnOffEnum`) |
| `lib/binding-candidates` | A boolean order of another category (`generic`) | not an on/off order |
| `lib/binding-candidates` | A device with a `set_shutter_position` order and a `shutter_position` data, for `shutter` | one candidate with both |
| `lib/binding-candidates` | `isThermostatDevice`: a `setpoint` data; a `set_setpoint` order; neither | true, true, false |
| `bindingUtils` plan | ZBMINIR2, `light_onoff` | `[data state→state, order state→state]` |
| `bindingUtils` plan | TH01, `sensor` | data `temperature`, `humidity`, `battery`; no `voltage`; no order |
| `bindingUtils` plan | TH01 then MS01, `sensor` | `temperature`, `humidity`, `battery`, `occupancy`, `battery_2` |
| `bindingUtils` plan | Dimmable bulb, `light_dimmable` | data `state`, `brightness`; orders `state`, `brightness` |
| `bindingUtils` plan | A thermostat device (`targetTemperature` setpoint data, `insideTemperature` temperature data, a `toggle_power` order, a boolean `power` data, another order `eco`) | aliases `setpoint`, `temperature`, order `power`, data `state`, order `eco` (every order bound) |
| `bindingUtils` plan | A relay with key `R1` (on/off enum), `light_onoff` | order alias `state` (per-type key map) |
| `bindingUtils` plan | A device with only `voltage`, any type | empty plan |
| `bindingUtils` plan | Six TH01 in one sensor (18 data) | 16 data in the plan, the last 2 left out |
| `bindingUtils` plan | A 30-character key given by three devices | `key`, `key_2`; the third (`key_3`, 33 characters) left out |
| `bindingUtils` apply | Two data and one order | three `POST`s in order, each after the previous answer, data before orders |
| `bindingUtils` apply | The second `POST` refused | stops; the error is thrown with the API's text; no third request |
| `equipment-type-meta` | `isCompatible` for each of the seven types with the five fixtures | as R5 (ZBMINIR2: light_onoff, switch, light_dimmable; TH01: sensor; MS01: sensor; bulb: the three light types; voltage only: none) |
| `equipment-type-meta` | `deduceType`: ZBMINIR2, bulb, TH01, MS01, voltage only | `light_onoff`, `light_dimmable`, `sensor`, `sensor`, `null` (the form falls back to `light_onoff`) |
| `deviceEquipments` | A light binding the relay's data, a sensor binding TH01 and MS01, in Garage under Maison | relay → `[Garage › Lumière]`; TH01 and MS01 → the sensor; an unbound device → none |
| `deviceEquipments` | One device in two equipments | two entries |
| `deviceEquipments` | An equipment with only order bindings | its device not mapped (Sowel's data-only rule) |
| `api/equipments` | `createEquipment` | `POST` body `{name, type, zoneId}`, no `deviceIds` |
| `api/equipments` | `addDataBinding`, `addOrderBinding` | `POST .../data-bindings {deviceDataId, alias}`, `.../order-bindings {deviceOrderId, alias}` |
| `api/devices` | `getDevice` | `GET /api/v1/devices/:id` |
| `store/useDevices` | The store's actions | no `updateDeviceZone`; `updateDeviceName` sends `{name}` only |

### Interface, screens (Vitest, Testing Library)

| Module | Scenario | Expected |
| ------ | -------- | -------- |
| `Sidebar` | On a room page | "Administration" closed, its entries hidden; the room marked |
| `Sidebar` | Click on "Administration" | the four entries in order Devices, Equipments, Topology, Discovery; click again closes |
| `Sidebar` | At `#/devices`, `#/equipments`, `#/zones`, `#/discovery/solite` | the section open, the entry marked (`aria-current="page"`), no room marked, one mark in all |
| `Sidebar` | Drawer, pick "Devices" | the hash set, the drawer closed |
| `App` | `#/discovery`, `#/discovery/unknown` | Zigbee's Discovery screen |
| `App` | The loose entries under the rooms | gone |
| `DiscoveryPage` | One integration | title "Discovery", one tab "Zigbee" with the count of its devices, marked |
| `ZigbeeDiscovery` | "Add a device" (moved from 017's tests) | `POST {"time": 180}`, the countdown, "Stop" posts `{"time": 0}`; the radio's refusal shown |
| `ZigbeeDiscovery` | `device.discovered` while shown | the row appears first, "New", Equipment "—", "Create equipment" |
| `ZigbeeDiscovery` | No device discovered yet | the empty text and the link to `#/devices` |
| `ZigbeeDiscovery` | Left and opened again | the list empty; the countdown read again from `GET` |
| `ZigbeeDiscovery` | Reconnection (moved from 017's tests) | state and devices read again; the countdown goes on meanwhile |
| `DevicesPage` | Three devices, the relay bound to "Lumière" in Garage | sorted by name; the relay's column "Garage › Lumière" with the room's link `#/zone/<id>`; the others "—" with "Create equipment"; no select in the page |
| `DevicesPage` | `equipment.created` read by the store | the column changes, the button goes |
| `DevicesPage` | No device | "No devices discovered", "Pair one in Discovery." with the link |
| `DevicesPage` | The devices cannot be read | Sowel's error state; "Retry" reads again |
| `DevicesPage` | "Add devices" | the hash is `#/discovery/solite` |
| `DevicesPage` | Rename (017's test, kept) | `PUT {"name"}`; a refusal's text |
| `DevicesPage` | Delete (017's tests, kept) | confirmation, `DELETE`; 409 text 5 s; 404 removes the row; cancel sends nothing |
| `DevicesPage` | Every request of the page | none has `zoneId` |
| `DeviceList` | "Create equipment" on the ZBMINIR2 | `GET /devices/:id`, the form opens: type "Light on/off", name the device's, room "Select a zone…", "Next" disabled; room chosen, Next: the relay ticked |
| `DeviceList` | "Create equipment" answered 404 | no form; the text under the row; the row removed |
| `DeviceList` | "Create equipment" on a voltage-only device | the form with `light_onoff`, the device ticked and listed in step 2; "Create" disabled with the "nothing this type uses yet" text |
| `EquipmentForm` | Step 2 opened with a ticked device that has no data yet; the next read (5 s, fake timers) gives it `state` and its order | first: "Create" disabled with the "wait for its setup" text; after the read: enabled, the plan `state`/`state`; no further read |
| `EquipmentForm` | Step 2 with a non-empty plan | one read when it opens, none every 5 s |
| `EquipmentForm` | Step 2 left (Back or close) while reading every 5 s | the reads stop |
| `EquipmentForm` | `POST /equipments` refused with `Too many equipments (64)` | the text in the form; no binding request |
| `EquipmentForm` | "Create" clicked twice | one `POST /equipments` |
| `DeviceList` | "Create equipment" clicked twice | one `GET /devices/:id`, one form |
| `EquipmentForm` | No device ticked (from Add equipment) | "Create" disabled, "Choose at least one device" |
| `EquipmentForm` | The type changed after ticking the relay | the relay still ticked and listed first |
| `EquipmentForm` | Create a light from the relay | requests in order: `POST /equipments` without `deviceIds`, data-binding `state`, order-binding `state`; the form closes; the list read again |
| `EquipmentForm` | Create a sensor from TH01 and MS01 | five data-binding requests with `battery_2` last; no order request |
| `EquipmentForm` | A binding refused (404) | `DELETE /equipments/:id` sent; the form open with the 404 text |
| `EquipmentForm` | A binding refused and the delete refused too | the form shows the binding's text; the equipment stays (the list read again shows it) |
| `EquipmentForm` | `POST /equipments` refused (018's test, kept) | the form open with its text, no binding request |
| `EquipmentForm` | Edit (018's test, kept) | one step, `PUT {name, zoneId}`, no binding request |
| `DeviceSelector` | `light_onoff` with the five fixtures | ZBMINIR2 and bulb listed; "All devices" lists five |
| `DeviceSelector` | `thermostat` with the five fixtures (018's test, kept) | none compatible, the text, "All devices" offered |
| `DeviceSelector` | Six TH01 ticked for a sensor | the warning "These devices give 18 values; only 16 are bound"; TH01 and MS01: no warning |
| `DeviceSelector` | 64 ticked (018's test, kept) | the others disabled, "At most 64 devices" |
| `ZonePage` | "Add equipment" (018's test, kept) | the form with the room chosen; "Create" disabled until a device is ticked |
| `ZonesPage` | Delete a zone (018's tests, kept) | unchanged; no text about devices |

### Real hardware (the demo, run by hand)

The Dongle Max with its original firmware, its radio over TCP (decision D6: nothing flashed), a
ZBMINIR2 wired to a lamp. On the Mac:

1. `cd ui && npm ci && npm run build` (once).
2. `cargo run -p solite-host -- --radio tcp://<address>:6638 --serve --data <a data folder>`:
   `network: resumed, ...` (or `formed` on a fresh folder), then `serving: http://127.0.0.1:8080`.
3. Open `http://127.0.0.1:8080/`. Administration › Topology: "Add zone" "Garage" (skip if it
   exists).
4. Administration › Devices: if the ZBMINIR2 is already known from E2, delete it (confirmed), so
   it comes back as new. Deleting it in Solite does not make it announce itself: it stays in the
   Zigbee network (spec 017).
5. Administration › Discovery, tab Zigbee (open the screen first, then press the button: the list
   shows only what is announced while it is open): "Add a device"; then hold the ZBMINIR2's
   button 5 s, a factory reset, which is what makes it leave and join again; within the countdown
   it appears marked "New", Equipment "—"; the host prints `joined:`, `device:`, then
   `reporting:`.
6. Wait for the host's `reporting:` line for the ZBMINIR2 (its data are set up then). "Create
   equipment" on its row: type "Light on/off" (if it shows another type, the data were not there
   yet: choose "Light on/off"), name "Lumière garage", room "Garage"; Next: the relay ticked and
   "Create" enabled (if "Create" says to wait for the setup, wait until it enables itself, within
   a few seconds of `reporting:`); Create.
7. The row reads "Garage › Lumière garage". In a terminal:
   `curl -s http://127.0.0.1:8080/api/v1/equipments` shows the light with one data binding
   `state` and one order binding `state`, both on the ZBMINIR2.
8. Sidebar › Garage: the light; press its button: the lamp turns on, the button shows on; press
   again: off.
9. Ctrl-C, start again (step 2), reload: the light is still in Garage and still switches.

Result written in the spec's "How the user will check it": date, firmware of the Dongle Max, what
was seen at steps 5, 7, 8, 9.

## Validation

- [ ] Validate command green
- [ ] Agent review done, blocking findings fixed
- [ ] Demo seen and approved by the user (simulator, then the real ZBMINIR2)
