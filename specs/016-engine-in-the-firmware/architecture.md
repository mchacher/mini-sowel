# Architecture: Spec 016

## Overview

```
                    core::engine (loop, saver)  ◄── RadioTransport, Storage, clock, random
                     ▲                         ▲
 host: TCP/sim radio, FileStore, HTTP/WS       firmware: sim radio (feature sim), FAT /data
                                                         network.json → core::network
                                                         engine: line → scripts/qemu.sh
```

## Data

| Name | Where |
| ---- | ----- |
| `/data/devices.json`, `zones.json`, `equipments.json`, `recipes.json` | FAT partition `storage`, same formats as the host |
| `/data/network.json` | FAT partition; spec 015 R5 |
| `/data/sim-*.json` | the simulator's memory, `sim` image only |
| `firmware/fixtures/` | the realistic home and its scenario, synthetic, embedded with `include_str!` |

## Files changed

| File | Change |
| ---- | ------ |
| `crates/core/src/engine/` | New: the loop and the saver, moved from the host; the `Storage` trait |
| `crates/host/src/serve.rs`, `store.rs`, `main.rs` | Use `core::engine`; `FileStore` implements `Storage` |
| `crates/sim/` | Unix-only parts behind `cfg(unix)`; builds for the ESP32 |
| `firmware/Cargo.toml`, `firmware/src/` | Feature `sim`; storage mount; network line; engine run; `engine:` line |
| `firmware/fixtures/` | New: the realistic home and its scenario |
| `crates/core/tests/` | Engine tests (moved with the code); the fixture test |
| `scripts/qemu.sh`, `scripts/validate.sh` | `sim` image, the `engine:` check; both images built |
| `docs/` | Living spec, architecture, D26, memory budget, roadmap |

## Reused

- `serve.rs::radio_loop` and `store.rs::Saver`, moved, not rewritten.
- `memory_ceiling.rs`'s home builder, for the fixtures.
- `crates/sim`'s `SimTransport` and scenario parser; the garage scenario's format.
- `firmware/src/memory.rs` (`heap_caps_*`), plus `heap_caps_get_minimum_free_size` for the peak.
