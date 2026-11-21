# Architecture: Spec 006

## Overview

```
crates/core/src/radio/
  zcl.rs         + Report Attributes (0x0A), Configure Reporting (0x06) and its response (0x07),
                   Default Response (0x0B), IAS zone status change notification (0x0500 / 0x00)
  zdo.rs         + Simple_Desc_req / _rsp (0x0004 / 0x8004), Bind_req / _rsp (0x0021 / 0x8021)
  measures.rs    New: Measure {kind, value, unit}; decode(cluster, attribute, type, bytes) -> Option<Measure>
                 the table of R1, invalid values, scales
  coordinator.rs + after Identified: Simple_Desc → for each known cluster: Bind_req then
                   Configure Reporting → ReportingConfigured event
                 + listen(): poll() -> Vec<MeasureEvent {sender, measures}>; resume only
  ezsp.rs        + lookupEui64ByNodeId
crates/host/src/main.rs   --listen; measure lines; reporting lines during --pair
```

## Data

| Name | Type | Where |
| ---- | ---- | ----- |
| `Measure` | category (Sowel's names: temperature, humidity, pressure, luminosity, motion, light_state, light_brightness, battery, voltage; and alarm, tamper), value f64 or bool, unit | `measures.rs` |
| Measures per frame | at most 16 (bound written next to it) | `measures.rs` |
| TSNs answered with a Default Response | last 32 (sender, TSN) pairs (bound written next to it) | `coordinator.rs` |
| `PairingEvent::ReportingConfigured` | eui64, clusters ok, clusters failed | `coordinator.rs` |
| `MeasureEvent` | sender (eui64 or nwk), endpoint, measures | `coordinator.rs` |
| Address cache nwk → eui64 | at most 64 entries, oldest dropped (bound written next to it) | `coordinator.rs` |

## Files changed

| File | Change |
| ---- | ------ |
| `crates/core/src/radio/{zcl,zdo,ezsp,coordinator}.rs`, `measures.rs` | As above |
| `crates/core/tests/{measures,zcl,zdo,coordinator}.rs`, `tests/common/mod.rs` | Tests; the fake SNZB-02 answers Simple_Desc, Bind, Configure Reporting, then sends reports |
| `crates/host/src/main.rs`, `crates/host/tests/listen.rs` | `--listen`, lines |
| docs | Living spec, architecture, README, CLAUDE.md |

## Reused

Spec 005's coordinator, request/answer machinery (timeouts, retry, busy), fake radio and device.
