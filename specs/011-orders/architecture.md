# Architecture: Spec 011

## Overview

```
HTTP worker (host)            core api::handle             radio loop (host, main thread)
POST .../orders/state  ─►  validate, resolve value,   ─►  order queue (≤ 8, reply slot each)
                           commands per device              │ take one order between polls
        wait reply ≤ 12 s ◄──────────────────────────────── │ coordinator: send ZCL command,
                                                            │ await Default Response (non-blocking
                                                            │ for other frames), retry once at 2 s
                                                            ▼ reply {success | error}
```

The core gives: `api::handle` returns `Reply::Order(OrderRequest)` for an order (validated and
resolved, no radio); `Coordinator::send_command(target, command) -> CommandHandle` and its outcome
polled with the other events (no blocking wait); the host joins them with a bounded channel.

## Data

| Name | Type | Where |
| ---- | ---- | ----- |
| `OrderRequest` | equipment id, alias, commands `[(eui64, endpoint, ZCL command)]` | `api/`, `home/` |
| Order queue | at most 8 waiting (bound written next to it) | host `serve.rs` |
| Commands in flight | at most 8 (bound) | `coordinator.rs` |

## Files changed

| File | Change |
| ---- | ------ |
| `crates/core/src/radio/{zcl,coordinator}.rs` | On/Off and Move to Level commands; send and await the Default Response |
| `crates/core/src/{api,home}/**` | The route, value resolution (Sowel's rules) |
| `crates/host/src/serve.rs` | Order queue and reply slots |
| `crates/sim/src/{devices,ncp}.rs` | `dimmer` kind (tests only); a switch to silence the relay |
| tests | `crates/core/tests/{orders,zcl,coordinator}.rs`, `crates/host/tests/serve.rs`, `crates/sim/tests/devices.rs` |
| docs | Living spec, architecture, decision D16 |
