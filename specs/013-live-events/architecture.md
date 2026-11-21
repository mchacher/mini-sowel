# Architecture: Spec 013

## Overview

```
Home (radio loop, HTTP threads) ──emit(event)──► LiveHub (core, Arc<Mutex>)
                                                  │ no client: nothing built
                                                  │ per client: topics, queue (256 / 8 KB, dedup)
WebSocket thread per client (host, ≤ 4) ◄──flush every 200 ms── JSON array text
   reads frames: subscribe → hub.subscribe(client, topics)
```

The core's `live/` module holds everything that decides: the topic of an event type, the dedup
key, the per-client queues and their bounds, the subscribe parsing, the batch text. It has no
socket: the host's thread calls `hub.take_batch(client)` every 200 ms and writes the text it gets.
Events are built by the home where the change happens (devices, zones, equipments, orders,
recipes), through one `emit` call that does nothing when `hub.has_clients()` is false, and are
serialized once into an `Arc<str>` shared by the queues.

## Data

- `live/mod.rs`: `LiveHub { clients: Vec<Client> (≤ 4) }`, `Client { id, topics: TopicSet,
  queue: VecDeque<Queued>, bytes }`, `Queued { key: Option<DedupKey>, json: Arc<str> }`.
- `live/events.rs`: `EngineEvent` (the events of R4, `#[serde(tag = "type")]`, Sowel's names),
  `topic()`, `dedup_key()`.
- `live/subscribe.rs`: parse a client message.

## Files changed

| File | Change |
| ---- | ------ |
| `crates/core/src/live/` | new |
| `crates/core/src/home/`, `devices/`, `zones/`, `equipments/`, `orders/`, `recipes/` | emit the events of R4 |
| `crates/core/src/api/mod.rs` | `GET /ws` recognised (answered by the host) |
| `crates/host/src/serve.rs`, new `crates/host/src/ws.rs` | listener with timeouts (`socket2`), upgrade, origin, 4-client bound, one thread per client, 200 ms flush, pings, Ctrl-C close |
| `crates/core/src/equipments/status_tracker.rs` | last status per equipment, `equipment.status.changed` |
| `crates/core/src/orders/` | the order's source carried to its outcome event |
| `crates/host/Cargo.toml` | `tungstenite` (`handshake` feature), `socket2` |
| `crates/core/tests/live.rs` (new), `memory_ceiling.rs`; `crates/host/tests/ws.rs` (new) | tests |
| docs | living spec, architecture, decision D18, roadmap |

## Reused

- The `Host` check and the 503 `Busy` text of the API (spec 008).
- Spec 012's compact events stay for the recipes; this increment adds Sowel's full events beside
  them, built only when a client listens.
- `tiny_http`'s `Request::upgrade` hands the socket to `tungstenite`.
