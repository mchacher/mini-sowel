# Spec 010: equipments-and-zones

## Context

Increment B4 of the roadmap (renumbered when B3 was inserted). Specs 008 and 009 give devices in
Sowel's format. In Sowel, nothing acts on a device directly: a **zone** (a room) holds
**equipments** (a light, a sensor, a switch...), and an equipment binds device data and orders under
**aliases** (`state`, `temperature`...). Recipes, the interface and zone aggregation read
equipments, never devices. This increment brings zones, equipments and Sowel's zone aggregation,
with Sowel's routes and JSON, so B6's `motion-light` can be given "the garage" and "its light".

Sowel files mirrored (summary with sources: `../008-devices-in-sowel-format/sowel-contract.md`):
`../sowel/src/shared/types.ts` (`Zone` T:268, `ZoneWithChildren` T:279, `ZoneAggregatedData`
T:283, `EquipmentType` T:339, `Equipment` T:419, `EquipmentStatus` T:512, `DataBinding` T:523,
`OrderBinding` T:576, `DataBindingWithValue` T:583, `OrderBindingWithDetails` T:613,
`EquipmentWithDetails` T:663), `../sowel/src/api/routes/{zones,equipments}.ts`,
`../sowel/src/zones/zone-aggregator.ts`, `../sowel/src/equipments/equipment-manager.ts`
(`createWithAutoBindings`).

Specified in autonomy (the user approved building milestone B as far as possible without
hardware, without questions).

## Goals

1. Zones as a tree under Sowel's fixed root zone; devices placed in a zone.
2. Equipments created from devices, with their data and order bindings by alias, as Sowel.
3. Zone aggregation (temperature, humidity, luminosity, motion, doors, windows, leak, smoke,
   lights), served and recomputed live.

## Out of scope

- Sending orders (B5): order bindings exist, `POST /equipments/:id/orders/:alias` is B5.
- Events and WebSocket (B7); recipes (B6); modes (C).
- Equipment types beyond decision D3's six (light, shutter, sensor, switch, thermostat, button),
  as Sowel's `light_onoff`, `light_dimmable`, `shutter`, `sensor`, `switch`, `thermostat`,
  `button`; computed data, timed actions, energy flags.
- Zone orders (`allLightsOff`), zone reorder.

## Requirements

### R1: Zones (core, `crates/core/src/zones/`)

- The root zone `00000000-0000-0000-0000-000000000001`, named `Maison`, icon `home`, parent `null`,
  display order 0, always exists (created at first start, as Sowel's `ensureRootZone`,
  `../sowel/src/zones/zone-manager.ts:79`).
- `GET /api/v1/zones`: the tree (`ZoneWithChildren[]`, the root with its descendants), children
  sorted by `displayOrder` then `name`.
- `GET /api/v1/zones/:id`: the zone with its children; 404 `{"error":"Zone not found"}`.
- `POST /api/v1/zones` `{name, parentId?, icon?, description?, displayOrder?}` → 201 `Zone`;
  `parentId` defaults to the root; 404 `{"error":"Parent zone not found: <id>"}`.
- `PUT /api/v1/zones/:id` (same fields, all optional): 200 `Zone`; 400 `{"error":"A zone cannot be
  its own parent"}`, 400 `{"error":"Moving this zone would create a circular reference"}`, 404.
- `DELETE /api/v1/zones/:id`: 204; 400 `{"error":"Cannot delete the root zone"}`, 400 `{"error":
  "Cannot delete zone with N child zone(s). Remove or move them first."}`, 400 `{"error":"Cannot
  delete zone with N equipment(s). Remove or move them first."}` (Sowel's exact texts, singular
  and plural as `zone-manager.ts:238-266`), 404. Devices in a deleted zone get `zoneId` `null`.
- Names: 1 to 64 characters after trimming (400 `{"error":"Invalid name"}`). At most 32 zones
  (400 `{"error":"Too many zones (32)"}`).

### R2: Devices in zones

`PUT /api/v1/devices/:id` accepts `zoneId` as an existing zone id or `null` (404 `{"error":"Zone not
found"}` otherwise). Spec 008's "zoneId must be null" is replaced.

### R3: Equipments (core, `crates/core/src/equipments/`)

