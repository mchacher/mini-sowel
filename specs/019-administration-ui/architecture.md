# Architecture: Spec 019

## Overview

```
sidebar: rooms' tree ─► #/zone/<id> (room page, unchanged; "Add equipment")
         Administration (Shield, toggled; open on an admin page)
           ├ Devices     #/devices            ─► DevicesPage ─► DeviceList (table)
           ├ Equipments  #/equipments         ─► EquipmentsPage (spec 018)
           ├ Topology    #/zones              ─► ZonesPage (spec 018)
           └ Discovery   #/discovery/<id>     ─► DiscoveryPage (tabs from integrations.ts)
                                                   └ solite: ZigbeeDiscovery (usePairing, spec 017)
                                                       └ DeviceList (only the "New" devices)

DeviceList row (Equipment "—") ─"Create equipment"─► GET /api/v1/devices/:id
                                                   ─► deduceType(device)
                                                   ─► NewEquipmentForm(type, name, [device])

EquipmentForm (info ─► devices) ─► DeviceSelector: isCompatible(type, device) (binding-candidates)
        │                                           plan count for the 16-binding warning
        └ Create ─► computeBindingPlan(type, devices ticked)   (bindingUtils, Sowel's port)
                    ├ POST /api/v1/equipments {name, type, zoneId}
                    ├ POST /api/v1/equipments/:id/data-bindings  {deviceDataId, alias}  × n
                    ├ POST /api/v1/equipments/:id/order-bindings {deviceOrderId, alias} × m
                    └ on a refusal: DELETE /api/v1/equipments/:id (best effort), the text shown

Equipment column = deviceEquipmentMap(useEquipments.equipments, useZones.tree):
  deviceId (from data bindings) ─► [{equipment, zone path label, zoneId}]
```

Only the interface changes. The core, the host, the firmware and `scripts/demo-garage.sh` are
untouched: every route used exists (specs 008, 010, 017). Rust tests replay the interface's new
sequence of requests against the core's API.

## Data

