# Plan: Spec 053

## Steps
- [x] 1. Living spec, memory budget, D85.
- [x] 2. The bounds, and what follows them.
- [x] 3. Tests at the bounds; a 32 KB test script; the measurements.
- [x] 4. validate (QEMU included); review.

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

## Result (2026-11-28)
- Done: script 32 KB, body and signed file 32 KB + 128 bytes, 4 script recipes of 4 instances,
  16 in all (D85); the runtime cap stays 224 KB (208 KB refuses the 32 KB script in QEMU).
- Worst case on the Mac: 1,043,684 bytes, 4.9 KB under the 1 MiB ceiling.
- 32 KB script, 4 instances: 247,480 bytes on the Mac; peak 183,828 bytes in QEMU.
- Four runtimes at their limit are 54 KB over the PSRAM on paper; the install admission rule
  refuses the runtime that does not fit (tested: the 4th is 409).
- validate green (QEMU included), review findings fixed.
