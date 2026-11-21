# Plan: Spec 011

## Steps

- [x] 1. Living spec updated
- [x] 2. Core types: `OrderRequest`, commands
- [x] 3. Core logic: route and value rules; commands and their acknowledgement in the coordinator
- [x] 4. Adapters: order queue in the host
- [x] 5. API: the route
- [x] 6. Interface: not applicable
- [x] 7. Host: radio loop takes orders
- [x] 8. Simulator: dimmer (tests), silenced relay
- [x] 9. Tests; each module seen failing once
- [x] 10. Docs, decision D16

## Test plan

| Module | Scenario | Expected |
| ------ | -------- | -------- |
| zcl | On / Off / Toggle frames | `01 <tsn> 01`, `01 <tsn> 00`, `01 <tsn> 02` (frame control 0x01: cluster-specific, client to server, default response asked) |
| zcl | Move to Level with On/Off 200 | `01 <tsn> 04 c8 00 00 00 00` |
| orders | `true`, `"ON"`, `" On "`, `"true"`, `"1"`, `1`, empty → On; `false`, `"off"`, `"0"`, `0` → Off; `"Toggle"` → Toggle; `3`, `"x"` → 400 | as listed |
| orders | brightness 0, 254, 255, `"200"`, 12.5, -1, 256, "a" | 0, 254, 254, 200, 400, 400, 400, 400 |
| orders | Unknown equipment / disabled / unknown alias | 404 / 400 / 404 with Sowel's texts |
| orders | Two devices under one alias | two commands |
| coordinator | Command acknowledged by a Default Response success | success |
| coordinator | Default Response error status 0x81 | refused (status) |
| coordinator | No answer, answer after the 2 s retry | success; no answer twice: did not answer |
| coordinator | Reports from another device while a command waits | measures delivered |
| host | Order on the sim relay over HTTP | 200 `{"success":true}`; then `state` true; `lightsOn` 1; `false` back |
| host | Silenced relay | 502 `device did not answer` after the retry |
| host | Two devices, one silenced | 200 |
| host | 3 concurrent orders on a silenced relay | the 3rd: 503 Busy; a GET meanwhile: 200 |
| host | Order to a relay not heard since start (address in devices.json) | sent, 200 |
| host | Relay address unknown everywhere | 502 `device address unknown` |
| host | Order queued past its deadline | dropped, never sent |
| coordinator | Success then `onOff` read back | the read answer updates `state` (sim relay without reporting) |
| coordinator | Busy then retried; error status not retried | as listed |
| coordinator | Default Response from another device or with another TSN or command id | not the acknowledgement |
| orders | Order pushed by the core (no HTTP) | executed through the same queue |
| host | Brightness on the sim dimmer | 200; `brightness` reported |
| host | An order while a slow client streams a list | the order still answered (no lock held across I/O) |

## Validation

- [x] Validate command green
- [x] Agent review done, blocking findings fixed
- [ ] Demo seen and approved by the user
