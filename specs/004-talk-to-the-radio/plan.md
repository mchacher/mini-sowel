# Plan: Spec 004

## Steps

- [x] 1. Living spec updated (delta of spec.md)
- [x] 2. Core types: `AshFrame`, `NetworkState`, `RadioIdentity`, `RadioTransport`
- [x] 3. Core logic: ASH codec, decoder, session; EZSP frames and negotiation; `Radio::identify`
- [x] 4. Adapters: `TcpTransport` in the host
- [x] 5. API: not applicable
- [x] 6. Interface: not applicable
- [x] 7. Host: `--radio` argument and output
- [x] 8. Tests of the test plan; each seen failing once
- [x] 9. Docs: architecture, README, CLAUDE.md; "No longer true" search
- [ ] 10. Demo on the real Dongle Max (2026-11-21, with the user)

## Test plan

Reference frames come from UG101 ("ASH Protocol Specification", Silicon Labs) where it has an
example; the test names the source of each byte sequence.

| Module | Scenario | Expected |
| ------ | -------- | -------- |
| ash codec | RST | `1A C0 38 BC 7E` (cancel byte first, as UG101 sends it) |
| ash codec | RSTACK version 2, reset code power-on (0x02) decoded | `RstAck {version: 2, code: 2}` from `C1 02 02 9B 7B 7E` |
| ash codec | ERROR version 2, code 0x51 decoded | `Error {version: 2, code: 0x51}` from `C2 02 51 A8 BD 7E` |
| ash codec | ACK ack number 1 | `81 60 59 7E` |
| ash codec | NAK ack number 6 | `A6 34 DC 7E` |
| ash codec | DATA frm 0 ack 0 carrying `00 00 00 02` | `00 42 21 A8 56 8D EA 7E` (randomised payload; sequence `42 21 A8 54 2A...`) |
| ash codec | DATA frm 0 ack 0 carrying the legacy `version` command the host sends (`00 00 00 13`) | `00 42 21 A8 47 8F FA 7E` |
| ash codec | Encode then decode a DATA frame of 220 bytes with every byte value | the same frame |
| ash codec | Payload holding every reserved byte (7E 7D 11 13 18 1A) | stuffed with 7D and bit 5 flipped; decoded back |
| ash codec | CRC | CRC-CCITT of `C0` is `38 BC` |
| ash decoder | Two frames in one chunk | two frames |
| ash decoder | A frame fed one byte at a time | one frame |
| ash decoder | Bad CRC | error event, the next frame decoded |
| ash decoder | Cancel byte inside a frame | frame dropped, the next decoded |
| ash decoder | Substitute byte inside a frame | frame dropped at the flag |
| ash decoder | Frame longer than 223 bytes | error event, buffer bounded, next frame decoded |
| ash decoder | Unknown control byte | error event |
| ash decoder | Stray XON / XOFF inside and between frames | discarded, frame decoded |
| ash session | `reset()` | RST bytes; connected after RSTACK |
| ash session | Send before connected | refused (error), no bytes |
| ash session | `send` | DATA with frm 0, then frm 1...; frm wraps 7 → 0 |
| ash session | Incoming DATA in sequence | ACK with the next ack number, payload delivered |
| ash session | Incoming DATA out of sequence, twice | one NAK, then silence; nothing delivered; the frame in sequence accepted after |
| ash session | Frame dropped by the decoder (bad CRC) while connected | one NAK |
| ash session | ERROR frame or power-on RSTACK before our RSTACK | ignored; connected on the RSTACK 0x0B |
| ash codec | UG101's example DATA frm 2 ack 5 payload `00 00 00 02` | `25 42 21 A8 56 A6 09 7E` |
| ash session | Same DATA received twice (retransmitted) | acknowledged twice, delivered once |
| ash session | No ACK within 1.6 s | DATA resent with the retransmit flag |
| ash session | Timeouts | 5 resends, link lost on the 6th timeout |
| ash session | ERROR frame received | failure with its code |
| ash session | RSTACK received once connected | failure with its reset code |
| ash session | 9 payloads delivered, none read | the oldest dropped, error event, 8 kept |
| ash session | NAK received | the frame resent |
| ezsp | Legacy `version` command, desired 19, seq 0 | `00 00 00 13` |
| ezsp | Extended `version` command, seq 1 | frame control `00 01`, frame id `00 00` |
| ezsp | `version` response 13 / stack type 2 / stack version 0x7440 | protocol 13, type 2, `7.4.4.0` |
| ezsp | Negotiation with a radio at 19, at 13 | session version 19, 13 |
| ezsp | Radio at 12, at 20 | error naming the version and the range 13 to 19 |
| ezsp | `networkState` response 0 / 2 / 6 / 4 | no network / joined / leaving / unknown (4) |
| ezsp | `version` response with stack type 1 | error |
| ezsp | Callback frame (callback bit set) between command and response | ignored, the response read |
| ezsp | Response with the truncated or overflow bit, or without the response bit | error |
| ezsp | `getEui64` response bytes `56 34 12 FE FF B9 6F 8C` (little-endian) | `0x8C6FB9FFFE123456` |
| ezsp | Response with a wrong sequence, wrong frame id, too short | error, no panic |
| radio | `identify()` against a fake NCP transport (scripted RSTACK and responses) | the identity of the fake radio |
| radio | Fake NCP never sends RSTACK | "no answer to reset" within 5 s of fake time |
| radio | Fake NCP never acknowledges the command | link lost (6th timeout), not "no answer to command" |
| radio | Fake NCP acknowledges, never answers | "no answer to command" 5 s after the acknowledgement |
| ezsp | First `version` response in the extended format | parsed |
| ezsp | `networkState` 1 / 3 | joining / joined with no parent |
| host | `--radio tcp://127.0.0.1:<port>` against a fake radio on a local port | the four lines, exit 0 |
| host | Connection refused (closed port) | `radio: cannot connect to ...`, exit 1, within 10 s |
| host | Accepts the connection but never answers | `radio: no answer to reset`, exit 1, within 10 s |
| host | Fake radio sends an ERROR frame after RSTACK | `radio: error 0x<code>`, exit 1 |
| host | Fake radio accepts the connection then closes it | `radio: connection closed by the radio`, exit 1 |
| host | `--radio nonsense` | the expected form, exit 2 |
| host | No argument | the banner only, exit 0 (unchanged) |

## Validation

- [x] Validate command green
- [x] Agent review done, blocking findings fixed
- [ ] Demo on the real Dongle Max seen and approved by the user
