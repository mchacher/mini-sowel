# Sowel's interface: what B8 ports (survey of Sowel's ui/, with sources)

Paths: S = `../sowel/ui/src` (read on 2026-11-21), M = this repository.
Sizes are estimates (inferred) unless a measure is given. No build was run.

## 1. Where zones -> equipments live in Sowel

Routes (S/App.tsx:55-140): `/home` and `/home/:zoneId` -> `pages/HomePage.tsx` (451 lines) is the
screen "zone with aggregated data + equipments". `/dashboard` -> DashboardPage (widgets, user-configurable,
not needed). `/zones/:id` -> ZoneDetailPage (admin edit view). Everything is wrapped by `ProtectedRoute`
(S/App.tsx:62-66) and `AppLayout` (791 lines).

HomePage imports (S/pages/HomePage.tsx:1-29): react-i18next, react-router-dom (useParams/useNavigate),
lucide, stores useZones, useEquipments, useAuth, useZoneAggregation, useUiState, useRecipes, useModes;
api `executeZoneOrder`, `getHistoryStatus`; components Panel, ZoneEquipmentsView, ZoneAggregationPills,
ZoneHeroLead, ZoneCommands, ZoneRecipesSection, ZoneModesSection (1078 lines), ActivityPanel,
EquipmentForm, bindingUtils; hook useWsSubscription. Mount fetches (HomePage.tsx:54-57): fetchZones,
fetchEquipments, fetchAggregation, getHistoryStatus. Subscribes to topics `zones, equipments, modes,
recipes, activity` (HomePage.tsx:32).

Leaf chain for the tile (what a minimal screen needs):
- `components/home/ZoneEquipmentsView.tsx` (112) -> `CompactEquipmentCard` (841 lines) + `Panel.tsx` (114,
  SliceHeader) + `lib/equipment-family.ts` (154, pure) + `icons/ShutterIcons`.
- `CompactEquipmentCard.tsx:1-59` imports ~35 modules: every control (Solar, Shutter, Thermostat, Gate,
  TimedCommand, Heater, WaterValve, Vmc, EvCharger, PoolHeatPump), weather utils, appliance, energy meter,
  camera snapshot hook, `equipment-row-layout` (uses react-router `Link`). This is the transitive
  explosion: importing it as-is pulls most of `components/equipments/`, `lib/` (format.ts 602 lines ->
  i18n), `types.ts` (2213 lines, type-only, free), and `../../../../src/shared/door-state` (S/components/home/row-state-icon.tsx:8,
  useEquipmentState.ts:7-11): a file OUTSIDE ui/, in Sowel's backend `src/shared/` (needs copying or an alias).
- Light toggle: `components/equipments/LightControl.tsx` (172): power button `Power` icon, classes
  `bg-active text-on-active` (LightControl.tsx:112-120); imports useSliderOverride, bindingUtils
  (901 lines, imports `../../api` and ev-charger/electric-vehicle contracts), brightness, format.
  Order call: `useEquipments.executeOrder` (S/store/useEquipments.ts:97) -> `executeEquipmentOrder`
  (S/api/equipments.ts:110-118) = `POST /api/v1/equipments/:id/orders/:alias` body `{"value": ...}`.
- Sensors: `components/equipments/SensorValues.tsx` (150) + `sensorUtils.tsx` (321, lucide icons) +
  `useEquipmentState.ts` (202; imports `useWebSocket` store, thermostat-state, gate-confirm, door-state,
  ShutterIcon). Status: `EquipmentStatusBadge.tsx` (88).
- Zone summary: `ZoneAggregationPills.tsx` (476; imports `ZoneSparkline` which calls
  `getZoneSparklineData` = history API, not in mini-Sowel; shutter + water valve icons) and
  `ZoneHeroLead.tsx` (79, only i18n + types, easy).
- Switch: `components/ui/Switch.tsx` (50, no imports, portable as-is).

Stores (zustand, S/store/): `useZones` (86; `tree`, `fetchZones`, handlers for zone.created/updated/removed),
`useEquipments` (190; imports useOrderTiming, coalesceCalls, api-error), `useZoneAggregation` (38;
`data` keyed by zoneId, `handleZoneDataChanged`). Pure and small; they depend on `../api`
(index.ts barrel re-exporting 16 modules) and `lib/api-error`.

