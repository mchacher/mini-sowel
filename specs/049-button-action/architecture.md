# Architecture: Spec 049

## Overview
Core only (interview, ZCL parsing, catalog, events), plus the simulator.

## Data
No new stored data: `action` is a data of the device like the others.

## Files changed
| File | Change |
| --- | --- |
| `crates/core/src/radio/coordinator.rs` (~1311-1330 the cluster-command dispatch; ~2100-2120 `SendBind`) | the remote's output clusters bound; the commands dispatched to the decoder |
| `crates/core/src/radio/zcl.rs` (~357 `parse_header`, ~489) | a decoder of the received On/Off, Level, Scenes commands |
| `crates/core/src/devices/catalog.rs` | the `action` data spec, `WORDS`, `data_rank` |
| the engine's data update path | an `action` value fires its listeners even when unchanged |
| `crates/sim/src/devices.rs` | a `Button` kind sending commands |
| docs | living spec, Sowel subset, roadmap |

## Reused
The bind path of the reporting setup, `parse_header` (`FrameType::Cluster`, direction), the IAS
command decoding pattern (`parse_zone_status_change`).
