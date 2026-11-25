# Spec 037: top-bar-and-device-rows

## Context

Two interface corrections the author noted on the real box on 2026-11-25.

1. **No equipment is made from a device.** The Devices page (and Discovery, which shows the same
   rows) has a "Create equipment" button on a device that no equipment binds (spec 019, R4;
   roadmap J2: "'Create equipment' from a new device's row"). The author says it is not the path
   the interface offers: in Sowel the equipment is created from the equipments side (the
   Equipments page, a room's "Add equipment"), its form picks the devices, and a device is never
   the entry point. Sowel's `ui/src/components/devices/DeviceList.tsx` (`EquipmentCell`) shows
   "—" on a row with no equipment, and no action.
2. **The top bar is missing.** Sowel's interface has a top bar on every page: the home's name (and
   where you are), the sunrise and sunset, the home's time when the browser sits in another time
   zone, and the link indicator when the live connection is degraded
   (`ui/src/components/layout/AppLayout.tsx`, `TopBarFrame.tsx`, `SunTimes.tsx`,
   `HomeTimeNote.tsx`, `ConnectionStatus.tsx`, `hooks/useTopBarState.ts`, `store/useTimezone.ts`).
   Solite shows only a "Connection lost" strip (`Banner.tsx`) in the page and, on a phone, a bar
   with the menu button and the mark.

Two inbox lines are answered here: "A 'current time' pill in the header (Sowel's
`CurrentTimePill`) and local time in the logs, from `home.timezone`" (the header half; the logs
half stays in the inbox), and "Sun times for Paris on 2026-11-25 read 08:14 and 17:00: check the
sunrise against Sowel's calculation" (R5).

This run is autonomous (the user is away): open questions are decided here and recorded
(decisions D61 and D62, and "Decisions taken in this spec" below).

## Goals

1. The Devices and Discovery pages offer no way to create an equipment; the equipment is created
   from the Equipments page or a room's "Add equipment", whose form picks the devices.
2. A top bar on every page, Sowel's, fed by what Solite has: the home's name and the page, the sun
   times, the home's time when the browser is elsewhere, the link indicator.
3. The sunrise the box shows is checked against Sowel's calculation, and the check is a test.

## Out of scope

- Sowel's alarms pill and sheet (`AlarmsSheet`, `useAggregatedIssues`): Solite has no
  integration-issue list behind them; the updates pill and sheet (`UpdatesSheet`,
  `usePluginUpdates`): Solite has no update feed (its update is a file chosen in Settings, D52);
  the restart pill (`restartRequired`): Solite has no such event. They stay out.
- The user's avatar, name and Log out in the bar (Sowel's desktop bar): Log out stays under
  Administration (spec 029); the account has a display name, but moving Log out is not asked.
- Sowel's `page-title.ts` (`TOP_LEVEL_H1`, the H1 hidden below `lg`): Solite's pages keep their
  visible titles; the phone bar shows the home's name with the page under it, as Sowel's.
- Sowel's shadow mode and takeover banners (`TopBarFrame` reads them): Solite has neither.
- Local time in the logs (the other half of the inbox line): stays in the inbox.
- Any change to the equipment form other than removing what only the device's button used.

## Requirements

### R1: no creation from a device

- The Devices page's rows (also shown by Discovery) lose the "Create equipment" button: a row's
  actions are delete only. A device with no equipment shows "—" in the Equipment column, as
  Sowel's `EquipmentCell`.
- The flow behind the button goes: `DeviceList` no longer reads the device (`getDevice`) nor opens
  `NewEquipmentForm`; `EquipmentForm` and `NewEquipmentForm` lose the props only that flow used
  (`initialType`, `typeIsGuess`, `initialName`, `initialDeviceIds`) and the type guessed from the
  devices read (`deduceType`, used by nothing else, removed with its test); the i18n key
  `devices.createEquipment` goes. Sowel's `EquipmentForm` has none of these props.
- What the equipment form needs to pick devices stays unchanged: `DeviceSelector`, the plan, the
  re-reads every 5 s while the plan is empty, "Create" with no device.
