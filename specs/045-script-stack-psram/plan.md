# Plan: Spec 045

## Steps
- [x] 1. Living spec, memory budget, D83, roadmap.
- [x] 2. Core: `job_thread_kept`, `internal_refusal`, tests.
- [x] 3. Firmware: `job_thread` caps, the `script` thread in PSRAM, the permanent `recipe`
      thread, the storage guard, QEMU config.
- [x] 4. validate (QEMU included).
- [x] 5. Box: flash (backup kept), the check of the acceptance criteria.
- [x] 6. Review, demo to the user.

Not applicable: Sowel types, API, interface.

## Test plan
| Module | Scenario | Expected |
| --- | --- | --- |
| core install admission | thread kept, largest block 17 KB, install | accepted |
| core install admission | thread kept, largest block 15 KB, install | 503 `... 15 KB, 16 KB needed` |
| core install admission | thread kept, largest block 1 KB, uninstall | accepted |
| core install admission | thread not kept (default), 35 KB, install | 503 `... 36 KB needed` (unchanged) |
| core install admission | thread not kept, 19 KB, uninstall | 503 `... 20 KB needed` (unchanged) |
| QEMU | the script phase on a PSRAM stack | `script thread stack` at least 2 KB unused; internal min written |
| box | R3's absent thread | not testable on the Mac (firmware, no tests): read in the code review |
| box | the acceptance criteria's cycles | every install accepted, internal min at least 64 KB |

## Validation
- `sh scripts/validate.sh` green, QEMU included.
