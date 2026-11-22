# Spec 019: the Administration section, and the equipment as the entry point

## In short

Specs 017 and 018 misread Sowel's model: they let the user put a **device** in a room. In Sowel a
device never has a room; the user makes an **equipment** (type, name, room) and ties one or more
devices to it, and the room of a device is the room of its equipments. This spec puts that right
and gives the interface Sowel's **Administration** section:

- **Discovery** (one screen per integration; Zigbee today): open pairing, watch the countdown, see
  the new devices appear.
- **Devices**: Sowel's list (name, manufacturer, model, status, and the "Equipment" column
  "Room › Equipment", or "—" when no equipment uses the device), rename, delete. No room select.
- **Equipments** and **Topology** (spec 018's pages) move under Administration.
- A new device's row has **"Create equipment"**: the form opens with that device ticked and its
  type guessed; the user picks the room there. The form now needs at least one device, and binds
  only what the type uses, under Sowel's names (`state`, `brightness`, `battery_2`...), as Sowel's
  interface does.

The quick test: on the real Dongle Max (original firmware over TCP, nothing flashed), pair the
ZBMINIR2 in Discovery, press "Create equipment" on its row, make the light "Lumière garage" in the
garage, then switch it on and off from the garage's page.

## The user's path, screen by screen

1. **Topology** (Administration › Topology, spec 018, unchanged): "Add zone" "Garage". The rooms
   exist before any equipment.
2. **Discovery** (Administration › Discovery, tab "Zigbee"): "Add a device" opens pairing for
   180 s, a countdown runs; the user resets the ZBMINIR2; it appears in the list, marked "New",
   named `ZBMINIR2 567A`, with "—" in its Equipment column and a "Create equipment" button.
3. **Create equipment** (a modal over Discovery): step 1 shows type "Light on/off" (guessed from
   the device: an on/off order and no brightness), name "ZBMINIR2 567A" (editable: "Lumière
   garage"), room "Select a zone…" (the user picks "Garage"; "Next" stays disabled until then).
   Step 2 shows the ZBMINIR2 ticked among the compatible devices; others can be ticked.
   "Create" sends the equipment, then its two bindings (`state` data, `state` order); the modal
   closes; the row's Equipment column now reads "Garage › Lumière garage" and the button is gone.
4. **The room page** (sidebar › Garage): "Lumière garage" with its power button; on, then off; the
   relay clicks and the button follows the relay's confirmed state (spec 014).

Sowel's own path stays open: Administration › Equipments › "Add equipment", or a room page's
"Add equipment", then type, name, room, then the devices.

## Context

Increments J1 and J2 of the roadmap (milestone J, the autonomous box) were built by specs 017 and
018. The analysis of Sowel's base model (Sowel's code, re-checked for this spec) shows where they
diverge from Sowel:

- **Sowel's layers** (`../sowel/docs/technical/data-model/equipments.md`, table "Equipment vs
  Device"): a device is discovered by an integration and never handled by the user; an equipment
  is made by the user, has a required `zoneId` (`Equipment.zoneId: string`,
  `../sowel/src/shared/types.ts`), and reaches its devices through bindings (`DataBinding`,
  `OrderBinding`, same file). The zone aggregation reads equipments only
  (`../sowel/src/zones/zone-aggregator.ts`); recipes have `zone` and `equipment` slots, never
  `device`.
- **`Device.zoneId`** exists in Sowel's model, base and `PUT /api/v1/devices/:id`
  (`../sowel/src/api/routes/devices.ts`), but nothing reads it and Sowel's interface never writes
  it (`../sowel/ui/src/store/useDevices.ts` sends `{ name }` only): a V0.1 leftover. Spec 017
  built a room select and "New room…" on it; spec 018 then wrote "the devices of a deleted zone
  go back to No room" into the Topology page.
- **Sowel's interface binds by a plan**, not by keys: `EquipmentsPage` creates the equipment, then
  `autoCreateBindings` (`../sowel/ui/src/components/equipments/bindingUtils.ts`) computes which
  data and orders the type uses (`RELEVANT_DATA`, `RELEVANT_ORDERS`, the channel candidates of
  `../sowel/src/shared/binding-candidates.ts`), names them by category (`resolveAlias`,
  `DATA_CATEGORY_ALIASES`, `ORDER_CATEGORY_ALIASES`; a second `battery` becomes `battery_2`), and
  sends one request per binding. Spec 018 (decision D29) instead sends `deviceIds` in the `POST`,
  which binds every data and order under its key (Sowel's server-side `createWithAutoBindings`).
  A light then also gets what it does not use, and two sensors' `battery` keep only the first.
- **Sowel's navigation**: an "Administration" section at the foot of the sidebar (Lucide
  `Shield`, `nav.administration`), opened on an administration page, toggled by its title, its
  entries from `../sowel/ui/src/components/layout/admin-nav-items.ts` (Devices, Equipments,
  Topology, ..., Integrations). Solite lists Devices, Equipments and Topology loose under the
  rooms.
- **Sowel's Devices page** (`../sowel/ui/src/pages/DevicesPage.tsx`,
  `components/devices/DeviceList.tsx`): a table with the column "Equipment" ("Room › Equipment",
  derived from the equipments whose data bindings use the device; "—" when none). No room select.
- **Pairing** is not in Sowel (Zigbee2MQTT pairs); spec 017's route and engine are Solite's own
  and are right. Sowel groups an integration's screens per integration (Integrations page, the
  Devices page's integration tabs); Solite will have more than Zigbee, so its pairing screen is
  per integration.

This spec keeps what 017 and 018 got right (the permit-join route and the pairing engine; the
Topology page; the equipment form's two steps, type, name, room, then devices; the room page's
"Add equipment"; every API route) and replaces what they got wrong. Specified in autonomy (no
questions); choices under "Decided in autonomy".

## Goals

1. An **Administration** section in the sidebar and the phone drawer, Sowel's layout: Devices,
   Equipments, Topology, Discovery.
2. **Discovery**, one screen per integration (Zigbee today): spec 017's pairing controls and the
   devices discovered while the screen is open.
3. **Devices**: Sowel's list with the Equipment column; rename, delete; no room on a device.
4. **The equipment is the entry point**: "Create equipment" on a device row with no equipment
   opens the form with that device ticked and its type guessed; the form needs at least one
   device; it binds as Sowel's interface does (what the type uses, Sowel's aliases).
5. Nothing the interface does writes `Device.zoneId` any more.

## Out of scope

