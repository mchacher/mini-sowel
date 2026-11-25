# Plan: Spec 037

## Steps

- [x] 1. Living spec updated (the delta of `spec.md`)
- [x] 2. Core types: not applicable (no type change)
- [x] 3. Core logic: not applicable (R5 confirmed the sun calculation); the R5 test added to
      `crates/core/tests/sun.rs`
- [x] 4. Adapters (host, firmware): not applicable
- [x] 5. API: not applicable
- [x] 6. Interface, R1: the device row's creation path removed (DeviceList, EquipmentForm,
      NewEquipmentForm, `deduceType`, i18n), tests updated
- [x] 7. Interface, R2 and R3: the ported files, `useHomeSettings`, `TopBar`, `top-bar-context`,
      App wiring, Banner and `MobileTopBar` removed, refetch on open and after Settings saves
- [x] 8. e2e test updated (creation through the Equipments page)
- [x] 9. Docs: design-system.md (Layout, ported files, budget figure), decisions D61 and D62,
      roadmap (J2 row, the two inbox lines, a log line), specs-index
- [x] 10. Screenshots: a room's page, the Devices page, the bar with no location, with the home's
      time (Playwright `timezoneId` other than the home's), with "Disconnected", Hybrid and Dark,
      1280 px and 390 px, in the session's scratchpad

## Test plan

| Module | Scenario | Expected |
| ------ | -------- | -------- |
| `DevicesPage.test.tsx` | A device with no equipment and one bound | No "Create equipment" button on either row; "—" on the first, "Room › Equipment" on the second |
| `DevicesPage.test.tsx` | Click on a row, on the name editor, on delete | Unchanged: the row opens the page, the editor and delete keep their action |
| `DiscoveryPage.test.tsx` | A discovered device shown first, marked New | Its row has no "Create equipment" |
| `EquipmentForm.test.tsx` | Existing create and edit tests (types, devices picked, empty plan blocks, no device creates) | Unchanged, pass |
| `EquipmentForm.test.tsx` | The three re-read tests that opened with `initialDeviceIds` (waits reading every 5 s; reads once when the plan is not empty; stops reading when step 2 is left) | Rewritten: the device ticked through the selector; same expectations. Only "opens with the type, name and device it is given" and "guesses the type again" go |
| `EquipmentsPage.test.tsx`, `ZonePage.test.tsx` | "Add equipment" opens the form | Unchanged, pass |
| `top-bar-context.test.ts` | The zone shown is a room two levels below the root | `["Ground floor", "Garage"]` (the root left out) |
| `top-bar-context.test.ts` | The zone shown is the root | `[]` (the phone sub-line then reads "Home") |
| `TopBar.test.tsx` / `App.test.tsx` | No hash, a room "Garage" with equipments; an unknown zone id | The context is "Garage" (the room `chooseZone` shows) |
| `top-bar-context.test.ts` | `#/devices`, `#/devices/<id>`, `#/equipments/<id>`, `#/zones`, `#/discovery/solite`, `#/settings` | The page's label: Devices, Devices, Equipments, Topology, Discovery, Settings |
| `TopBar.test.tsx` | Name read "Annex", on a room | Breadcrumb "Annex › Garage" from `md`; phone title "Annex", sub "Garage" |
| `TopBar.test.tsx` | Name not read yet | The skeleton (`home-name-skeleton`), no "Home" |
| `TopBar.test.tsx` | Name missing or empty, or the read failed | "Home" ("Maison" in French) |
| `TopBar.test.tsx` | Root aggregate with sunrise 08:14, sunset 17:00, by day | "08:14 – 17:00" with the sun |
| `TopBar.test.tsx` | No location, root sunrise null | A link to `#/settings` labelled "Set the location" |
| `TopBar.test.tsx` | Location set, root sunrise null (clock not set) | No sun times, no link |
| `TopBar.test.tsx` | Settings not read yet, root sunrise null | No link |
| `TopBar.test.tsx` | The menu button | Calls `onMenu` |
| `SunTimes.test.tsx` (Sowel's, ported) | Sun times shown, named; nothing without them; `MobileSunTimes` with 0, 1, 2, 3 indicators | As Sowel's test |
| `HomeTimeNote.test.tsx` (Sowel's, ported) | Same zone as the browser; another zone; another spelling of the same zone; not loaded; refreshed after 30 s | As Sowel's test |
| `ConnectionStatus.test.tsx` (Sowel's, ported onto `useLive`) | Open: nothing; closed less than 3 s: nothing; closed 3 s: "Disconnected"; connecting 3 s: "Connecting..."; back open: nothing | As Sowel's test |
| `TopBarFrame.test.tsx` (Sowel's, ported) | Fixed below `md` with the spacer | The frame has `max-md:fixed`, the spacer is there |
| `useHomeSettings.test.ts` | Read with name and both coordinates; with one coordinate missing; a failure first; a failure after a read | `name` and `hasLocation` as read; `hasLocation` false; `name` `""`, `hasLocation` null; the previous values kept |
| `useLive.test.ts` | A close, then the reconnection timer fires | The status is `connecting` while the new socket opens, `closed` after its failure |
| `useLive.test.ts` | The socket opens | The home settings and the time zone are fetched with the other stores |
| `SettingsPage.test.tsx` | A successful save | The home settings and the time zone are fetched again |
| `App.test.tsx` | The home shown | The top bar is there (`banner` role header with the home's name); no old Banner strip |
| `crates/core/tests/sun.rs` | Paris, `2026-11-25T12:00:00.000Z`, Europe/Paris | sunrise "08:14", sunset "17:00" |
| `crates/core/tests/sun.rs` | New York, `2026-11-25T12:00:00.000Z`, America/New_York | sunrise "06:54", sunset "16:31" |
| `tools/e2e` | The garage made through the Equipments page | The light and the sensors exist, their rows in Devices show them; no table row of Devices or Discovery has "Create equipment" |
| Bundle | First load | Under 170 KB gzip (about 76 KB expected) |

## Validation

- [x] Validate command green (QEMU included)
- [x] Agent review done, blocking findings fixed
- [x] Demo: screenshots in the scratchpad (the user reviews them on return); the demo itself waits for the user
