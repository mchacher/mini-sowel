# Plan: Spec 006

## Steps

- [x] 1. Living spec updated
- [x] 2. Core types: `Measure`, `MeasureEvent`, `ReportingConfigured`
- [x] 3. Core logic: ZCL and ZDO frames, measures table, reporting setup, listen
- [x] 4. Adapters: not applicable (TCP transport of spec 004)
- [x] 5. API: not applicable
- [x] 6. Interface: not applicable
- [x] 7. Host: `--listen`, lines during `--pair`
- [x] 8. Tests of the test plan; each module seen failing once
- [x] 9. Docs
- [ ] 10. Demo on the real Dongle Max with the SNZB-02 (with the user)

## Test plan

| Module | Scenario | Expected |
| ------ | -------- | -------- |
| measures | Temperature 1952 (int16) | 19.52 °C |
| measures | Temperature −550 | −5.5 °C |
| measures | Temperature 0x8000, and 15500 (155 °C) | no measure |
| measures | Humidity 10100 (101 %) | no measure |
| measures | Temperature 1950 | shown `19.5` |
| measures | Humidity 5410 | 54.1 % |
| measures | Pressure 1013 | 1013 hPa |
| measures | Illuminance 0 / 10001 / 30001 / 0xFFFF | 0 lx / 10 lx / 1000 lx / none |
| measures | Occupancy 0x01 / 0x00 | occupied / not |
| measures | On/off 1 / 0, level 254 / 127 | on / off, 100 % / 50 % |
| measures | Battery 200 / 0xFF, voltage 30 | 100 % / none, `3 V` |
| measures | Pressure 0x8000 | no measure |
| measures | Known attribute with an unexpected type | none |
| measures | Unknown cluster or attribute | none |
| zcl | Report Attributes with two records (temperature) | two values |
| zcl | Truncated record | the complete ones, no panic |
| zcl | Report whose first record is a string (0x42) then a temperature | the temperature |
| zcl | Report with a record of an unknown-length type then a temperature | nothing after it, no panic |
| zcl | Read Attributes Response record with status 0x86 then a temperature record | the temperature |
| zcl | Configure Reporting request for occupancy (bitmap8) | no change field |
| zcl | Configure Reporting request for battery % | uint8 change 2 |
| zcl | Configure Reporting Response single byte 0x00 | all accepted |
| zcl | Default Response 0x86 to Configure Reporting | cluster failed |
| zcl | Default Response received | nothing sent back |
| zcl | Same report received twice (same TSN) | one Default Response |
| zcl | Report with "disable default response" set, or broadcast | no Default Response |
| zcl | Configure Reporting request for temperature | bytes: direction 0, attr, type 0x29, min 30, max 3600, change 20 (LE) |
| zcl | Configure Reporting response all success / one failure | ok / that attribute failed |
| zcl | IAS zone status change 0x0001 / 0x0004 | alarm / tamper |
| zcl | Default Response built for a report (disable-default-response clear) | bytes, TSN echoed; none when the bit is set |
| zdo | Simple_Desc_req bytes, Simple_Desc_rsp with in clusters 0x0000 0x0001 0x0402 0x0405 | the clusters |
| zdo | Bind_req bytes (src EUI64, ep, cluster, dst mode 3, dst EUI64, ep 1) | as UG/ZDO spec |
| zdo | Bind_rsp success / failure | ok / failed |
| coordinator | Fake SNZB-02 paired | Bind and Configure Reporting for 0x0001, 0x0402, 0x0405; ReportingConfigured with the three |
| coordinator | Simple_Desc unanswered twice | ReportingConfigured failed: no descriptor |
| coordinator | Bind refused for 0x0405 | 0x0405 failed, the others ok |
| coordinator | Configure unanswered after its retry | setup stops, remaining clusters failed |
| coordinator | Failed clusters, then a message from the device | tried again once |
| coordinator | A report arrives between Bind and the Configure Reporting Response | measure shown, the response still matched |
| coordinator | EUI64 all zeros in the packet info, radio does not know it | nwk shown |
| coordinator | Frame giving more than 16 measures | 16 kept |
| coordinator | Configure Reporting answered by a Default Response 0x86 | cluster failed, retry armed |
| coordinator | Configure Reporting response with a refusal record for the only attribute | cluster failed |
| coordinator | Power cluster: battery % accepted, voltage refused | set up |
| coordinator | Power cluster answered by a single failure byte 0x86 | failed |
| coordinator | Manufacturer-specific report on cluster 0x0006 | no measure |
| zcl | Default Response to a Read Attributes request from a device / to a cluster-specific request | 0x82 / 0x81 |
| coordinator | Reports from the paired device | MeasureEvent with its EUI64 |
| coordinator | Report from an unknown nwk, radio knows its EUI64 / does not | EUI64 / nwk |
| coordinator | Address cache past 64 entries | oldest dropped |
| coordinator | listen() on a radio with no network | error "no network" |
| coordinator | listen() resumes, never forms | no formNetwork sent |
| host | `--pair 1` with the fake SNZB-02 | `reporting:` line, then measure lines |
| host | Second run `--listen` on the same fake radio, which sends reports | `network: resumed`, `listening`, measure lines; SIGINT → 130 |
| host | `--listen` on a fake radio with no network | `radio: no network, pair a device first`, exit 1 |
| host | `--listen` and `--pair` together | usage, exit 2 |

## Validation

- [x] Validate command green
- [x] Agent review done, blocking findings fixed
- [ ] Demo on the real Dongle Max seen and approved by the user
