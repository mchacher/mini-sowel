# Plan: Spec 018

## Steps

- [x] 1. Living spec updated (spec, "Living spec delta", History row 018); D29 written; the four
      roadmap inbox lines added; the
      sentences of "No longer true" corrected (messages and their tests come with step 6).
- [x] 2. Core: not applicable (no change); `crates/core/tests/interface_flow.rs` written first,
      green against today's API.
- [x] 3. Adapters (host, firmware): not applicable.
- [x] 4. API or WebSocket: not applicable (spec 010 and 013 routes and events, unchanged).
- [x] 5. Interface, data: `types.ts` additions, `lib/zone-path.ts` (ported), `api/zones.ts`,
      `api/equipments.ts`, `store/useZones.ts`, `store/useEquipments.ts`; their tests.
- [x] 6. Interface, screens: `ZoneTree`, `ZoneForm`, `ZonesPage`; `equipment-type-meta`,
      `DeviceSelector`, `EquipmentForm`, `EquipmentsPage`; the room page's button; sidebar entries
      and routes in `App.tsx`; messages; tests beside each file. Screenshots of the two pages, the
      two forms and the room page, for the demo.
- [x] 7. `docs/design-system.md` (port table, size), `docs/architecture.md`, the
      `scripts/demo-garage.sh` comment; `npm run check:bundle`.
- [ ] 8. `sh scripts/validate.sh` green (bundle budget, QEMU run and memory gates included).
- [ ] 9. Code review (`code-reviewer`), demo on the simulator, then with the ZBMINIR2.

## Test plan

### Rust, core (`crates/core/tests/interface_flow.rs`)

The requests the screens send, in their order, through `api::handle`, on a `Home` holding the
garage's three devices (TH01, MS01, ZBMINIR2 from the test fixtures). The API is unchanged: these
pin the contract the interface relies on.

