# Plan: Spec 050

## Steps
- [x] 1. Living spec, Sowel subset, roadmap.
- [x] 2. Catalog, measures, reporting.
- [x] 3. ZCL commands and order resolution.
- [x] 4. Simulator shutter and scenario; tests; validate.
- [x] 5. Review.

Not applicable: interface (the equipment page shows any order and data).

## Test plan
| Module | Scenario | Expected |
| --- | --- | --- |
| measures | lift 30 | `position` 70 % |
| measures | lift 0xFF | dropped |
| orders | `state` OPEN / CLOSE / STOP | commands 0x00 / 0x01 / 0x02 on `0x0102` |
| orders | `position` 25 | Go To Lift Percentage 75 |
| orders | `state` UP, `position` 120 | refused |
| zone orders | `allShuttersClose` on a zone with the shutter | Solite has no zone orders: stated absent, no test |
| catalog | a device with `0x0102` | `position` data, `state` and `position` orders |
| sim | the shutter scenario | moves to its target, reports |

## Validation
- `sh scripts/validate.sh --skip-slow` green.
