# Spec 051: A thermostat's temperature and setpoint (I1, fourth part)

Written in an autonomous run (user, 2026-11-27: continue through the night, merge on green).

## Context
The roadmap's I1: the equipment type `thermostat` exists and takes `setpoint` and the order
`set_setpoint` (`crates/core/src/equipments/rules.rs:60-64,108`; the stale delay of `setpoint`,
`crates/core/src/equipments/status.rs:84`), Sowel's zone order `allThermostatsSetpoint` sends
`set_setpoint` in °C; but the Thermostat cluster (`0x0201`) appears in no code. A Zigbee radiator
valve or thermostat (Sonoff TRVZB, Danfoss Ally, Bosch, Hive) is a `0x0201` server.

## Goals
- A paired thermostat shows its measured temperature and its heating setpoint, and its setpoint is
  set from Solite, in Sowel's categories, generic over the ZCL.

## Out of scope
- `toggle_power` (System Mode `0x001C`), the cooling setpoint, schedules, the valve's opening
  (`0x0008` PI heating demand), manufacturer clusters (Tuya `0xEF00` valves): later.

## Requirements

### R1: the data
| Key | Category | Unit | Source |
| --- | --- | --- | --- |
| `local_temperature` | `temperature` | `°C` | `0x0201` LocalTemperature `0x0000`, int16, 0.01 °C; `0x8000` invalid, dropped |
| `setpoint` | `setpoint` | `°C` | `0x0201` OccupiedHeatingSetpoint `0x0012`, int16, 0.01 °C |
- Values rounded to 0.1 °C. A device that also has `0x0402` keeps its `temperature`; the
  thermostat's is `local_temperature` (two keys, no clash).
- Reporting: LocalTemperature 30..600 s, change 10 (0.1 °C); OccupiedHeatingSetpoint 1..600 s,
  change 10.

### R2: the order
- Key `setpoint`, category `set_setpoint`, a number in °C, 5 to 30, any step of 0.5 accepted as
  given (0.01 °C resolution): a Write Attributes of `0x0012` (int16, value × 100). Outside 5..30 or
  not a number: refused (the existing order validation).
- The device's report then gives the setpoint; no optimistic value is published.

### R3: the simulator
- A simulated thermostat (`0x0201`) that reports its temperature and setpoint and takes the
  setpoint write.

## Living spec delta
### docs/living-spec/README.md
- The recognised data and orders: a thermostat's `local_temperature`, `setpoint` and its
  `setpoint` order (5 to 30 °C).
### docs/sowel-subset.md
- The data table: `setpoint` (and `temperature` from `0x0201`); the order table: `set_setpoint`.
### No longer true
- The roadmap's I1 counts and its note on `thermostat`.

## Acceptance criteria
- [x] LocalTemperature 2150: `local_temperature` 21.5 °C; 0x8000 dropped (tested).
- [x] OccupiedHeatingSetpoint 1900: `setpoint` 19 °C (tested).
- [x] Order `setpoint` 20.5: Write Attributes `0x0012` int16 2050 (tested).
- [x] Order 4 or 31 or "a": refused (tested).
- [x] The simulated thermostat takes the write and reports it (tested).
- [x] validate green.

## Edge cases
- `0x8000` (invalid) on either attribute: dropped.
- A write the device refuses (its Write Attributes Response not success): logged with the
  device's own status (0x86, 0x87, 0x88...); the setpoint stays what the device reports.
- Rounding to 0.1 °C is half away from zero (20.25 → 20.3).
- Limits of the device (MinHeatSetpointLimit `0x0015`, Max `0x0016`) narrower than 5..30: the
  device refuses; not read in this part.

## Sowel contract
`DataCategory` "temperature", "setpoint"; `OrderCategory` "set_setpoint"
(`src/shared/types.ts:7,109` of the Sowel repository).

## How the user will check it
`--sim` with a thermostat scenario (`scenarios/thermostat.json`): the equipment page shows the
temperature and the setpoint, and setting 20.5 °C is followed by the report.

### Result (2026-11-28)
`validate --skip-slow` green (1894 tests). Code review (Sonnet): no blocking finding; a refused write reported 0x01 instead of the device's code (fixed, tested). Not run with a real thermostat: none on the box.