| Module | Scenario | Expected |
| ------ | -------- | -------- |
| `interface_flow.rs` | `POST /zones {"name":"Rez-de-chaussée","parentId":<root>}` | 201, parent the root |
| `interface_flow.rs` | `POST /zones {"name":"Garage","parentId":<rdc>,"description":"Box"}` | 201; `GET /zones` shows Garage under Rez-de-chaussée under Maison |
| `interface_flow.rs` | `PUT /zones/<garage> {"name":"Garage 2","parentId":<root>,"description":""}` | 200, moved under the root, renamed |
| `interface_flow.rs` | `PUT /zones/<rdc> {"name":"RDC","parentId":<garage>}` with Garage under RDC | 400 `Moving this zone would create a circular reference` |
| `interface_flow.rs` | `PUT /zones/<root> {"name":"Ma maison"}` (no `parentId`, as the root's form sends) | 200, renamed, still the root |
| `interface_flow.rs` | `POST /zones` with a 64-character name; with 65 | 201; 400 `Invalid name` |
| `interface_flow.rs` | `POST /zones` with a 500-character description; with 501 | 201; 400 `Invalid description` |
| `interface_flow.rs` | `POST /equipments {"name":"Lumière garage","type":"light_onoff","zoneId":<garage>,"deviceIds":[<relay>]}` | 201; a data binding alias `state` of category `light_state` and an order binding `state` |
| `interface_flow.rs` | `POST /equipments` `sensor` with `[<TH01>,<MS01>]` | 201; aliases `temperature`, `humidity`, `motion`, one `battery` (the second skipped) |
| `interface_flow.rs` | `GET /zones/aggregation` after both | the garage: `lightsTotal` 1, `motionSensors` 1, temperature of the fixture |
| `interface_flow.rs` | `POST /equipments` with no `deviceIds` key and with `[]` | 201, no binding, status `offline` |
| `interface_flow.rs` | `POST /equipments` with a deleted device's id | 201, no binding from it |
| `interface_flow.rs` | `POST /equipments` with devices giving 17 distinct data keys and 17 order keys (fixture devices with many keys) | 201; 16 data bindings and 16 order bindings, the rest skipped, no error |
| `interface_flow.rs` | `POST /equipments` with 65 `deviceIds` | 400 `Invalid field: deviceIds` |
| `interface_flow.rs` | `PUT /equipments/<light> {"name":"Plafonnier","zoneId":<root>}` | 200, renamed and moved; bindings unchanged (`GET`) |
| `interface_flow.rs` | `DELETE /zones/<rdc>` with Garage under it | 400 `Cannot delete zone with 1 child zone. Remove or move them first.` |
| `interface_flow.rs` | `DELETE /zones/<garage>` with an equipment | 400 `Cannot delete zone with 1 equipment. Remove or move them first.` |
| `interface_flow.rs` | `DELETE /equipments/<id>` then `DELETE /zones/<garage>` | 204, 204; a device placed in the garage before has `zoneId` null |
| `interface_flow.rs` | `DELETE /equipments/<id>` again; `PUT` of it | 404 `Equipment not found` |
| `interface_flow.rs` | `GET /devices` | each device with its `data` (`key`, `category`), what the device selector filters on |
| existing `zones.rs`, `equipments.rs`, `api.rs`, `live.rs`, `memory_worst_case.rs`, host `serve.rs`, `ws.rs`, `demo_script.rs` | unchanged | green as they are |

### Interface (Vitest, beside the files)

| Module | Scenario | Expected |
| ------ | -------- | -------- |
| `api/zones.test.ts` | `createZone({name, parentId, description})`; `updateZone(id, {...})` | `POST /zones` and `PUT /zones/<id>` with that JSON |
| `api/equipments.test.ts` | `createEquipment({..., deviceIds})`, `updateEquipment`, `deleteEquipment` | `POST`, `PUT`, `DELETE` with that JSON; parsed answer |
| `store/stores.test.ts` | `useZones.createZone` succeeds; refused (400) | tree read again; the error thrown, tree unchanged |
| `store/stores.test.ts` | `useEquipments.createEquipment` / `updateEquipment` / `deleteEquipment` | list read again; a refusal thrown |
| `lib/zone-path.test.ts` | Sowel's test cases for `flattenZonesWithPath`, `groupEquipmentsByZone` (ported) | as in Sowel |
| `components/zones/ZoneForm.test.tsx` | Blank name; 64 characters typed past | button disabled; input holds 64 |
| `components/zones/ZoneForm.test.tsx` | Description typed past 500 characters | input holds 500 |
| `components/zones/ZoneForm.test.tsx` | API answers `Invalid description`; `A zone cannot be its own parent` | form open, the text shown |
| `components/zones/ZoneForm.test.tsx` | Create with default parent | `parentId` the root; no "None" option |
| `components/zones/ZoneForm.test.tsx` | Edit a zone with a subtree | the zone and its descendants absent from the parent select |
| `components/zones/ZoneForm.test.tsx` | Edit the root | no parent select; `parentId` not sent |
| `components/zones/ZoneForm.test.tsx` | API refuses (`Too many zones (32)`) | form open, the text shown |
| `components/zones/ZoneForm.test.tsx` | Escape, cross, click outside | closed, nothing sent |
| `components/zones/ZonesPage.test.tsx` | Tree of Maison > RDC > Garage | three rows, indented; child counts; the root without "Delete" |
| `components/zones/ZonesPage.test.tsx` | "Add zone"; "Add sub-zone" on RDC | form with parent the root; with parent RDC |
| `components/zones/ZonesPage.test.tsx` | Delete confirmed / cancelled | `DELETE /zones/<id>` / nothing sent |
| `components/zones/ZonesPage.test.tsx` | Delete refused (sub-zones, equipments) | Sowel's text under the row, gone after 5 s (fake timers) |
| `components/zones/ZonesPage.test.tsx` | Delete answered 404 | tree read again |
| `components/zones/ZonesPage.test.tsx` | Tree cannot be read | Sowel's error state with "Retry", which reads again |
| `components/zones/ZonesPage.test.tsx` | Click a zone's name | hash `#/zone/<id>` |
| `components/equipments/DeviceSelector.test.tsx` | Type `light_onoff`, devices TH01, MS01, ZBMINIR2 | only the ZBMINIR2 listed |
| `components/equipments/DeviceSelector.test.tsx` | Type `sensor` | TH01 and MS01 |
| `components/equipments/DeviceSelector.test.tsx` | Type `thermostat` (no setpoint) | Sowel's no-compatible text; "All devices" lists the three |
| `components/equipments/DeviceSelector.test.tsx` | Name filter "zb" | the ZBMINIR2 only |
| `components/equipments/DeviceSelector.test.tsx` | `GET /devices` fails | error text, empty list |
| `components/equipments/DeviceSelector.test.tsx` | No device at all | the no-compatible text |
| `components/equipments/DeviceSelector.test.tsx` | 64 devices chosen out of 65 offered (synthetic list) | the 65th check box disabled, "At most 64 devices" shown |
| `components/equipments/EquipmentForm.test.tsx` | Step 1 incomplete (blank name, no room) | "Next" disabled |
| `components/equipments/EquipmentForm.test.tsx` | Types offered | the seven, sorted by label |
| `components/equipments/EquipmentForm.test.tsx` | Full create: light, "Lumière garage", Garage, ZBMINIR2 | one `POST /equipments` `{"name","type":"light_onoff","zoneId","deviceIds":[<relay>]}`, closed |
| `components/equipments/EquipmentForm.test.tsx` | Create with no device | `deviceIds: []` sent |
| `components/equipments/EquipmentForm.test.tsx` | Chosen devices give 17 distinct data keys | the warning with 17 shown; "Create" enabled and sends all the `deviceIds` |
| `components/equipments/EquipmentForm.test.tsx` | TH01 and MS01 (16 or fewer distinct keys, `battery` twice) | no warning |
| `components/equipments/EquipmentForm.test.tsx` | Change the type after choosing devices | the devices cleared |
| `components/equipments/EquipmentForm.test.tsx` | "Back" from step 2 | the fields kept |
| `components/equipments/EquipmentForm.test.tsx` | Refused (`Too many equipments (64)`) | form open, the text shown |
| `components/equipments/EquipmentForm.test.tsx` | Edit | one step, type locked; `PUT {"name","zoneId"}` |
| `components/equipments/EquipmentForm.test.tsx` | `defaultZoneId` given | the room preselected |
| `components/equipments/EquipmentsPage.test.tsx` | Equipments in Garage and in the root | grouped by room with the path label; type label, status badge, devices count |
| `components/equipments/EquipmentsPage.test.tsx` | No equipment | Sowel's empty title, message and button |
| `components/equipments/EquipmentsPage.test.tsx` | Delete confirmed / cancelled; refused; 404 | `DELETE` / nothing; text 5 s; list read again |
| `components/equipments/EquipmentsPage.test.tsx` | A disabled equipment | its row dimmed |
| `motion_light.rs` (Rust, existing) | A light deleted under a running instance; restart | unchanged and green: the error per order, then `Failed to restore`; if no test covers it, one added in `motion_light.rs` |
| `components/home/ZonePage.test.tsx` | "Add equipment" | the form opens with that room chosen |
| `components/home/ZonePage.test.tsx` | Existing tests | unchanged |
| `store/useLive.test.ts` | `zone.created` / `equipment.created` events | the tree / the equipments read again (existing behaviour, kept) |
| `App.test.tsx` | `#/zones`, `#/equipments` | the page shown; its sidebar entry marked; no room marked |
| `App.test.tsx` | Sidebar and phone drawer | "Devices", "Equipments", "Topology" in this order in both |
| `App.test.tsx` | `#/zone/<deleted id>` | the first room with equipments (unchanged) |
| `App.test.tsx` (`:90`, `:96`), `i18n/i18n.test.ts` (`:39`) | Only the root; no zone; French | "No room yet: create one in the Topology page"; "Aucune pièce : créez-en une dans la page Topologie" |
| `i18n/i18n.test.ts` | Every new key | present in fr and en |

## Validation

- [x] `npm run check:bundle` under 170 KB gzip (about 50 KB expected), size written in
      `docs/design-system.md`
- [ ] Validate command green (bundle budget, QEMU run and memory gates included)
- [ ] Agent review done, blocking findings fixed
- [ ] Demo seen and approved by the user (simulator, then the ZBMINIR2)
