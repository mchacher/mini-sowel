# Architecture: Spec 004

## Overview

```
crates/core/src/radio/
  ash.rs     frames: encode / decode (CRC, stuffing, randomisation); Decoder (byte stream → frames)
             Session: state machine, time given by the caller (Instant-free: milliseconds u64)
               inputs:  bytes received, now, payloads to send
               outputs: bytes to write, payloads delivered, events (connected, link lost, errors)
  ezsp.rs    EZSP frames (legacy and extended), commands version / networkState / getEui64,
             responses parsed into types; Ezsp: sequence numbers, negotiation
  transport.rs   trait RadioTransport { read(buf, timeout) -> Result<usize>; write(bytes) -> Result<()> }
  mod.rs     Radio<T: RadioTransport>: drives Session + Ezsp over a transport, blocking calls
             identify() -> RadioIdentity { protocol, stack_type, stack_version, eui64, network_state }

crates/host/src/
  radio_tcp.rs   TcpTransport: std::net::TcpStream, connect timeout 5 s, read timeouts
  main.rs        argument --radio tcp://host:port → Radio::identify → four lines
```

The core has no socket and no clock: `RadioTransport` is implemented by the host (TCP) and, later,
by the firmware (UART); `Radio` takes a `now()` function from its caller. Everything below
`Radio` is pure and tested with byte arrays.

## Data

| Name | Type | Unit | Where |
| ---- | ---- | ---- | ----- |
| `AshFrame` | enum: `Data {frm, ack, retransmit, payload}`, `Ack {ack, not_ready}`, `Nak {ack, not_ready}`, `Rst`, `RstAck {version, code}`, `Error {version, code}` | | `radio/ash.rs` |
| `RadioIdentity` | struct: `protocol: u8`, `stack_type: u8`, `stack_version: u16`, `eui64: [u8; 8]`, `network_state: NetworkState` | | `radio/mod.rs` |
| `NetworkState` | enum: `NoNetwork` (0), `Joining` (1), `Joined` (2), `JoinedNoParent` (3), `Leaving` (6), `Unknown(u8)` | | `radio/ezsp.rs` |
| Decoder buffer | bytes | at most 223 (one frame before stuffing); bound written next to it | `radio/ash.rs` |
| Pending payloads | queue | at most 8 (frame numbers modulo 8); bound written next to it | `radio/ash.rs` |
| Delivered payloads not yet read | queue | at most 8, the oldest dropped one past it; bound written next to it | `radio/ash.rs` |

## Files changed

| File | Change |
| ---- | ------ |
| `crates/core/src/radio/{mod,ash,ezsp,transport}.rs`, `crates/core/src/lib.rs` | New |
| `crates/core/tests/ash.rs`, `crates/core/tests/ezsp.rs`, `crates/core/tests/radio.rs` | New: tests of the plan, with a fake NCP transport |
| `crates/host/src/radio_tcp.rs`, `crates/host/src/main.rs` | New transport; `--radio` argument |
| `crates/host/tests/radio.rs` | New: host against a fake radio on a local TCP port |
| `docs/living-spec/README.md`, `docs/architecture.md`, `README.md`, `CLAUDE.md` | Delta, the radio block, the command |

No new crate dependency (std only).

## Reused

- The banner and memory pattern: the core formats, the program prints.
- `zigbee-herdsman`'s `ember` adapter (MIT, in `tools/z2m-coverage/node_modules/zigbee-herdsman/dist/adapter/ember/`) as a cross-check for constants (frame lengths, LFSR, timeouts, EZSP frame ids), never copied.
