# Architecture: Spec 050

## Overview
Core only (catalog, measures, orders, ZCL commands), plus the simulator.

## Data
None new stored.

## Files changed
| File | Change |
| --- | --- |
| `crates/core/src/devices/catalog.rs` | `data_for_cluster`/`orders_for_cluster` for `0x0102`, `WORDS`, `data_rank`, `order_rank` |
| `crates/core/src/radio/measures.rs` | decode `0x0102`/`0x0008` (100 − x), `REPORTING` |
| `crates/core/src/radio/zcl.rs` (`enum Command` ~146-180) | Up, Down, Stop, GoToLiftPercentage |
| the order resolution (`crates/core/src/orders/resolve.rs`) | `shutter_move` and `set_shutter_position` values to commands |
| `crates/sim/src/devices.rs` | a `Shutter` kind |
| `scenarios/shutter.json` | a scenario |
| docs | living spec, Sowel subset, roadmap |

## Reused
The light orders' path (`light_toggle` → On/Off), the measure decode of `0x0008` Level.
