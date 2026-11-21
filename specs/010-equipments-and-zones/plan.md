# Plan: Spec 010

## Steps

- [x] 1. Living spec updated
- [x] 2. Core types: zones, equipments, bindings, aggregation
- [x] 3. Core logic: tree rules, auto-binding, status and stale, aggregation; API routes
- [x] 4. Adapters: zones.json, equipments.json
- [x] 5. API: routes of R1 to R4
- [x] 6. Interface: not applicable
- [x] 7. Host: nothing new on the command line
- [x] 8. Tests; each module seen failing once
- [x] 9. Docs, decision D14

## Test plan

| Module | Scenario | Expected |
| ------ | -------- | -------- |
| zones | First start | root `Maison`, id `…0001`, icon `home` |
| zones | Create without parent / with unknown parent | under root / 404 `Parent zone not found: <id>` |
| zones | Tree order | children by displayOrder then name |
| zones | Move under itself / under a descendant | 400 Sowel's texts |
| zones | Delete root / with 1 child / with 2 equipments / unknown | 400 texts (singular, plural) / 404 |
| zones | Delete a zone holding devices | 204; devices `zoneId` null |
| zones | Name empty / 65 characters; 33rd zone | 400 Invalid name; 400 Too many zones (32) |
| devices | PUT zoneId existing / unknown / null | 200 / 404 Zone not found / 200 |
| equipments | POST sensor with the SNZB-02 | 201; bindings temperature, humidity, battery, voltage with alias = key |
| equipments | POST light_onoff with the relay | data binding `state` and order binding `state` |
| equipments | POST with two devices having the same key | second binding skipped |
| equipments | Unsupported type / unknown zone / unknown device | 400 / 404 / 404 |
| equipments | GET ?type=sensor / ?type=unknown | filtered / [] |
| equipments | PUT name, zoneId, enabled; DELETE | updated; 204 |
| equipments | Add / delete bindings; alias taken | 201 / 204; 400 Alias already used |
| equipments | `lastChanged` | changes only when the value changes |
| equipments | Stale: temperature 66 min old / motion 2 days old | stale / not stale |
| equipments | Status: bound device offline / stale binding / fine | offline / degraded / online |
| equipments | Device deleted | its bindings removed, equipment stays |
| equipments | 65th equipment / 17th binding | 400 |
| aggregate | Two temperature sensors 19.0 and 20.25 | 19.6 (1 decimal) |
| aggregate | A temperature binding with alias `temp` | not counted |
| aggregate | Humidity, luminosity | mean 1 decimal, rounded mean |
| aggregate | Motion true / false transitions | motion, motionSensors, motionSince at each change |
| aggregate | Door contact false / window contact false | openDoors 1 / openWindows 1 |
| aggregate | Leak, smoke | true when one is true |
| aggregate | Light equipment state true | lightsOn 1, lightsTotal 1 |
| aggregate | Offline equipment | skipped; unavailable maps filled |
| aggregate | Child zone values | included in the parent and in the root |
| aggregate | No values yet | null, 0, false; every field present |
| sowel json | Zone, ZoneWithChildren, EquipmentWithDetails, ZoneAggregatedData | equal to examples written from Sowel's `types.ts` |
| store | Save and reload zones and equipments | same |
| store | equipments.json not parsable | the host does not start |
| memory ceiling | 50 devices × 6 data + 10 zones + 50 equipments × 6 bindings, lists and aggregation serialised | peak < 200 KB |
| host | `--sim garage --serve` after pairing: zone, devices in it, three equipments | aggregation of Garage and Maison: temperature, motion, lightsTotal 1 |

## Validation

- [x] Validate command green
- [x] Agent review done, blocking findings fixed
- [ ] Demo seen and approved by the user
