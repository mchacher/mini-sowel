# Architecture: Spec 007

## Overview

```
crates/sim  (new, Mac only; not a dependency of the core or the firmware)
  ncp.rs        the simulated EmberZNet NCP (moved from crates/core/tests/common/mod.rs):
                ASH, EZSP 13 and 14+, network kept in memory or in a file, fault switches the
                tests use
  devices.rs    simulated devices: temperature-humidity, motion, relay (On/Off commands)
  scenario.rs   Scenario (serde): radio, devices, events; parse with line/field errors; bounds
  clock.rs      scenario time = (clock - start) × speed; the clock is given by the caller
                (Instant in the host, the tests' fake clock in tests)
  transport.rs  SimTransport: implements the core's RadioTransport; on each read, delivers the
                events whose time has come, then the NCP's answers
crates/host
  main.rs       --sim <file> [--speed <factor>]: SimTransport instead of TcpTransport
crates/core/tests, crates/host/tests
  use mini-sowel-sim (dev-dependency) instead of tests/common/mod.rs
scenarios/      snzb02.json, garage.json
```

## Data

| Name | Type | Where |
| ---- | ---- | ----- |
| Scenario | JSON (R3) | `scenarios/*.json` |
| Memory of the simulated radio | JSON `<data>/sim-<scenario name>.json` (default data folder under `--sim`: `mini-sowel-data/sim/`, ignored by git) | `crates/sim/src/ncp.rs` |
| Test records | off by default; at most 10 000 entries each when on | `crates/sim/src/ncp.rs` |
| Devices per scenario | at most 64 | `scenario.rs` |
| Events per scenario | at most 10 000 | `scenario.rs` |

## Files changed

| File | Change |
| ---- | ------ |
| `crates/sim/**` | New crate, member of the workspace |
| `crates/core/tests/common/mod.rs` | Removed (moved into `crates/sim`) |
| `crates/core/Cargo.toml`, `crates/host/Cargo.toml` | `mini-sowel-sim` as dev-dependency (core tests) and dependency (host) |
| `crates/core/tests/*.rs`, `crates/host/tests/*.rs` | `use mini_sowel_sim::...` |
| `crates/host/src/main.rs`, `crates/host/tests/sim.rs` | `--sim`, `--speed` |
| `scenarios/*.json` | Scenarios |
| `Cargo.toml` | Workspace member; `serde`, `serde_json` |
| docs | Living spec, architecture, README, CLAUDE.md |

## Reused

The fake NCP and fake devices of specs 004 to 006, unchanged in behaviour.
