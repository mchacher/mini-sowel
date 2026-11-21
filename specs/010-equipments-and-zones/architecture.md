# Architecture: Spec 010

## Overview

```
crates/core/src/zones/       Zone, ZoneWithChildren, tree rules (root, move, delete), Zones store
crates/core/src/zones/aggregate.rs   ZoneAggregatedData from equipments' bindings, Sowel's rules
crates/core/src/equipments/  Equipment, DataBinding, OrderBinding, *WithValue/*WithDetails,
                             auto-binding, status and stale (Sowel's windows), bounds
crates/core/src/api/         + /zones, /zones/aggregation, /equipments, bindings; devices zoneId
crates/host/src/store.rs     + zones.json, equipments.json (same writer and policy as devices.json)
```

The registry of spec 008 becomes part of a `Home` (devices, zones, equipments) behind the one
lock; the API and the radio loop work on it as before; aggregation is recomputed after each change
(cheap: at most 32 zones × 64 equipments).

## Data

| Name | Type | Where |
| ---- | ---- | ----- |
| `zones.json` | `Zone[]` | data folder |
| `equipments.json` | equipments with their bindings | data folder |
| Zones | at most 32 | `zones/` |
| Equipments | at most 64, 16 data bindings and 16 order bindings each | `equipments/` |

## Files changed

| File | Change |
| ---- | ------ |
| `crates/core/src/{zones,equipments}/**`, `api/mod.rs`, `lib.rs` | New and extended |
| `crates/core/src/devices/registry.rs` | `zoneId`; removal hook for bindings |
| `crates/host/src/{store,serve}.rs` | Files, `Home` |
| `crates/core/tests/{zones,equipments,aggregate,sowel_json,api}.rs`, `crates/host/tests/serve.rs` | Tests |
| `crates/core/tests/memory_ceiling.rs` | The realistic home also has 10 zones and 50 equipments; peak < 200 KB |
| docs | Living spec, architecture, decision D14 |
