# Architecture: Spec 053

## Overview
Two constants and what follows them; measurements.

## Data
None.

## Files changed
| File | Change |
| --- | --- |
| `crates/core/src/recipes/script/limits.rs:15` | `MAX_SCRIPT_BYTES` 32 KB |
| `crates/core/src/api/mod.rs:40` | `MAX_BODY_BYTES` 32 KB + `HEADER_BYTES` (128) |
| `crates/core/src/recipes/install/{file,catalogue}.rs` | follow (`MAX_FILE_BYTES`, the entry's size) |
| `crates/core/src/recipes/mod.rs` | `MAX_SCRIPT_RECIPES` 4, `MAX_INSTANCES_PER_SCRIPT` 4 (`MAX_INSTANCES` stays 16) |
| `firmware/src/script_phase.rs`, `scripts/qemu.sh` | the script phase's 8 instances become 4; `scripts/lib/firmware.sh` gates a 32 KB script's `script sizes: big-recipe` line |
| tests at their bounds | the new values; `crates/core/tests/fixtures/scripts/big-recipe.js` (32 KB of real code) |
| `docs/living-spec/README.md`, `docs/memory-budget.md`, `docs/decisions.md` (D85) | the delta |

## Reused
The existing bound tests and the memory tests (`memory_worst_case`, the QEMU script phase).
