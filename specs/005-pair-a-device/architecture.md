# Architecture: Spec 005

## Overview

```
crates/core/src/radio/
  ezsp.rs        + commands: setConfigurationValue, setPolicy, getExtendedValue, addEndpoint,
                   networkInit, setInitialSecurityState, setValue, clearKeyTable, formNetwork,
                   getNetworkParameters, setConcentrator, setSourceRouteDiscoveryMode,
                   importTransientKey, clearTransientLinkKeys, permitJoining, sendUnicast
                 + callbacks: stackStatusHandler, trustCenterJoinHandler, incomingMessageHandler,
                   messageSentHandler (layouts of EZSP 13 and 14+)
  zdo.rs         Active_EP_req / _rsp, Device_annce (bytes ↔ types)
  zcl.rs         Read Attributes request / response (frame header, records, string type 0x42)
  coordinator.rs start_network(settings, random) -> Network
                 permit_join(seconds); poll(now) -> Vec<PairingEvent>  (Joined, Identified)
                 a small state machine per joining device (endpoint request → Basic read)
  mod.rs         Radio: keeps callbacks received between command and response in a bounded queue

crates/host/src/
  main.rs        --pair [seconds], --data <dir>; Ctrl-C (crate `ctrlc`) closes joining
  random.rs      reads /dev/urandom
  backup.rs      network.json (mode 0600) in the data folder
```

## Data

| Name | Type | Where |
| ---- | ---- | ----- |
| `NetworkSettings` | channel (u8, default 15) | `coordinator.rs` |
| `Network` | pan_id u16, extended_pan_id [u8; 8], channel u8, formed: bool | `coordinator.rs` |
| `PairingEvent` | `Joined {eui64, nwk}`, `Identified {eui64, manufacturer: Option<String>, model: Option<String>}`, `TooManyDevices {eui64}` | `coordinator.rs` |
| Devices of the session | at most 32 (bound written next to it) | `coordinator.rs` |
| Callback queue | at most 16 frames, the oldest dropped (bound written next to it) | `radio/mod.rs` |

## Files changed

| File | Change |
| ---- | ------ |
| `crates/core/src/radio/{ezsp,mod}.rs` | Commands, callbacks, queue |
| `crates/core/src/radio/{zdo,zcl,coordinator}.rs` | New |
| `crates/core/tests/{zdo,zcl,coordinator}.rs`, `crates/core/tests/common/mod.rs` | Tests; the fake radio learns the new commands in both EZSP layouts |
| `crates/host/src/{main,random}.rs`, `crates/host/tests/pair.rs` | `--pair` |
| docs | Living spec, architecture, README, CLAUDE.md |

## Reused

- Spec 004's ASH session, EZSP framing and fake radio.
- `ezsp-reference.md` (this folder) for every frame id, layout and value.
