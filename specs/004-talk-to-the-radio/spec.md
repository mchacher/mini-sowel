# Spec 004: talk-to-the-radio

## Context

The radio of the Dongle Max, an EFR32MG24, runs Silicon Labs' EmberZNet stack as a network
co-processor (NCP); a host drives it over a serial line with two protocols: **ASH** (UG101, the
frame layer: framing, CRC, byte stuffing, data randomisation, acknowledgements, retransmission)
and **EZSP** (UG100, the commands). No proven Rust implementation exists: it is the first risk of
the framing note (`inputs/note-de-cadrage-mini-sowel.md`, "Risks and unknowns"; "Steps" 1).

With its original firmware (decision D6), the Dongle Max's ESP32 relays that serial line over
TCP, port 6638, one connection at a time (Zigbee2MQTT connects to `tcp://<address>:6638` with its
`ember` adapter). The dongle arrives on 2026-11-21: this increment is written and tested without
it, against Silicon Labs' reference frames and a scripted fake radio, and shown on the real dongle
when it arrives.

Specified in autonomy: the user approved in advance (2026-11-20) the specs reviewed by
`spec-reviewer` without an important finding. Choices made without the user are listed under
"Decided in autonomy".

## Goals

1. The core speaks ASH and the EZSP commands needed to identify the radio, with no I/O in it.
2. The host connects to the Dongle Max over TCP and shows the radio's versions and network state.
3. Every byte sequence is tested on the Mac, without hardware.

## Out of scope

- Forming a network, joining, permit-join, security keys: A4.
- Receiving messages from devices (callbacks), ZCL: A5.
- The UART adapter of the firmware (milestone E); a USB EFR32 dongle (same protocol, other
  transport: not tested here).
- Reconnection after the link drops; several ASH frames in flight (window above 1).

## Requirements

### R1: ASH, in the core, without I/O

`crates/core/src/radio/ash.rs`:

- **Frames**: DATA (frame number, ack number, retransmit flag, payload), ACK, NAK (ack number,
  not-ready flag), RST, RSTACK (version, reset code), ERROR (version, error code); encoded and
  decoded with the control byte, CRC-CCITT (polynomial 0x1021, initial 0xFFFF, big-endian),
  data randomisation of DATA payloads (pseudo-random sequence seeded 0x42, as UG101), byte
  stuffing of the reserved bytes (0x7E flag, 0x7D escape, 0x11 XON, 0x13 XOFF, 0x18 substitute,
  0x1A cancel) and the flag 0x7E at the end.
- **Decoder**: fed bytes in any chunking (one byte at a time or many frames at once); returns
  whole frames; a bad CRC, an unknown control byte, a frame too short or too long (over 223 bytes
  before stuffing: control byte, 220 bytes of EZSP, 2 bytes of CRC), or a cancel byte (0x1A) discards the frame in progress and is reported as an
  error event, never a panic; the substitute byte (0x18) marks the frame in progress as bad.