- A click anywhere on a row (but the name editor, delete and the links) still opens the device's
  page.

### R2: the top bar

A bar at the top of the content column, on every page of the home (not on the first-start and
login screens), below the phone's status bar. Ported from Sowel's `AppLayout` header and its
components through `preact/compat`, with Sowel's classes; Sowel's switch point `lg` becomes `md`,
Solite's sidebar switch point (design-system.md, "Layout").

- **Left, from `md` up**: Sowel's `TopbarBreadcrumb`: the home's name, then `›` and where you are,
  the last part in bold (`text-[var(--n-700)]`):
  - a room's page: the path, below the root zone (`Maison` left out, as Sowel's), of the room the
    page shows: the zone `chooseZone` picks (`ui/src/zones.ts`), so with no zone in the address
    (the landing page) or an unknown one it is the room shown (the first with equipments), never
    the hash alone; the root zone's own page: the home's name alone;
  - an administration page: its label (Devices, Equipments, Topology, Discovery, Settings); an
    equipment's or a device's page: the label of its list (Equipments, Devices).
- **Left, below `md`**: the menu button (opens the drawer, as today) and Sowel's
  `MobileTopbarTitle`: the home's name (`text-lead`, truncated) and, under it, the same context
  (`text-micro`), the parts joined by Sowel's `PATH_SEPARATOR` (` › `); on the root zone's own
  page the line under the name is "Home" ("Maison"), as Sowel's. The phone bar no longer
  shows the mark (it stays in the drawer and the sidebar).
- **The home's name**: `home.name` from `GET /api/v1/settings`. While the settings load: Sowel's
  skeleton (a grey bar, `data-testid="home-name-skeleton"`). When the key is missing, empty, or the
  read fails: Sowel's default, the translation `nav.maison` ("Home", French "Maison").
- **Right, in Sowel's order**: the home's time, the sun times, the link indicator.
  - **Sun times** (Sowel's `SunTimes`, unchanged): from the root zone of the zone aggregation
    (`sunrise`, `sunset`, `isDaylight`, live through `zone.data.changed`): a sun (by day, the
    `text-active` amber Sowel tolerates there) or a moon (by night), and "08:14 – 17:00"
    (`formatRange`); hidden for screen readers' "Sun" prefix as Sowel's. Below `md`, Sowel's
    `MobileSunTimes`: shown beside zero or one other indicator, from `sm` up beside two.
  - **No location** (Solite's addition, D62): when the settings are read, `home.latitude` or
    `home.longitude` is missing, and the root zone's `sunrise` is null, the place of the sun times
    holds a link to Settings (`#/settings`): the sun icon in `text-text-tertiary` and "Set the
    location" (French "Définir la position"), the words from `sm` up, the icon alone below with the
    words as its label in a 44 px tap area (`min-h-11 min-w-11`); below `md` it follows
    `MobileSunTimes`' rule (hidden below `sm` beside two other indicators). Sowel's `SunTimes` shows nothing there; Solite ships with no location,
    which "Inactive during day" needs.
  - **Location set, clock not set** (the box before SNTP: the core gives `null` until the clock
    passes 2020): nothing, as Sowel's.
  - **The home's time** (Sowel's `HomeTimeNote`, unchanged): only when the browser's time zone
    differs from the home's (`GET /api/v1/system/timezone`'s `tz`, compared in their canonical
    spelling); a clock icon and "Home time 11:05 UTC+1" (Sowel's `header.homeTime`, French "Heure maison") (the time as `HH:MM` with the zone's
    short name, refreshed every 30 s; compact below `md`: the clock and "11:05 UTC+1", without the
    words). Nothing until the time
    zone is read. With no time zone set, the host answers `UTC` (`source: "fallback"`), the zone
    the box then runs on: a browser elsewhere shows the home's time in UTC. The time is the
    browser's clock in the home's zone: the box's clock is not read, so an unset box clock does
    not hide it.
  - **Link indicator** (Sowel's `ConnectionStatus` and `useLinkDegraded`, unchanged but for the
    store): nothing while the live connection is open, and nothing for the first 3 s
    (`DEGRADED_DELAY_MS` = 3000) after it stops being open, so a restart of the host shows nothing;
    then "Connecting..." (French "Connexion…", Sowel's `status.connecting`) in `text-warning-ink` (Wi-Fi icon) while the interface tries again, or
    "Disconnected" ("Déconnecté", `status.disconnected`) in `text-error` (Wi-Fi-off icon) while it waits between tries; the words from
    `md` up, the icon alone below in a 44 px box, the label always in `aria-label`. Read from
    `useLive`'s status (`open` is Sowel's `connected`, `closed` its `disconnected`). `useLive`
    today sets `connecting` only at start: `connect()` now sets it each time it creates a socket,
    so a reconnection attempt shows "Connecting..." and the wait between attempts "Disconnected".
    As Sowel's, the indicator also shows on the first load when the socket has not opened after 3 s
    (the old Banner counted only from a close).