- The equipment detail page (Sowel's `EquipmentDetailPage`): completing an equipment, adding or
  changing its devices (`AddBindingModal`, `ChangeDeviceModal`). Hence the form's rule of at least
  one device (R5). Inbox.
- The device detail page (Sowel's `DeviceDetailPage`: data, orders, raw exposes); the Devices
  page's search, sort, source, battery, LQI and last-seen columns. Inbox.
- Sowel's `GET /api/v1/integrations`: the integrations come from a list in the interface (R2).
  Inbox.
- The binding plan in the core: the interface computes it, as Sowel's does (decision D30). The
  core gets it when the box makes equipments itself (milestone C's proposals). Inbox.
- Any change to the core's code, the host, the firmware or `scripts/demo-garage.sh`: every route
  exists. `POST /api/v1/equipments` with `deviceIds` keeps binding every key (Sowel's
  `createWithAutoBindings`), for the API's clients; the interface no longer sends `deviceIds`.
- Removing `Device.zoneId`, `PUT /devices/:id {"zoneId"}` or the core's `clear_zone`: kept for
  Sowel's format (D3); they have no effect anywhere.
- Channel choice for a multi-channel device (Sowel's candidate picker): the first channel is
  bound; Solite's devices have one.
- Sowel's other administration entries (Calendar, Plugins, publishers, Logs, Backup), its
  collapsed sidebar, roles (every Solite user is an administrator until J4).

## Where it runs

Interface only (`ui/`), served by the host on the Mac and, later, by the box. The core, the host
and the firmware are untouched (Rust tests only are added). The real-hardware check uses the
Dongle Max's original firmware with its radio over TCP: no flashing (decision D6).

**Risk on the box**: creating an equipment sends up to 33 sequential requests (1 + 16 + 16), each
its own connection; the core accepts them in sequence (a Rust test sends 33), but their time and
the API's 503 `Busy` on the ESP32 are covered by neither the Mac tests nor QEMU. The hardware demo
(from the Mac) is this spec's only real check; the box's run is E3's (inbox line).

## Requirements

### R1: The Administration section

Ported from Sowel's `components/layout/Sidebar.tsx` (its Administration block),
`SidebarItem.tsx` and `admin-nav-items.ts` (commit `5da84dc1`, the commit of every earlier port).

- **Place**: at the foot of the sidebar's navigation, under a top border (`border-t
  border-border-light`), below the rooms' tree, which is unchanged. Same in the phone drawer.
- **Title**: "Administration" (Sowel's `nav.administration`, the same word in French), Lucide
  `Shield` 16 px, with Sowel's chevron. A click opens or closes the list; it is open whenever an
  administration page is shown (Sowel's `getSectionForPath`), closed on a room page until clicked.
- **Entries**, in this order, each a link in the hash (Sowel's `SidebarItem` classes: `bg-primary-light
  text-primary font-medium` when shown, `aria-current="page"`):
  1. "Devices" / "Appareils" (`nav.devices`, Lucide `Radio`), `#/devices`;
  2. "Equipments" / "Équipements" (`nav.equipments`, `Box`), `#/equipments`;
  3. "Topology" / "Topologie" (`nav.zones`, `Map`), `#/zones`;
  4. "Discovery" / "Découverte" (Solite's `nav.discovery`, `Radar`), `#/discovery/solite`, in the
     place of Sowel's "Integrations" entry.
- When an administration page is shown, no room of the tree is marked; on a room page, no entry is
  marked. Picking an entry in the drawer closes the drawer.
- The three loose entries of specs 017 and 018 under the rooms are removed.

### R2: Discovery, one screen per integration

- **Address**: `#/discovery/<integrationId>`. `#/discovery` alone, or an id the interface does not
  know, shows the first integration (Sowel's fallback for a tab that no longer exists).
- **Integrations**: a list in the interface (`components/discovery/integrations.ts`), one entry per
  integration: its id (the devices' `integrationId` and the API's `/integrations/:id`), its label,
  its screen. Today one entry: `solite`, "Zigbee". A second integration is one more entry.
- **Header**: the title "Discovery" ("Découverte"); below it, with one integration (today), its
  name as a heading ("Zigbee"), no tab bar. Past one integration, Sowel's integration tabs
  (`DevicesPage`'s `TabButton`: label and count of that integration's devices, `border-b-2
  border-primary text-primary` on the one shown) replace the heading; their container clips its
  vertical overflow (`overflow-y-hidden`), because the active tab's `-mb-px` would otherwise draw a
  stray vertical scrollbar beside `overflow-x-auto`.
- **The Zigbee screen** (`ZigbeeDiscovery`), spec 017's controls moved here unchanged: "Add a
  device" sends `POST /api/v1/integrations/solite/permit-join {"time": 180}`; while open, "Pairing
  open: put the device in pairing mode" with the seconds left, counting down, and "Stop" (`{"time":
  0}`); the state read again at 0, one second after a `POST` (to show `error`), and on every
  reconnection; the radio's refusal shown in `text-error`.
- **The list**: the devices of this integration discovered (`device.discovered`) while the screen is
  open, newest first, each marked "New", as rows of R3's list (name editor, manufacturer, model,
  status, Equipment column, "Create equipment", delete). Leaving the screen empties it (spec 017's
  "until the page is left"). Empty: "Press Add a device, then put the device in pairing mode." and
  a link "See all the devices" to `#/devices`.

### R3: Devices

Ported from Sowel's `pages/DevicesPage.tsx` and `components/devices/DeviceList.tsx`, cut.

- **Header**: Sowel's title "Devices" ("Appareils"), the subtitle (`devices.subtitle`: count and
  online; with no device, `devices.empty.title`), and a button "Add devices" ("Ajouter des
  appareils", Solite's key) that opens `#/discovery/solite`. Sowel's integration tabs only when
  the devices come from more than one integration (Sowel's rule); never today.
- **The list**: Sowel's table (`card`, Sowel's header classes, rows `border-b border-border-light`),
  sorted by name (Sowel's default sort; no sort buttons). Columns:
  - status: Sowel's dot (`bg-success` online, `bg-error` offline, `bg-text-tertiary` unknown), its
    label for screen readers;
  - "Name": Sowel's `DeviceNameEditor` (spec 017, unchanged: pencil, Enter saves, Escape cancels, 1
    to 64 characters, the API's refusal under it), and the "New" mark when R2 shows the row;
  - "Manufacturer" (`hidden lg:table-cell`), "Model" (`hidden md:table-cell`): the value or "—";
  - "Equipment" (`devices.col.equipment`): Sowel's `EquipmentCell`: one line per equipment whose
    data bindings use the device, "Room › Equipment": the room's path label
    (`flattenZonesWithPath`) as a link to its page (`#/zone/<id>`), "›", the equipment's name (plain
    text: Solite has no equipment page). None: "—";
  - actions: "Create equipment" ("Créer un équipement", Lucide `Plus`) when the Equipment column
    is "—" (R4); Delete (spec 017's confirmation and refusals: 409 shows its text for 5 s, 404
    removes the row).
  On a phone the hidden columns follow Sowel's breakpoints; the Equipment column stays visible
  (it is what this spec is about), below the name.
- **The Equipment column is live**: it is computed from the equipments the interface already holds
  (`useEquipments`, refreshed on `equipment.*` events and after the page's own requests) and the
  zone tree (`useZones`).
- **No room on a device**: no room select, no "No room", no "New room…"; nothing sends `PUT
  /api/v1/devices/:id` with `zoneId`. The store's `updateDeviceZone` is removed.
- **Empty**: Sowel's `devices.empty.title` "No devices discovered" and "Pair one in Discovery."
  with a link to `#/discovery/solite`.
- **Error**: Sowel's error state (`devices.error.title`, the text, "Retry" that reads again).

### R4: "Create equipment" from a device row

- Offered on a row (R2, R3) whose Equipment column is "—".
- Pressed: the interface reads the device (`GET /api/v1/devices/:id`, with its data and orders),
  then opens R5's form (Sowel's `equipments.createEquipment` title) with:
  - **the type guessed** from the device, the first that fits in this order: `light_dimmable` (an
    on/off order and a `light_brightness` data), `light_onoff` (an on/off order), `shutter`,
    `thermostat`, `button`, `sensor` (each by R5's compatibility rule). None fits: Sowel's default
    `light_onoff`, the user chooses;
  - **the name**: the device's name, editable;
  - **the room**: "Select a zone…", to choose (this is where the room is picked);
  - **the device ticked**.
- The read fails (404: deleted meanwhile; or another error): the form does not open; the error's
  text shows under the row for 5 s; on 404 the row is removed.

### R5: The equipment form (spec 018's, changed)

Unchanged from spec 018: two steps (type, name, room; then devices), the seven types of D3, names
1 to 64, room required, "Back", edit in one step, refusals in the form, Escape and click outside.
Changed:

- **Opening with devices** (R4): the devices given are ticked; "Next" leads to step 2 as usual.
- **Changing the type keeps the devices ticked** (Sowel clears them): a device ticked stays ticked
  and listed even when it is not compatible with the new type.
- **Step 2 reads the devices again**: each time step 2 opens, it reads `GET /api/v1/devices`
  (with data and orders); then, while it is open and the ticked devices give nothing the type uses
  (the plan below is empty), it reads them again every 5 s, and stops as soon as the plan is not
  empty or the form closes. A device just paired gets its data only once its reporting is set up
  (the host prints `reporting:`), after `device.discovered`: this read picks them up without
  closing the form. The plan is computed from the last read.
- **Step 2 lists**: the devices ticked first (whether compatible or not), then the compatible ones
  (Sowel's "All devices" toggle and name filter unchanged).
- **Compatibility**, as Sowel's `DeviceSelector` decides it for D3's types:
  - `light_onoff`, `switch`: an on/off order (category `light_toggle` or `toggle_power`, or an
    enum order whose values are only ON, OFF, TOGGLE); Sowel's channel rule
    (`computeBindingCandidates`);
  - `light_dimmable`: the same on/off order (Sowel's rule; its brightness is bound when there is one);
  - `shutter`: an order of category `set_shutter_position` or `shutter_move` (Sowel's category
    fallback; Sowel's `shutter_*` key convention is not ported, no Solite device has it);
  - `thermostat`: a data of category `setpoint` or an order of category `set_setpoint` (Sowel's
    `isThermostatDevice`);
  - `sensor`, `button`: a data of the type's categories (`EQUIPMENT_TYPE_CATEGORIES`, unchanged).
- **At least one device**: "Create" is disabled while no device is ticked, with the text "Choose
  at least one device" ("Choisissez au moins un appareil"); and while the ticked devices give
  nothing the type uses (the plan below is empty), with "These devices give nothing this type
  uses yet. A device just paired gets its data once its setup is done: wait for it, or change the
  type." ("Ces appareils ne donnent encore rien que ce type utilise. Un appareil tout juste appairé
  reçoit ses données une fois sa configuration finie : attendez-la, ou changez de type."). The room page's and the Equipments
  page's "Add equipment" follow the same rule.
- **What is bound** (Sowel's `computeBindingPlan`, `resolveAlias`, `uniqueAlias`, ported for D3's
  seven types in `bindingUtils.ts` and `lib/binding-candidates.ts`), device by device in the order
  ticked:
  - `light_onoff`, `switch`: the first on/off order and the data of the same key;
  - `light_dimmable`: the first on/off order, the orders whose key contains `brightness`, and the
    data of the same keys;
  - `shutter`: the data of category `shutter_position` and the orders of R5's shutter categories;
  - `sensor`: the data of categories temperature, humidity, pressure, luminosity, co2, voc, noise,
    motion, contact_door, contact_window, water_leak, smoke, battery; no order;
  - `button`: the data of categories action, battery; no order;
  - `thermostat`: the data of categories temperature, temperature_outdoor, setpoint, power,
    operation_mode, generic; every order.
  - **Alias**: Sowel's order: the thermostat's own (`toggle_power` → `power`, `temperature_outdoor`
    → `outsideTemperature`, `operation_mode` and `set_operation_mode` → `operationMode`, a boolean
    `power` data → `state`); then by category (data: `light_state` → `state`,
    `light_brightness` → `brightness`, `shutter_position` → `position`, `setpoint` → `setpoint`,
    `battery` → `battery`; orders: `light_toggle` and `toggle_power` → `state`, `set_brightness`
    → `brightness`, `set_shutter_position` → `position`, `shutter_move` → `state`, `set_setpoint`
    → `setpoint`); then Sowel's per-type keys (`R1` to `R4` → `state` for lights and switches;
    `targetTemperature` → `setpoint`, `insideTemperature` → `temperature` for a thermostat); else
    the key. An alias already taken on the equipment gets `_2`, `_3`... (`uniqueAlias`): the TH01
    and the MS01 in one sensor give `battery` and `battery_2`.
  - So the ZBMINIR2's light gets the data `state` and the order `state`; the TH01's sensor
    `temperature`, `humidity`, `battery` (not `voltage`); with the MS01, also `occupancy` and
    `battery_2`.
- **Bounds**: the plan keeps at most 16 data and 16 order bindings (the API's `MAX_BINDINGS`); the
  form's warning of spec 018 now counts the plan's bindings ("These devices give N values; only 16
  are bound", unchanged text). At most 64 devices ticked (spec 018, unchanged).
- **Create, the requests**, in order, each after the previous one answered:
  1. `POST /api/v1/equipments {"name", "type", "zoneId"}` (201), without `deviceIds`;
  2. for each data of the plan, `POST /api/v1/equipments/:id/data-bindings {"deviceDataId",
     "alias"}` (201); then for each order, `POST .../order-bindings {"deviceOrderId", "alias"}`
     (201).
  - The plan is computed from step 2's last read of the devices.
  - A refusal at step 1: the form stays open with its text (spec 018).
  - A refusal at step 2 (a device deleted meanwhile: 404 `DeviceData not found: <id>` or `DeviceOrder not found: <id>`; 409 alias taken;
    503 `Busy`; the connection lost): the interface deletes the new equipment (`DELETE
    /api/v1/equipments/:id`, best effort), the form stays open with the refused request's text,
    and nothing is left half made unless that delete fails too (then the equipment shows in
    Equipments, deletable, with the bindings made so far).
  - Success: the form closes; the stores read the equipments again (their own read, and the
    `equipment.*` events).

### R6: Language, look and budget

- French and English; Sowel's keys and texts where Sowel has them (`nav.administration`,
  `nav.devices`, `nav.equipments`, `nav.zones`, `devices.col.*`, `devices.error.title`,
  `common.retry`, `common.all`), Solite's keys otherwise (`nav.discovery`, `discovery.title`,
  `discovery.integration.solite`, `devices.addDevices`, `devices.createEquipment`,
  `devices.empty.message`, `discovery.empty`, `discovery.seeAll`, `equipments.form.deviceRequired`,
  `equipments.form.nothingToBind`). The keys of the room select (`devices.room`,
  `devices.noRoom`, `devices.newRoom`, `devices.newRoomName`, `devices.createRoom`) are removed.
- Sowel's tokens, classes and Lucide icons by name; no raw colour.
- The first load stays under the 170 KB gzip budget (enforced by `npm run check:bundle`) and,
  for this spec, at most **60 KB gzip** (48.2 KB after spec 018, about 55 KB expected): a gate of
  the plan's step 7, read from `check:bundle`'s output; over it, the spec is reviewed before going
  on.

## Living spec delta

### docs/living-spec/README.md

- **Users**: Changed: "They pair Zigbee devices, choose the room, accept the proposed recipes" →
  "They pair Zigbee devices, make each one an equipment in its room, accept the proposed recipes".
- **Serves Sowel's device API**, row `PUT /api/v1/devices/:id`: Changed: "renames it (1 to 64
  characters) and/or places it in a zone (an id, or `null`; 404 `Zone not found`)" → "renames it
  (1 to 64 characters); `zoneId` (an id, or `null`; 404 `Zone not found`) is kept as in Sowel's
  format, read by nothing: a device is in a room only through its equipments".
- **Organises the home in zones and equipments**:
  - Opening paragraph: New: "A device has no room of its own: it is in the rooms of the
    equipments that bind it."
  - Zones bullet: Changed: "A device is placed with `PUT /api/v1/devices/:id {"zoneId": ...}`." →
    removed (said in the API row).
  - Equipments bullet: New, after the `POST` sentence: "(The interface binds otherwise: see
    'Shows the home in the browser'.)"
- **Shows the home in the browser**:
  - Opening paragraph: Changed: "The interface makes the rooms (Topology) and the equipments
    (Equipments, and a room's 'Add equipment'), as well as showing them." → "The interface pairs
    the devices (Discovery), makes the rooms (Topology) and the equipments (Equipments, a room's
    'Add equipment', a device's 'Create equipment'), as well as showing them."
  - **Sidebar**: Changed: "the rooms as a tree, then the entries Devices, Equipments and Topology"
    → "the rooms as a tree, then, at its foot, the Administration section: Devices, Equipments,
    Topology, Discovery; open on an administration page, opened or closed by its title".
  - **Devices**: rewritten: "`#/devices`: every device by name, with its status, name (editable),
    manufacturer, model and Equipment column ('Room › Equipment' for each equipment that binds it,
    the room a link to its page; '—' when none); 'Create equipment' on a device with none; delete
    (confirmed; a refusal shows its text for 5 s); 'Add devices' opens Discovery. A device has no
    room select."
  - New bullet **Discovery** (`#/discovery/<integration>`, Zigbee's is `#/discovery/solite`): one
    tab per integration (Zigbee); "Add a device" opens pairing for 180 s (a countdown from the
    route's `remaining`, and "Stop"); the devices discovered while the screen is open appear first,
    marked "New", as rows of the Devices list; leaving the screen empties it. It follows
    `device.discovered`, `device.removed` and `device.status_changed` live.
  - **Topology**: Changed: "(confirmed; refused with Sowel's text when it has sub-zones or
    equipments, shown 5 s; its devices go back to no room)" → "(confirmed; refused with Sowel's
    text when it has sub-zones or equipments, shown 5 s)".
  - **Equipments**: Changed: "then the compatible devices (all on demand), whose data and orders
    are bound under their key" → "then the devices (at least one; those ticked first, then the
    compatible ones, all on demand); only what the type uses is bound, under Sowel's names
    (`state`, `brightness`, `position`, `battery`; `battery_2` for a second one), one request per
    binding after the equipment's; if one is refused the equipment is deleted again and the form
    shows the text". New: "A device's 'Create equipment' opens the form with the device ticked,
    its type guessed (dimmable light, light, shutter, thermostat, button, sensor) and its name;
    the room is chosen there."
  - **The room**: unchanged (its "Add equipment" opens the same form).
- **History**: New row `| 019 | The Administration section (Devices, Equipments, Topology,
  Discovery per integration); a device has no room; equipments made from a device's row, bound as
  Sowel's interface binds |`.

### docs/decisions.md

- New decision **D30 "The equipment is the entry point"** (2026-11-22), replacing D29's way of
  binding (D29's screens and zone form stay): the interface never gives a device a room
  (`Device.zoneId` kept in the API for Sowel's format, D3, read by nothing); the equipment form
  needs at least one device, since Solite has no equipment page to complete one; it binds as
  Sowel's interface does, from a plan computed in the browser (Sowel's `bindingUtils.ts` and
  `binding-candidates.ts`, ported for D3's types) and one request per binding, the equipment
  deleted again if one is refused; `POST /equipments` with `deviceIds` keeps Sowel's
  `createWithAutoBindings` (every key) for the API's clients. Sowel's Administration section is
  ported, with a Discovery screen per integration in the place of Sowel's Integrations entry.
  Why: Sowel's model (equipments.md, "Equipment vs Device"); the plan beside the selector that
  uses the same rule (Sowel's architecture: one module decides compatibility and bindings); the
  core keeps Sowel's route as Sowel's server has it, and the realistic home of D20, its firmware
  fixtures and memory figures are untouched. **D29's two arguments, reversed**: (1) "the
  browser-side plan is over 800 lines of Sowel code the box's interface has no room for": only
  the part for D3's seven types is ported (about 250 lines, about 5 KB gzip), and the first load
  stays far under the 170 KB budget (a gate of 60 KB for this spec, checked with `npm run
  check:bundle`); (2) "one request is atomic": atomicity is given up for Sowel's bindings; a
  refused binding deletes the equipment again, and the half-made equipment is visible for the time
  of the requests (as in Sowel). The equipment made by the old way, every key bound, was wrong for
  the type, which outweighs atomicity. **Risk on the box**: one creation sends up to 33 sequential
  requests (1 + 16 + 16), one connection each, to an ESP32 that answers 503 `Busy` past 4 requests
  at once; sequential requests stay under it, but neither the Mac tests nor QEMU measure the
  box's time or connection behaviour; the hardware demo (from the Mac) and E3 on the box are the
  checks.

### docs/roadmap.md

- Row J1: Changed: "Add a device from the interface: pairing while serving, the device appears
  live, named and placed in a room" → "Add a device from the interface: pairing while serving,
  per integration (Administration › Discovery), the device appears live and named; it has no room
  of its own (the room is its equipment's, J2)". Done when: unchanged.
- Row J2: Changed: "Rooms and equipments from the interface (Sowel's screens ported)" → "Rooms and
  equipments from the interface (Sowel's screens ported under Administration): the equipment is
  the entry point (type, name, room, 1 or N compatible devices, bound as Sowel's interface binds);
  'Create equipment' from a new device's row". Done when: "The garage and its equipments made
  with the mouse, the light from the ZBMINIR2's row".
- Inbox, new lines (2026-11-22, "Came from": Spec 019):
  - The equipment detail page (Sowel's `EquipmentDetailPage`: its bindings, values, history), to
    complete an equipment or replace its devices (Sowel's `ChangeDeviceModal`); until then the form
    needs at least one device. This line **replaces** spec 018's line "The equipment detail page
    (Sowel's `EquipmentDetailPage`): its bindings, values, history", which it widens; spec 018's
    line "Bindings added and removed one by one from the interface (`AddBindingModal`,
    `MissingBindingsModal`)" **stays** as it is (not duplicated here).
  - Measure on the box (E3) the creation of an equipment with 16 data and 16 order bindings (33
    sequential requests): time, and no 503 `Busy`.
  - The device detail page (Sowel's `DeviceDetailPage`: data, orders); the Devices page's search,
    sort and battery, LQI, last-seen columns.
  - Sowel's `GET /api/v1/integrations`, so Discovery's tabs come from the core rather than a list
    in the interface.
  - The binding plan in the core (Rust), when the box makes equipments itself (milestone C's
    proposals); the interface's plan (spec 019) is its reference.
  - A channel choice for a multi-channel device (Sowel's candidate picker in `DeviceSelector`).
- Log: one line for the J1 and J2 rows corrected and the inbox lines.

### No longer true

Found by searching `docs/`, `README.md`, `CLAUDE.md`, `inputs/README.md`, `.claude/`, `ui/src`,
`crates`, `scripts` for: placed in a room, room select, No room, New room, `newRoom`,
`updateDeviceZone`, `zoneId` on devices, devices go back, under their key, key as alias, alias =
data key, one request, `deviceIds`, Devices / Equipments / Topology entries, pick or choose the
room.

- `docs/living-spec/README.md`: the sentences of the delta above (Users; the `PUT` row; "A device
  is placed with..."; the interface's opening, Sidebar, Devices, Topology and Equipments bullets).
  "Deleting a zone returns its devices to no zone" (Zones bullet) stays: still what the core does.
- `docs/living-spec/README.md` History row 017 "new devices appear live, named and placed in a
  room" → "new devices appear live and named" (the room placement is gone; row 019 says why).
- `docs/specs-index.md` row 017 "new devices announced live, named and placed in a room from a
  devices page" → "new devices announced live and named, from a devices page (its room select
  superseded by spec 019)".
- `specs/017-add-device-from-interface/spec.md`, superseded parts (a note at the top names them;
  the text stays as the record of what was built): Goal 3 "place them in a room (an existing one,
  or a new one made on the spot)"; Out of scope "Editing rooms beyond creating one under the
  home's root"; R5 "a room select", the whole "Room select" bullet ("No room", "New room…", the
  zone made under the root, the rollback), "Empty home text" ("create one from the devices page",
  already changed by 018); the delta's "the list with rename, room (including 'New room…')" and
  History "named and placed in a room"; acceptance "renaming, choosing a room, 'New room…'";
  edge cases "'New room…' refused (32 zones)", "A rename or zone refused", "'New room…' with an
  empty name", "Only the root zone: the select offers 'No room' and 'New room…'"; Decided in
  autonomy "'New room…' in the room select"; How the user will check it "choose 'New room…'
  'Garage'", "is renamed and placed in the garage".
- `specs/018-rooms-and-equipments-from-interface/spec.md`, superseded parts (a note at the top):
  R1 "three entries in this order ... under the rooms"; R2 "The devices of a deleted zone go back
  to 'No room' (the API's rule, spec 010)"; R4 "Changing the type clears the devices chosen",
  "Create, the request: one `POST /api/v1/equipments {"name", "type", "zoneId", "deviceIds"}` ...
  the core binds every data and order of the chosen devices under their key as alias ... No device
  chosen: the equipment is created without binding (status `offline`, as Sowel). Decision D29:
  one request rather than Sowel's interface", "Binding cap ... When the chosen devices carry more
  than 16 distinct data keys", "A key given by two devices ... is bound once, the second skipped
  silently"; the delta's "whose data and orders are bound under their key", "its devices go back
  to no room", D29's binding sentence; edge cases "creating with no device is allowed (status
  offline)", "Two devices with the same data key (TH01 and MS01 both give `battery`): the second
  is skipped", "the equipment is created without its bindings" (a device deleted before Create:
  now the equipment is deleted again, R5); Decided in autonomy "One `POST` with `deviceIds`
  (D29)... Aliases are the data keys".
- `docs/decisions.md` D29: its binding choice ("one `POST /api/v1/equipments` with `deviceIds`
  (alias = data key ...)") is replaced by D30 (a new decision; D29 is not edited).
- `docs/roadmap.md` rows J1 and J2: as in the delta.
- `docs/architecture.md:43`, block "Interface": "the devices page (`components/devices/`,
  `store/usePairing.ts`) starts pairing and places the devices in rooms; the Topology and
  Equipments pages ... make the rooms and the equipments" → "the Administration section
  (`components/layout/`): Discovery per integration (`components/discovery/`,
  `store/usePairing.ts`) pairs, Devices (`components/devices/`) lists them with their equipments,
  Topology and Equipments (`components/zones/`, `components/equipments/`) make the rooms and the
  equipments, bound from a plan computed in the browser (`components/equipments/bindingUtils.ts`,
  `lib/binding-candidates.ts`)".
- `docs/design-system.md`: the port table rows `api/...` ("equipments: get, create with
  `deviceIds`, ...") → create, add a data binding, add an order binding; `bindingUtils.ts` ("the
  named subsets ..., copied unchanged") → plus the binding plan for D3's types (spec 019);
  `EquipmentsPage ... EquipmentForm ... DeviceSelector` ("no search, candidates or contracts; ...
  one `POST` with `deviceIds`") → Sowel's candidates for lights, switches and shutters, the
  thermostat contract, the plan and one request per binding, at least one device; new rows:
  `components/layout/{admin-nav-items,SidebarItem}`, the Administration block of `Sidebar.tsx`,
  `pages/DevicesPage.tsx` and `components/devices/DeviceList.tsx` (cut), `lib/binding-candidates`;
  the layout paragraph "the rooms' tree below" → "the rooms' tree below, the Administration
  section at its foot"; "first load 48.2 KB gzip after spec 018" → the size measured after this
  spec.
- `ui/src/i18n/messages.ts`: `devices.empty.message` ("Press Add a device, then put the device
  in pairing mode.") moves to `discovery.empty`; `devices.empty.message` becomes "Pair one in
  Discovery." / "Appairez-en un dans Découverte."; the room select keys removed (R6).
- `ui/src/api/equipments.ts:13` "Solite creates the bindings in the same request: `deviceIds` are
  bound under their key as alias (decision D29; ...)" → the bindings are created one by one from
  the plan (D30), with `addDataBinding` and `addOrderBinding`.
- `ui/src/store/useDevices.ts:3` "Added: `updateDeviceZone` ..." → removed.
- `ui/src/components/devices/DeviceRow.tsx` (its header "the room select with 'New room…'") →
  the file is removed, replaced by `DeviceList.tsx`.
- `ui/src/components/devices/DevicesPage.tsx:1-3` header "Sowel's DevicesPage and DeviceList ...
  are out of scope" → ported, cut.
- `ui/src/components/equipments/EquipmentForm.tsx:1-6` header "the form returns `deviceIds` only
  (the core binds them in the one `POST`, decision D29 ...)" → the form returns the devices ticked
  and the plan's requests are sent by `NewEquipmentForm` (D30).
- `ui/src/components/equipments/DeviceSelector.tsx:1-5` header "Cut (spec 018): the candidate
  channels, contracts (thermostat, ...)" → candidates for lights, switches and shutters and the
  thermostat contract ported; `bindableCount` (distinct keys) → the plan's count.
- `ui/src/components/equipments/EquipmentsPage.tsx:1-5` header "the form sends one request with
  `deviceIds` (decision D29)" → the plan's requests (D30).
- `ui/src/components/layout/Sidebar.tsx`: `PAGES` (three loose entries) → the Administration
  section.
- `ui/src/store/usePairing.ts:18-21` "The devices page is on screen: only then is a discovered
  device marked 'New'" → the Discovery screen.
- `ui/src/api/zones.ts:17-19` "`createZone` and `deleteZone` were added by Solite for the devices
  page (spec 017: a room under the root, undone when the device cannot be placed in it)" → used by
  the Topology page (spec 018).
- `ui/src/api/devices.ts:1-4` "the devices page shows no data (spec 017); the equipment form's
  device selector reads it (spec 018)" → adds `getDevice`, read by "Create equipment" (spec 019).
- `ui/src/store/useDevices.ts:1-3` "Cuts: the device data and heartbeat (the devices page shows
  none, spec 017)" → still cut; the `updateDeviceZone` line removed.
- `ui/src/components/zones/ZoneTree.tsx:5-6` "a refusal of the delete shows under the row for 5 s,
  as the devices page's rows" → as the Devices list's rows.
- Tests that assert what is no longer true (rewritten in the plan's test steps):
  `ui/src/components/devices/DevicesPage.test.tsx` (`:148-226`, the room select and "New room"
  tests, removed; `:73-125` and `:261`, the pairing header and the "New" mark, moved to
  `DiscoveryPage.test.tsx`), `ui/src/store/stores.test.ts:98` (`updateDeviceZone`),
  `ui/src/App.test.tsx:64` (the sidebar's links in order, ending Devices, Equipments, Topology),
  `:170` (the drawer's links, same), `:232-251` (the devices page reached from a loose sidebar
  entry and the drawer), `:253-275` (the loose entries marked and their order),
  `ui/src/store/useLive.test.ts:178` ("marks no device new while the devices page is not open" →
  the Discovery screen),
  `ui/src/components/equipments/EquipmentForm.test.tsx:86`, `:100`, `:108`, `:129`, `:141`
  (one request with `deviceIds`, creating with no device, the warning by distinct keys, the type
  clearing the devices), `ui/src/components/equipments/DeviceSelector.test.tsx:47`, `:57`, `:122`
  (category-only compatibility, the warning's count), `ui/src/api/equipments.test.ts:8` (`POST`
  with `deviceIds`).
- Still true, kept: `crates/core/tests/interface_flow.rs:271` "a device placed there goes back to
  no room" (the API's `PUT {"zoneId"}` and zone delete, unchanged); `crates/core/tests/interface_flow.rs`
  `equipments_are_made_from_devices_with_the_keys_as_aliases` and
  `an_equipment_with_no_device_or_a_deleted_one_has_no_binding` (the API's `deviceIds` path and
  zero-device case, unchanged); `scripts/demo-garage.sh` (it uses the API's `deviceIds`);
  `CLAUDE.md:33` and `docs/roadmap.md:98` "pair a sensor and pick the room" (the user picks the
  room, in the equipment form); D14 "Devices now take a zone" (the API's field); D28 (pairing).
- Searched, none found: `README.md`, `inputs/README.md`, `.claude/` (skills and agents name no
  screen of the interface), `crates/core/src` and `crates/host/src` (no comment speaks of the
  interface's room select or bindings).

## Acceptance criteria

- [ ] The sidebar shows the rooms, then at its foot "Administration" with Devices, Equipments,
      Topology, Discovery; open on those pages, closed on a room page until clicked; the same in
      the phone drawer.
- [ ] With `--sim scenarios/garage.json --serve --speed 10` on an empty data folder: Topology,
      "Add zone" "Garage"; Discovery, tab "Zigbee", "Add a device": the countdown runs, TH01, MS01
      and ZBMINIR2 appear marked "New", each with "—" and "Create equipment".
- [ ] "Create equipment" on the ZBMINIR2: type "Light on/off", name "ZBMINIR2 ...", room to
      choose; "Next" disabled until "Garage" is chosen; step 2 shows the relay ticked; "Create":
      the equipment has exactly the data binding `state` and the order binding `state`
      (`GET /api/v1/equipments`); the row reads "Garage › <name>", without the button.
- [ ] "Create equipment" on the TH01 guesses "Sensor"; ticking the MS01 too gives `temperature`,
      `humidity`, `battery`, `occupancy`, `battery_2`, and no `voltage`.
- [ ] "Create" is disabled with no device ticked (from Equipments' "Add equipment" too), with its
      text.
- [ ] The Devices page shows every device by name with the Equipment column; no room select
      anywhere; nothing sends `zoneId` to `/api/v1/devices` (the tests check the requests).
- [ ] The garage's page shows the light; its button switches the simulated relay on and off.
- [ ] `npm run check:bundle` under 170 KB gzip; Rust tests, the QEMU run and its memory gates
      unchanged and green.
- [ ] With the real Dongle Max (original firmware, radio over TCP, nothing flashed): the ZBMINIR2
      paired in Discovery, its light made from its row in the garage, switched on and off from
      the garage's page; no terminal after the start.

## Edge cases

- No room but the root: the form's room select offers `Maison`; an equipment may sit in the root
  (spec 018).
- No device at all: Discovery's empty text; Devices' empty state with its link; the equipment form
  step 2 says so, "Create" disabled with "Choose at least one device".
- A device that fits no type (a device with only `voltage`, or a fresh device whose reporting is
  not set up yet): "Create equipment" opens with `light_onoff`, the device ticked and listed;
  "Create" disabled with "These devices give nothing this type uses" until the type or the
  devices change.
- "Create equipment" pressed on a device just discovered, before its reporting setup ends: the
  device has no data yet; the type falls back to `light_onoff`; step 2 shows the device ticked,
  "Create" disabled with the "wait for its setup" text, and reads the devices every 5 s; once the
  setup is done (the host's `reporting:` line) the plan fills and "Create" is enabled (the type
  still to be checked by the user, since it was guessed without data).
- 64 equipments already (the API's `MAX_EQUIPMENTS`): `POST /equipments` answers 400 `Too many
  equipments (64)`; the form stays open with the text; no binding request is sent.
- "Create equipment" or "Create" pressed twice quickly: the row's button is disabled while its
  `GET` runs; "Create" is disabled while the requests run (spec 018's `saving`); one equipment
  is made.
- Between `equipment.created` and the last binding (or the rollback's `equipment.removed`), the
  equipment shows half made in Equipments, the room page and other browsers (offline, then
  filling up; or appearing then disappearing after a rollback). Accepted: it lasts the few
  requests of one creation, and it is what Sowel's interface does too.
- The device deleted between the row and the click: the `GET` answers 404, the text shows, the row
  goes.
- The device deleted, or its data gone, between step 2 and "Create": a binding `POST` answers 404;
  the equipment is deleted again; the form shows the text.
- A binding refused for another reason (409 alias taken: only if another browser added one in
  between; 503 `Busy`; the connection lost): the same rollback; if the delete fails too, the
  equipment stays, visible and deletable in Equipments.
- More than 16 data (or orders) in the plan (six TH01 in one sensor: 18): the warning; the first
  16 are bound, the rest left out (never sent, so never refused).
- An alias over 32 characters after `_N` (a key of 30 characters given by three devices): left out
  of the plan, as the API would refuse it (`Invalid alias`).
- A device bound by an order only: Sowel's column reads data bindings only, so it shows "—" (no
  Solite device is in that case: an order always comes with its state data).
- A device used by two equipments: two lines in its Equipment column; no button.
- An equipment deleted: its devices' column goes back to "—" and the button comes back, at the
  stores' next read (the `equipment.removed` event).
- A zone renamed: the column follows at the zone tree's next read.
- `#/discovery/unknown`, `#/discovery`: Zigbee's screen.
- Leaving Discovery while pairing is open: the window stays open (spec 017's rule: it ends on its
  time or on "Stop"); coming back shows the countdown read from `GET`, and an empty list.
- A device discovered while Discovery is not shown: no "New" mark; it is in Devices with "—".
- Two browsers: one makes an equipment; the other's Devices column follows within about a second
  (a demo check; the tests cover the stores' reaction).
- A phone's width: the table keeps name, Equipment and actions; the modal fits (spec 018). Checked
  by the demo's screenshots only (the tests run at one width).
- Discovery lists a device only when it is announced while the screen is shown: on the simulator
  the devices are announced when pairing opens, so "Add a device" must be pressed after the screen
  is opened; a device announced earlier is in Devices, not in Discovery's list.

## Sowel contract

- Types, unchanged: `Device` (its `zoneId` written by nothing), `Equipment`, `DataBinding`,
  `OrderBinding`, `EquipmentType` (`../sowel/src/shared/types.ts`). Routes, unchanged: `GET
  /api/v1/devices/:id`, `POST /api/v1/equipments`, `POST/DELETE
  /api/v1/equipments/:id/{data,order}-bindings` (`../sowel/src/api/routes/equipments.ts`,
  `../sowel/docs/technical/api-reference.md`); Solite's `permit-join` (D28).
- Interface, ported from `../sowel/ui/src/` at `5da84dc1`: `components/layout/{Sidebar (the
  Administration block), SidebarItem, admin-nav-items}`, `components/layout/sidebar-section.ts`
  (`getSectionForPath`, cut to the `admin` section: which section a route opens),
  `components/layout/AppLayout.tsx` (read for the drawer's Administration block: Solite's
  `App.tsx` and `Sidebar.tsx` keep their own layout, 791 lines of Sowel's are not ported), `pages/DevicesPage.tsx` and
  `components/devices/DeviceList.tsx` (cut: the table, `EquipmentCell`, the tabs' `TabButton`),
  `components/equipments/bindingUtils.ts` (`computeBindingPlan`, `resolveAlias`,
  `isRelevantData`, `isRelevantOrder`, `uniqueAlias`, `applyBindingPlan`, the tables for D3's
  types), `src/shared/binding-candidates.ts` (`computeBindingCandidates` for `light_onoff`,
  `switch`, `light_dimmable`, `shutter`; `isOnOffOrder`), `src/shared/thermostat-contract.ts`
  (`isThermostatDevice`, `THERMOSTAT_STATE_ALIAS`), `DeviceSelector`'s compatibility cascade; the
  keys of `i18n/locales/{en,fr}.json`.
- Divergences (decision D30): Discovery, Solite's own (Sowel does not pair), in the place of
  Sowel's Integrations entry; the form needs at least one device (Sowel allows none and completes
  the equipment on its page); the type kept the devices when it changes; the "Create equipment"
  shortcut (Sowel's Devices page has none); the device's name editor and delete in the list
  (Sowel has them on its device page, not ported); the first channel bound (no picker); a switch
  binds no metering (Solite reads none); the order of the entries of the section (Sowel's three,
  then Discovery).

## Decided in autonomy

- **The plan in the browser, one request per binding** (D30), rather than changing what `POST
  /equipments` with `deviceIds` binds in the core: it is how Sowel's interface binds, and Sowel's
  server route binds every key, so Solite's route keeps Sowel's behaviour for the API's clients
  (`scripts/demo-garage.sh`, the tests); the compatibility of step 2 and the plan come from one
  module, as in Sowel (`binding-candidates.ts` serves both), so a device listed as compatible
  always gets a binding; the realistic home of decision D20 (50 equipments made with `deviceIds`,
  6 bindings each), its firmware fixtures and the memory figures measured on it stay valid. The
  cost: no atomicity, made up by deleting the equipment again when a binding is refused, and up
  to 33 requests for one equipment (one connection each, sequential, within the API's limit of 4
  at once). The core gets its own plan when the box makes equipments itself (inbox).
- **At least one device in the form** (the caller's choice, recorded here): Sowel allows none and
  completes it from its equipment page, which Solite does not port; an equipment with no device
  could only be deleted. The API still allows none (Sowel's format).
- **"Create" also waits for a non-empty plan**: at least one device is not enough when none of
  them gives what the type uses; the plan says so before any request is sent.
- **Administration's order**: Sowel's Devices, Equipments, Topology, then Discovery in the place of
  Sowel's Integrations (it is the per-integration screen), rather than Discovery first: the port
  keeps Sowel's navigation; the path goes through links (Devices' "Add devices", Discovery's "See
  all the devices", the empty states).
- **Discovery's tabs shown with one integration**: Sowel hides tabs below two; here the tab says
  which integration the screen is about, as the caller asked the screen to be per integration.
- **Integrations listed in the interface**: Solite serves no `GET /api/v1/integrations`; one entry
  in a list keeps the screen structured per integration without a core change (inbox).
- **Discovery lists only the devices discovered while it is open**: it is the pairing screen; the
  full list is Devices. The rows are the same component, so "Create equipment" is offered in both.
- **"Create equipment" on every row with "—"**, not only on the "New" ones: a device paired
  earlier, or whose equipment was deleted, needs the same way in.
- **The type guessed in a fixed order** (dimmable, light, shutter, thermostat, button, sensor):
  the most specific first; a relay is a light (`light_onoff`, Sowel's default type) rather than a
  switch; the user can change it.
- **The device read at the click** (`GET /api/v1/devices/:id`), not kept in the store: a device's
  data arrive after `device.discovered` (its reporting is set up afterwards), so the click reads
  what it has then; the devices store keeps no data, as since spec 017.
- **The type change keeps the devices ticked** (Sowel clears them): with the shortcut the device
  is the starting point, and losing it on a type change would undo it; a ticked device stays
  listed, and the plan rule above blocks "Create" if it gives nothing.
- **The name proposed from the device**: Sowel starts empty; the device's name is a better start
  than nothing, and is edited in place.
- **No equipment link in the Equipment column**: Solite has no equipment page; the room part links
  to the room's page, where the equipment is.
- **The Equipment column from data bindings only**: Sowel's rule; Solite's on/off devices always
  have their state data bound with their order.
- **Device rename and delete stay in the list** (spec 017): Sowel has them on its device page,
  which is not ported.
- **`Device.zoneId`, its `PUT` and `clear_zone` kept in the core**: Sowel's format (D3); they
  cost nothing and change nothing; the living spec says they are read by nothing.
- **No core, host or firmware change**: every request exists (specs 008, 010, 017). The Rust side
  gets tests that replay the interface's new requests (plan, test plan), so an API change that
  breaks them fails on the Mac.
- **Ported from Sowel's `5da84dc1`**, the commit of every earlier port, though `../sowel` is now at
  `6708701f`: one commit for the whole interface.
- **After the spec review** (spec-reviewer, three blocking findings, seven minor):
  - Step 2 reads the devices again when it opens and every 5 s while the plan is empty, since a
    device's data come after its `reporting:` setup, not with `device.discovered`; the text says
    to wait for the setup (the user's question Q1, answered so).
  - D30 states why D29's two arguments (size, atomicity) no longer hold; 60 KB gzip is this spec's
    gate; the ESP32 risk of 33 sequential requests is written, a Rust test sends them in sequence,
    and an inbox line measures them on the box.
  - J2's "Done when" ("the light from the ZBMINIR2's row") accepted (Q2).
  - The tests to rewrite listed line by line; the stale comments of `api/zones.ts`,
    `api/devices.ts`, `store/useDevices.ts`, `ZoneTree.tsx` added; the half-made equipment between
    `equipment.created` and the last binding accepted (as Sowel); the roadmap's `AddBindingModal`
    line kept, spec 018's detail-page line replaced.
- **About 55 KB gzip expected**: the plan and candidates (about 250 lines), the table, the section
  and the Discovery screen, less the room select.

## How the user will check it

1. Simulator: `cargo run -p solite-host -- --sim scenarios/garage.json --serve --speed 10 --data
   <empty folder>`; open `http://127.0.0.1:8080/`; Administration › Topology: "Add zone" "Garage";
   Administration › Discovery: "Add a device", the three devices appear marked "New"; "Create
   equipment" on the ZBMINIR2: light on/off "Lumière garage" in Garage; "Create equipment" on the
   TH01, tick the MS01: sensor "Capteurs garage" in Garage; Devices: the three rows read "Garage ›
   ..."; the garage's page: switch the light; `curl -s http://127.0.0.1:8080/api/v1/equipments`
   shows the aliases of the acceptance criteria. Reload: everything kept. Screenshots of the
   section, Discovery, Devices and the form, Hybrid and Dark themes.
2. Real hardware: the plan's demo script (`plan.md`, "Real hardware"), with the ZBMINIR2. The
   result of the run (date, what was seen) is written here.
   Run on 2026-11-22 on the real Dongle Max (original firmware, USB relayed to TCP by `socat`):
   the ZBMINIR2 paired from Discovery, the equipment "Lumière garage" was made in the Garage from
   its row, and the light switched on and off from the room's page. The author reported that
   everything worked. The engine ran on the Mac; the Dongle Max only relayed the radio.