- **Session**: a state machine with time given by the caller (no clock, no thread, no socket):
  `reset()` produces RST; on an RSTACK of ASH version 2 with the reset code "software reset"
  (0x0B) the session is connected (any other RSTACK while resetting, and any ERROR frame before
  connection, is stale and ignored); `send(payload)` produces a DATA
  frame with the next frame number (modulo 8) and the current ack number; an incoming DATA frame
  in sequence is acknowledged (ACK produced) and its payload delivered; out of sequence: one NAK,
  then silence until a frame arrives in sequence (UG101's reject condition); a frame dropped by
  the decoder (bad CRC, too long, substitute byte) also gets that one NAK; an
  outgoing frame not acknowledged within 1.6 s (fixed, not adapted) is resent with the retransmit
  flag; the 6th timeout in a row (after 5 resends) is a link failure (Zigbee2MQTT's `ember`
  adapter uses the same count). An ERROR frame, or an RSTACK once connected, is a failure carrying
  the error or reset code. The stray flow-control bytes XON (0x11) and XOFF (0x13) are discarded
  wherever they appear; a stray escape, substitute or cancel byte follows the rules above.
  Payloads delivered and not yet read are bounded to 8; one past it, the oldest is dropped and an
  error event reported. One frame in
  flight at a time (window 1).

### R2: EZSP, in the core, without I/O

`crates/core/src/radio/ezsp.rs`:

- Values are those of UG100 and `zigbee-herdsman`'s enums: `networkState`: 0 no network,
  1 joining, 2 joined, 3 joined with no parent, 6 leaving; any other value is shown as
  `unknown (<n>)`. Stack type 2 (mesh) is the only one accepted; another is an error.
- A frame with the callback bit set (an asynchronous callback) is ignored, not an error; a frame
  without the response direction bit, or with the truncated or overflow bit, is an error.
- Frame format of EZSP version 8 and later: sequence byte, frame control (2 bytes, little-endian),
  frame id (2 bytes, little-endian), parameters. The first `version` command is sent in the
  legacy format (frame control 1 byte, frame id 1 byte), as UG100 requires.
- **Version negotiation**: send `version` (0x0000) with the desired protocol version 19 (0x13,
  the newest Zigbee2MQTT's `ember` adapter knows); the radio answers with its protocol version,
  stack type and stack version. If its protocol version is between 13 and 19 (0x0D to 0x13), send
  `version` again with that version in the new format; it is the session's version. Otherwise: an
  error naming the version found and the range supported. Versions are written in decimal. A
  radio already at 19 gets the second `version` all the same (harmless, one code path).
- **Commands**: `version`; `networkState` (0x0018: no network, joining, joined, joined with no
  parent, leaving), `getEui64` (0x0026: the radio's 64-bit address).
- A response whose sequence does not match, whose frame id is not the one sent, or whose length is
  wrong is an error, never a panic.
- The format of each response is read from the response itself (extended when the low bits of
  its third byte are 1), not assumed.
- The stack version (2 bytes) is shown as `major.minor.patch.build` (nibbles, as UG100).

### R3: The host shows the radio

`cargo run -p mini-sowel-host -- --radio tcp://<address>:<port>` prints the banner, connects (which resets the radio: ASH starts with RST), then:

```
radio: tcp://192.168.1.50:6638
ezsp: protocol 13, stack type 2, EmberZNet 7.4.4.0
radio address: 0x8C6FB9FFFE123456
network: no network
```

and exits 0. Without `--radio`, the host prints the banner and exits, as today.

The TCP transport (`crates/host/src/radio_tcp.rs`) reads and writes bytes and gives the core the
time; it implements the core's radio-transport trait. Errors print one line and exit 1: address that does not resolve or refuses the connection (`radio: cannot connect to
<address>: <reason>`), no RSTACK within 5 s after RST (`radio: no answer to reset`), the session
failure of R1 (`radio: link lost`, within 10 s of the radio's last answer), an ERROR frame or
an unexpected reset (`radio: error 0x<code>`), a command acknowledged but never answered within
5 s of its acknowledgement (`radio: no answer to command`; a command never acknowledged is the
link failure), the radio closing the connection (`radio: connection closed by the radio`), an EZSP error (its message). Error lines go to the error
output (stderr); the four lines are printed together, after success. A malformed `--radio` argument
prints the expected form and exits 2.

### R4: Tests without hardware

- **Reference frames**: the encoder and decoder are tested against the example frames of UG101
  (RST `1A C0 38 BC 7E`; RSTACK; ERROR; ACK; NAK; DATA carrying the EZSP `version` command), each
  checked byte for byte and taken from UG101 (or from a published implementation, named in the
  test, when UG101 has no example).
- **Fake radio**: a scripted NCP in the tests answers RST with RSTACK and EZSP commands with
  recorded responses, through the same session code; the host's test runs the real TCP transport
  against a fake radio listening on a local port.

## Living spec delta

### docs/living-spec/README.md

- New section "Talks to the radio": `cargo run -p mini-sowel-host -- --radio tcp://<address>:6638`
  connects to the Dongle Max's radio (original firmware) and shows its EZSP protocol version, its
  EmberZNet version, its address and its network state; the errors and their messages.
- Changed: "it has no radio, no storage and no API" → "it has no storage and no API; it talks to
  the radio only to identify it".
- New: History row 004.

### No longer true

- `docs/living-spec/README.md`: "it has no radio, no storage and no API" (in the delta); "prints
  `mini-sowel 0.1.0` and exits" → "without `--radio`, prints `mini-sowel 0.1.0` and exits".
- `docs/architecture.md`: the overview "crates/core (Rust library: name, version, banner)" and the
  blocks table → the core also holds the radio protocols; the host connects to the radio over TCP.
- `CLAUDE.md`, `README.md`: the commands → the `--radio` command added.
- Searched: `radio`, `host`, `--radio`, `EZSP`, `ASH`, `A3`.

## Acceptance criteria

- [x] Every UG101 reference frame is encoded and decoded byte for byte.
- [x] The host shows the fake radio's versions, address and state in its test.
- [x] With the real Dongle Max (2026-11-21): the four lines show, with its real values.

## Edge cases

- Bytes arrive one by one, or several frames in one read.
- A frame with a bad CRC, a cancel byte, a substitute byte, a reserved byte not stuffed.
- A DATA frame received twice (retransmitted): acknowledged, delivered once.
- No ACK: resent until 6 timeouts in a row, then link lost.
- RSTACK never comes; the connection is refused; the address does not resolve.
- The radio's EZSP version is below 13 or above 19 (a factory radio firmware older than
  EmberZNet 7.2): the error says so; widening the range or updating the radio's firmware is then
  the user's choice.
- A stale ERROR or power-on RSTACK still buffered by the dongle's relay when we connect (after
  Zigbee2MQTT was stopped): ignored until the RSTACK answering our RST.
- The relay accepts the connection then closes it: `radio: connection closed by the radio`.
- An ERROR frame, or a second RSTACK, from the radio.
- A stray XON or XOFF byte.
- A response with the wrong sequence or frame id.
- Another client already holds the dongle's single TCP connection: the connection is accepted
  then nothing answers, or refused: either way one of the messages above, within 10 seconds.

## Sowel contract

None: nothing is exposed to Sowel.

## Decided in autonomy

- EZSP versions 13 to 19 (EmberZNet 7.2 and later, what Zigbee2MQTT's `ember` adapter supports
  in `zigbee-herdsman` 11.2.2); older radios are out of scope.
- Reference for constants and behaviour when UG101 or UG100 is silent: `zigbee-herdsman`'s
  `ember` adapter (MIT), read, never copied: the Rust code is written from the documents.
- ASH and EZSP live in the core, without I/O, so the firmware's UART adapter reuses them.
- Added while building: a command acknowledged and never answered fails after 5 s (otherwise
  `identify` would wait forever on a mute radio); errors on stderr; non-fatal ASH events (a bad
  frame, a dropped payload) are counted by the session, not logged (the core has no `log`
  dependency yet).
- Window of one frame: simpler, enough to identify the radio; a larger window when throughput
  needs it.

## How the user will check it

Plug the Dongle Max into the network, find its IPv4 address (the router's list, or the dongle's
web page), run `cargo run -p mini-sowel-host -- --radio tcp://<address>:6638`, and read the four
lines. Nothing else may be connected to the dongle at the same time: stop Zigbee2MQTT or Home Assistant
if they use it. The command resets the radio (harmless: its network settings stay in its own
memory).

Run on the real hardware (2026-11-21): The Dongle Max (original firmware) was connected by USB to the Mac (CP210x serial port) and relayed to `tcp://127.0.0.1:6638` with `socat` at 115200 baud. The host printed `ezsp: protocol 13, stack type 2, EmberZNet 7.4.5.0`, the radio address and `network: no network`. First try over USB, no Solite defect.
