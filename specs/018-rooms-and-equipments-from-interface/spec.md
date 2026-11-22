# Spec 018: rooms and equipments from the interface

> **Superseded in part by spec 019** (the text below stays as the record of what was built): R1's
> "three entries in this order ... under the rooms" (now the Administration section); R2's "The
> devices of a deleted zone go back to 'No room'"; R4's "Changing the type clears the devices
> chosen", "Create, the request" (one `POST` with `deviceIds`, decision D29), "Binding cap" and "A
> key given by two devices ... is bound once"; the delta's "whose data and orders are bound under
> their key", "its devices go back to no room" and D29's binding sentence; the edge cases "creating
> with no device is allowed (status `offline`)", "Two devices with the same data key" and "the
> equipment is created without its bindings"; Decided in autonomy "One `POST` with `deviceIds`
> (D29)... Aliases are the data keys". The form now needs a device and binds from a plan (D30).

## Context

Increment J2 of the roadmap (milestone J, the autonomous box): rooms (Sowel's zones) and
equipments from the interface, Sowel's screens ported. Done when the garage and its equipments
are made with the mouse.

The API has existed since spec 010: zones (`GET/POST /api/v1/zones`, `GET/PUT/DELETE
/api/v1/zones/:id`) and equipments (`GET/POST /api/v1/equipments`, `GET/PUT/DELETE
/api/v1/equipments/:id`, the bindings), with Sowel's rules, texts and live events (spec 013:
`zone.created/updated/removed`, `equipment.created/updated/removed`). Nothing is missing on the
core's side for this increment: every request the screens send exists, is tested, and emits the
event the interface already follows (`useLive.ts`). What is missing is the interface: today it
only reads zones and equipments (spec 014), and spec 017's devices page creates a room under the
home's root and places a device in it, nothing more. Making the garage's light and sensors still
needs `curl` (`scripts/demo-garage.sh`).

This spec ports Sowel's zone and equipment screens (`ZonesPage`, `ZoneTree`, `ZoneForm`,
`EquipmentsPage`, `EquipmentForm`, `DeviceSelector`), cut to what Solite has, so the user makes
the rooms (a tree), then the equipments of a room from its devices, renames, moves and deletes
them, with no terminal. Specified in autonomy (no questions); choices under "Decided in autonomy".

## Goals

