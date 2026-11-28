# Plan: Spec 055

## Steps
- [x] 1. Living spec, roadmap (md and html).
- [x] 2. The four scripts, from Sowel's sources, one after the other (targeted tests each).
- [x] 3. Tests (info pinned, validation texts, behaviour, memory) per recipe.
- [x] 4. validate once at the end; review.
- [ ] 5. Signed, published in one catalogue release with spec 054's, installed on the box.

## Test plan
| Module | Scenario | Expected |
| --- | --- | --- |
| info | `GET /recipes/<id>` for each | Sowel's, pinned |
| validation | each of Sowel's refusals | Sowel's text |
| behaviour | each recipe's main path, simulated clock | Sowel's result |
| limits | 4 instances of each | under 224 KB; source under 32 KB |
| slots (R3) | a `text` slot; `hiddenWhen` | a text over 64 bytes refused; `hiddenWhen` served as given, a malformed one refused at load |
| form (R3) | `hiddenWhen`, a `text` slot | the slot hides and shows with the select it names; a text field |
| clock | `now().localTime` while the clock is not set | `null`; `motion-light-dimmable` uses its plain `brightness` |

## Validation
- `sh scripts/validate.sh --skip-slow` green.
