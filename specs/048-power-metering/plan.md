# Plan: Spec 048

## Steps
- [x] 1. Living spec, Sowel subset, roadmap.
- [x] 2. Catalog, measures, scale read, reporting.
- [x] 3. Simulator plug and scenario.
- [x] 4. Tests; validate.
- [x] 5. Review.

Not applicable: API (the data go through the existing routes), interface (the device page shows
any data).

## Test plan
| Module | Scenario | Expected |
| --- | --- | --- |
| measures | ActivePower 1234 with multiplier 1 divisor 10 | `power` 123 W |
| measures | RMSCurrent 523, multiplier 1, divisor 1000 | `current` 0.52 A |
| measures | CurrentSummationDelivered 123456, divisor 1000 | `energy` 123.46 kWh |
| measures | InstantaneousDemand on a Metering-only plug | `power` in W |
| measures | report before the scale is read | nothing published |
| measures | divisor 0 or absent | counts as 1 |
| catalog | a device with `0x0B04` and `0x0702` | one `power`, one `energy`, one `current` |
| sim | the plug paired in a scenario | the three data with their units |
| contract | the three categories | in `sowel_contract.rs` and `docs/sowel-subset.md` |

## Validation
- `sh scripts/validate.sh --skip-slow` green.