1. A **Topology** page (Sowel's "Topologie"): the zone tree; create a zone under any zone, edit
   its name, parent and description, delete it, with Sowel's refusals shown.
2. An **Equipments** page: the equipments grouped by room; create one (type, name, room, then its
   devices, in Sowel's two steps), edit its name and room, delete it.
3. On a room's page, an "Add equipment" button opens the same form with that room chosen.
4. Everything follows Sowel's live events, so a second browser sees the changes.

## Out of scope

- Bindings edited one by one (Sowel's `AddBindingModal`, `MissingBindingsModal`), the equipment
  detail page (`EquipmentDetailPage`, 1288 lines), `enabled` toggled from the interface, icons
  chosen, zone reorder (Sowel's up and down arrows; Solite has no reorder route, spec 010), the
  zone detail page (`ZoneDetailPage`: its edit and delete move into the tree's rows, R2).
- Equipment types beyond decision D3's seven type strings (`light_onoff`, `light_dimmable`,
  `shutter`, `sensor`, `switch`, `thermostat`, `button`); Sowel's channel candidates, contracts
  (thermostat, charger, car), water-valve and pool rules in `DeviceSelector`.
- Sowel's search field on the equipments page.
- A change of type on edit (Sowel allows it only among shutter, awning and door; Solite has only
  `shutter` of the three).
- Recipes from the interface: J3. Authentication of the routes: J4 (D13).
- Any change to the core, the host or the firmware.

## Requirements

### R1: Navigation

- The sidebar (and the phone drawer) shows, under the rooms, three entries in this order:
  "Devices" (spec 017, `#/devices`), "Equipments" ("Équipements", Sowel's `nav.equipments`,
  `#/equipments`, Lucide `Box`), "Topology" ("Topologie", Sowel's `nav.zones`, `#/zones`, Lucide
  `Map`). The entry of the page shown is marked (`aria-current="page"`, Sowel's `bg-primary-light`
  and `text-primary`), as "Devices" is; no room is then marked.
- `#/zone/<id>` is unchanged (the room page); `#/zones` and `#/equipments` are not taken for a
  room id (`zoneIdFromHash` only reads the `#/zone/` prefix).
- `app.noRoom` becomes "No room yet: create one in the Topology page" / "Aucune pièce : créez-en
  une dans la page Topologie".

### R2: The Topology page (`#/zones`)

Ported from Sowel's `pages/ZonesPage.tsx`, `components/zones/ZoneTree.tsx` and
`components/zones/ZoneForm.tsx` (commit `5da84dc1`, UI 1.87.0, the commit of every other port).

- **Header**: Sowel's title ("Topologie" / "Home Topology"), the count (`zones.count`, the root
  included, as Sowel counts), the button "Add zone" ("Ajouter une zone").
- **Tree**: every zone, the root `Maison` first, children indented, in the API's order
  (`displayOrder` then name). Each row: the name, the description when there is one, the number of
  child zones (`zones.childCount`), and three buttons: "Add sub-zone" (Sowel's), "Edit" and
  "Delete" (from Sowel's `ZoneDetailPage`, which is not ported). The root has no "Delete". A click
  on the name opens the room's page (`#/zone/<id>`), where Sowel opens its zone detail page.
- **Form** (Sowel's `ZoneForm`, a modal): name (1 to 64 characters, `maxLength` 64, the API's
  bound; Sowel 100), parent zone, description (optional, `maxLength` 500, the API's bound, as
  Sowel's field). The parent select lists
  every zone with Sowel's path label (`flattenZonesWithPath`), the root first; Sowel's "None (root
  zone)" option is cut. Created from "Add zone": parent the root; from "Add sub-zone": parent that
  row's zone. The button stays disabled while the name is blank.
  - Create: `POST /api/v1/zones {"name", "parentId", "description"?}` (201); the form closes.
  - Edit: the form filled with the zone; the parent select leaves out the zone and its subtree
    (Sowel's `flattenZoneTree(tree, excludeId)`); `PUT /api/v1/zones/:id {"name", "parentId",
    "description"}` (200). For the root, the parent select is not shown and `parentId` is not
    sent (the root has no parent: the API refuses `parentId: null` on another zone and has
    nothing to accept for the root).
  - A refusal keeps the form open with the API's text under it (Sowel's behaviour): `Invalid name`,
    `Invalid description` (over 500 characters: the field prevents it, a pasted or scripted value
    still meets the API), `Too many zones (32)`, `Parent zone not found: <id>`, `Moving this zone
    would create a circular reference`, `Zone not found` (deleted meanwhile by another browser).
    `A zone cannot be its own parent` and `Only the root zone has no parent` are never sent by the
    form (it leaves out the zone itself and hides the parent for the root); reachable only by a
    race, they are shown as any other API text.
- **Delete**: Sowel's confirmation (`zones.deleteConfirm`, "Delete zone "Garage"? This cannot be
  undone."), then `DELETE /api/v1/zones/:id`. A refusal shows its text under the row for 5 s, as
  spec 017's rows: Sowel's `Cannot delete zone with N child zone(s). Remove or move them first.` (singular or plural, `zones/mod.rs:312`)
  and `Cannot delete zone with N equipment(s). Remove or move them first.`; on 404 the tree is
  read again. The devices of a deleted zone go back to "No room" (the API's rule, spec 010).
- **Empty and error states**: Sowel's (`zones.empty.*` cannot show in practice, the root always
  exists; `zones.error.title` with "Retry" when the tree cannot be read).

### R3: The Equipments page (`#/equipments`)

Ported from Sowel's `pages/EquipmentsPage.tsx`, cut (no search, no singleton types).

- **Header**: Sowel's title ("Équipements" / "Equipments"), the count, the button "Add equipment"
  ("Ajouter un équipement").
- **List**: grouped by room with Sowel's path label (`groupEquipmentsByZone`, `zone-path.ts`),
  rooms in the tree's order, equipments by name within a room (the API's order). Each row: the
  name (a click opens its room's page), the type's label (Sowel's `equipments.type.*`), the status
  badge (Sowel's `EquipmentStatusBadge`, already ported), the number of devices bound, and the
  buttons "Edit" and "Delete". A disabled equipment is shown dimmed, as on the room page.
- **Empty**: Sowel's `equipments.empty.title` and `.message` with its "Create equipment" button.
- **Delete**: Sowel's confirmation (`equipments.deleteConfirm`), then `DELETE
  /api/v1/equipments/:id`; a refusal shows its text for 5 s; on 404 the list is read again. A
  recipe that used it behaves as when it is deleted with the API (unchanged, read in
  `recipes/motion_light/machine.rs` and `params.rs`): a running `motion-light` instance keeps
  running; the deleted light counts as off and each order to it is logged as an error
  (`<id>: no toggle order binding`), its other lights are served; at the next start the instance
  no longer validates (`Light equipment not found: <id>`), logs `Failed to restore: <reason>` and
  stays stopped. The confirmation does not warn about recipes (J3 shows them).

### R4: The equipment form

Ported from Sowel's `components/equipments/EquipmentForm.tsx` and `DeviceSelector.tsx`, cut.

- **Create, step 1 ("info")**: type (the seven types of D3, sorted by their label; default
  `light_onoff`, as Sowel), name (1 to 64, `maxLength` 64), room (every zone with its path label,
  Sowel's "Select a zone…" first, required). "Next: Select devices" is disabled while the name is
  blank or no room is chosen. Changing the type clears the devices chosen.
- **Create, step 2 ("devices")**: Sowel's instruction ("Select the devices to bind."), the
  devices read with `GET /api/v1/devices` (with their data), each with a check box, its name,
  manufacturer and model. Only the devices with a data of the type's categories are listed
  (Sowel's `EQUIPMENT_TYPE_CATEGORIES`): `light_onoff` and `switch` `light_state`;
  `light_dimmable` `light_state`, `light_brightness`; `shutter` `shutter_position`; `sensor`
  temperature, humidity, pressure, luminosity, co2, voc, noise, motion, contact_door,
  contact_window, water_leak, smoke; `button` `action`; `thermostat` `setpoint`. Sowel's "All
  devices" toggle shows every device; Sowel's name filter narrows the list; Sowel's expander shows
  a device's data (key, category, value). None compatible: Sowel's `deviceSelector.noCompatible`
  text, the toggle still offered. "Back" returns to step 1 keeping the fields.
- **Selection cap**: at most 64 devices chosen (the API's bound on `deviceIds`, `MAX_DEVICES`;
  past it, 400 `Invalid field: deviceIds`): once 64 are chosen, the other check boxes are
  disabled and the selector says "At most 64 devices" ("64 appareils au plus", Solite's key
  `deviceSelector.max`). Unreachable today with 64 devices at most in the home, kept so the form
  never sends what the API refuses.
- **Binding cap**: an equipment holds at most 16 data bindings and 16 order bindings
  (`MAX_BINDINGS`, spec 010 R7.5). When the chosen devices carry more than 16 distinct data keys
  (or order keys), the core creates the equipment and silently skips the bindings past 16; the
  form says so beforehand with a warning under the list, which does not block "Create": "These
  devices give N values; only 16 are bound" ("Ces appareils donnent N valeurs ; seules 16 sont
  liées", Solite's key `equipments.form.tooManyBindings`). A key given by two devices (`battery`
  of the TH01 and the MS01) is bound once, the second skipped silently, as Sowel's core.
- **Create, the request**: one `POST /api/v1/equipments {"name", "type", "zoneId", "deviceIds"}`
  (201): the core binds every data and order of the chosen devices under their key as alias
  (spec 010, Sowel's `createWithAutoBindings`), so the ZBMINIR2's light gets `state`, the TH01's
  sensor `temperature`, `humidity`, `battery`. No device chosen: the equipment is created without
  binding (status `offline`, as Sowel). Decision D29: one request rather than Sowel's interface,
  which creates the equipment then its bindings one by one.
- **Edit**: one step: the type shown and locked, name and room editable; `PUT
  /api/v1/equipments/:id {"name", "zoneId"}` (200). The bindings are unchanged.
- **Refusals**: the form stays open with the API's text (Sowel's behaviour): `Invalid name`,
  `Too many equipments (64)`, `Zone not found: <id>`, `Equipment not found`. A device deleted
  between the two steps is skipped by the core (spec 010 R7.4): the equipment is created without
  its bindings.
- **Closing**: the cross, "Cancel", and a click outside close it without sending anything.

### R5: "Add equipment" on a room's page

The room page's header gets Sowel's "Add equipment" button (`equipments.addEquipment`, Lucide
`Plus`); it opens R4's form with that room chosen (Sowel's `defaultZoneId`). The room's empty
text "No equipment in this room" is unchanged. The root's page offers the button too (an
equipment may sit in the root, as in the API).

### R6: Live and consistency

- The stores follow the events they already follow: `zone.created/updated/removed` read the tree
  again (`useZones`), `equipment.created/updated/removed` read the equipments again, at most once
  per second (`useEquipments`' coalesced read). After its own request succeeds, a page also reads
  again at once, so it does not wait for the event (the event may be lost when the socket is
  down).
- Two browsers: a zone or an equipment made in one shows in the other within about a second; the
  sidebar's rooms follow the tree. This is a check of the demo only: the tests cover the store's
  reaction to each event (`useLive.test.ts`), not two real browsers.
- The room shown at `#/zone/<id>` that is deleted: the page falls back to the first room with
  equipments, as for an unknown id (spec 014's `chooseZone`, unchanged).

### R7: Language, look and budget

- French and English, Sowel's keys and texts where Sowel has them (`zones.*`, `equipments.*`,
  `deviceSelector.*`, `nav.*`, `common.*` as copied from Sowel's `en.json` and `fr.json` at
  `5da84dc1`); Sowel's tokens, classes and Lucide icons by name.
- Sowel's modal (`fixed inset-0 bg-black/40`, `bg-surface rounded-lg`), closed by Escape too.
- The first load stays under the 170 KB gzip budget: about 50 KB expected (40.5 KB after spec
  017), confirmed by `npm run check:bundle`.

## Living spec delta

### docs/living-spec/README.md

- **Organises the home in zones and equipments**: New, at the end of the section's first
  paragraph: "Zones and equipments are also made from the interface (see 'Shows the home in the
  browser')."
- **Shows the home in the browser**:
  - **Sidebar**: Changed: "the rooms as a tree" → "the rooms as a tree, then the entries Devices,
    Equipments and Topology".
  - **The room**: New: "an 'Add equipment' button opens the equipment form with the room chosen".
    Changed: "No room yet: create one from the devices page" → "No room yet: create one in the
    Topology page".
  - New bullet **Topology** (`#/zones`): the tree from `Maison`; add a zone (under the root) or a
    sub-zone; edit its name, parent (never under itself or a descendant) and description; delete
    it (confirmed; refused with Sowel's text when it has sub-zones or equipments, shown 5 s; its
    devices go back to no room). The root is renamed but never deleted or moved.
  - New bullet **Equipments** (`#/equipments`): grouped by room, each with its type, status and
    number of devices; "Add equipment" in two steps: type (light on/off, dimmable light, shutter,
    sensor, switch, thermostat, button), name and room, then the compatible devices (all on
    demand), whose data and orders are bound under their key; edit the name and room; delete
    (confirmed). A refusal keeps the form open with its text. At most 64 devices chosen; a warning
    when they give more than 16 values, of which only 16 are bound.
  - Opening paragraph of the section: New: "The interface makes the rooms (Topology) and the
    equipments (Equipments, and a room's 'Add equipment'), as well as showing them."
- **Runs the motion-light recipe**: New, under "What survives a restart": "A light equipment
  deleted (from the API or the interface) while an instance uses it: the running instance counts
  it as off and logs an error for each order to it, the other lights still served; at the next
  start the instance no longer validates (`Light equipment not found: <id>`) and stays stopped."
- **Users**: unchanged ("They pair Zigbee devices, choose the room..." stays true).
- **Chooses its network**: not concerned (the link choice is untouched).
- **History**: New row `| 018 | Rooms and equipments made from the interface: the Topology and
  Equipments pages, Sowel's forms |`.

### docs/decisions.md

- New decision D29 "Equipments from the interface": Sowel's screens ported with D3's types and
  without bindings edited one by one; the form creates the equipment and its bindings in one
  `POST` with `deviceIds` (alias = key, the core's auto-binding) rather than Sowel's interface,
  which computes a binding plan in the browser (`autoCreateBindings`, `bindingUtils.ts`) and sends
  one request per binding; the zone form has no "None" parent (Solite's tree has one root, spec
  010 R7.10).

### docs/roadmap.md

- Row J2: unchanged by the spec; its status is set by the roadmap review after the demo.
- Inbox, four new lines (date 2026-11-22, "Came from": Spec 018), each left out of this increment:
  - The equipment detail page (Sowel's `EquipmentDetailPage`): its bindings, values, history.
  - Bindings added and removed one by one from the interface (Sowel's `AddBindingModal`,
    `MissingBindingsModal`); the routes exist (spec 010).
  - Zone reorder (Sowel's up and down arrows): needs Sowel's `PUT /api/v1/zones/reorder` in the
    core first.
  - An equipment enabled or disabled from the interface (`PUT {"enabled"}` exists).

### No longer true

- `docs/living-spec/README.md` "Shows the home in the browser": "No room yet: create one from the
  devices page" → "No room yet: create one in the Topology page"; "the rooms as a tree" → with the
  three entries; the section's opening, which speaks only of showing the home → it also makes the
  rooms and equipments (the Topology and Equipments pages).
- `docs/living-spec/README.md` "Runs the motion-light recipe": silent on a light deleted under a
  running instance → the sentence of the delta.
- `docs/living-spec/README.md` "Chooses its network", "Users", "Starts and shows its name": read,
  still true.
- `ui/src/i18n/messages.ts:81` (en `app.noRoom`) → "No room yet: create one in the Topology page";
  `:188` (fr) → "Aucune pièce : créez-en une dans la page Topologie".
- `ui/src/App.test.tsx:90`, `:96` and `ui/src/i18n/i18n.test.ts:39`: the expected texts, as above.
- `ui/src/api/zones.ts:1` header "Cuts: everything but `getZones`, `getZoneAggregation`,
  `createZone` and `deleteZone`" → `updateZone` added to the list, and `createZone` takes Sowel's
  full body (`parentId`, `description`).
- `ui/src/api/equipments.ts:1` header "Cuts: everything but `getEquipments` and
  `executeEquipmentOrder`" → `createEquipment`, `updateEquipment`, `deleteEquipment` added.
- `ui/src/store/useZones.ts:1-2` header "Cuts: the actions that edit zones (createZone,
  updateZone, deleteZone): the interface only reads them" → the actions restored, no cut but
  `reorderZones`.
- `ui/src/store/useEquipments.ts:1-3` header "Cuts: the actions that edit equipments and their
  bindings" → "Cuts: the actions that edit bindings, ...".
- `docs/design-system.md`, the port table: the rows `store/useZones.ts, store/useEquipments.ts`
  ("the editing actions ... cut") and `api/...` ("only the four calls used") rewritten; new rows
  for the files ported by this spec (R2, R3, R4, `lib/zone-path.ts`,
  `components/equipments/equipment-type-meta.tsx`); "first load 34 KB gzip in spec 014" → the
  size measured after this spec.
- `docs/architecture.md:43`, block "Interface": "the devices page (...) starts pairing and places
  the devices in rooms" → add "the Topology and Equipments pages (`components/zones/`,
  `components/equipments/`) make the rooms and the equipments".
- `scripts/demo-garage.sh`: its comment says the script makes the garage through the API; still
  true (the script stays, for the tests and a quick demo); a line added: "the same garage is made
  from the interface's Topology and Equipments pages".
- Searched, none found elsewhere: "only reads", "with `curl`" (in `docs/roadmap.md`, J's "Why
  next" speaks of the box as a whole: J3 to J5 remain; the J2 row changes only at the review),
  "create one", "Topologie", "zone detail", "reorder", "binding" (roadmap: no item names the
  out-of-scope pieces, hence the inbox lines above).

## Acceptance criteria

- [ ] With `--sim scenarios/garage.json --serve --speed 10`, after adding the three devices from
      the devices page: in Topology, "Add zone" "Rez-de-chaussée", then "Add sub-zone" "Garage"
      under it; both show in the tree and the sidebar.
- [ ] On the Garage's page, "Add equipment": `light_onoff` "Lumière garage" with the ZBMINIR2;
      `sensor` "Capteurs garage" with the TH01 and MS01. They appear in the room with the light's
      switch and the sensor's values; the room's pills show its temperature and motion.
- [ ] The light's switch turns the simulated relay on and off (spec 014, unchanged).
- [ ] In Equipments, "Lumière garage" is renamed and moved to the root, then back; deleted after
      confirmation.
- [ ] In Topology, deleting "Rez-de-chaussée" is refused with Sowel's text (it has a sub-zone);
      deleting "Garage" with equipments is refused; editing "Garage" offers neither itself nor a
      descendant as parent.
- [ ] A second browser sees each change within about a second, without a reload (a demo check;
      the tests cover each event's effect on the stores).
- [ ] A reload keeps everything (the files of spec 010).
- [ ] `npm run check:bundle` under 170 KB gzip; Rust tests, the QEMU run and its memory gates
      unchanged and green.
- [ ] With the real Dongle Max: the garage, its light (the ZBMINIR2) and its sensors made with
      the mouse only (roadmap J2's "done when").

## Edge cases

- Name blank or spaces only: the button disabled, nothing sent; 65 characters: the input stops at
  64.
- The 33rd zone, the 65th equipment: the API's text in the form, nothing created.
- A zone description over 500 characters: the field stops at 500; the API's `Invalid description`
  shown if it is met anyway.
- `A zone cannot be its own parent`, `Only the root zone has no parent`: never sent by the form;
  shown as any API text if a race produces them.
- Chosen devices giving more than 16 distinct data or order keys: the warning shown, the
  equipment created with 16 bindings of each kind, the rest skipped.
- 64 devices chosen: the other check boxes disabled, "At most 64 devices" shown.
- A light deleted while a `motion-light` instance uses it: the instance runs on, logs an error per
  order to it, and stays stopped at the next start (R3).
- Delete the root: not offered. A zone with sub-zones or equipments: Sowel's text 5 s. A zone
  already deleted by another browser: 404, the tree read again.
- Edit a zone to a parent that became its descendant meanwhile (another browser): the API's 400
  text in the form.
- Edit or delete an equipment deleted meanwhile: 404 text, the list read again.
- No device at all, or none compatible with the type: the selector's text; "All devices" shows
  every device; creating with no device is allowed (status offline).
- A device chosen that is deleted before "Create": the equipment is created without its bindings.
- Two devices with the same data key (TH01 and MS01 both give `battery`): the second is skipped
  (spec 010's rule), the equipment still created.
- `GET /api/v1/devices` fails in step 2: the error text, an empty list, "Back" and "Create" still
  usable.
- The socket down while editing: the page reads again after its own request; on reconnection the
  stores read everything again (spec 014).
- The room in the address deleted: the first room with equipments is shown.
- Only the root exists: the equipment form's room select offers `Maison`; the Topology page shows
  the root alone with "Add sub-zone" and "Edit".
- Escape, the cross or a click outside a modal: closed, nothing sent; a request in flight
  finishes and its result is shown by the page's read again.
- A phone's width: the pages and the modals fit (Sowel's `mx-4`, `max-h-[90vh]` with the list
  scrolling).

## Sowel contract

- Routes and JSON, unchanged (spec 010): `Zone`, `ZoneWithChildren`, `Equipment`,
  `EquipmentWithDetails`, `EquipmentType` (`../sowel/src/shared/types.ts`); `POST
  /api/v1/equipments` with `deviceIds` (`../sowel/src/api/routes/equipments.ts`,
  `equipment-manager.ts` `createWithAutoBindings`).
- Events, unchanged (spec 013): `zone.created`, `zone.updated`, `zone.removed`,
  `equipment.created`, `equipment.updated`, `equipment.removed` (`EngineEvent`, `types.ts`).
- Interface, ported from `../sowel/ui/src/` at `5da84dc1`: `pages/ZonesPage.tsx`,
  `components/zones/{ZoneTree,ZoneForm}.tsx`, `pages/EquipmentsPage.tsx`,
  `components/equipments/{EquipmentForm,DeviceSelector,equipment-type-meta}.tsx`,
  `lib/zone-path.ts`, `api/{zones,equipments}.ts` and `store/{useZones,useEquipments}.ts` (their
  editing functions), the keys of `i18n/locales/{en,fr}.json`.
- Divergences (decision D29): one `POST` with `deviceIds` instead of the binding plan in the
  browser; no "None" parent; names at most 64 (D14); D3's seven types; zone edit and delete in the
  tree's rows instead of the zone detail page; no reorder, no search, no type change on edit.

## Decided in autonomy

- **No core change**: the routes, bounds, texts and events of specs 010 and 013 cover every
  request; the increment is the interface. The Rust side gets one test file that replays the
  interface's sequence of requests (plan, test plan), so a change of the API that breaks the
  screens fails on the Mac.
- **Separate pages (Topology, Equipments)** as Sowel's, rather than editing inside the room page:
  the ports stay close to Sowel, and the room page stays the everyday view. The "Add equipment"
  button on the room page (R5) is the one shortcut, because "the garage and its equipments" is
  made from the garage.
- **Zone edit and delete in the tree's rows**: Sowel's `ZoneDetailPage` (312 lines, child zones,
  recipes of the zone) is not ported; its two actions are what the increment needs.
- **One `POST` with `deviceIds`** (D29): atomic (no half-made equipment when a request fails), and
  it is the core's tested auto-binding (spec 010); Sowel's browser-side plan (`bindingUtils.ts`,
  844 lines and more) is out of budget. Aliases are the data keys, as the recipe and the zone
  aggregation already expect (spec 010's demo, `demo-garage.sh`).
- **Devices already bound elsewhere stay listed** in step 2 (Sowel's current page does not exclude
  them either; it hides channels, which Solite does not have).
- **No "None" parent**: in Sowel it makes a second top-level zone; in Solite `parentId: null`
  means the root on `POST` and is refused on `PUT` (spec 010 R7.10). The root is listed first and
  chosen by default.
- **The root renamed but not moved or deleted**: the API allows the name, refuses the rest.
- **Thermostat by the `setpoint` category**: Sowel uses a thermostat contract (spec 177 in Sowel)
  that Solite does not port; no Solite device has a setpoint today, so the list is empty and "All
  devices" remains.
- **Ports from Sowel's commit `5da84dc1`** (read with `git show`), the commit of every earlier
  port, though `../sowel` is now further on: one commit for the whole interface.
- **Sidebar labels from Sowel's `nav.*` keys** ("Équipements", "Topologie"), so a Sowel user finds
  the same words.
- **About 50 KB gzip expected**: three pages and two modals, Sowel's keys for two languages; well
  under 170 KB.
- After the spec review (spec-reviewer, Sonnet, no blocker, five gaps):
  - `Invalid description` listed, the field capped at 500 (the API's bound, Sowel's field), with
    a Rust and a form test.
  - The 16-binding cap stated: the core skips silently (spec 010 R7.5, unchanged); the form warns
    beforehand without blocking, counting distinct keys, since only the user can choose fewer
    devices. Duplicated keys stay silent, as Sowel's core.
  - The selection capped at 64 in the form (the API's `deviceIds` bound), so the form never sends
    what the API refuses, even though the home holds 64 devices at most.
  - `A zone cannot be its own parent` and `Only the root zone has no parent` listed as
    race-only refusals, shown like any other.
  - The living spec's interface section, the recipe section (a deleted light, read in
    `machine.rs` and `params.rs`) and the "Chooses its network" check added to the delta; the
    four out-of-scope pieces placed as roadmap inbox lines; the roadmap added to the search.
  - The two-browser check is a demo check only; the label stays "Topologie" (Sowel's `nav.zones`).
  - The confirmation of an equipment delete does not warn about recipes: recipes are shown in
    the interface only from J3.

## How the user will check it

1. Simulator: `cargo run -p solite-host -- --sim scenarios/garage.json --serve --speed 10 --data
   <empty folder>`; open `http://127.0.0.1:8080/#/devices`, add the three devices (spec 017);
   open Topology: "Add zone" "Rez-de-chaussée", "Add sub-zone" "Garage"; open the Garage, "Add
   equipment": a light on/off "Lumière garage" with the ZBMINIR2, a sensor "Capteurs garage" with
   the TH01 and the MS01; the room shows the switch, the values, the pills; switch the light;
   in Equipments rename and move one, delete one; try deleting "Rez-de-chaussée" (refused);
   reload: everything kept. A second browser open meanwhile follows.
2. Real hardware: `cargo run -p solite-host -- --radio tcp://<address>:6638 --serve`; the same
   steps with the real ZBMINIR2 (and the sensors the user has), no terminal after the start. The
   result of the run (date, what was seen) is written here.