- **The Banner goes**: the "Connection lost" strip at the top of the page (`Banner.tsx`, its test
  and the key `app.connectionLost`) is replaced by the link indicator (D62).
- **The frame** (Sowel's `TopBarFrame`): below `md` the bar is fixed at the top (`z-30`) over a
  band the height of the phone's safe area, with a spacer keeping its place, as Sowel's (iOS blurs
  the page under a status bar unless a solid box covers it); from `md` up it is in the flow. No
  shadow-mode or takeover banner in Solite: the fixed case always applies below `md`. Solite's
  `index.html` has no `viewport-fit=cover`, so the safe-area band is 0 px today; it is kept for
  parity (it costs nothing), and `index.html` is not changed here.
- Sowel's `hit-group` utility (its `index.css`) is not in Solite's CSS: the class is dropped from
  the ported header (the 44 px areas are given on the elements themselves).
- **Heights**: `h-topbar` (56 px, `--spacing-topbar`) below `md`, `min-h-[49px]` from `md` up
  (the sidebar's header is 49 px, so the two lines meet).

### R3: where the bar's data come from, and when they are read again

- The home's name and whether a location is set: a small store (`store/useHomeSettings.ts`,
  Solite's own: `name: string | null`, `hasLocation: boolean | null` (null until read), `fetch()`), filled from
  `GET /api/v1/settings`.
- The home's time zone: Sowel's `store/useTimezone.ts` (`tz`, `source`, `offsetHours`, `loaded`,
  `fetch()`) with Sowel's `getSystemTimezone` in `api/settings.ts`.
- Both are read when the home shows (as Sowel's `AppLayout`, so the name shows even if `/ws` never opens), when the live connection opens (the first time and after every reconnection, with
  the other stores: a restored backup or a restart may change them), and again after the Settings
  page saves (its name, location or time zone may have changed). A failed read keeps the previous
  values (the name falls back to the default only when nothing was read yet).
- The sun times follow the aggregation store already fed by `/ws` (`zone.data.changed`); after a
  location is saved the core sends the root zone's new data.

### R4: size

The first load stays under the 170 KB gzip budget (`npm run check:bundle`); the bar adds about
3 KB (five Lucide icons, the components, two stores). The figure after this spec is written in
`docs/design-system.md`.

### R5: the sunrise for Paris

The box showed 08:14 and 17:00 for Paris on 2026-11-25. Checked during the spec:

- Sowel's calculation (`src/zones/sunlight-manager.ts`, `suncalc` 2.1, `getTimes`) for
  48.8566 N, 2.3522 E on 2026-11-25 gives 07:14:24 and 16:00:20 UTC, 08:14 and 17:00 in Paris
  (CET, UTC+1); New York (40.7128 N, 74.006 W) 06:54 and 16:31 (EST), Sydney 05:38 and 19:45
  (AEDT).
- An independent calculation (NOAA's approximation) gives 08:13 and 17:01 for Paris.
- The core's port of suncalc (`crates/core/src/settings/sun.rs`) already matches suncalc to the
  second on five cases (`crates/core/tests/sun.rs`).

So the core is right and agrees with Sowel; 07:58 is Paris's sunrise around 14 November, not on
the 25th. No change to the core. A test pins it: `sunlight` for Paris and for New York at
`2026-11-25T12:00:00.000Z`, with their time zones, gives "08:14"/"17:00" and "06:54"/"16:31".

## Living spec delta

### docs/living-spec/README.md, "Shows the home in the browser"

- Changed: "the equipments (Equipments, a room's "Add equipment", a device's "Create equipment")"
  → "the equipments (Equipments, a room's "Add equipment")".
- New bullet **Top bar** (after "Sidebar"): on every page of the home, the home's name (the
  Settings' name, "Home" when none, a grey placeholder while it loads) and where you are (the
  room's path, or the administration page); on the right the home's time when the browser sits in
  another time zone, the sunrise and sunset of the root zone with a sun by day and a moon by night
  ("Set the location", a link to Settings, when no location is set; nothing while the box's clock
  is not set), and the link indicator: nothing while connected and for 3 s after, then
  "Connecting..." or "Disconnected". On a phone the bar holds the menu button, the home's name with
  the page under it, and the indicators as icons.
- Changed, **Sidebar**: "on a phone, behind a menu button at the top" stays; the mark is no longer
  in the phone's top bar (it stays in the drawer).
- Changed, **Devices**: "; "Create equipment" on a device with none" goes; "except on the name
  editor, "Create equipment", delete and the links" → "except on the name editor, delete and the
  links". A device with no equipment shows "—".
- Changed, **Equipments**: "A device's "Create equipment" opens the form with the device ticked,
  its type guessed (...) and its name;" goes.
- Changed, **Live**: "when the connection is lost for 3 s a banner says so" → "when the connection
  is not open for 3 s (also at the first load) the top bar's link indicator says so:
  "Connecting..." while it tries, "Disconnected" between tries".

### No longer true

Searched in `docs/` (roadmap and decisions apart), `README.md`, `CLAUDE.md`, `inputs/README.md`,
`.claude/`, for "Create equipment", "createEquipment", "Banner", "banner", "connection lost",
"top bar", "MobileTopBar", "deduceType", "guessed":

- `docs/living-spec/README.md`: the five sentences above.
- `docs/design-system.md`, "Layout": "Below `md` the sidebar gives way to a top bar with a menu
  button that opens it as a drawer" and the code list `{Sidebar,ZoneTree,Banner}.tsx` → the top bar
  on every page, the code list with `TopBar.tsx` and the ported files; the ported-files table row of
  `DevicesPage`/`DeviceList`: "the table with the Equipment column, "Create equipment" and delete" →
  "the table with the Equipment column and delete"; the sentence "The Solite mark ... is the one the
  sidebar, the mobile top bar, the drawer and the browser tab show" → "the sidebar, the drawer and
  the browser tab show".
- `ui/src/api/devices.ts` header: "and so does "Create equipment" through `getDevice`" → removed
  (the device page reads it).
- Ported file headers that name the device's button (`DeviceList.tsx`, `EquipmentForm.tsx`,
  `NewEquipmentForm.tsx`): rewritten.
- `CLAUDE.md`'s "banner" is the program's start banner (`--test banner`), not this one: unchanged.

## Acceptance criteria

- [x] The Devices page shows no "Create equipment" on any row; a device with no equipment shows "—".
- [x] Discovery's new rows show no "Create equipment".
- [x] An equipment is still created from the Equipments page and from a room's "Add equipment",
      devices picked in the form (the e2e test does it through the Equipments page).
- [x] Every page of the home shows the top bar with the home's name ("Home" when unset) and the
      page; the room's path on a room's page.
- [x] With a location set and the clock set, the bar shows the sunrise and sunset of the root
      zone, a sun by day, a moon by night.
- [x] With no location, the bar shows "Set the location", which opens Settings.
- [x] With the browser in another time zone than the home's, the bar shows the home's time.
- [x] The live connection closed for less than 3 s shows nothing; longer shows the indicator; the
      old banner is gone.
- [x] Saving a new home name or location in Settings changes the bar without a reload.
- [x] `sunlight` for Paris on 2026-11-25 gives 08:14 and 17:00 (test).
- [x] First load under 170 KB gzip.
- [x] Screenshots of the changed screens (a room's page, the Devices page, the bar with no
      location, the bar with the home's time, the bar with "Disconnected") in Hybrid and Dark,
      desktop and phone width.

## Edge cases

- Settings not read yet: the skeleton in place of the name; no hint (`hasLocation` unknown).
- Settings read fails (403 while the token refreshes, network): the default name; no hint.
- `home.name` set to whitespace: the host trims it (spec 026); an empty answer shows the default.
- A very long home name: truncated (Sowel's `truncate`), the bar never wraps.
- No root zone aggregate yet: no sun times, no hint.
- A location set but the root zone's `sunrise` still null (clock not set): nothing.
- `sunrise` set but `sunset` null (never both in Solite: the core gives both or none): nothing, as
  Sowel's.
- Polar day or night: the core gives `"--:--"`; shown as "--:-- – --:--", as Sowel's.
- The home's time zone the same as the browser's under another spelling (`Etc/UTC` and `UTC`): no
  time (Sowel's canonical comparison).
- The time zone read fails: no time (Sowel's: `loaded` stays false).
- The connection alternates between connecting and closed during an outage: the indicator stays,
  its words follow the status.
- An unknown zone id in the address, or none: the context is the room the page shows.
- A device deleted from a row: unchanged (delete stays).

## Sowel contract

No API change. The interface reads `GET /api/v1/settings` (`home.name`, `home.latitude`,
`home.longitude`, Sowel's keys, spec 026), `GET /api/v1/system/timezone` (Sowel's `tz`, `source`,
`offsetHours`, D44) and the root zone's `sunrise`, `sunset`, `isDaylight` (Sowel's
`ZoneAggregatedData`, `src/shared/types.ts`). The ported files are Sowel's
`ui/src/components/layout/{TopBarFrame,SunTimes,HomeTimeNote,ConnectionStatus}.tsx`, the header of
`AppLayout.tsx` (`TopbarBreadcrumb`, `MobileTopbarTitle`, `HomeNameSkeleton`),
`ui/src/hooks/useTopBarState.ts` and `ui/src/store/useTimezone.ts` at Sowel commit `2d360726`.
Divergences (D62): the `md` switch point, the "Set the location" link, no pills, no user block.

## Decisions taken in this spec

- **D61**: no equipment is created from a device; the Devices and Discovery rows show "—" and
  delete only; Discovery offers no next step after pairing (an inbox line: a Sowel-style hint
  after pairing that links to Equipments) (replaces spec 019 R4's button and roadmap J2's "Create equipment from a new device's
  row"; D30's "the equipment is the entry point" now holds without exception).
- **D62**: Sowel's top bar is ported, with Solite's `md` switch point, the link indicator in place
  of the Banner (same 3 s rule), the home's time only when the browser is in another zone (Sowel's
  `HomeTimeNote`, which replaced the `CurrentTimePill` the inbox line named), and a "Set the
  location" link where Sowel shows nothing.
- The e2e test creates the light and the sensors from the Equipments page; its check that no
  "Create equipment" remains looks at the table's rows only (the form's title and the Equipments
  page's button keep the words).

## Spec review leftovers

Round 1's minor items were all applied (the form's re-read tests rewritten rather than removed,
`hasLocation` nullable, the root page's phone sub-line, the compact home time, the hint's tap area
and phone rule, `hit-group` dropped, the e2e check's scope, the test instant, two more screenshots,
the safe-area note).

## How the user will check it

On the Mac, `cargo run -p solite-host -- --sim scenarios/garage.json --serve --speed 10` and the
interface at `http://127.0.0.1:8080/`: the bar shows "Home" and the room; "Set the location"
until Settings has one; once set, the sun times; the browser's time zone changed (Playwright
`timezoneId`) shows the home's time; stopping the host shows "Disconnected" after 3 s. The Devices
page has no "Create equipment". Screenshots in the session's scratchpad, Hybrid and Dark, desktop
(1280 px) and phone (390 px). No box run: interface only, and the sun check is a Mac test.
