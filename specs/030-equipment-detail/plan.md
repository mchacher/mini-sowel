# Plan: Spec 030

## Steps

- [x] 1. Living spec updated (the delta of `spec.md`), decision D50 written
- [x] 2. Core types: not applicable (no type added)
- [x] 3. Core logic: not applicable (no change)
- [x] 4. Adapters (host, firmware): not applicable
- [x] 5. API or WebSocket: not applicable (the routes exist)
- [x] 6. Interface, in this order:
  - [x] 6.1 Addresses (`admin-nav-items.ts`, `App.tsx`)
  - [x] 6.2 API and store: binding actions, `enabled`, `useDeviceDetail`, `useLive`
  - [x] 6.3 `bindingUtils`: `computeMissingBindings`, `fetchDevices`, `removeAllBindings`
  - [x] 6.4 Equipment page, `ConfigRow`, `EnabledRow`, `DevicesSection`, `ChangeDeviceModal`
  - [x] 6.5 `AddBindingModal`, `MissingBindingsModal`
  - [x] 6.6 Device page, `DeviceDataTable`, `OrdersTable`
  - [x] 6.7 Ways in: Equipments page, room rows, Devices rows, Equipment column
  - [x] 6.8 Creation without a device
  - [x] 6.9 i18n keys (French, English)
- [x] 7. Tests (table below), each failing once with the code broken
- [x] 8. e2e steps and screenshots (Hybrid, Dark)
- [x] 9. Docs: design system (components, budget), roadmap (J2, inbox, log), specs index,
       "No longer true" corrected

## Test plan

