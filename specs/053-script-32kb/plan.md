# Plan: Spec 053

## Steps
- [ ] 1. Living spec, memory budget, D85.
- [ ] 2. The bounds, and what follows them.
- [ ] 3. Tests at the bounds; a 32 KB test script; the measurements.
- [ ] 4. validate (QEMU included); review.

Not applicable: interface, Sowel types.

## Test plan
| Module | Scenario | Expected |
| --- | --- | --- |
| script load | 32 KB source / 32 KB + 1 | accepted / refused, the existing text |
| install | a signed file of a 32 KB script uploaded | installed |
| api | a body of 32 KB + 128 / + 1 | passes / 413 |
| recipes | a 5th instance of a script / a 5th script recipe / a 17th instance in all | refused (`max 4`) / refused (`at most 4`) / refused (`max 16`) |
| catalogue | an entry of size 32 KB + header / over | read / refused |
| memory | worst case | under 1 MiB, figure written |
| memory | 32 KB script, 4 instances | under 224 KB (Mac, QEMU), figures written |

## Validation
- `sh scripts/validate.sh` green, QEMU included.
