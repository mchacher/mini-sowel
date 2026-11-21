# Architecture: Spec 014

## Overview

```
browser ──GET /, /assets/*──► host serve.rs ──► files of ui/dist (read in chunks)
        ──/api/v1/...───────► API (specs 008-012)
        ──/ws───────────────► live events (spec 013)

ui/src
  main.tsx → App.tsx: Sidebar (zones tree) + ZonePage (hash #/zone/<id>)
  store/   useZones, useEquipments, useZoneAggregation (ported), useLive (mini-Sowel's WebSocket)
  components/  ported: Switch, Panel, ZoneHeroLead, LightControl, SensorValues, sensorUtils,
               EquipmentStatusBadge; equipment-family.ts
  i18n/    t() with Sowel's keys (fr, en)
  api/     client.ts (ported, no token), zones.ts, equipments.ts (4 calls)
```

## Data

No new data on the host. In the browser: the three stores (zustand) and the socket state; the
selected zone in `location.hash`.

## Files changed

| File | Change |
| ---- | ------ |
| `crates/host/src/serve.rs`, new `crates/host/src/static_files.rs` | `GET /`, `/assets/*`, content types, cache headers, path checks, `--ui` |
| `crates/host/src/main.rs` | the `--ui <folder>` option |
| `crates/core/src/api/mod.rs`, new `api/static_files.rs` | `Reply::Static` for `/` and `/assets/*`: path checks, content types, 405 |
| `scripts/demo-garage.sh` | the demo set-up (R9.11) |
| `ui/src/theme.ts`, `ui/src/components/home/ZoneAggregationPills.tsx` | ported (R9.1, R9.10) |
| `ui/package.json`, `ui/vite.config.ts` | `zustand`; the dev proxy |
| `ui/src/**` | the page, the ported files, the live store, `t()` |
| `ui/src/**/*.test.tsx` | Vitest tests of the plan |
| `crates/host/tests/static_files.rs` (new) | host tests of the plan |
| docs | living spec, design system, architecture, README, CLAUDE.md, roadmap |

## Reused

- Sowel's components and stores listed in the spec, unchanged where possible (the reference §5 B
  lists the cuts: `bindingUtils` reduced to `findMainOnOffOrder`, `findOrderByCategory`,
  `relayStateOn`; `useEquipments` without order timing; `client.ts` without token and refresh).
- mini-Sowel's `Sidebar`, `SowelLogo`, tokens, theme, ESLint design rules, budget check.
- The host's chunked responses (D15) for the files.