| Module | Scenario | Expected |
| ------ | -------- | -------- |
| `admin-nav-items` | `#/equipments/<id>`, `#/devices/<id>` | page `equipments` / `devices`, the id read |
| `admin-nav-items` | `#/equipments/%`, `#/equipments/`, `#/devices/%` | the page, id `null` (the list) |
| `admin-nav-items` | `#/equipments`, `#/zone/x` unchanged | as before |
| `App` | `#/equipments/<id>` | the equipment page shown, Equipments marked in the sidebar |
| `bindingUtils.computeMissingBindings` | a light whose `state` binding was removed | `state` offered with its device data id |
| `bindingUtils.computeMissingBindings` | everything bound | empty |
| `bindingUtils.computeMissingBindings` | a sensor already bound `battery`, a second device's battery missing | offered as `battery_2` (alias unique against bound ones) |
| `bindingUtils.computeMissingBindings` | 16 data bound, more available | nothing data offered past 16 |
| `bindingUtils.removeAllBindings` | 2 data, 1 order | three `DELETE`, one after the other, data first |
| `bindingUtils.removeAllBindings` | second `DELETE` refused | stops, throws, third not sent |
| `useEquipments` | `removeDataBinding` | `DELETE .../data-bindings/<id>`, then the list read again |
| `useEquipments` | `addOrderBinding` | `POST .../order-bindings {deviceOrderId, alias}`, then read again |
| `useEquipments` | `updateEquipment(id, {enabled:false})` | `PUT {"enabled":false}` |
| `useDeviceDetail` | `device.data.updated` for a held data | value and `lastUpdated` changed, no request |
| `useDeviceDetail` | `device.data.updated` for an unknown key | the device read again |
| `useDeviceDetail` | event for another device | nothing changes |
| `useDeviceDetail` | `device.removed` for it | `notFound` |
| `useDeviceDetail` | load answers 404 / 503 | `notFound` / `error` with the text |
| `EquipmentDetailPage` | a light, enabled | header (name, type, room, badge), LightControl, Configuration rows |
| `EquipmentDetailPage` | disabled equipment | "Disabled" in the header, no control |
| `EquipmentDetailPage` | unknown id, store loaded | "Equipment not found" and back link to `#/equipments` |
| `EquipmentDetailPage` | store not loaded yet | no "not found" |
| `EquipmentDetailPage` | Bindings opened, remove a data binding | `DELETE` sent; refusal text shown |
| `EquipmentDetailPage` | Bindings opened, a bound device gives an unbound used item | banner with the count; not before opening |
| `EquipmentDetailPage` | device read fails for the count | no banner |
| `EquipmentDetailPage` | Delete confirmed | `DELETE /equipments/<id>`, hash `#/equipments` |
| `EnabledRow` | switch off | `PUT {"enabled":false}`; summary "Disabled" after the answer |
| `EnabledRow` | refusal | host text shown, switch back to on |
| `DevicesSection` | two devices bound | both with name link, last seen, counts; "Change devices" |
| `DevicesSection` | no binding | "No device" and "Add" |
| `ChangeDeviceModal` | a device ticked, Save | every old binding deleted, then the plan written, dialog closed |
| `ChangeDeviceModal` | nothing ticked | Save disabled |
| `ChangeDeviceModal` | a `POST` refused | text shown, dialog open, no further request |
| `AddBindingModal` | pick device then data | alias suggested by `resolveAlias`; Add sends `{deviceDataId, alias}` |
| `AddBindingModal` | order mode | the device's orders listed; sends `{deviceOrderId, alias}` |
| `AddBindingModal` | alias already used | "already in use", nothing sent |
| `AddBindingModal` | alias of 33 characters | refused, nothing sent |
| `AddBindingModal` | host refuses (17th) | its text in the dialog, still open |
| `AddBindingModal` | device with nothing of the mode | "Nothing to bind on this device" |
| `MissingBindingsModal` | two missing, one unticked, Add | one `POST`, `onAdded` called |
| `MissingBindingsModal` | refusal | text shown, stops |
| `DeviceDetailPage` | device with data and orders | info, data table sorted by category then key, orders with range |
| `DeviceDetailPage` | no order | no Orders section |
| `DeviceDetailPage` | 404 | "Device not found", back link |
| `DeviceDetailPage` | other failure | text and "Retry", Retry reads again |
| `DeviceDetailPage` | Delete confirmed / refused 409 | hash `#/devices` / text shown |
| `DeviceList` | click on a row | hash `#/devices/<id>` |
| `DeviceList` | click on the name editor, "Create equipment", delete | no navigation (existing behaviour kept) |
| `EquipmentCell` | equipment name | link `#/equipments/<id>`; room link unchanged |
| `EquipmentsPage` | name | link `#/equipments/<id>`; edit and delete unchanged |
| `EquipmentRow` (room) | name | link `#/equipments/<id>`; light button still sends its order |
| `EquipmentForm` | no device ticked | "Create" enabled, the hint shown |
| `NewEquipmentForm` | no device | `POST /equipments` only, no binding request |
| `EquipmentForm` | devices ticked that give nothing | "Create" still blocked (unchanged) |
| `admin-nav-items` | `#/devices/` (empty id) | page `devices`, id `null` |
| `App` | `#/devices/<id>` | the device page, Devices marked in the sidebar |
| `bindingUtils.fetchDevices` | three ids, the second read fails | read one after the other; two devices returned |
| `EquipmentDetailPage` | back link | "← <room>" to `#/zone/<roomId>`; "← Equipments" when the room is unknown |
| `EquipmentDetailPage` | Edit | the form opens in edit mode with name and room |
| `EquipmentDetailPage` | the equipment leaves the store (`equipment.removed`) | "Equipment not found" |
| `EquipmentDetailPage` | equipments read failed | the error and "Retry", which reads again |
| `EquipmentDetailPage` | remove an order binding refused | `DELETE .../order-bindings/<id>` sent; the text shown, gone after 5 s |
| `EquipmentDetailPage` | a `door` equipment | no banner; Add binding offered |
| `ChangeDeviceModal` | devices ticked give nothing the type uses (or a `door`) | Save disabled |
| `ChangeDeviceModal` | a `DELETE` refused | stops, text shown, no `POST` |
| `MissingBindingsModal` | refusal after one write, then Add again | the second Add sends only the unwritten items |
| `MissingBindingsModal` | nothing ticked / empty | Add disabled / "Nothing missing" |
| `DeviceDetailPage` | `device.status_changed` | badge updated (from `useDevices`) |
| `DeviceDetailPage` | rename in the header | the new name shown |
| `useDeviceDetail` | `device.data.updated` | matched by `dataId`, `lastUpdated` = event `timestamp` |
| `DeviceList` | click on the equipment link or room link in a row | no device navigation, the link's own |
| `AddBindingModal` | alias of 32 emoji (64 UTF-16 units) | accepted (counted as characters) |
| `NewEquipmentForm` | creation with no device | the new equipment's page opens |
| e2e (opt-in) | the motion-light instance after removing and re-adding the light's `state` data binding | still enabled |
| Existing tests | all | still green (the three "Choose at least one device" tests changed to the new rule) |
| Bundle | first load | within 170 KB gzip (`check:bundle`) |
| e2e (opt-in) | open the light's page from the room, disable then enable, remove `state` and put it back from the banner, open the motion sensor's device page | each step checked through the screen and the API; screenshots Hybrid and Dark |

## Validation

- [x] Validate command green
- [x] Agent review done, blocking findings fixed
- [ ] Demo seen and approved by the user (screenshots in the scratchpad; the user is away)
