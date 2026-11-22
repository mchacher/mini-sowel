# Architecture: Spec 018

## Overview

```
sidebar ─► #/zones ─────► ZonesPage ─► ZoneTree (rows: add sub-zone, edit, delete)
                                     └► ZoneForm (modal) ─► useZones.createZone / updateZone / deleteZone
         ─► #/equipments ─► EquipmentsPage (grouped by room) ─► edit, delete
                                     └► EquipmentForm (modal: info ─► DeviceSelector)
room page (#/zone/<id>) ─► "Add equipment" ─► EquipmentForm (room chosen)
                                     │
useZones / useEquipments ──POST/PUT/DELETE /api/v1/zones[/:id], /api/v1/equipments[/:id]──►
                                     existing core routes (spec 010), unchanged
browser ◄── /ws zones, equipments topics: zone.*, equipment.* ──► the stores read again (spec 014)
```

Only the interface changes. The core, the host and the firmware are untouched; one Rust test
replays the interface's requests against the core's API.

## Data

| Name | Type | Where | Bound |
| ---- | ---- | ----- | ----- |
| Zones, equipments, bindings | existing (`zones.json`, `equipments.json`) | host data folder, ESP32 data partition | 32 zones, 64 equipments, 16 bindings of each kind (spec 010), unchanged |
| Interface: `useZones.tree` | `ZoneWithChildren[]` | browser | 32 zones (the host's bound) |
| Interface: `useEquipments.equipments` | `EquipmentWithDetails[]` | browser | 64 (the host's bound) |
| Interface: the equipment form's devices | `DeviceWithData[]`, read once when step 2 opens, dropped when the form closes | browser | 64 devices (the host's bound) |
| Interface: a row's refusal text | `string`, cleared after 5 s | browser | one per row |

Nothing new on disk, in `Home` or in the radio loop. The memory budget (`docs/memory-budget.md`)
gains no line and the worst-case test runs as it is. The interface's first load: about 50 KB gzip
expected (40.5 KB after spec 017), checked by `npm run check:bundle` against 170 KB.

## Files changed

| File | Change |
| ---- | ------ |
| `crates/core/tests/interface_flow.rs` (new) | The interface's sequence of requests through `api::handle` on a `Home` with the garage's three devices (fixtures of `tests/support`): zones made, edited, refused, deleted; equipments made with `deviceIds`, edited, deleted; the aliases and aggregation the room page relies on |
| `ui/src/types.ts` | `ZoneOption` and anything `zone-path.ts` needs, copied from Sowel's `types.ts` |
| `ui/src/lib/zone-path.ts` (ported) | Sowel's `flattenZonesWithPath`, `groupEquipmentsByZone`, `ZONE_PATH_SEPARATOR`, unchanged; unused functions cut |
| `ui/src/api/zones.ts` | `createZone(data: {name, parentId?, icon?, description?})` (Sowel's signature), `updateZone(id, data)`; header updated |
| `ui/src/api/equipments.ts` | `createEquipment({name, type, zoneId, deviceIds?})`, `updateEquipment(id, {name?, zoneId?})`, `deleteEquipment(id)`; header updated |
| `ui/src/store/useZones.ts` | Sowel's `createZone`, `updateZone`, `deleteZone` restored (each reads the tree again on success); header updated |
| `ui/src/store/useEquipments.ts` | Sowel's `createEquipment`, `updateEquipment`, `deleteEquipment` restored (each reads the list again on success); header updated |
| `ui/src/components/zones/ZoneTree.tsx` (ported) | Sowel's tree: rows, indentation, child count, "Add sub-zone"; cuts: reorder arrows and `reorderZones`, `useNavigate` (a link in the hash); additions marked: "Edit" and "Delete" buttons, the 5 s refusal text, no delete on the root |
| `ui/src/components/zones/ZoneForm.tsx` (ported) | Sowel's modal and `flattenZoneTree`; changes: `maxLength` 64, the "None" option cut (default parent the root), the parent select hidden for the root, Escape closes |
| `ui/src/components/zones/ZonesPage.tsx` (ported from `pages/ZonesPage.tsx`) | Sowel's page; cuts: `useAuth` (no roles: D13), `useWsSubscription` (`useLive` subscribes for the session), `PageHeader` replaced by the same classes inline if its port costs more than it saves |
| `ui/src/components/equipments/equipment-type-meta.tsx` (ported) | `EQUIPMENT_TYPE_CATEGORIES` for the seven types, unchanged values; the rest cut |
| `ui/src/components/equipments/DeviceSelector.tsx` (ported) | Sowel's list, check boxes, "All devices", name filter, expander; cuts: candidates, contracts, water valve, pool keys, `formatPower`; added (marked): the 64-device cap |
| `ui/src/components/equipments/EquipmentForm.tsx` (ported) | Sowel's two steps; changes: the seven types, `maxLength` 64, `selectedDeviceIds` sent as `deviceIds`, `pool_heat_pump` validation cut, `typesOfferedOnEdit` cut (type locked), Escape closes; added (marked): the warning when the chosen devices give more than 16 distinct data or order keys (counted from the devices read in step 2) |
| `ui/src/components/equipments/EquipmentsPage.tsx` (ported from `pages/EquipmentsPage.tsx`) | Sowel's page grouped by room; cuts: search, singleton types, `EquipmentCard` (a Solite row with Sowel's classes: name, type, `EquipmentStatusBadge`, devices count, edit, delete) |
| `ui/src/components/home/ZonePage.tsx` | the "Add equipment" button in the header, the form with `defaultZoneId` |
| `ui/src/components/layout/Sidebar.tsx` | the "Equipments" and "Topology" entries (the "Devices" link factored into one `NavLink` for the three) |
| `ui/src/App.tsx` | `#/zones` and `#/equipments` recognised (constants beside `DEVICES_HASH`), the page shown, the sidebar's active entry |
| `ui/src/i18n/messages.ts` | Sowel's keys, en and fr: `nav.equipments`, `nav.zones`, `zones.*` (title, count, addZone, createZone, editZone, addSubZone, childCount, deleteConfirm, form.*, empty.*, error.title), `equipments.*` (title, addEquipment, createEquipment, editEquipment, deleteConfirm, empty.*, form.*, type.* for the seven), `deviceSelector.*` (noCompatible, showAll, and the filter's placeholder), `common.*` (create, creating, back, edit, delete, optional, saving, retry, if missing); Solite's `deviceSelector.max` and `equipments.form.tooManyBindings`; `app.noRoom` changed |
| `ui/src/App.test.tsx`, `ui/src/i18n/i18n.test.ts` | the new `app.noRoom` texts; the routes |
| `scripts/demo-garage.sh` | one comment line: the same garage is made from the interface |
| `docs/roadmap.md` | four inbox lines (the out-of-scope pieces, spec's roadmap delta) |
| `docs/living-spec/README.md` (interface, recipe, History), `docs/decisions.md` (D29), `docs/design-system.md` (port table, size), `docs/architecture.md` (block "Interface") | the living spec delta and the sentences made false (spec, "No longer true") |

## Reused (Phase 1.3)

- Spec 017's devices page: the row's 5 s refusal text, the confirmation with `window.confirm`,
  the store actions that read again after a request, the hash route `#/devices` and its sidebar
  entry (`Sidebar.tsx` `Nav`).
- Spec 014's stores and `useLive`: the `zones` and `equipments` topics are already subscribed and
  their events already read the stores again (`useLive.ts:69-89`); the coalesced read of
  equipments (`useEquipments.ts`, `coalesceCalls`, 1 s).
- `ui/src/zones.ts` (`flattenZones`, `hashForZone`, `chooseZone`) for the hash links and the
  fallback when the room shown is deleted.
- Spec 010's API and its tests (`crates/core/tests/zones.rs`, `equipments.rs`, `api.rs`): the new
  Rust test reuses their fixtures (`tests/support`) and helpers.
- Sowel's ports already in Solite: `EquipmentStatusBadge`, `api-error.ts` (`apiErrorText`),
  `client.ts` (`fetchJSON`).
