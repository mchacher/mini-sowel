# Architecture: Spec 051

## Overview
Core only (catalog, measures, the Write Attributes order), plus the simulator.

## Data
None new stored.

## Files changed
| File | Change |
| --- | --- |
| `crates/core/src/devices/catalog.rs` | `0x0201` data and order, `WORDS`, ranks |
| `crates/core/src/radio/measures.rs` | decode `0x0000`, `0x0012`; `REPORTING` |
| `crates/core/src/radio/zcl.rs` (`COMMAND_WRITE_ATTRIBUTES` :32, :94) | a write of an int16 attribute as an order |
| `crates/core/src/orders/resolve.rs` | `set_setpoint` to the write, 5..30 |
| `crates/sim/src/devices.rs` | a `Thermostat` kind |
| `scenarios/thermostat.json` | a scenario |
| docs | living spec, Sowel subset, roadmap |

## Reused
The existing Write Attributes frame (`zcl.rs:94`), the shutter's order path (spec 050).
