# Architecture: Spec 048

## Overview
Core only (recognition and measures), plus the simulator.

## Data
Per device, the scale of `0x0B04` and `0x0702` (multiplier, divisor), kept with the device's
data in RAM; persisted with the device when the device store allows it, else read again at the
next start.

## Files changed
| File | Change |
| --- | --- |
| `crates/core/src/devices/catalog.rs:118-131, 189-295` | the data specs, `key_for_measure`, `data_rank`, `WORDS` |
| `crates/core/src/radio/measures.rs:23-31, 110-182, ~255` | cluster consts, decode with the scale, `REPORTING` entries |
| `crates/core/src/radio/coordinator.rs`, `commands.rs:435` | the scale read at the reporting setup |
| `crates/sim/src/devices.rs` | a `Plug` kind |
| `scenarios/plug.json` | a scenario |
| `docs/sowel-subset.md`, `docs/living-spec/README.md`, `docs/roadmap.md` | the delta |

## Reused
The Read Attributes path (`commands.rs:435`), the measure decode pattern of `0x0402`, the sim's
`report_frame`.
