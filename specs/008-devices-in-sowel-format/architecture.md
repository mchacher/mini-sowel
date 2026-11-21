# Architecture: Spec 008

## Overview

```
radio events (coordinator: Identified, ReportingConfigured, Enrolled, MeasureEvent, any message)
      │
crates/core/src/devices/
  model.rs     Device, DeviceData, DeviceOrder, DeviceWithDetails (serde, Sowel's names)
  catalog.rs   cluster / zone type → data and orders (the table of R2)
  registry.rs  Devices: add or complete on pairing, apply measures, lastSeen, status(now),
               bounds (64 / 16 / 4); DeviceStore trait (load, save)
crates/core/src/api/
  mod.rs       handle(method, path, body, now) -> Response {status, body}; routes of R4
crates/host/src/
  store.rs     devices.json (DeviceStore): definitions at once, values every 10 min and on stop,
               0600; only the radio loop writes
  serve.rs     tiny_http on 127.0.0.1:<port> with a Host check; one thread for HTTP, the radio loop
               in the main thread; the Devices behind a Mutex (no radio I/O under it; poison
               recovered); Ctrl-C unblocks the server
  main.rs      --serve [port], --verbose
```

## Data

| Name | Type | Where |
| ---- | ---- | ----- |
| `devices.json` | `DeviceWithDetails[]` JSON | data folder (`mini-sowel-data/`, or `mini-sowel-data/sim/` under `--sim`) |
| Devices | at most 64, 16 data, 4 orders each (bounds written next to them) | `registry.rs` |
| Request body | at most 16 KB | `api/mod.rs` |

## Files changed

| File | Change |
| ---- | ------ |
| `crates/core/src/devices/**`, `crates/core/src/api/**`, `lib.rs` | New |
| `crates/core/Cargo.toml` | `serde`, `serde_json` |
| `crates/core/tests/{devices,api,memory_ceiling,sowel_json,coordinator}.rs` | Tests |
| `crates/host/src/{store,serve,main}.rs`, `crates/host/Cargo.toml` (`tiny_http`), `crates/host/tests/serve.rs` | Host |
| `crates/sim/src/ncp.rs` | The relay announces capability 0x8C (mains, receiver on) |
| docs | Living spec, architecture, decisions (D13), README, CLAUDE.md |

## Reused

The coordinator and its events (specs 005, 006), the simulator and `scenarios/garage.json`
(spec 007), the atomic 0600 write of `backup.rs` (spec 005).
