# Architecture: Spec 030

## Overview

Interface only; the core and the API are unchanged.

```
#/equipments/<id> ──► App ──► EquipmentDetailPage ──► useEquipments (store, live)
                                 │  ├─ LightControl / SensorValues (existing)
                                 │  ├─ EnabledRow (ConfigRow + Switch) ── PUT {enabled}
                                 │  ├─ DevicesSection ── ChangeDeviceModal ── DELETE* then POST* (plan)
                                 │  └─ Bindings ── AddBindingModal ── POST data|order-binding
                                 │               └─ MissingBindingsModal ── GET /devices/:id*, POST*
#/devices/<id>    ──► App ──► DeviceDetailPage ──► useDeviceDetail (store) ◄── useLive device.*
                                    ├─ DeviceNameEditor (existing), DeviceDataTable, OrdersTable
```

## Data

No new stored data. In the browser:

- `useDeviceDetail` (zustand): `{ deviceId, data: DeviceData[], orders: DeviceOrder[], notFound,
  error, loading }` for the one device page open; `load(id)`, `clear()`, and the live handlers
  `handleDataUpdated(event)` (by `dataId`, `lastUpdated` from `timestamp`) and `handleRemoved(id)`.
  The name, status and last seen stay in `useDevices` (as Sowel's page). One device at a time: no
  growth.
- `useEquipments` gains `addDataBinding`, `removeDataBinding`, `addOrderBinding`,
  `removeOrderBinding` (Sowel's store actions: the request, then `fetchEquipments()`), and
  `updateEquipment` takes `enabled`.
- `bindingUtils.ts` gains `computeMissingBindings(devices, type, dataBindings, orderBindings)`
  (Sowel's name; Solite's body: `computeBindingPlan` minus what is bound, aliases made unique
  against the bound ones, 16 per kind counted with the bound ones), `fetchDevices(ids)` (one after the other, a
  failed read skipped, as Sowel's) and
  `removeAllBindings(equipmentId, data, orders)`.

## Files changed

| File | Change |
| ---- | ------ |
| `ui/src/components/layout/admin-nav-items.ts` | `adminPageFromHash` maps `#/equipments/<id>` and `#/devices/<id>` to their page; `detailIdFromHash(hash, "equipments" \| "devices")` (malformed or empty: `null`) |
| `ui/src/App.tsx` | Shows `EquipmentDetailPage` or `DeviceDetailPage` when the hash carries an id |
| `ui/src/api/equipments.ts` | `removeDataBinding`, `removeOrderBinding` (Sowel's names); `updateEquipment` takes `enabled` |
| `ui/src/store/useEquipments.ts` | The four binding actions; `enabled` |
| `ui/src/store/useDeviceDetail.ts` (new) | The device page's store |
| `ui/src/store/useLive.ts` | `device.data.updated` to `useDeviceDetail`; `device.status_changed` and `device.removed` also to it |
| `ui/src/components/equipments/EquipmentDetailPage.tsx` (new) | Port of Sowel's page, cut (R3), with `DevicesSection` and `ChangeDeviceModal` |
| `ui/src/components/equipments/ConfigRow.tsx` (new) | Port of Sowel's, unchanged |
| `ui/src/components/equipments/EnabledRow.tsx` (new) | Solite's Enabled row (D50) |
| `ui/src/components/equipments/AddBindingModal.tsx` (new) | Port of Sowel's (R4), on Solite's `Modal` when Sowel's markup does not close on Escape |
| `ui/src/components/equipments/MissingBindingsModal.tsx` (new) | Port of Sowel's (R6) |
| `ui/src/components/equipments/bindingUtils.ts` | `computeMissingBindings`, `fetchDevices`, `removeAllBindings` |
| `ui/src/components/equipments/EquipmentsPage.tsx` | The name links to the page |
| `ui/src/components/equipments/EquipmentForm.tsx` | No device required (R8), the hint |
| `ui/src/components/equipments/NewEquipmentForm.tsx` | An empty plan with no device creates the equipment alone |
| `ui/src/components/home/EquipmentRow.tsx` (and the light and sensor rows it renders) | The name links to the page |
| `ui/src/components/devices/DeviceList.tsx`, `EquipmentCell.tsx` | Row click opens the device page (not on its controls); equipment name links to its page |
| `ui/src/components/devices/DeviceDetailPage.tsx` (new) | Port of Sowel's page, cut (R7), with `OrdersTable` |
| `ui/src/components/devices/DeviceDataTable.tsx` (new) | Port of Sowel's, unchanged but for the imports Solite has (`formatDataValue`, a relative time) |
| `ui/src/i18n/messages.ts` | Sowel's keys for the new texts, French and English |
| `tools/e2e/src/e2e.mjs` | Steps on the equipment and device pages, screenshots in both themes |
| `docs/living-spec/README.md`, `docs/design-system.md`, `docs/decisions.md` (D50), `docs/roadmap.md`, `docs/specs-index.md` | As the spec says |

## Reused

- `EquipmentForm` (edit mode), `DeviceSelector`, `computeBindingPlan`, `applyBindingPlan`,
  `resolveAlias`, `MAX_BINDINGS`, `MAX_ALIAS` (spec 019).
- `LightControl`, `SensorValues`, `EquipmentStatusBadge`, `TYPE_ICONS`/`TYPE_LABELS`,
  `useEquipmentState` (spec 014/018), `DeviceNameEditor` (spec 017), `Switch` (spec 024), `Modal`
  and its button classes, `apiErrorText`/`apiErrorStatus`, `format-relative.ts`, `zone-path.ts`.
- The 5 s refusal pattern of `DeviceList` and `EquipmentsPage` (`ERROR_MS`).
