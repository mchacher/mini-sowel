# Plan: Spec 002

## Steps

- [x] 1. Living spec updated (delta of spec.md)
- [x] 2. Core types: not applicable (no Rust change)
- [x] 3. Core logic: not applicable; the logic lives in the tool (`tools/z2m-coverage/src/`)
- [x] 4. Adapters: not applicable
- [x] 5. API: not applicable
- [x] 6. Interface: not applicable
- [x] 7. Tool: package, `origin`, `collect`, `classify`, `brands`, `aggregate`, `report`, `main`
- [x] 8. Tests of the test plan, each one seen failing once by breaking the code
- [x] 9. `report.md` generated and committed
- [x] 10. `scripts/validate.sh`, `.github/workflows/ci.yml`, `.gitignore`
- [x] 11. Docs: architecture, decision D10, README, CLAUDE.md; "No longer true" search

## Test plan

Synthetic data only (small fake source files and definitions), except the last row.

| Module | Scenario | Expected |
| ------ | -------- | -------- |
| origin | Function text found in a common file (`converters/fromZigbee.js`) | shared |
| origin | Function text found in a brand file (`lib/ikea.js`) | brand |
| origin | Function text found in `lib/tuya.js` | shared |
| origin | Function text found in a device file | device |
| origin | Function text found nowhere | unknown |
| origin | Function text found in a shared file and a device file | shared |
| origin | Function text found in a brand file and a device file | brand |
| origin | Same text in two device files | device |
| origin | Function text found in `lib/legacy.js` | brand |
| origin | Function text found in `index.js` | shared |
| collect | Definition with fromZigbee, toZigbee, configure, onEvent, meta function | every function collected once |
| collect | A modern extension in `extend[]` with fromZigbee, configure list, onEvent list | each function collected from the extension, none from a wrapper |
| collect | `endpoint` function and converter `options` | not collected |
| collect | A function from `configure` | marked configure |
| collect | Tuya datapoint table with a value converter function | the converter's function collected |
| collect | Definition with no function | empty list |
| collect | `toZigbee` converter with `convertSet` and `convertGet` (the library's shape) | both collected |
| main | Converters added to every definition by `prepareDefinition` | found (11 in 26.135.0), and the report's count matches the names it lists |
| aggregate | Unknown functions | the report gives their number and the number of definitions device code only because of them |
| classify | Tuya datapoints, shared functions only | Tuya table |
| classify | Tuya datapoints with a device function | device code |
| classify | Shared functions only | declarative |
| classify | No function at all | declarative |
| classify | Shared and brand functions | brand code |
| classify | A brand and a device function | device code |
| classify | An unknown function | device code |
| classify | Device functions only from configure | device code, configure only |
| classify | A device function in a fromZigbee converter | device code, not configure only |
| classify | fromZigbee clusters all standard | standard |
| classify | One fromZigbee cluster not standard (`manuSpecificTuya`) | not standard |
| classify | No fromZigbee converter | not standard |
| classify | Standard clusters and `manuSpecificPhilips2` | standard with extras, not standard |
| classify | Only `manuSpecificTuya` | neither standard nor standard with extras |
| classify | A standard cluster and a non-standard cluster that is not manufacturer-specific | neither |
| aggregate | Definitions of two vendors of one brand | one row, counts added |
| aggregate | Vendor `SONOFF` for brand Sonoff | matched |
| aggregate | Two brands with the same number of definitions | sorted by name |
| aggregate | Converters added to every definition by `prepareDefinition` | left out of the shared-function count |
| aggregate | Brand with no definition | row with 0, percentages 0, no NaN |
| aggregate | White labels | counted in the column, not as definitions |
| aggregate | Definition of a vendor of no brand | in the library total only |
| aggregate | Distinct shared functions of exportable definitions | the same function used twice counts once |
| aggregate | A definition that threw when prepared | device code, its model listed |
| report | Rendering of a known aggregate | header, brand rows sorted by size, totals, devices of interest, limits |
| main | Library loads nothing (injected empty list) | non-zero exit, no report written |
| main | The loader throws | non-zero exit, the reason printed, no report written |
| main | Real library, pinned version | the report has the SNZB-02 line and the totals add up |
| main | Real library: a modern-extend light (IKEA or Philips) | declarative |
| main | Real library: a Philips Hue colour bulb | standard with extras |
| main | Real library: the share of Tuya tables in Tuya's row | above the share before the fix (9 %) |
| main | Real library: the SNZB-02 | device code, configure only is false (it has a local fromZigbee converter), standard |

## Validation

- [x] Validate command green
- [x] Agent review done, blocking findings fixed
- [ ] Demo seen and approved by the user (the report, for the user to read on 2026-11-21)
