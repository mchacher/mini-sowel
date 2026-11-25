# Spec 030: equipment-detail

## Context

Roadmap J2 (milestone J, the autonomous box), third item of the "Order of work while the box is
away" (2026-11-25): "J2: equipments from the interface (detail page, bindings one by one, enable
and disable)". Specs 018 and 019 made the Equipments, Topology and Devices pages and the creation
form; an equipment can then only be renamed, moved or deleted from the interface. Its bindings,
once written by the form, cannot be completed, trimmed or moved to another device without `curl`,
and a disabled equipment (made through the API) cannot be enabled again.

Four inbox lines of 2026-11-22 belong to J2 and are taken here:

- the equipment detail page (Sowel's `EquipmentDetailPage`: bindings, values; `ChangeDeviceModal`
  to replace its devices); "until then the form needs at least one device" (spec 019);
- bindings added and removed one by one (Sowel's `AddBindingModal`, `MissingBindingsModal`; the
  routes exist since spec 010);
- an equipment enabled or disabled from the interface (`PUT {"enabled"}` exists);
- the device detail page (Sowel's `DeviceDetailPage`: data, orders); the same line's search, sort
  and battery, LQI, last-seen columns of the Devices page stay in the inbox.

The zone reorder line (Sowel's arrows) needs a core route first and stays in the inbox.

Authentication exists (spec 029): the new pages read through the existing API client, which
carries the token; they add no route to the core.

This run is autonomous (the user is away): open questions are decided here and recorded
(decision D50, and "Decisions taken in this spec" below).

## Goals

1. An equipment opens on its own page: its values, its devices, its bindings, its state.
2. Its bindings are added and removed one by one, the ones its devices give and it lacks are
   offered, and its devices are replaced, all from that page.
3. An equipment is enabled or disabled from that page.
4. A device opens on its own page: its data with their values, its orders, its name, delete.
5. The creation form no longer needs a device, as Sowel's.

## Out of scope

- Sowel's history section, its type panels (shutter, thermostat, gate, energy, timed command,
  family, button actions...), the type change on edit, the "order outcome" line: Solite has none
  of them (they stay out, as in specs 018 and 019).
- The device page's raw expose (Solite has no route for it) and the Devices page's search, sort,
  battery, LQI and last-seen columns (inbox line narrowed).
- The zone reorder (inbox, needs `PUT /api/v1/zones/reorder` in the core).
- A channel choice for a multi-channel device in the change-devices dialog (inbox line kept: the
  first candidate, as the creation form).
- Any change to the core or the API.

## Requirements

### R1: Addresses

- `#/equipments/<id>` shows the equipment's page, `#/devices/<id>` the device's page; the
  sidebar's Administration section stays open with Equipments or Devices marked as on their list.
- A malformed escape in the id, or an empty id, shows the list (`#/equipments`, `#/devices`).

### R2: Ways in

- Equipments page: the equipment's name is a link to its page.
- A room's page: the equipment's name (light, sensor or other row) is a link to its page; the
  light's button and the values keep working as before.
- Devices page: a click on a device's row opens its page, as Sowel's row does, except on the name
  editor, "Create equipment", delete and the equipment links, which keep their action. The
  Equipment column's equipment name links to the equipment's page (the room keeps its link).
- The device page lists nothing about equipments (Sowel's does not).

### R3: The equipment page (Sowel's `EquipmentDetailPage`, cut)

- **Back link** at the top: "← <room name>" to the equipment's room (Sowel's fallback, Solite has
  no router state), "← Equipments" when the room is unknown.
- **Header**: the type's icon tile, the name, the status badge (with its reason), under it the
  type's label, " · <room>", and "Disabled" (warning colour) when disabled; an **Edit** button
  opens the existing form in edit mode (name and room, the type locked, as today).
- **Controls**, only when enabled: a `light_onoff`, `light_dimmable`, `light_color` or `switch`
  whose main on/off order is bound (`findMainOnOffOrder`, as Sowel's page) shows Sowel's
  `LightControl` (the same as the room row's: sends `ON`/`OFF`, a refusal shows its text 5 s);
  an equipment with sensor data shows `SensorValues` (the room row's values), also when disabled
  (as the room row: values are read, not sent).
- **Configuration** card, rows in this order:
  1. **Activation** (Solite's row, decision D50): a `ConfigRow` titled "Activation", whose summary
     says "Enabled" or "Disabled", opened it holds a `Switch`; switching sends `PUT /api/v1/equipments/:id
     {"enabled": <bool>}`; the switch waits for the answer; a refusal shows the host's text in the
     row and the switch goes back.
  2. **Devices** (Sowel's `DevicesSection`): collapsed by default, the number of distinct bound
     devices; opened, each device with its name (a link to its page), when it was last seen
     (relative, "never" when `lastSeen` is null) and "<n> data · <m> orders" bound to it (Sowel's
     counts); the device name is a link (a Solite addition, Sowel's is plain text); the button "Change
     devices" (or "Add" with none) opens the change-devices dialog (R5). "No device" when none.
  3. **Bindings** (Sowel's section): collapsed by default, the total count; opened: the data
     bindings (alias, category, device name, key, value with unit, "—" when null) and the order
     bindings (alias, type, device name, key), each with an "Add" button (R4) and, per row, a
     remove button (shown on hover and always on a touch screen, title "Remove"); removing sends
     `DELETE .../data-bindings/:bindingId` or `.../order-bindings/:bindingId` with no
     confirmation (as Sowel); a refusal shows its text for 5 s under the list. "None" for an
     empty list. Above them, when the bound devices give data or orders the type uses and the
     equipment does not bind (computed only once the section is opened: one `GET /devices/:id`
     per bound device, one after the other, as Sowel's `fetchDevices`; a device that cannot be
     read is skipped, as Sowel), a banner "<n> new data points available" with "Review" (R6).
     **Recipes**: removing a binding is what the API already does: a motion-light instance whose
     light loses its `state` order binding is disabled by the core ("Disabled: ..."), and putting
     the binding back does not enable it again (it is enabled again from the room's Recipes
     panel).
  4. **Delete equipment** (confirmed, as today); success goes to `#/equipments`, a refusal shows
     its text.
- **Not found**: an id the store does not hold once loaded shows "Equipment not found" with the
  back link to `#/equipments`; before the first load, nothing (no flash of "not found"); when the
  equipments cannot be read, the store's error (already shown above every page) and "Retry",
  which reads them again.
- **Live**: values, status, bindings and the enabled state follow the store, which follows the
  equipment events (as the rooms do).

### R4: Add a binding (Sowel's `AddBindingModal`)

- Step 1: the devices, read by `GET /api/v1/devices` (a spinner while reading; "No device" when
  empty); a click reads that device (`GET /devices/:id`).
- Step 2: its data (mode data) or orders (mode order), each with key, category or type and, for
  data, its value; a click selects it and fills the alias with Sowel's suggested name
  (`resolveAlias`, the creation form's names: `state`, `brightness`, `battery`...); the alias is
  editable; "Back" returns to step 1.
- "Add" sends `POST .../data-bindings {deviceDataId, alias}` or `.../order-bindings {deviceOrderId,
  alias}` with the alias trimmed. Checked before sending, with nothing sent: an empty alias (Add
  disabled), an alias the equipment already uses in that kind ("Alias "<alias>" already in use"),
  an alias over 32 characters counted as the host counts them (Unicode characters, not UTF-16
  units: `[...alias].length`) ("At most 32 characters"). The host's refusal (the 17th binding, a device item gone)
  shows its text in the dialog, which stays open. Success closes it; the list shows the binding.
- A device with no data (or no order) shows "Nothing to bind on this device".
- Escape and a click outside close it, as the other dialogs.

### R5: Change devices (Sowel's `ChangeDeviceModal`)

- The dialog holds the creation form's `DeviceSelector` for the equipment's type (the devices
  that give what the type uses first, the others on demand), nothing ticked at first, and the
  sentence "The current bindings are removed and replaced by those of the devices chosen."
- "Save" (disabled with no device ticked, or when the devices ticked give nothing the type uses)
  removes every binding of the equipment, one `DELETE` after the other, then writes the plan of
  the creation form (`computeBindingPlan`: what the type uses, Sowel's aliases, at most 16 of
  each kind), one `POST` after the other. The first refusal stops it (Sowel's `removeAllBindings`
  skips a refused removal and carries on: Solite stops, so no binding is written over one it
  could not remove), shows the host's text in the dialog, which stays open, and the equipment
  keeps what was written so far (not atomic, as Sowel; the bindings list shows it). Success closes
  the dialog. Removing the bindings has the recipe effect of R3.
- An equipment of a type without binding rules (one of Sowel's 23 others, `door`, made through
  the API): its plan is always empty, so "Save" stays disabled with the sentence "This type is
  completed binding by binding" and the banner never shows; "Add" in Bindings works for it.

### R6: Missing bindings (Sowel's `MissingBindingsModal`)

- What is missing: the plan the creation form would write today for the equipment's bound devices
  (`computeBindingPlan`, in the order the devices are first bound), minus the items already bound
  (compared by `deviceDataId` / `deviceOrderId`), with aliases made unique against those the
  equipment already uses; the same 16-per-kind bound counts the existing bindings.
- The dialog lists them, all ticked, each with alias, kind (data or order), device name and key;
  unticking one leaves it out ("Add" disabled with none ticked); "Add <n>" writes the ticked ones
  one after the other; the first refusal stops, shows its text, and the items already written
  leave the list (a second "Add" sends only the rest). Success closes the dialog and the banner
  disappears when nothing is missing any more. "Nothing missing" when the list is empty. Escape and
  a click outside close it.
- A device that cannot be read is skipped (its items are not offered), as Sowel's `fetchDevices`.

### R7: The device page (Sowel's `DeviceDetailPage`, cut)

- Back link "← Devices" to `#/devices`.
- Header: a radio icon, the name with Sowel's `DeviceNameEditor` (as on the Devices page), the
  status badge (online, offline, unknown). As Sowel's page, the name, status and last seen are
  read from the devices store (`useDevices`, which a rename and `device.status_changed` update);
  only the data and orders come from the page's own read.
- Information, Sowel's: manufacturer and model, the source device id, the IEEE address, last seen
  (relative; "never"), created; a field the device does not give is left out.
- **Data** (Sowel's `DeviceDataTable`): key, category, value with its unit ("—" when null), updated
  (relative), sorted by category then key; "No data" when empty.
- **Orders** (Sowel's `OrdersTable`): key, type, range (`min`–`max` with unit, or the enum values,
  or "—"); the section is shown only when the device has orders.
- **Delete device** (confirmed: "The device stays in the Zigbee network"); success goes to
  `#/devices`; a refusal (for example 409 "Device is being paired") shows its text.
- The page reads `GET /api/v1/devices/:id` on opening; a 404 shows "Device not found" with the
  back link; another failure shows its text and "Retry".
- **Live**: a `device.data.updated` event for this device updates the data of that `dataId`:
  its value, and `lastUpdated` from the event's `timestamp` (an event for a data the page does not
  hold reads the device again); `device.status_changed` updates the badge through `useDevices`;
  `device.removed` for it shows "Device not found".

### R8: Creation without a device

- The creation form's step 2 no longer requires a device: with none ticked, "Create" creates the
  equipment without bindings (`POST /api/v1/equipments` only), as Sowel's form allows; the hint
  under the list says "You can also add devices later from the equipment's page", and after
  such a creation the new equipment's page opens (`#/equipments/<id>`). Devices ticked that give
  nothing the type uses still block "Create" (unchanged).

### R9: Size

The first load stays within the 170 KB gzip budget (`npm run check:bundle`); the two pages are
in the main bundle (no code splitting, as the other pages). Estimate: about +8 KB gzip over
63.3 KB (spec 029); a growth past 15 KB is a stop to look at. The same growth lands in the copy
of the interface embedded in the radio image.

## Decisions taken in this spec

- **Activation row** (D50): Sowel's interface has no control for `enabled` on an equipment (its API
  takes the field); Solite adds one row to the page's Configuration card, built from Sowel's
  `ConfigRow` and `Switch`, so a disabled equipment can be enabled again without `curl`.
- **Hash addresses** (D50): `#/equipments/<id>` and `#/devices/<id>`, as `#/zone/<id>` (no
  router, D19); the back link goes to the room (Sowel's fallback).
- **Whole pages ported** (D50, narrows D19's "leaf components, not its pages"): the two detail
  pages are Sowel's, cut, as specs 018 and 019 already did for the lists.
- **Creation without a device** (D50, narrows D30's "needs at least one device, since Solite has
  no equipment page"): as Sowel's form.
- **Change devices is not atomic** (as Sowel): a refusal midway leaves the bindings written so
  far; the page shows them and the user can add the rest one by one.
- **Device page live data**: the page holds its device's data and orders in a small store fed by
  `useLive`'s `device.data.updated` (already received, the `devices` topic is subscribed), rather
  than a periodic read; the name, status and last seen stay in `useDevices`, as Sowel's page.

## Living spec delta

### docs/living-spec/README.md, "Shows the home in the browser"

- Changed: the opening paragraph: the interface also shows an equipment's page (its values,
  devices and bindings, completed or changed one by one, enabled or disabled) and a device's page
  (its data and orders).
- Changed, **The room**: an equipment's name opens its page.
- Changed, **Devices**: a click on a row opens the device's page; the Equipment column's equipment
  name opens the equipment's page.
- New, **Device page** (`#/devices/<id>`): R7 in business words.
- Changed, **Equipments**: the name opens the equipment's page; step 2 needs no device ("at least
  one" removed); a device ticked that gives nothing still blocks.
- New, **Equipment page** (`#/equipments/<id>`): R3 to R6 in business words.
  Including: removing a binding a recipe needs disables the recipe (the core's rule), and putting
  it back does not enable it again.

### docs/living-spec/README.md, "Organises the home in zones and equipments"

- Changed: "Zones and equipments are also made from the interface" → also completed from it: the
  bindings one by one, the devices replaced, enabled and disabled.

### No longer true

- `docs/living-spec/README.md`, Equipments: "then the devices (at least one; ...)" → no minimum.
- `ui/src/components/equipments/EquipmentsPage.tsx` header comment: "Sowel's `EquipmentCard` (its
  detail page, 1288 lines, is out of scope)" → the detail page is ported (spec 030).
- `ui/src/components/devices/DeviceList.tsx` header comment: "no row link (Solite has no device
  page)" → the row opens the device's page.
- `ui/src/components/equipments/EquipmentForm.tsx` header comment: "at least one device" → none
  required.
- `ui/src/components/devices/EquipmentCell.tsx` header comment: "the equipment's name is plain
  text (Solite has no equipment page...)" → a link to its page.
- `docs/design-system.md` lines 40, 41, 52, 54: the binding actions cut, the list of API calls
  (gains the two `DELETE`), "at least one device", "no device page".
- The tests asserting "Choose at least one device" (`EquipmentForm.test.tsx`,
  `EquipmentsPage.test.tsx`, `ZonePage.test.tsx`) and the key `equipments.form.deviceRequired`
  in `i18n/messages.ts`: the new rule and hint.
- `ui/src/store/useEquipments.ts` header comment: "Cuts: the actions that edit bindings" → ported.
- `ui/src/api/equipments.ts` header comment: the cut list gains the two `DELETE` and `getEquipment`
  is not needed (the store holds every equipment).
- `docs/design-system.md`: the list of ported components and the budget line (first load after
  spec 030).
- Done in part C: `docs/design-system.md` lines 40, 52, 54 and the API-calls list rewritten, two rows
  added (equipment page, device page), budget line 72.7 KB; `docs/roadmap.md` J2 row, order of work
  item 3, four inbox lines removed or narrowed, log line. The search of 3.4 found nothing else.
- Others found by the search of 3.4 (names: `EquipmentDetailPage`, `DeviceDetailPage`,
  `AddBindingModal`, `at least one device`, `no device page`, `detail page`).

## Acceptance criteria

- [x] From the Equipments page, a room or the Devices page, a click opens the equipment's page;
      it shows the header, the controls when enabled, and the Configuration card (Activation, Devices,
      Bindings, Delete).
- [x] A data binding and an order binding are added from the page and removed again, the list
      changing without a reload; a duplicate alias is refused before sending.
- [x] A binding removed by hand is offered again by the "new data points" banner and put back.
- [x] "Change devices" replaces the bindings with those of the device chosen.
- [x] The Activation switch disables the equipment (controls hidden, "Disabled" in the header, the
      room row dimmed) and enables it again.
- [x] A device's page shows its data with live values, its orders, renames and deletes it.
- [x] An equipment is created with no device and completed from its page.
- [x] `sh scripts/validate.sh` green, first load within 170 KB gzip.
- [x] Screenshots of the equipment page, the add-binding dialog, the change-devices dialog and
      the device page, Hybrid and Dark themes.

## Edge cases

- An unknown or deleted equipment id: "Equipment not found"; deleted while open (an
  `equipment.removed` event): the page shows "Equipment not found".
- An equipment with no binding: Devices "No device" with "Add", Bindings "None", no banner, no
  controls.
- A binding whose device was removed (the core removes it): it disappears with the event.
- Removing the light's `state` order binding (or "Change devices") disables its motion-light
  instance; putting it back leaves it disabled (enabled again from the room).
- An equipment of a type without binding rules (`door`): Save of Change devices disabled, no
  banner, Add binding works.
- The equipments cannot be read: the error and "Retry", not a blank page.
- Missing bindings: a refusal after one write; the second "Add" sends only the rest.
- The 17th binding: the host's 400 text shown in the dialog.
- An alias of 33 characters, an alias already used: refused before sending.
- A device read that fails in the missing-bindings count: no banner.
- Change devices with a refusal midway: the text shown, the partial bindings listed.
- A device page for an unknown id: "Device not found"; the server unreachable: the error and
  "Retry".
- A `device.data.updated` for a data key the page does not hold: the device read again.
- Clicking the name editor or a button in a device row does not open the device's page.
- A malformed `#/equipments/%` or `#/devices/%`: the list is shown.

## Sowel contract

No type, field, route or event added. The pages mirror Sowel's `ui/src/pages/EquipmentDetailPage.tsx`
(with its `DevicesSection` and `ChangeDeviceModal`), `ui/src/components/equipments/AddBindingModal.tsx`,
`MissingBindingsModal.tsx`, `ConfigRow.tsx`, `ui/src/pages/DeviceDetailPage.tsx` and
`ui/src/components/devices/DeviceDataTable.tsx` (Sowel commit 5da84dc1), with the requests of
`docs/technical/api-reference.md` (equipments, bindings, devices) that Solite already serves. The
Activation row is an interface addition (D50); Sowel's API takes the same `PUT {"enabled"}`.

## How the user will check it

On the Mac, without the dongle: `cargo run -p solite-host -- --sim scenarios/garage.json --serve
--speed 10`, `sh scripts/demo-garage.sh`, then `cd ui && npm run dev` and open
`http://localhost:5173`: open the garage light's page from the room, remove its `state` data
binding (the order binding stays, so the recipe keeps running), see the banner offer it again and
put it back; on the "Garage motion" equipment, add the motion sensor's `battery` data binding if
it lacks it, or remove and add it again; disable the light (its room row dims) and enable it; open the motion sensor's device page
and watch its data change; create a "Test" sensor with no device and give it the temperature
sensor with "Add" in Devices. Screenshots in Hybrid and Dark (the session's scratchpad).