- Types: `light_onoff`, `light_dimmable`, `shutter`, `sensor`, `switch`, `thermostat`, `button`
  (decision D3's six kinds); another type: 400 `{"error":"Unsupported equipment type: <t>"}`.
- `Equipment` fields: `id`, `name`, `zoneId`, `type`, `icon?`, `description?`, `enabled` (true by
  default), `createdAt`, `updatedAt`; Sowel's optional flags are omitted.
- `POST /api/v1/equipments` `{name, type, zoneId, icon?, description?, deviceIds?}` → 201
  `EquipmentWithDetails`; with `deviceIds`, every data and every order of each device is bound with
  `alias = key` (Sowel's `createWithAutoBindings`, `equipment-manager.ts:428-502`); a second binding
  with an alias already taken is skipped. 404 for an unknown zone or device.
- `GET /api/v1/equipments[?type=<t>]` (an unknown type gives `[]`), `GET /api/v1/equipments/:id`
  (404 `{"error":"Equipment not found"}`), `PUT /api/v1/equipments/:id` `{name?, type?, zoneId?,
  icon?, description?, enabled?}`, `DELETE /api/v1/equipments/:id` (204).
- Bindings: `POST /api/v1/equipments/:id/data-bindings` `{deviceDataId, alias}` → 201 `DataBinding`,
  `DELETE .../data-bindings/:bindingId` → 204; the same for `order-bindings` `{deviceOrderId,
  alias}`; 400 `{"error":"Alias already used: <alias>"}`; 404 for an unknown data, order or binding.
- `EquipmentWithDetails`: the equipment, `dataBindings` (`DataBindingWithValue`: the binding plus
  the device data's `deviceId`, `deviceName`, `key`, `type`, `category`, `value`, `unit?`,
  `lastUpdated`, `lastChanged` (the last time the value changed, not just was received), `stale`),
  `orderBindings` (`OrderBindingWithDetails`), `status`.
- `stale`: a binding of a streaming category whose `lastUpdated` is older than its window
  (temperature, humidity, pressure, luminosity 65 min; voltage 5 min; others 15 min; never for
  event categories such as motion or contact: Sowel's `STREAMING_TIMEOUT_MS`,
  `../sowel/src/shared/constants.ts`); `status`: `offline` when a bound device is offline,
  `degraded` when a binding is stale, `online` otherwise, as Sowel derives it (spec 116 in Sowel,
  `equipment-manager.ts` ~894: the implementation mirrors the code there).
- When a device is deleted, its bindings are removed from the equipments. At most 64 equipments
  and 16 bindings of each kind per equipment (bounds written next to them).

### R4: Zone aggregation (core, `crates/core/src/zones/aggregate.rs`)

`ZoneAggregatedData` for every zone, from the equipments of the zone and its descendants, with
Sowel's rules (`../sowel/src/zones/zone-aggregator.ts`): `temperature` the mean of the numeric
bindings of category `temperature` whose alias is exactly `temperature`, 1 decimal; `humidity` the
mean of `humidity` bindings, 1 decimal; `luminosity` the rounded mean of `luminosity` bindings;
`motion` true when a `motion` binding is true, `motionSensors` their count, `motionSince` the time
of the last change of `motion` (`null` without motion sensors); `openDoors` / `openWindows` the
count of `contact_door` / `contact_window` bindings that are false; `waterLeak` / `smoke` true when
one such binding is true; `lightsOn` / `lightsTotal` per equipment of type `light_onoff` or
`light_dimmable` with a `light_state` binding; offline equipments skipped and counted in
`unavailableEquipmentsByCategory` and `unavailableEquipmentNamesByCategory`; every other field of
`ZoneAggregatedData` present with its empty value (`null`, `0`, `false`, `{}`). A `null` average
when no binding has a value. `GET /api/v1/zones/aggregation` → `Record<zoneId,
ZoneAggregatedData>` (every zone). Recomputed whenever a bound value, a binding, an equipment or a
zone changes.

### R5: Storage

`zones.json` and `equipments.json` (with their bindings) in the data folder, beside
`devices.json`, with spec 008's rules: definitions written at once (temporary file, rename, 0600),
only the radio loop writes, a file that cannot be read stops the host. `lastChanged` is kept with
the device values (every 10 minutes).

### R6: The host

Nothing new on the command line: `--serve` serves the new routes; the root zone is created at the
first `--serve`.

### R7: Precisions (after the spec review; they override R1 to R6 where they differ)

1. **Stale** (Sowel's `isStaleBinding`, `../sowel/src/equipments/equipment-status.ts:68`): only
   bindings of a streaming category (`STREAMING_CATEGORIES`, `constants.ts:293`) can be stale;
   never a boolean binding; never one whose `lastUpdated` is `null`; the electrical categories
   (`voltage`, `power`, `energy`, `current`) only on metering equipment types (none of D3's six);
   stale when `now − lastUpdated > window` (strict): temperature, humidity, pressure, luminosity
   65 min; other streaming categories 15 min. `battery`, `motion`, contacts, `light_state` and
   `generic` are never stale.
2. **Status** (Sowel's `deriveEquipmentStatus`, `equipment-status.ts:122`): no binding →
   `offline`; type `button` → `online`; all distinct bound devices offline → `offline`; some
   offline, or a binding stale → `degraded`; a device `unknown` counts as online.
   `statusReason {offlineDevices, staleBindings, offlineSince}` is present when the status is
   not `online`, as Sowel.
3. **Errors and codes** (Sowel's `equipment-manager.ts`): bad type 400 `Invalid equipment type:
   <t>`; unknown zone (POST or PUT) 404 `Zone not found: <id>`; data alias taken 409 `Alias "<a>"
   already exists on this equipment`; order alias taken 409 `OrderBinding for alias "<a>" with this
   device order already exists`; unknown data 404 `DeviceData not found: <id>`; unknown order 404
   `DeviceOrder not found: <id>`; unknown binding, or one of another equipment, 404 `DataBinding not
   found` / `OrderBinding not found`; unknown equipment 404 `Equipment not found`.
4. **Unknown device in `deviceIds`**: skipped (logged), the equipment is created, as Sowel.
5. **Bounds during auto-binding**: a binding past 16 is skipped (logged), like an alias conflict;
   explicit `POST .../data-bindings` past 16: 400 `Too many bindings (16)`; 65th equipment: 400
   `Too many equipments (64)`; 33rd zone (the root counts): 400 `Too many zones (32)`.
6. **`lastChanged`** is kept in `devices.json` with each device data (`#[serde(default)]`: a file
   written before this spec loads, with `null`), set only when a value differs from the previous
   one, and left out of the device API's JSON (Sowel's `DeviceData` has none).
7. **Recompute and `motionSince`**: the aggregation is recomputed in the radio loop after each
   batch of measures and after the device statuses are refreshed (silence alone turns a device
   offline), and after each API change; per zone (at most 32) the last `(motion, motionSince)` is
   kept: `motionSince` is the time of the recompute that first sees `motion` change (after a
   restart, the first computation's time), unchanged while `motion` stays the same, `null` when
   `motionSensors` is 0 (`zone-aggregator.ts:254`).
8. **Unavailable maps**: a category is counted once per offline equipment, not per binding;
   `light_state` only for light types; `temperature` only with alias `temperature`;
   `shutter_position` only for `shutter`; names sorted. Shutters: `shuttersTotal` (type `shutter`
   with a `shutter_position` binding), `shuttersOpen` (position > 0), `averageShutterPosition`
   (rounded mean); no device gives a position yet, so 0 and `null`. `motionSensors` counts motion
   bindings. Contact devices are `contact_door` (spec 009), so `openWindows` stays 0 with real
   devices. `enabled: false` does not remove an equipment from the aggregation (as Sowel).
9. **Lengths**: zone and equipment names 1 to 64 characters after trimming (400 `Invalid name`);
   `icon` at most 32 (400 `Invalid icon`); `description` at most 500 (400 `Invalid description`);
   `alias` 1 to 32, trimmed, not blank (400 `Invalid alias`); each bound written next to its
   constant.
10. **`PUT /zones/:id`**: unknown zone 404 `Zone not found`; unknown parent 404 `Parent zone not
    found: <id>`, checked before the own-parent test; `parentId: null` on a zone other than the
    root: 400 `Only the root zone has no parent`. `POST /zones` with `parentId: null` means the
    root.
11. **Consistency on load**: a binding whose data or order is missing is dropped; a device whose
    `zoneId` is unknown gets `null`; an equipment whose zone is missing moves under the root; a
    `zones.json` without the root gets it back and orphans move under it (Sowel's
    `ensureRootZone`).
12. **Random bytes in the API**: `api::handle(&mut Home, request, now, random)`; `Home` holds the
    devices, zones and equipments behind the one lock, with a change flag per file; HTTP workers
    use the shared random source under the lock.
13. **Input validation**: `Invalid JSON` and wrong field types (400 `Invalid field: <f>`), missing
    `name`, `type` or `zoneId` (400 `Missing field: <f>`); unknown fields refused (`Unknown field:
    <f>`) on zone routes and bindings, **ignored** on `POST` and `PUT /equipments` (Sowel's UI and
    `sowel-plugin-mini` may send Sowel's extra flags).
14. **Shapes**: `GET /zones/:id` returns two levels (children with their children,
    grandchildren with `children: []`, `zone-manager.ts:176`); `GET /equipments` sorted by name
    (`equipment-manager.ts:256`); `PUT /equipments/:id` returns `Equipment`; `POST /equipments`
    returns `EquipmentWithDetails`. The route `zones/aggregation` is matched before `zones/:id`.
15. **Memory ceiling**: 200 KB covers the model (50 devices × 6 data, 10 zones, 50 equipments × 6
    bindings) plus the largest response in flight. A first build measured 269 KB with `GET
    /equipments` built whole (143 KB): the user's 200 KB ceiling is kept, and the long lists
    (`GET /devices`, `GET /equipments`) are sent in chunks instead (decided in autonomy,
    2026-11-21): the core gives a cursor that serialises a few items at a time under the lock
    (at most 4 per chunk); the host writes each chunk with HTTP chunked transfer, the lock released
    between chunks, so neither memory nor a slow client grows with the list. Items changed between
    chunks are seen in their latest state (a list is not a snapshot, as Sowel's is not either).

## Living spec delta

### docs/living-spec/README.md

- New section "Organises the home in zones and equipments": zones (tree, root `Maison`, rules),
  devices placed in zones, equipments (types, auto-binding by alias, bindings, status, stale), the
  zone aggregation and its fields, the storage; the routes and their errors.
- "Serves Sowel's device API": `zoneId` accepts a zone.
- The opening sentence "No equipment, zone, mode or recipe exists in the tool" → "No mode or recipe
  exists in the tool".
- History row 010.

### No longer true

- `docs/living-spec/README.md`: "`zoneId` `null`" in "Keeps devices"; the storage paragraph naming
  only `devices.json`; the closing sentence ("No equipment, zone, mode or recipe exists in the
  tool; ... serves Sowel's device API") → "No mode or recipe ...; ... serves Sowel's device,
  zone and equipment API".
- `docs/architecture.md` (blocks: zones, equipments; data files).
- `docs/decisions.md`: D13's "`zoneId` `null` until zones exist" → replaced by D14 (D13 not
  edited).
- `docs/roadmap.md`: B4's status.
- Searched: `zoneId`, `zone`, `equipment`, `B4`.

## Acceptance criteria

- [x] With `--sim scenarios/garage.json --serve --speed 10`, after pairing: a zone "Garage", the
      three devices in it, three equipments (sensor from the SNZB-02, sensor from the motion
      sensor, `light_onoff` from the relay); `GET /zones/aggregation` shows the garage's
      temperature, motion and `lightsTotal` 1; the root `Maison` aggregates the same.
- [x] A test compares a zone, an equipment with details and an aggregation with examples written
      from Sowel's `types.ts`.

## Edge cases

- Deleting the root, a zone with children, a zone with equipments: Sowel's 400 texts.
- Moving a zone under itself or a descendant: 400.
- Equipment with a device whose data keys collide with existing aliases: skipped.
- A device deleted: its bindings disappear; the equipment stays.
- Aggregation with no value yet: `null`; an offline device's equipment: skipped and listed.
- 33rd zone, 65th equipment, 17th binding: 400.
- `zones.json` or `equipments.json` not parsable: the host does not start.

## Sowel contract

Mirrors the types and routes named in the context. Divergences (decision D14, which replaces
D13's "`zoneId` `null` until zones exist"): only D3's six equipment kinds; no computed data, timed
actions or energy flags; no zone orders or reorder; bounds (32 zones, 64 equipments, 16 bindings)
and string lengths (names 64, Sowel 100); `PUT /devices` checks `zoneId` (404) and deleting a zone
returns its devices to `null` (Sowel does neither); `POST /equipments` always returns details
(Sowel: a plain `Equipment` without `deviceIds`); validation errors as `{"error"}` texts
(Sowel's Fastify has its own format); unknown fields refused on zone and binding routes.

## Decided in autonomy

- Added while building: measured peak 128 KB with chunks of 4 items (269 KB before; 124 KB with chunks of 8): lists and files streamed (D15); the
  model derives instead of storing parent ids, `integrationId`, `source`, `ieeeAddress`, shares
  aliases (at most 128 distinct), uses `Box<str>`; the JSON is unchanged byte for byte. Zones and
  the aggregation are not streamed (7 KB and 18 KB at 32 zones). Sowel's full
  `STREAMING_TIMEOUT_MS` table is mirrored. `PUT` with `{}` on a zone or an equipment is a no-op
  (200), as Sowel; aliases are unique per equipment; a taken alias (409) is checked before the
  16-binding bound (400); `deviceIds` at most 64.
- Root zone named `Maison` like Sowel.
- Status and stale rules mirrored from Sowel's code, with Sowel's windows.

## How the user will check it

First `--sim scenarios/garage.json --pair 30 --speed 10`, then with `--sim scenarios/garage.json
--serve --speed 10`, using `curl` (the exact lines are written in the demo): `POST /api/v1/zones
{"name":"Garage"}`, `PUT /api/v1/devices/<id> {"zoneId":"<garage>"}` for the three devices, `POST
/api/v1/equipments` with `deviceIds` for each, then `GET /api/v1/zones/aggregation`: Garage with
temperature (one decimal), motion true for about 6 s after the motion event at speed 10,
`lightsTotal` 1, and Maison the same.

Run on the real hardware (2026-11-21): The zone "Garage" was created under the root, the devices placed in it, and the equipments "Garage motion" (sensor) and "Garage light" (light_onoff) created through the API.
