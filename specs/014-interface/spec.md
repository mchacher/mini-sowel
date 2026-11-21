# Spec 014: interface

## Context

Increment B8 of the roadmap: the interface. The host serves the built interface, which shows the
rooms (zones) with their state (temperature, humidity, motion, lights) and their equipments, live
through spec 013's WebSocket, and lets the user switch a light. It looks like Sowel's home screen
because it **ports Sowel's components and stores** (design-system rule "ported, not redrawn",
decision D4), but not Sowel's whole page: the survey of Sowel's interface,
[sowel-ui-reference.md](sowel-ui-reference.md), shows that Sowel's `HomePage` pulls about 35
modules, the 11-store WebSocket, i18next with 67 KB of locales and a router, which do not fit the
170 KB budget (its option B is the one chosen).

Sowel files ported (sources and sizes in the reference §1, §5 option B): `components/ui/Switch.tsx`,
`lib/equipment-family.ts`, `components/equipments/{LightControl,SensorValues,sensorUtils,
EquipmentStatusBadge}.tsx`, `components/home/{ZoneHeroLead,Panel}.tsx`, `store/{useZones,
useEquipments,useZoneAggregation}.ts`, `hooks/useWsSubscription.ts`, `api/client.ts`; from
`../sowel/ui/src`, Sowel commit `5da84dc1` (UI 1.87.0), read on 2026-11-21; plus
`components/home/ZoneAggregationPills.tsx` and `theme.ts` (R9).

Specified in autonomy (no questions); choices under "Decided in autonomy".

## Goals

1. `--serve` serves the interface at `http://127.0.0.1:8080/`.
2. The user picks a room and sees its state and equipments, updated live.
3. A light's switch sends the order and shows the new state when the device confirms it.

## Out of scope

- Editing zones, equipments and devices in the interface (the API does it); recipes, devices and
  pairing screens; modes, history, energy, activity (not in the tool).
- Authentication (D13). The box serving the interface (milestone E).

## Requirements

### R1: Served by the host