| Name | Type | Where | Bound |
| ---- | ---- | ----- | ----- |
| Devices, zones, equipments, bindings | existing files (`devices.json`, `zones.json`, `equipments.json`) | host data folder, ESP32 data partition | unchanged: 64 devices, 32 zones, 64 equipments, 16 bindings of each kind |
| `Device.zoneId` | existing, `string \| null` | `devices.json` | no longer written by the interface; the API still accepts it |
| Interface: the integrations | `{ id, labelKey, Screen }[]`, a constant | `components/discovery/integrations.ts` | one per integration Solite has (1 today) |
| Interface: the "New" devices | `string[]` (`usePairing.newIds`) | browser | 64 (the device limit), emptied when Discovery is left |
| Interface: the Equipment column | `Map<deviceId, {id, name, zoneId, zoneLabel}[]>`, computed on render | browser | 64 devices × 64 equipments at most (the host's bounds) |
| Interface: the device read by "Create equipment" | `DeviceWithData`, held by the open form | browser | one |
| Interface: the binding plan | `PlannedBinding[]` (`kind`, `deviceId`, `sourceId`, `key`, `alias`, `category`) | browser, while "Create" runs | 16 data + 16 orders (cut at `MAX_BINDINGS`) |
| Interface: the Administration section open | `boolean` | browser (component state) | one |

Nothing new on disk, in `Home` or in the radio loop: the memory budget
(`docs/memory-budget.md`) and the QEMU figures are untouched.

## Files changed

### Interface (`ui/src/`)

| File | Change |
| ---- | ------ |
| `App.tsx` | Routes: `#/discovery` and `#/discovery/<id>` (page `discovery`, with the id); the loose `PAGES` become the administration pages |
| `components/layout/admin-nav-items.ts` | New, ported from Sowel's (cut to Devices, Equipments, Topology; Discovery added; `to` is a hash) |
| `components/layout/SidebarItem.tsx` | New, ported from Sowel's: a hash link instead of `NavLink`, `active` given by the caller |
| `components/layout/sidebar-section.ts` | New, ported from Sowel's `getSectionForPath`, cut to `admin` (a hash in `ADMIN_NAV_ITEMS` opens the section); Sowel's `AppLayout.tsx` is read for the drawer, not ported |
| `components/layout/Sidebar.tsx` | The Administration block of Sowel's sidebar (title with `Shield` and chevron, toggled; open on an admin page) in `Nav`, below the rooms' tree, for the sidebar and the drawer; the three loose entries removed |
| `components/discovery/integrations.ts` | New: the integrations list (`solite`, "Zigbee", `ZigbeeDiscovery`) and `integrationFromHash` |
| `components/discovery/DiscoveryPage.tsx` | New: title, Sowel's `TabButton` tabs, the screen of the integration shown |
| `components/discovery/ZigbeeDiscovery.tsx` | New: spec 017's pairing header (moved from `DevicesPage.tsx`), sets `usePairing.pageOpen`, lists the "New" devices with `DeviceList` |
| `components/devices/DevicesPage.tsx` | Rewritten as Sowel's page, cut: header with "Add devices", tabs only past one integration, `DeviceList` of every device, Sowel's empty and error states |
| `components/devices/DeviceList.tsx` | New, from Sowel's `DeviceList`, cut: the table (status, name with `DeviceNameEditor` and "New", manufacturer, model, `EquipmentCell`, actions "Create equipment" and delete); sorted by name; opens `NewEquipmentForm` from a row |
| `components/devices/deviceEquipments.ts` | New: `deviceEquipmentMap(equipments, zones)`, Sowel's `useMemo` body of `DevicesPage` |
| `components/devices/DeviceRow.tsx` | Removed (room select, "New room…"; its delete and 5 s refusal move to `DeviceList`) |
| `components/equipments/bindingUtils.ts` | Added, from Sowel's: `RELEVANT_DATA`, `RELEVANT_ORDERS`, `BIND_ALL_ORDER_TYPES`, `STANDARD_ALIASES`, `ORDER_CATEGORY_ALIASES`, `DATA_CATEGORY_ALIASES`, `TYPE_CATEGORY_ALIASES` (the entries of D3's seven types), `resolveAlias`, `isRelevantData`, `isRelevantOrder`, `uniqueAlias`, `computeBindingPlan` (candidate path for lights, switches, shutters; legacy path for the rest; no VMC, door, contract or skip options), `applyBindingPlan` (sequential), `MAX_BINDINGS` cut, the alias limit of 32 |
| `lib/binding-candidates.ts` | New, from Sowel's `src/shared/binding-candidates.ts` and `thermostat-contract.ts`: `isOnOffOrder`, `computeBindingCandidates` for `light_onoff`, `switch` (no metering), `light_dimmable`, `shutter` (category fallback), `CANDIDATE_BASED_TYPES` (those four), `isThermostatDevice`, `THERMOSTAT_STATE_ALIAS` |
| `components/equipments/equipment-type-meta.tsx` | Added: `isCompatible(type, device)` (Sowel's `DeviceSelector` cascade for D3's types) and `deduceType(device)` (R4's order) |
| `components/equipments/DeviceSelector.tsx` | Compatibility by `isCompatible`; the ticked devices listed first whatever the type; the warning counts the plan (`computeBindingPlan(...).length` per kind) instead of distinct keys; `bindableCount` removed |
| `components/equipments/EquipmentForm.tsx` | Props `initialType`, `initialName`, `initialDeviceIds`; the type change keeps the devices; step 2 reads the devices on opening and every 5 s while the plan is empty; "Create" disabled with no device or an empty plan, with their texts; returns the devices ticked and the devices read (for the plan) |
| `components/equipments/NewEquipmentForm.tsx` | Passes the shortcut's props; on submit: create (no `deviceIds`), plan, bindings, rollback by delete on a refusal |
| `components/equipments/EquipmentsPage.tsx` | Header comment only (D30); its form is `NewEquipmentForm`, unchanged in use |
| `api/equipments.ts` | `createEquipment` without `deviceIds`; `addDataBinding(equipmentId, {deviceDataId, alias})`, `addOrderBinding(equipmentId, {deviceOrderId, alias})` (Sowel's names) |
| `api/devices.ts` | `getDevice(id)`; `DeviceWithData.orders` typed as Sowel's `DeviceOrder[]` (category, type, enum values) |
| `store/useDevices.ts` | `updateDeviceZone` removed |
| `store/useEquipments.ts` | `createEquipment` without `deviceIds`; the plan's requests are sent by `applyBindingPlan`, then `fetchEquipments` |
| `store/usePairing.ts` | Comment: `pageOpen` is the Discovery screen |
| `api/zones.ts`, `components/zones/ZoneTree.tsx` | Header comments only (spec's "No longer true") |
| `i18n/messages.ts` | R6's keys added and removed, French and English |
| `types.ts` | `DeviceOrder` with Sowel's `category`, `enumValues` if missing |

### Interface tests (`ui/src/`)

| File | Change |
| ---- | ------ |
| `App.test.tsx` | Rewritten for the Administration section: `:64` (sidebar links in order), `:170` (drawer links), `:232-251` (devices page from the sidebar and drawer), `:253-275` (entries marked, their order); new: `#/discovery` routes |
| `store/useLive.test.ts` | `:178`: "no device new while the devices page is not open" → the Discovery screen |
| `components/layout/Sidebar.test.tsx` | New: the section, its toggle, its marks, the drawer |
| `components/discovery/DiscoveryPage.test.tsx` | New: tabs, fallback, the pairing controls (moved from `DevicesPage.test.tsx`), the "New" list |
| `components/devices/DevicesPage.test.tsx` | Rewritten: the table, the Equipment column, "Create equipment", rename and delete kept; the room select tests (`:148-226`) removed; the pairing tests (`:73-125`, `:261`) moved to `DiscoveryPage.test.tsx` |
| `components/devices/deviceEquipments.test.ts` | New |
| `components/equipments/bindingUtils.test.ts`, `lib/binding-candidates.test.ts` | New: the plan and the candidates |
| `components/equipments/equipment-type-meta.test.ts` | New: `isCompatible`, `deduceType` |
| `components/equipments/DeviceSelector.test.tsx`, `EquipmentForm.test.tsx`, `EquipmentsPage.test.tsx` | Changed as listed in the spec's "No longer true"; new cases of the test plan |
| `api/equipments.test.ts` | `createEquipment` body; the two binding calls |
| `store/stores.test.ts` | `updateDeviceZone` test removed |

### Core (tests only)

| File | Change |
| ---- | ------ |
| `crates/core/tests/interface_flow.rs` | New tests replaying the interface's requests of spec 019 (test plan), including 33 sequential creation requests; the existing ones unchanged |

### Docs and specs

| File | Change |
| ---- | ------ |
| `docs/living-spec/README.md` | The delta |
| `docs/decisions.md` | D30 |
| `docs/roadmap.md` | J1 and J2 rows, inbox lines, log line |
| `docs/architecture.md` | Block "Interface" (spec's "No longer true") |
| `docs/design-system.md` | Port table rows, layout paragraph, first-load size |
| `docs/specs-index.md` | Row 019; row 017's text |
| `specs/017-add-device-from-interface/spec.md`, `specs/018-rooms-and-equipments-from-interface/spec.md` | A note at the top: "Superseded in part by spec 019: ..." naming the parts listed in spec 019's "No longer true"; the rest kept as the record |

## What is reused

- Spec 017: `usePairing` and `api/pairing.ts` unchanged; the pairing header's JSX moved as is;
  `DeviceNameEditor` unchanged; the delete with confirmation and the 5 s refusal (`DeviceRow`'s
  `fail`), moved into `DeviceList`.
- Spec 018: `EquipmentForm`, `DeviceSelector`, `NewEquipmentForm`, `Modal`, `lib/zone-path.ts`
  (`flattenZonesWithPath` for the column's room label), `useEquipments.fetchEquipments`'s
  coalesced read, `formErrorText`, `apiErrorStatus`.
- Sowel (`5da84dc1`): the Administration block and `SidebarItem` classes, `TabButton`,
  `EquipmentCell`, the plan and candidate functions, read with `git show 5da84dc1:<path>` in
  `../sowel`.
- Core tests: the `Rig` of `crates/core/tests/support/` (`rig.relay`, `rig.sensor`,
  `rig.motion_sensor`, `rig.post`, `rig.get`, `rig.delete`), as `interface_flow.rs` uses it.
