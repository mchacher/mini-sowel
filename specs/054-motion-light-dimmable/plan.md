# Plan: Spec 054

## Steps
- [ ] 1. Living spec, roadmap (md and html).
- [ ] 2. The script, from Sowel's source.
- [ ] 3. Tests (info pinned, validation texts, the behaviour per slot, memory).
- [ ] 4. validate; review.
- [ ] 5. Signed, published in the catalogue, installed on the box.

Not applicable: core types, API, interface (a script recipe uses the existing ones).

## Test plan
| Module | Scenario | Expected |
| --- | --- | --- |
| info | `GET /recipes/motion-light-dimmable` | Sowel's id, name, slots, i18n, pinned |
| validation | each of Sowel's refusals | Sowel's text |
| behaviour | motion in a slot | lights on at the slot's brightness |
| behaviour | no motion for the delay | lights off |
| behaviour | the time moves to another slot while on | Sowel's behaviour |
| limits | 4 instances | under 224 KB; source under 32 KB |

## Validation
- `sh scripts/validate.sh --skip-slow` green.
