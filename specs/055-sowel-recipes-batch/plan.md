# Plan: Spec 055

## Steps
- [ ] 1. Living spec, roadmap (md and html).
- [ ] 2. The four scripts, from Sowel's sources, one after the other (targeted tests each).
- [ ] 3. Tests (info pinned, validation texts, behaviour, memory) per recipe.
- [ ] 4. validate once at the end; review.
- [ ] 5. Signed, published in one catalogue release with spec 054's, installed on the box.

## Test plan
| Module | Scenario | Expected |
| --- | --- | --- |
| info | `GET /recipes/<id>` for each | Sowel's, pinned |
| validation | each of Sowel's refusals | Sowel's text |
| behaviour | each recipe's main path, simulated clock | Sowel's result |
| limits | 4 instances of each | under 224 KB; source under 32 KB |

## Validation
- `sh scripts/validate.sh --skip-slow` green.
