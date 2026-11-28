# Spec 050: A shutter's position and orders (I1, third part)

Written in an autonomous run (user, 2026-11-27: continue through the night, merge on green).

## Context
The roadmap's I1: the equipment type `shutter` exists, its rules take `shutter_position`,
`shutter_move` and `set_shutter_position` (`crates/core/src/equipments/rules.rs:43,102-104`), the
zones aggregate it (`crates/core/src/zones/aggregate.rs:196,249`), Sowel's zone orders
`allShuttersOpen|Stop|Close` send `shutter_move` `OPEN|STOP|CLOSE` (Sowel
`docs/technical/data-model/zones.md:111-113`); but no device makes these data or takes these
orders: the Window Covering cluster (`0x0102`) appears in no code.

## Goals
- A paired Zigbee shutter (Window Covering) shows its position and opens, closes, stops and goes to
  a position from Solite, in Sowel's categories and values.

## Out of scope
- Tilt (blinds' slats), the calibration of travel times, awnings' own type rules.
- A shutter with no position attribute (moves only): its orders work, it has no position.

## Requirements

### R1: the data
- Key `position`, category `shutter_position`, `%`, 0 to 100, **100 open, 0 closed** (Sowel and
  Zigbee2MQTT's convention): `100 - CurrentPositionLiftPercentage` (`0x0102` attribute `0x0008`,
  uint8, where the ZCL's 100 is closed). A value above 100 is dropped.
- Reporting: `0x0008` at most every 1 s, at least every 300 s, on a change of 1.

### R2: the orders
| Key | Category | Values | Command |
| --- | --- | --- | --- |
| `state` | `shutter_move` | `OPEN`, `CLOSE`, `STOP` | Up/Open `0x00`, Down/Close `0x01`, Stop `0x02` |
| `position` | `set_shutter_position` | 0..100 (100 open) | Go To Lift Percentage `0x05` with `100 - value` |
- Another value: refused as other orders' wrong values are (the existing order validation).
- After an order, the device's reports give the position; no optimistic value is published.

### R3: the simulator
- A simulated shutter (`0x0102`, position) that moves towards its target by 10 points a second
  and reports, and answers Up, Down, Stop and Go To Lift Percentage.

## Living spec delta
### docs/living-spec/README.md
- The recognised data and orders: a shutter's `position` (100 open) and its orders `state`
  (OPEN, CLOSE, STOP) and `position`.
### docs/sowel-subset.md
- The data table: `shutter_position`; the order table: `shutter_move`, `set_shutter_position`.
### No longer true
- The roadmap's I1 count (19 → 20 data, order categories 4 → 6); its note on `shutter`.

## Acceptance criteria
- [ ] A lift percentage of 30 reported: `position` 70 (tested).
- [ ] `state` `OPEN`, `CLOSE`, `STOP` send commands 0x00, 0x01, 0x02 (tested).
- [ ] `position` 25 sends Go To Lift Percentage 75 (tested).
- [ ] A wrong value (`state` `UP`, `position` 120) refused (tested).
- [ ] Sowel's zone order `allShuttersClose` reaches the shutter (tested, if the zone orders route
      exists in Solite; else stated).
- [ ] The simulated shutter moves and reports (tested).
- [ ] validate green.

## Edge cases
- A lift percentage above 100 (0xFF "unknown"): dropped.
- STOP while not moving: sent; the device ignores it.
- A shutter without the position attribute: orders only.

## Sowel contract
`DataCategory` "shutter_position", `OrderCategory` "shutter_move", "set_shutter_position"
(`src/shared/types.ts:7,109` of the Sowel repository); zone orders (`docs/technical/data-model/zones.md:111-113`).

## How the user will check it
`--sim` with a shutter scenario (`scenarios/shutter.json`): the equipment page opens, closes,
stops and sets a position, the position following.
