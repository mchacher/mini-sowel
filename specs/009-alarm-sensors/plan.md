# Plan: Spec 009

## Steps

- [x] 1. Living spec updated
- [x] 2. Core types: enrollment outcome
- [x] 3. Core logic: enrollment in the coordinator; zone data in the catalog and registry
- [x] 4. Adapters: not applicable
- [x] 5. API: not applicable (data appear through spec 008's routes)
- [x] 6. Interface: not applicable
- [x] 7. Host: `reporting:` line
- [x] 8. Simulator: the motion device answers the IAS reads
- [x] 9. Tests; each module seen failing once
- [x] 10. Docs

## Test plan

| Module | Scenario | Expected |
| ------ | -------- | -------- |
| zcl | Write Attributes of 0x0010 (EUI64, type 0xF0) | bytes: attr LE, 0xF0, EUI64 LE |
| zcl | Zone Enroll Response, zone 23 | `11 <tsn> 00 00 17` |
| zcl | Zone Enroll Request parsed | zone type, manufacturer code |
| coordinator | Motion sensor not enrolled | read, write CIE, 500 ms, response, read state 1 → Enrolled (motion), before the binds |
| coordinator | Already enrolled with our EUI64 | no write; Enrolled |
| coordinator | Enrolled with another EUI64 | rewritten, enrolled |
| coordinator | Zone state never 1 (20 reads) | Unconfirmed; zone data added |
| coordinator | A zone state poll unanswered | Unconfirmed at once, within the 70 s cap |
| coordinator | CIE write refused / unanswered | Failed; no zone data |
| coordinator | Failed enrollment, then a message from the device | tried once more |
| coordinator | Enroll Request in `--listen` mode | answered |
| coordinator | Enroll Request after the outcome with another zone type | answered, no data added |
| coordinator | `reporting:` line combinations (none / failed clusters / no descriptor) | the texts of R4.8 |
| coordinator | No answer to the first read | Failed; no zone data; reporting setup still runs |
| coordinator | Zone Enroll Request after pairing | Enroll Response with the request's TSN, disable-default-response set |
| coordinator | Read gave no zone type, Enroll Request carries 0x0015 | contact |
| devices | Zone types motion / contact / water / fire / 0x002D | occupancy / contact / water_leak / smoke / only tamper (+ tamper for all) |
| devices | Notification alarm 1 then 0 on motion | occupancy true then false |
| devices | Notification alarm 1 on contact | contact false (open) |
| devices | Tamper bit / battery-low bit | `tamper` / `battery_low` added then true |
| devices | Motion sensor never sending bit 2 | no `tamper` data |
| devices | Zone status attribute 0x0002 in a report | same as a notification |
| devices | Zone data kept in devices.json, read by a later process | known |
| sim | Motion device after enrollment, scenario motion | notifications sent; before: none |
| sim | Second `--pair` of the enrolled sensor | CIE read back = ours, no write, enrolled |
| host | `--sim garage --pair 30 --speed 10` | `reporting: <eui64> battery, enrolled (motion)` |
| host | Then `--serve`: motion event | `occupancy` true in the API, false 60 s (scenario time) later |

## Validation

- [x] Validate command green
- [x] Agent review done, blocking findings fixed
- [ ] Demo seen and approved by the user
