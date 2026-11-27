# Plan: Spec 049

## Steps
- [x] 1. Living spec, Sowel subset, roadmap.
- [x] 2. Binding of a remote's output clusters.
- [x] 3. The command decoder and the `action` data; every press an event.
- [x] 4. Simulator button; tests; validate.
- [x] 5. Review.

Not applicable: API, interface (the device page shows any data).

## Test plan
| Module | Scenario | Expected |
| --- | --- | --- |
| zcl | each command of R2's table | its `action` value |
| zcl | an unknown command of `0x0008` | none |
| zcl | Recall scene 7 | `recall_7` |
| coordinator | a remote's interview (outputs `0x0006`, `0x0008`, no input `0x0006`) | Bind_req for both output clusters |
| coordinator | a light's interview (input `0x0006`) | no bind of its outputs |
| engine | `toggle` twice | two events to the listeners |
| recipes | `motion-light` with the button: a press | lights toggled |
| sim | the button scenario | `action` values in order |

## Validation
- `sh scripts/validate.sh --skip-slow` green.
