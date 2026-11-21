# Architecture: Spec 009

## Overview

```
coordinator (spec 005/006)   Identified ─► [0x0500 in Simple Descriptor?] ─► enrollment ─► reporting setup
  enrollment state machine:  read 0x0000/0x0001/0x0010 ─► (enrolled with us? done)
                             write 0x0010 = coordinator EUI64 ─► wait 500 ms ─► Enroll Response (zone 23)
                             ─► read 0x0000 every 500 ms, ≤ 20 ─► Enrolled / Unconfirmed / Failed
  any time: Zone Enroll Request ─► Enroll Response with the request's TSN
devices (spec 008) catalog:  zone type ─► data (occupancy / contact / water_leak / smoke, tamper)
                             zone status notification ─► values
sim (spec 007): motion device answers the reads, the write, the response; zone state 1 after it
```

The Simple Descriptor is now asked before the reporting setup's own clusters (it already is, spec
006): enrollment is inserted between the descriptor and the binds.

## Data

| Name | Type | Where |
| ---- | ---- | ----- |
| `ReportingConfigured.enrollment: Option<{zone_type: Option<u16>, outcome: Enrolled / Unconfirmed / Failed}>` | | `coordinator.rs` |
| Enrollment polls | at most 20 per device (bound written next to it) | `coordinator.rs` |

## Files changed

| File | Change |
| ---- | ------ |
| `crates/core/src/radio/{coordinator,zcl}.rs` | Enrollment; Write Attributes; Enroll Request / Response |
| `crates/core/src/devices/{catalog,registry}.rs` | Zone data; zone notifications applied |
| `crates/sim/src/{ncp,devices}.rs` | The motion device answers the IAS reads |
| `crates/host/src/main.rs` | The `cut short` line at the time cap (the `reporting:` enrollment text lives in the core, `PairingEvent::line`) |
| tests | `crates/core/tests/{coordinator,zcl,devices}.rs`, `crates/sim/tests/devices.rs`, `crates/host/tests/serve.rs` |
| docs | Living spec, architecture |
