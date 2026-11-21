# Architecture: Spec 015

## Overview

```
Sonoff docs, community ──► docs/hardware.md
core bounds (MAX_*) ─────► memory_worst_case.rs ──► peak ──► docs/memory-budget.md
ESP-IDF estimates ───────────────────────────────────────► docs/memory-budget.md
sdkconfig.defaults + partitions.csv ──► firmware image ──► scripts/qemu.sh (boots)
{"link": ...} + cable ──► core::network::choose ──► Link (used by spec 016, E3)
```

## Data

| Name | Type | Where |
| ---- | ---- | ----- |
| `NetworkSetting` | enum `Auto`, `Wifi`, `Ethernet`; JSON `{"link": "auto" \| "wifi" \| "ethernet"}` | core; stored in the data partition from spec 016 |
| `Link` | enum `Wifi`, `Ethernet` | core |
| Partition table | CSV, 16 MB | `firmware/partitions.csv` |

## Files changed

| File | Change |
| ---- | ------ |
| `docs/hardware.md` | New: the hardware sheet (R1) |
| `docs/memory-budget.md` | New: the budget (R2) |
| `docs/decisions.md` | D23: worst-case ceiling 1 MB and the placement; D24: one network link, chosen at boot, Wi-Fi first; D25: the partition table |
| `docs/architecture.md` | Firmware block: partition table, settings; links to the two new docs |
| `docs/living-spec/README.md` | Delta of the spec |
| `crates/core/src/network.rs`, `lib.rs` | New module (R5) |
| `crates/core/tests/network.rs` | Tests of R5 |
| `crates/core/tests/memory_worst_case.rs` | New (R3), its own file so the counting allocator sees nothing else |
| `crates/core/tests/memory_ceiling.rs` | Test renamed (300 KB) |
| `firmware/sdkconfig.defaults` | Placement and partition settings (R4) |
| `firmware/partitions.csv` | New |
| `scripts/qemu.sh` | Image built with the new table |
| `docs/roadmap.md`, `docs/specs-index.md` | Status |

## Reused

- `memory_ceiling.rs`: its home builder (devices through measures, zones and equipments through
  the API, recipes, live client, largest answer, save path) is the pattern of the worst case.
- `recipe_params_memory.rs`: the adversarial recipe instances.
- The core's JSON parsing with `serde`, as in `zones::parse_list`.