WebSocket: `store/useWebSocket.ts` (645 lines) is a monolith: imports 11 stores (devices, zones,
equipments, orderOutcomes, zoneAggregation, recipes, modes, activity, arbiter, evSessions, sharedAccess,
:3-16) plus battery/PV alerts and health API. Topics type (:18-27): devices, equipments, zones, modes,
recipes, calendar, system, activity, energy. `getWsUrl` (:307-311): in DEV uses `hostname:3000`, else
`window.location.host` + `/ws`. Subprotocol `bearer.<localStorage sowel_access_token>` (:314-316, :525).
Reads arrays or single events (:600-609, matches mini-Sowel's batched arrays). On open it refetches
devices, equipments, zones, aggregation, recipes, instances, modes, arbiter, activity, health,
battery alerts, PV health (:527-545 onwards). `hooks/useWsSubscription.ts` (24) is tiny and pure.

API client (S/api/client.ts, 143 lines): `API_BASE="/api/v1"` (:6), Bearer token from module variable
(:9-22, :74-77), 401 -> `_onUnauthorized` refresh (:111-124), 429 retry for GET (:101-109), `ApiError`.
Login flow: `LoginPage`, `SetupPage`, `store/useAuth.ts` (217), `ProtectedRoute` (27: redirects to /login
when no user). Token key `sowel_access_token`.

i18n (S/i18n/index.ts, 34 lines): i18next + react-i18next + browser-languagedetector, `en.json` 132 KB
(32.2 KB gzip, measured) and `fr.json` 143 KB (35.2 KB gzip, measured). Both bundled statically.
Icons: lucide-react by name, `strokeWidth={1.5}`.

## 2. Dependencies and preact/compat (sizes inferred from knowledge, not measured)

| Dependency | gzip (inferred) | preact/compat | Note |
| --- | --- | --- | --- |
| zustand 5 | ~1.2 KB | yes (inferred; uses `useSyncExternalStore`, which compat provides) | stores portable unchanged |
| lucide-react | ~0.3-0.5 KB per icon (inferred) | yes, already in mini-Sowel's package.json | by name only |
| react-i18next + i18next + detector | ~5 + 14 + 2 = ~21 KB, plus locales | works (inferred; commonly used with Preact) | locale JSON is the real cost: 32 KB per language |
| react-router-dom 7 | ~25-30 KB (inferred) | risky (inferred): v7 relies on newer React APIs (startTransition, use); preact/compat 11 may cover it, test | needed only for `Link`, `useParams`, `useNavigate` |
| recharts 3 | ~100 KB (inferred) | works but heavy | pulled by history/energy components only; leave out |
| @dnd-kit, hls.js, qrcode, vite-plugin-pwa | n/a | n/a | not needed (camera, drag, QR, PWA) |
| react-dom/react | 0 | replaced by preact (4 KB) | already aliased in M/ui/tsconfig.app.json:17-20 |

Not used by the minimal screen: recharts (only via ZoneSparkline? ZoneSparkline.tsx:1-2 imports only
react hooks and the history API, so it is an API dependency, not a chart lib), hls.js, dnd-kit.

## 3. Sowel-only features and stubs

| Feature | Where | Stub |
| --- | --- | --- |
| Auth, login, refresh | client.ts:74-124, useAuth, ProtectedRoute, LoginPage | mini-Sowel serves 127.0.0.1 without auth (M/docs/living-spec/README.md:169). Drop ProtectedRoute, no `setAccessToken`; keep `fetchJSON` (or write a 20-line one) |
| `useAuth.user` in HomePage (HomePage.tsx:44) | admin gating of "add equipment" | stub store returning an admin-less user, or remove |
| Modes (ZoneModesSection 1078 lines, useModes) | HomePage.tsx:19,89 | remove the section; `/api/v1/modes` not served (inferred, not in spec) |
| Recipes UI | ZoneRecipesSection, useRecipes | optional: mini-Sowel serves `/recipes`, `/recipe-instances`; start without |
| Energy / arbiter / EV / shared access | useWebSocket.ts:12-16 | the monolithic store forces them in. Cut: write a small `useWebSocket` (topics devices, zones, equipments, recipes) |
| History / sparklines | getHistoryStatus (HomePage.tsx:57), ZoneSparkline | not served: stub `getHistoryStatus` returning "disabled" or drop `ZoneSparkline` |
| Health, battery, PV alerts | useWebSocket onopen | not served (inferred): drop |
| AppLayout (791 lines) | settings, plugins, energy, updates, wizard, shadow mode | replace with mini-Sowel's own Sidebar (M/ui/src/components/layout/Sidebar.tsx, 16 lines) |
| `../../../../src/shared/door-state` | row-state-icon.tsx:8 | copy the file or drop the door case |
| WS dev host `:3000` | useWebSocket.ts:307 | mini-Sowel uses :8080 (M spec: Origin check allows the interface's dev server) |

## 4. REST and WS used vs what mini-Sowel serves

Used by the minimal screen: `GET /zones` (api/zones.ts:8), `GET /zones/aggregation` (:12), `GET /equipments`
(equipments.ts:22), `POST /equipments/:id/orders/:alias` (:110). HomePage also calls `GET /history/status`
(not served), `executeZoneOrder` (zone-level orders, not in mini-Sowel spec, inferred), and the store
modes/recipes/activity fetches.

mini-Sowel serves (M/docs/living-spec/README.md:172-180, 211-245): devices, zones (+ aggregation,
tree via `GET /api/v1/zones`), equipments, orders with `{"value"}` and aliases `state`/`brightness`,
recipes and instances, `GET /ws`. Not served: auth, modes, history, activity, energy, settings, health.
WS: topics `devices, zones, equipments, recipes` are fed (README:198-199); `modes`, `activity`, `system`
extras are ignored. Events are batched arrays every 200 ms, which Sowel's client already handles
(useWebSocket.ts:600-609). Limits to respect in the UI: 4 WS clients, messages from client over 1 KB close
the socket, Origin must be the API address or the dev server (README:203), no token needed.
`GET /zones` answer: zone tree, as Sowel (inferred from "Sowel's rules" in README:211).

## 5. Options for B8

A. Port Sowel's HomePage as-is with stubs. Needs HomePage + CompactEquipmentCard + ~35 modules,
   bindingUtils, format.ts, i18n with both locales, react-router, useWebSocket monolith with 11 stores to
   stub, shared/ files copied. Bundle (inferred): preact 4 + router 28 + i18n 21 + locales 67 + app code
   ~60-80 + icons ~10 = 190-210 KB gzip: over the 170 KB budget unless only one locale and routing are cut.
   Effort: highest (many stubs to keep in sync, failing types). Not recommended.

B. Port leaf components and write a small page (recommended). Copy unchanged: `Switch`, `equipment-family.ts`,
   `LightControl` (+ `useSliderOverride`, `brightness`, a trimmed `bindingUtils`), `SensorValues`
   + `sensorUtils` + `EquipmentStatusBadge`, `ZoneHeroLead`, `Panel`, the three small stores (`useZones`,
   `useEquipments`, `useZoneAggregation`), `useWsSubscription`, `client.ts`. Write: a ~150-line `HomePage` (zone
   list, zone selected in state or a hash, ZoneEquipmentsView-like grid), a ~120-line `useWebSocket` for
   four topics, and an `api/` of 4 functions. i18n: keep `react-i18next`-compatible `t()` through a tiny
   own shim (flat dictionary of ~60 keys, French and English) instead of i18next + 67 KB of locales.
   No router (state + `location.hash`). Bundle (inferred): preact 4 + zustand 1 + icons ~8 + app ~25 + CSS ~8
   + current 10 = 55-80 KB gzip, headroom of 90 KB. Effort: medium (about 3-4 days of work, biggest
   risk: `bindingUtils` is 901 lines with ev/energy ties, so cut it to the 3 functions `findMainOnOffOrder`,
   `findOrderByCategory`, `relayStateOn`, and say so in the spec as the design system rule requires,
   M/docs/design-system.md "A Sowel component is ported, not redrawn ... say what changed in the spec").

C. Write from scratch with Sowel's tokens. Bundle 40-60 KB gzip. Effort: lowest to start, but diverges from
   the design-system rule "ported, not redrawn" (M/docs/design-system.md, Rules) and the visuals drift from Sowel.

Recommendation: B. It keeps Sowel's look and classes where the code is small and pure, avoids the 35-module
CompactEquipmentCard and the useWebSocket monolith, and leaves ~90 KB of headroom for B9+ (recipes tile).

## 6. mini-Sowel ui/ today

- `M/ui/package.json`: only `preact` ^11.2 and `lucide-react` as runtime deps; Vite 8, Tailwind 4,
  `@preact/preset-vite`, Vitest 5 with `@testing-library/preact`. No zustand, no router, no i18n.
- `M/ui/vite.config.ts`: preact + tailwind plugins, target es2022, NO dev proxy to :8080 and no `base`.
  The host does not yet serve the static `dist/` (inferred: no such route in README table), so B8 must add
  that, or run Vite on :5173 (the allowed `/ws` Origin) and fetch `http://127.0.0.1:8080` with a proxy.
  A proxy (`/api`, `/ws`) is needed: the Host check refuses another site, and `client.ts` uses relative `/api/v1`.
- `M/ui/src/`: `App.tsx` (13 lines: Sidebar + title), `main.tsx`, `constants.ts`, `components/layout/Sidebar.tsx`
  (16), `SowelLogo.tsx`, `styles/{tokens,theme,embedded}.css`, `App.test.tsx`.
- Build today (measured, M/ui/dist/assets): JS 14.1 KB raw / 5.8 KB gzip, CSS 13.5 KB raw / 4.4 KB gzip.
  Budget check `ui/scripts/check-bundle-budget.mjs:8` = 170 KB gzip over index.html + referenced js/css only
  (dynamic chunks not in index.html are not counted, inferred from the regex at :11), run by `scripts/validate.sh:52-53`.
  First load is about 10.5 KB of 170 KB: headroom ~159 KB.
- Design system: icons Lucide by name 16 px `strokeWidth={1.5}`, system fonts (no @fontsource), ESLint
  `design-classes.js` refuses raw hex and off-scale sizes (M/docs/design-system.md:Rules).
- CLAUDE.md "Interface": Sowel's tokens and Tailwind classes only; a Sowel component is ported unchanged through
  `preact/compat` when possible.
- Open points to decide in the spec: static serving of `dist/` by the host (path, memory, ETag), dev proxy,
  French only vs i18n shim, zone navigation without router, handling of a WS drop (reconnect plus refetch, as
  useWebSocket.ts:527-545 does).
