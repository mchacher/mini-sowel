# Plan: Spec 044

## Steps

- [x] 1. Living spec, `CLAUDE.md`, `README.md`, `docs/sowel-subset.md`, D80
- [x] 2. The constants: `MAX_DATA` 8, `MAX_BINDINGS` 8 (core and interface), `MAX_LOG_ENTRIES` 20
      (params unchanged, R4)
- [x] 3. The device's "not added" line once per device and key; the loads' cut lines; the home's
      load drops dangling bindings before it cuts at 8
- [x] 4. The interface's form and texts at 8
- [x] 5. `measure-api.sh` and the box session's B6 at 17 requests
- [x] 6. Tests moved to the new bounds; the worst-case test at the new bounds
- [x] 7. `docs/memory-budget.md` measured again (Mac worst case, realistic home, QEMU)

## Test plan

| Module | Scenario | Expected |
| ------ | -------- | -------- |
| devices | A device given a 9th data, by two frames | Not added; `more than 8 data, <key> not added` logged once |
| devices | The IAS motion multi-sensor paired (occupancy, temperature, humidity, illuminance, battery, voltage, then tamper and battery low set) | Its 8 data all there |
| devices | `devices.json` with a device of 12 data loaded | First 8 kept, in the file's order; one log line; the other devices unchanged |
| home | An equipment bound to that device's 10th data, loaded | That binding dropped, the others kept |
| home | `equipments.json` with 10 data bindings on one equipment, one to a data gone | 8 kept (the dangling one dropped first, then the first 8 valid), one log line, the equipments marked to be written |
| home | `equipments.json` with 9 order bindings, all valid | First 8 kept, one log line |
| equipments | The 9th data binding, then the 9th order binding, through the API | 400 `Too many bindings (8)` each |
| equipments | An equipment created from 3 colour bulbs | 8 data and 8 order bindings, the others skipped and logged |
| recipes | 25 log lines pushed to one instance | The last 20 kept, oldest dropped |
| api | `GET .../log?limit=50`; `?limit=5` | 20 lines; 5 lines |
| ui | `bindingUtils`: devices giving 10 values | 8 planned; the warning "more than 8 values, of which only 8 are bound" (French and English) |
| ui | `DeviceSelector`: the same devices | The warning shown |
| tools | `measure-api.sh` against the Mac host | 17 requests, 17 answered 201 |
| memory | `memory_worst_case` at the new bounds | Under 1 MiB; the figure printed (target about 715 KB) |
| memory | `memory_ceiling` (realistic home) | Under 300 KB, unchanged or lower |
| box | The garage after the flash with `--keep-storage` | Loads unchanged; the recipe runs; at most 20 log lines |

## Validation

- [x] Validate command green
- [x] Agent review done, blocking findings fixed
- [x] Demo seen and approved by the user (the box, 2026-11-27: "ça marche")