- `--serve` serves the files of `ui/dist` (the interface's build output) from a folder given by
  `--ui <folder>`, default `ui/dist` relative to the working directory: `GET /` gives
  `index.html`, `GET /assets/<file>` the file, with its content type (`text/html`, `text/css`,
  `application/javascript`, `image/svg+xml`, `font/woff2`; others `application/octet-stream`),
  read from disk in chunks (never whole in memory). A path with `..`, a backslash or a
  percent-encoded dot: 404. A missing folder: the API still serves, `GET /` answers 404
  `{"error":"Interface not built: run npm run build in ui/"}` and the host prints a line saying
  so at start.
- `Cache-Control: no-cache` on `index.html`, `public, max-age=31536000, immutable` on
  `/assets/*` (their names carry Vite's hash).
- The `Host` check of the API applies to these files too.

### R2: Development

`cd ui && npm run dev` serves on `:5173` and proxies `/api` and `/ws` to `127.0.0.1:8080`
(Vite's `server.proxy`, `changeOrigin`, `ws: true`), so the `Host` check passes and the
`Origin` `http://localhost:5173` is the one spec 013 accepts.

### R3: The screen

One page, no router:

- **Sidebar** (mini-Sowel's): the logo, then the zones as Sowel's tree, indented by depth; the
  selected zone highlighted; the selection kept in `location.hash` (`#/zone/<id>`), the first
  zone with equipments chosen when there is none, and the root zone when there are no others.
- **Zone header**: Sowel's `ZoneHeroLead` (the zone's name and its aggregate line: temperature,
  humidity, luminosity, motion, lights on / total), with the aggregate of `GET
  /api/v1/zones/aggregation` and `zone.data.changed`.
- **Equipments of the zone** (not of its sub-zones, as Sowel's home screen), in Sowel's family
  order (`equipment-family.ts`): a `light_onoff` with Sowel's `LightControl` (power button, state
  from its `state` binding); a `sensor` with Sowel's `SensorValues` (temperature, humidity,
  battery, occupancy...); any other type with its name, its icon and `EquipmentStatusBadge`
  only. A disabled equipment is shown dimmed, without its control.
- **Empty states**: no zone: "No room yet: create one with the API" (with the French text); a zone
  with no equipment: "No equipment in this room".

### R4: Live

- Stores ported from Sowel (`useZones`, `useEquipments`, `useZoneAggregation`) fetch at start and
  apply the events: `zone.created|updated|removed` (refetch zones), `zone.data.changed`
  (aggregate), `equipment.created|updated|removed` and `equipment.status.changed` (refetch
  equipments, coalesced 1 s as Sowel), `equipment.data.changed` (the binding's value).
- A WebSocket store of mini-Sowel's (Sowel's is an 11-store monolith): connects to
  `ws(s)://<location.host>/ws` with the subprotocol `bearer.none`, subscribes to
  `["zones","equipments"]`, accepts an array or a single event, ignores what it cannot parse;
  on close, reconnects after Sowel's `min(1000 × 2^n, 30000) + random(0..500)` ms, and on each
  open refetches zones, equipments and the aggregate (events may have been lost: spec 013 closes
  slow clients); also reconnects when the page becomes visible and when the browser goes online.
- A banner "Connection lost, reconnecting…" while the socket is closed for more than 3 s.

### R5: Orders

The light's button calls `POST /api/v1/equipments/:id/orders/state` with `{"value": true|false}`
(Sowel's `executeOrder`). While waiting the button shows Sowel's pending state; the state shown
changes only with `equipment.data.changed` (no optimistic value, spec 011). A 502 or 503 shows the
error's text under the equipment for 5 s.

### R6: Language

A small `t()` of mini-Sowel's with the same keys as Sowel's `en.json` / `fr.json`, only the keys
the ported components use, copied with their texts; the language is French when
`navigator.language` starts with `fr`, English otherwise. No i18next.

### R7: Budget and look

- First load at most **80 KB gzip** (the budget check stays at 170 KB; 80 KB is this increment's
  target, measured and written in the spec at the end).
- Sowel's tokens and classes only (ESLint design rules); icons from Lucide by name, 16 px,
  `strokeWidth={1.5}`; Hybrid and Dark themes from `prefers-color-scheme`.
- New dependency: `zustand` (Sowel's stores use it). Each ported file starts with a header naming
  its Sowel source; what was changed in it is listed in `docs/design-system.md`'s table.

### R8: Where it runs

Interface and host. The firmware does not serve the interface (E). The demo runs with the
simulator; with the real Dongle Max later.

### R9: Precisions after the spec review (they override R1 to R8 where they differ)

1. **Zone header**: the page's `<h1>` is the zone's name; Sowel's `ZoneAggregationPills` is
   ported with its sparkline cut (no history in the tool) and shows temperature, humidity,
   luminosity, motion; `ZoneHeroLead` stays, for its lights line ("1 light on", "all lights off").
2. **Light orders**: `LightControl` ported unchanged, **compact** variant (as Sowel's home cards):
   it sends `{"value":"ON"}` / `{"value":"OFF"}` (the binding has no `enumValues`), which the API
   accepts (spec 011); the button is disabled while the order waits. `light_dimmable` gets no
   control (its slider needs classes the theme does not have yet): name, icon and badge only.
3. **Every Sowel file touched is listed** in a table, one row per file: ported unchanged, ported
   with its cuts named, or replaced by a mini-Sowel file (with `types.ts`: a named subset of its
   types, copied unchanged; the icons of other types from Sowel's type metadata). The builder
   reports it; it goes into this spec ("Added while building") and into `docs/design-system.md`.
4. **`t()`**: `react-i18next` is aliased (Vite and TypeScript) to a mini-Sowel module exporting
   `useTranslation` and the `TFunction` type, so ported files stay unchanged; `t(key, {count,
   defaultValue, ...vars})` interpolates `{{var}}`, chooses `_one` / `_other` with
   `Intl.PluralRules`, falls back to `defaultValue`, then to the key. `<html lang>` follows the
   language.
5. **Empty states**: the root zone always exists. Only the root zone: it is selected and the page
   shows "No room yet: create one with the API" / « Aucune pièce : créez-en une avec l'API ». A
   zone with no equipment: "No equipment in this room" / « Aucun équipement dans cette pièce ».
   Banner: "Connection lost, reconnecting…" / « Connexion perdue, reconnexion… ». A store whose
   fetch failed: "Cannot reach mini-Sowel" / « Impossible de joindre mini-Sowel ». "First zone
   with equipments": depth-first order of `GET /zones`.
6. **Start-up load**: the stores fetch only when the socket opens (the first open is the start),
   so at most 3 GETs at once. The ported `client.ts` retries a GET answered 503 once after 1 s,
   as it does for 429 (a listed change).
7. **Static files in the core**: after its `Host` check, the core answers `/` and `/assets/<name>`
   with `Reply::Static(<relative file>, <content type>)`; the path checks (`..`, backslash, any
   `%`, empty name) and the content-type table are core logic, unit-tested; other methods: 405; a
   query string is ignored. The host opens the file under `--ui` and streams it (a `File` reader,
   `Content-Length` = its size). R8: core, host and interface.
8. **The start line**: `interface: <folder> not found, run npm run build in ui/` (the folder as
   given), also for a folder without `index.html`. `--ui` without `--serve`: the usage error,
   exit 2, as the other options.
9. **Security headers** on every static answer: `X-Frame-Options: DENY`,
   `Content-Security-Policy: frame-ancestors 'none'`, `X-Content-Type-Options: nosniff` (another
   site framing the page could make it switch a light).
10. **Themes**: Sowel's `theme.ts` ported: `data-theme` follows `prefers-color-scheme` (dark →
    `dark`, else `hybrid`) and its changes; `index.html` keeps `hybrid` until the script runs.
11. **Demo script**: `scripts/demo-garage.sh [port]` sets up, through the API, spec 010's garage
    (the zone, the three simulated devices placed in it, the sensor, motion and light equipments)
    and spec 012's instance (timeout `20s`), and prints the ids; it refuses to run when a zone
    named "Garage" exists. Used by the demos of specs 010 to 014.
12. `useWsSubscription` is not ported: the live store subscribes to a fixed list. Order errors
    show the server's text (English), accepted.

## Living spec delta

### docs/living-spec/README.md

- New section "Shows the home in the browser": the address, the sidebar of rooms, the zone
  header, the equipments and their controls, live updates and reconnection, the language, the
  `--ui` option and the "not built" answer.
- "Serves Sowel's device API": the routes table gains `GET /` and `GET /assets/*`.
- History row 014.

### No longer true

- `docs/living-spec/README.md`: the bullet "In the browser: … a page titled `mini-sowel`. It
  follows Sowel's light theme (Hybrid)" becomes "In the browser: the home screen (see "Shows the
  home in the browser"), in Sowel's Hybrid or Dark theme as the system asks"; the closing sentence
  (the host serves the interface).
- `docs/design-system.md`: the Source table gains the ported files (commit `5da84dc1`, the table
  of R9.3); "Layout" ("its title in text-display", `App.tsx`); "the theme is chosen by
  `data-theme`… Hybrid is the default"; "Updating from Sowel: copy the four files again"; the
  "Embedded: what changes from Sowel" table (i18next → `t()`, router → hash, `useWebSocket` → the
  live store, `ZoneAggregationPills` without sparkline).
- `docs/roadmap.md`: milestone H's "interface languages" (French and English come now; H keeps
  the others); inbox: "Interface: recipes, devices and pairing screens; editing zones and
  equipments", "The box serves the interface precompressed (`.gz`)".
- `docs/decisions.md`: D19 records the choices of "Decided in autonomy".
- `docs/architecture.md`: the interface block, the host serving `ui/dist`.
- `README.md`, `CLAUDE.md`: the commands (`--serve` now opens the interface; `npm run dev` with
  the host running).
- `docs/roadmap.md`: B8's status.
- Searched: `ui/`, `interface`, `5173`, `dist`, `App.tsx`, `B8`, `data-theme`, `Hybrid`, `languages`.

## Acceptance criteria

- [x] `cd ui && npm run build`, then `--sim scenarios/garage.json --serve --speed 10` with spec
      010's garage and spec 012's instance (timeout `20s`), host restarted: `http://127.0.0.1:8080/`
      shows the garage with its temperature; at about 6 s the motion and the light's state turn
      on, at about 44 s off, without reloading; the light's button switches it. Screenshots in the
      Hybrid and Dark themes.
- [x] The first load measured at 80 KB gzip or less.
- [ ] Real Dongle Max (pending): the same with the real devices.

## Edge cases

- No zone; a zone without equipment; a zone deleted while shown (back to the first zone); an
  unknown zone id in the hash.
- The host stopped then started: the banner, then everything refetched.
- An order answered 502 / 503: the text shown 5 s.
- A disabled equipment; an equipment offline (Sowel's badge).
- `ui/dist` missing; a path with `..`; an unknown asset.
- A 5th tab (spec 013: 503 on `/ws`): the banner, retries with the backoff.

## Decided in autonomy

- Sowel's survey option B (port leaf components and stores, write a small page) over porting
  Sowel's `HomePage` (over budget) or redrawing (against D4).
- No router (hash), no i18next (a `t()` with Sowel's keys), a WebSocket store of mini-Sowel's.
- The host serves `ui/dist` from disk on the Mac; the box will embed it (E).
- 80 KB target, leaving room for the next screens.

## Added while building

1. The Sowel files touched, and how, are the table under "Source" in `docs/design-system.md`
   (R9.3). `Switch.tsx` is not ported: nothing uses it.
2. Vitest needs two test-only aliases (zustand inlined, `lucide-react` to its ESM build) so
   that Preact, not React, is loaded.
3. The empty states show only after the first fetch (no flash of "No room yet" at load). The
   family labels (12 keys) joined the dictionary; mini-Sowel's own keys are `app.*`.
4. `/` and `/assets/<name>` are routed before the 413 and 415 checks (a POST to `/` is 405);
   static 404s carry the security headers too; `tiny_http`'s chunking is turned off for files so
   `Content-Length` holds; `index.html` has an empty favicon (no 404 on `/favicon.ico`).
5. Sowel's `coalesceCalls` refetches at once then once more at the end of the 1 s window.
6. The demo needs the devices paired first (`--pair 30`); the script says so otherwise.
7. First load **34.3 KB gzip** (JS 27.3, CSS 6.7, HTML 0.3).
8. After the code review: a malformed hash shows the first zone; below `md` the sidebar is hidden
   behind a top bar with a menu button opening the rooms in a drawer (Sowel's pattern, which
   breaks at `lg`); the demo script shows HTTP errors and, after a failure, the command that
   deletes the zone it created; every answer on `/` and `/assets/*`, refusals included, carries
   the security headers (the 403 of a foreign `Host` excepted); a symlink in the interface's
   folder is refused (404); a network error on an order shows the translated "Cannot reach
   mini-Sowel"; first load 35.0 KB gzip.
9. Demo on the simulator, 2026-11-21 (port 8090, the Mac's 8080 being taken; French browser):
   at 10 s the garage shows "1 lumière allumée", 12,0° and 60 %, motion detected, the light's
   button amber; at 50 s "lumière éteinte", "Calme"; in Hybrid and Dark; a press on the light's
   button switched it within 0.3 s through the API and the device's read-back.

## How the user will check it

```
cd ui && npm run build && cd ..
cargo run -p mini-sowel-host -- --sim scenarios/garage.json --pair 30 --speed 10  # once: the devices join
cargo run -p mini-sowel-host -- --sim scenarios/garage.json --serve --speed 10   # to set up
sh scripts/demo-garage.sh                                                         # in another terminal; then Ctrl-C the host
cargo run -p mini-sowel-host -- --sim scenarios/garage.json --serve --speed 10   # open the page at once
```

`http://127.0.0.1:8080/`: the garage; motions at about 6 s and 18 s, the light on at about 6 s,
off at about 44 s, without reloading; press the light's button.

Run on the real hardware (2026-11-21): The interface served by the host (port 8090, 8080 being taken) showed the Garage, its motion and its light live; the switch in the interface drove the relay (confirmed by the user). Another program listening on the same port in IPv6 captured `localhost`; the interface was reached at 127.0.0.1 (open defect: the host binds IPv4 only and does not notice the clash).
