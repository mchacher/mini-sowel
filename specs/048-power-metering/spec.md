# Spec 048: Power, energy and current of a metering plug (I1, first part)

Written in an autonomous run (user, 2026-11-27: "continue jusqu'au bout de la nuit", merge on
green). I1 is cut in parts; this one is the electrical measures, the most common case (a smart
plug). Buttons (`action`), shutters, thermostats and `contact_window` are later parts.

## Context
The roadmap's I1 (rank 2): Solite knows 15 of Sowel's data categories. A metering plug (Sonoff
S60ZBTPF, Tuya and Ikea plugs) reports Electrical Measurement (`0x0B04`) and Metering
(`0x0702`); Solite probes both clusters at the interview (`crates/core/src/radio/coordinator.rs:617-618`)
but makes no data of them (`crates/core/src/devices/catalog.rs:118-131`). The equipment side
already takes them: the stale delays of `power`, `energy`, `current`
(`crates/core/src/equipments/status.rs:84-88`) and Sowel's equipment types for meters.

## Goals
- A paired plug shows its power (W), its energy (kWh) and its current (A), live, in Sowel's
  categories; an equipment can bind them.

## Out of scope
- The mains voltage (`0x0B04` RMSVoltage): the key `voltage` is the battery's (mV) today; a
  later part.
- Buttons, shutters, thermostats, `contact_window` (later parts of I1).
- Energy history, tariffs, the energy meter equipment types' rules (I2).

## Requirements

### R1: the data
| Key | Category (Sowel `DataCategory`) | Unit | Source |
| --- | --- | --- | --- |
| `power` | `power` | `W` | `0x0B04` ActivePower `0x050B` (int16); else `0x0702` InstantaneousDemand `0x0400` (int24, kW × multiplier / divisor, shown in W) |
| `energy` | `energy` | `kWh` | `0x0702` CurrentSummationDelivered `0x0000` (uint48) × multiplier `0x0301` / divisor `0x0302` |
| `current` | `current` | `A` | `0x0B04` RMSCurrent `0x0508` (uint16) × ACCurrentMultiplier `0x0602` / ACCurrentDivisor `0x0603` |
- Power from `0x0B04` uses ACPowerMultiplier `0x0604` / ACPowerDivisor `0x0605`.
- A device with both clusters gets one `power` (from `0x0B04`).
- Values are numbers rounded to 2 decimals for kWh and A, whole W for power.

### R2: the scale
- The multiplier and divisor attributes are read once per device and cluster when its reporting is
  set up (the existing Read Attributes path), and kept with the device (2 × 2 numbers, bounded by
  the device count). A divisor of 0 or absent counts as 1; a multiplier absent counts as 1.
- Until the scale is known, a value is not published (no wrong figure shown); a value received
  before is dropped, the next report carries it.

### R3: reporting
- Configure Reporting: ActivePower and RMSCurrent at most every 10 s, at least every 300 s, on a
  change of 1 unit; InstantaneousDemand (int24, `0x2A`) the same, for a plug with Metering only; CurrentSummationDelivered at most every 60 s, at least every 600 s.

### R4: the simulator
- A simulated plug (On/Off, `0x0B04`, `0x0702`) answers the scale reads and reports power,
  current and energy, for the tests and `--sim` scenarios.

## Living spec delta
### docs/living-spec/README.md
- The recognised data (around 350): add `power` (W), `energy` (kWh), `current` (A) from
  Electrical Measurement and Metering, published once their scale is read.
### docs/sowel-subset.md
- The data category table (20-40): three rows, `power`, `energy`, `current`, with their cluster.
### No longer true
- The roadmap's I1 note "Solite knows 15 of Sowel's about 75 categories" → 18.

## Acceptance criteria
- [x] A simulated plug paired: `power`, `energy`, `current` in Sowel's categories and units, with
      the scale applied (tested).
- [x] A report before the scale is read publishes nothing (tested).
- [x] Divisor 0 or absent counts as 1 (tested).
- [x] A device with both clusters has one `power` (tested).
- [x] Sowel contract: the three categories are words of the catalog, which `sowel_contract.rs`
      checks against Sowel's `DataCategory` (no assertion of its own needed).
- [x] validate green.

## Edge cases
- Divisor 0: counts as 1. Negative power (int16, export): kept, as Sowel.
- uint48 energy beyond f64 precision: not reached by a home meter (2^53 Wh).
- A plug with Metering only: `power` from InstantaneousDemand, `energy`, no `current`.
- More than `MAX_DATA` (8) data: the existing cut applies (state + 3 measures fits).

## Sowel contract
`DataCategory` "power", "energy", "current" (`src/shared/types.ts:7` of the Sowel repository).

## How the user will check it
A `--sim` scenario with a plug (`scenarios/plug.json`): the device page shows power, energy and
current; on the box, with a real plug when the user has one.

### Result (2026-11-27)
`validate --skip-slow` green; the simulated plug's power, energy and current with the scale applied. Code review (Sonnet): the Metering-only plug's demand was not configured (fixed), and the int24 type code was 0x2B, an int32 (fixed to 0x2A). Not run with a real plug: the user has none on the box yet.
