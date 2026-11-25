# Architecture: Spec 037

## Overview

```
GET /api/v1/settings ──► useHomeSettings (name, hasLocation) ─┐
GET /api/v1/system/timezone ──► useTimezone (tz, loaded) ──────┤
/ws zone.data.changed ──► useZoneAggregation (root: sun) ──────┼──► TopBar (in TopBarFrame)
useLive.status ──► useLinkDegraded (3 s) ──────────────────────┤      left: breadcrumb / phone title
zone shown (chooseZone) + page ──► topBarContext ───────────────┘      right: HomeTimeNote, SunTimes
                                                                             (or the location link),
                                                                             ConnectionStatus
DeviceList row: status, name, manufacturer, model, Equipment ("—"), delete   (no creation)
```

Interface only. The core gains one test (R5); no code in `crates/`, `firmware/` or the API.

## Data

| Name | Type | Where |
| ---- | ---- | ----- |
| `useHomeSettings.name` | `string \| null` (null until read) | memory, from `GET /settings` `home.name` |
| `useHomeSettings.hasLocation` | `boolean \| null` (null until read) | memory, `home.latitude` and `home.longitude` both present |
| `useTimezone.{tz,source,offsetHours,loaded}` | Sowel's | memory, from `GET /system/timezone` |

Nothing stored, nothing new on the host.

## Files changed

| File | Change |
| ---- | ------ |
| `ui/src/components/devices/DeviceList.tsx` | the "Create equipment" button, `create`, `onCreate`, the `creating` state, `NewEquipmentForm`, `getDevice` and `deduceType` imports removed; header rewritten |
| `ui/src/components/devices/DevicesPage.test.tsx` | the "Create equipment" tests replaced by: no button on any row, "—" on a device with none; the row click test without it |
| `ui/src/components/discovery/DiscoveryPage.test.tsx` | the new row has no "Create equipment" |
| `ui/src/components/equipments/EquipmentForm.tsx` | `initialType`, `typeIsGuess`, `initialName`, `initialDeviceIds`, the guess (`guessing`, `deduceType`) removed; `handleDevicesRead` only stores; header rewritten |
| `ui/src/components/equipments/EquipmentForm.test.tsx` | "opens with the type, name and device it is given" and "guesses the type again" removed; the three re-read tests rewritten to tick the device through the selector |
| `ui/src/components/equipments/NewEquipmentForm.tsx` | the same props removed; header rewritten |
| `ui/src/components/equipments/equipment-type-meta.tsx`, `.test.ts` | `deduceType` and its test removed (no other user) |
| `ui/src/api/devices.ts` | header sentence on "Create equipment" removed |
| `ui/src/i18n/messages.ts` | `devices.createEquipment` and `app.connectionLost` removed; `nav.maison`, `header.sun`, `header.homeTime`, `header.setLocation`, `status.connecting`, `status.disconnected` added (en, fr; Sowel's texts where Sowel has the key) |
| `ui/src/components/layout/TopBarFrame.tsx` (new) | Sowel's, without `useShadowMode` (no banner: always fixed below `md`); `max-lg:` → `max-md:`, `lg:hidden` → `md:hidden` |
| `ui/src/components/layout/SunTimes.tsx` (new) | Sowel's `SunTimes` and `MobileSunTimes` unchanged; plus `SunLocationHint` (Solite's) |
| `ui/src/components/layout/HomeTimeNote.tsx` (new) | Sowel's, unchanged |
| `ui/src/components/layout/ConnectionStatus.tsx` (new) | Sowel's; `lg:` → `md:` |
| `ui/src/hooks/useTopBarState.ts` (new) | Sowel's; `useLinkDegraded` reads `useLive` (`open` = connected, `closed` = disconnected) |
| `ui/src/store/useTimezone.ts` (new) | Sowel's, unchanged |
| `ui/src/store/useHomeSettings.ts` (new) | Solite's: `name`, `hasLocation`, `fetch()` (a failure keeps what was read; first failure: name `""`) |
| `ui/src/api/settings.ts` | `getSystemTimezone` (Sowel's `ui/src/api/system.ts` shape: `{tz, source, offsetHours}`) |
| `ui/src/components/layout/TopBar.tsx` (new) | Sowel's `AppLayout` header: `TopbarBreadcrumb`, `MobileTopbarTitle`, `HomeNameSkeleton`, the right-hand group and `headerPills` (home time + link); the page from the hash (`adminPageFromHash`, `detailIdFromHash`), the zone from the `zoneId` prop (the `chooseZone` result) and the tree (`lib/zone-path.ts`); `onMenu` for the phone's menu button, `zoneId` (the zone shown) and `page` from `Home` |
| `ui/src/components/layout/top-bar-context.ts` (new) | pure function: the page (from the hash), the zone the page shows (App's `chooseZone` result, passed down), the tree and the labels → the context parts (`string[]`), tested alone |
| `ui/src/components/layout/Sidebar.tsx` | `MobileTopBar` removed (the TopBar takes its place) |
| `ui/src/components/layout/Banner.tsx`, `Banner.test.tsx` | removed (D62) |
| `ui/src/store/useLive.ts` | `refetchAll` also calls `useHomeSettings.fetch` and `useTimezone.fetch`; `connect()` sets `connecting` each time it creates a socket |
| `ui/src/components/settings/SettingsPage.tsx` | after a successful save, `useHomeSettings.fetch()` and `useTimezone.fetch()` |
| `ui/src/App.tsx` | the content column: `TopBarFrame` + `TopBar` above `<main>`; `Banner` and `MobileTopBar` gone; `Home` reads `useHomeSettings` and `useTimezone` once on mount (review), then `useLive` reads them at every open |
| `crates/core/tests/sun.rs` | Paris and New York on 2026-11-25, local times |
| `tools/e2e/src/e2e.mjs` | create-light and create-sensor through the Equipments page's "Add equipment"; a check that no row has "Create equipment"; screenshots of the bar |
| Docs | living spec, design-system.md, decisions (D61, D62), roadmap, specs-index |

## Reused

- Sowel's components named above, ported as the design system's rule says (classes kept).
- `lib/zone-path.ts` (Sowel's, already ported) for the room's path; `admin-nav-items.ts` for the
  pages' labels and the hash parsing; App's `chooseZone` result for the zone shown.
- `lib/format.ts` `formatRange` and `currentLocale` (Sowel's, already ported).
- `useLive`'s refetch on every open, the pattern every store already follows.
