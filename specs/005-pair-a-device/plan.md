# Plan: Spec 005

## Steps

- [x] 1. Living spec updated
- [x] 2. Core types: `Network`, `NetworkSettings`, `PairingEvent`
- [x] 3. Core logic: EZSP commands and callbacks, ZDO, ZCL, coordinator
- [x] 4. Adapters: random source in the host
- [x] 5. API: not applicable
- [x] 6. Interface: not applicable
- [x] 7. Host: `--pair`
- [x] 8. Tests of the test plan; each module seen failing once
- [x] 9. Docs
- [ ] 10. Demo on the real Dongle Max with the SNZB-02 (with the user)

## Test plan

| Module | Scenario | Expected |
| ------ | -------- | -------- |
| ezsp | `formNetwork` parameters | 20 bytes, layout of the reference (ext PAN, PAN LE, power 5, channel, 0, 0, 0, mask 0x07FFF800 LE) |
| ezsp | `setInitialSecurityState` | 43 bytes, bitmask 0x0B84 LE, keys, seq 0, EUI64 ff×8 |
| ezsp | `stackStatusHandler` NETWORK_UP in EZSP 13 (u8) and 14+ (u32) | network up |
| ezsp | `trustCenterJoinHandler` bytes | node id, EUI64, status, decision |
| ezsp | `incomingMessageHandler` in EZSP 13 and in 14+ layouts | the same message (profile, cluster, endpoints, sender, payload) |
| ezsp | `messageSentHandler` in 13 and 14+ | status and tag |
| ezsp | `sendUnicast` in 13 (u8 tag) and 14+ (u16 tag) | bytes of the reference |
| zdo | `Device_annce` payload | nwk, EUI64 |
| zdo | `Active_EP_req` | `[tsn][nwk LE]` |
| zdo | `Active_EP_rsp` success with endpoints [1] / failure status | endpoints / error |
| zcl | Read Attributes request 0x0004 and 0x0005 | `10 <tsn> 00 04 00 05 00` |
| zcl | Response with two strings | manufacturer and model |
| zcl | Response with 0x0004 unsupported (status 0x86) | manufacturer unknown, model read |
| zcl | String longer than the frame | unknown, no panic |
| coordinator | Radio with no network (EZSP 13) | formed with the given random values and channel 15; commands in order: initial security state, extended security bitmask, clear key table, form, (network up), token writing 1, getNetworkParameters |
| coordinator | Start-up configuration | INDIRECT_TRANSMISSION_TIMEOUT 7680, MAX_END_DEVICE_CHILDREN 32, END_DEVICE_POLL_TIMEOUT 8 among the values set |
| coordinator | NETWORK_UP arrives before the formNetwork response | started |
| coordinator | 20 unused callbacks (child join, route record) then a trust-centre join | the join not lost |
| coordinator | Response with the overflow flag | used, not an error |
| coordinator | Random PAN 0xFFFF drawn first | drawn again |
| coordinator | `networkInit` not joined: 0x93 (EZSP 13), 0x17 (EZSP 14+) | formed in both |
| coordinator | Radio with no network (EZSP 14+) | formed |
| coordinator | Radio with a network as coordinator | resumed, no form command sent |
| coordinator | Radio joined as router (getNetworkParameters node type 2) | error "not a coordinator", no leave command |
| coordinator | No NETWORK_UP within 10 s (fake time) | "network did not start" |
| coordinator | `permit_join(180)` | transient key, policy 0x03, permitJoining 180, in that order |
| coordinator | `permit_join(0)` and 255 | refused (1 to 254) |
| coordinator | Join then announce of the same device | one Joined event |
| coordinator | Joined device announces, answers endpoints and Basic | Identified with eWeLink / TH01 |
| coordinator | Trust-centre join without announce | identification starts 5 s later |
| coordinator | Answer from another device, or another TSN, or a Default Response | ignored |
| coordinator | Strings with trailing 0x00, or not UTF-8 | trimmed, read lossily |
| coordinator | Device joining 1 s before the window ends | identified before closing |
| coordinator | Device silent on endpoints | Basic read on endpoint 1 |
| coordinator | Device silent on Basic, one retry | Identified with both unknown |
| coordinator | Trust-centre join denied, or status "left" | no event |
| coordinator | sendUnicast busy twice then OK | identified; busy 3 times: NotIdentified, session goes on |
| ezsp | clearTransientLinkKeys | its empty response awaited |
| ezsp | messageSentHandler EZSP 13 with trailing length byte | parsed |
| coordinator | 33rd device | TooManyDevices, ignored |
| coordinator | Callback between a command and its response | not lost |
| coordinator | Closing | permitJoining 0, clear transient keys, policy 0x02 |
| host | `--pair 5` against a fake radio with one device | the lines of R4, `pairing: closed`, exit 0; `network.json` written, mode 0600, keys not in the output |
| host | `--pair` twice on the same fake radio (it keeps its network) | second run `network: resumed`, backup unchanged |
| host | Fake radio with a coordinator network and no backup | `network: resumed (made by another program)` |
| host | Data folder not writable | `cannot write <path>`, exit 1, no bytes sent to the radio |
| host | SIGINT during pairing | permitJoining 0 sent, exit 130 |
| host | `--pair 0` / `--pair abc` | usage, exit 2 |
| host | No `--pair` | spec 004's output, unchanged |

## Validation

- [x] Validate command green
- [x] Agent review done, blocking findings fixed
- [ ] Demo on the real Dongle Max seen and approved by the user
