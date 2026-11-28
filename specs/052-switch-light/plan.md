# Plan: Spec 052

## Steps
- [x] 1. Living spec, roadmap.
- [x] 2. The script, from Sowel's source.
- [x] 3. Tests (info pinned, validation texts, the toggle, memory).
- [x] 4. validate; review.
- [x] 5. Signed, published in the catalogue.

Not applicable: core types, API, interface (a script recipe uses the existing ones).

## Test plan
| Module | Scenario | Expected |
| --- | --- | --- |
| info | `GET /recipes/switch-light` | Sowel's id, name, slots, i18n, pinned |
| validation | each of Sowel's refusals | Sowel's text |
| behaviour | button `toggle` with lights off | lights on |
| behaviour | a second press | lights off |
| behaviour | a press of a button not in the slot | nothing |
| limits | 8 instances | under 224 KB |

## Validation
- `sh scripts/validate.sh --skip-slow` green.
