# Plan: Spec 051

## Steps
- [ ] 1. Living spec, Sowel subset, roadmap.
- [ ] 2. Catalog, measures, reporting.
- [ ] 3. The setpoint write and its order.
- [ ] 4. Simulator thermostat and scenario; tests; validate.
- [ ] 5. Review.

Not applicable: interface.

## Test plan
| Module | Scenario | Expected |
| --- | --- | --- |
| measures | LocalTemperature 2150 | `local_temperature` 21.5 °C |
| measures | LocalTemperature 0x8000 | dropped |
| measures | OccupiedHeatingSetpoint 1900 | `setpoint` 19 °C |
| orders | `setpoint` 20.5 | Write Attributes 0x0012 int16 2050 on 0x0201 |
| orders | 4, 31, "a" | refused |
| catalog | a device with 0x0201 and 0x0402 | `temperature` and `local_temperature`, both |
| sim | the thermostat scenario | the write taken and reported |

## Validation
- `sh scripts/validate.sh --skip-slow` green.
