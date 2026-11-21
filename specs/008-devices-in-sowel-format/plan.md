# Plan: Spec 008

## Steps

- [x] 1. Living spec updated
- [x] 2. Core types: `Device`, `DeviceData`, `DeviceOrder`, `DeviceWithDetails`
- [x] 3. Core logic: catalog, registry, API routing
- [x] 4. Adapters: `devices.json` store in the host
- [x] 5. API: routes of R5, served by `tiny_http`
- [x] 6. Interface: not applicable
- [x] 7. Host: `--serve`, `--verbose`; `--pair` updates `devices.json`
- [x] 8. Tests of the test plan; each module seen failing once
- [x] 9. Docs, decision D13

## Test plan

| Module | Scenario | Expected |
| ------ | -------- | -------- |
| sowel json | A SNZB-02 device serialised | equals an example written from Sowel's `types.ts`: field names, camelCase, `zoneId: null`, `unit` present for temperature, absent for state, `value` and `lastUpdated` `null` before a value, ISO dates with milliseconds |
| sowel json | Relay orders | `state` boolean light_toggle valueOn "ON" valueOff "OFF" |
| sowel json | Unknown manufacturer and model | fields omitted, name `Zigbee 5678` |
| catalog | Each row of R2's table | the data (key, type, category, unit); `voltage` in mV |
| registry | Pairing a SNZB-02 (0x0001, 0x0402, 0x0405 set up) | temperature, humidity, battery, voltage; name `TH01 5678` |
| registry | Pairing the same EUI64 again with one more cluster | same id, name, data ids; one data added; `updatedAt` changed |
| registry | Measure temperature 19.52 | value 19.52, lastUpdated, lastSeen; `updatedAt` unchanged |
| registry | Measure from a device not in the store | ignored |
| registry | Measure for a data the device lacks | ignored |
| registry | Status: battery 24 h / 26 h; mains 9 / 11 min; unknown power 26 h; no message | online / offline; online / offline; offline; unknown |
| registry | 65th device / 17th data / 5th order | not added; `pairing: device store full (64), <eui64> ignored` for the device |
| api | GET list | 200, sorted by createdAt; data in table order |
| api | GET by id / unknown id | 200 / 404 `{"error":"Device not found"}` |
| api | PUT name / `{}` / empty name / 65 characters / unknown field / zoneId "x" / zoneId null / not JSON | 200 Device / 400 Nothing to update / 400 Invalid name / 400 Invalid name / 400 Unknown field: x / 400 zoneId must be null / 200 / 400 Invalid JSON |
| api | DELETE / unknown | 204 no body / 404 |
| api | Unknown path / wrong method / body over 16 KB / Host `evil.example` | 404 / 405 / 413 / 403 |
| store | Save then load | same devices |
| store | File not parsable | `devices: <file>: cannot read: ...`, file unchanged, no start |
| store | Measures for 9 minutes (fake clock) | no value write; at 10 min one write |
| store | A rename | written at once |
| store | Stop | written |
| store | HTTP rename while the radio loop runs | one writer (the radio loop), no lost update |
| store | File mode | 0600 |
| memory ceiling | 50 devices × 6 data × 1 order, list serialised, alone in its test file | peak under 200 KB |
| sim | Relay announce | capability 0x8C → `powerSource` `mains` |
| host | `--sim garage --pair 30 --speed 10 --data <tmp>` then `--serve <free port> --speed 10` | `serving:` line; three devices; the relay `mains` with `state` data and order; temperature changes between two calls |
| host | Port in use | `serve: cannot listen on ...`, exit 1 |
| host | `--serve` with `--pair` / without radio | usage, exit 2 |
| host | Restart `--serve` | same devices, same ids, last values kept |
| host | SIGINT while serving | store written, exit 130 |

## Validation

- [x] Validate command green
- [x] Agent review done, blocking findings fixed
- [ ] Demo seen and approved by the user
