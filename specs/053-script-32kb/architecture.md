# Architecture: Spec 053

## Overview
Two constants and what follows them; measurements.

## Data
None.

## Files changed
| File | Change |
| --- | --- |
| `crates/core/src/recipes/script/limits.rs:15` | `MAX_SCRIPT_BYTES` 32 KB |
| `crates/core/src/api/mod.rs:40` | `MAX_BODY_BYTES` 32 KB |
| `crates/core/src/recipes/install/{file,catalogue}.rs` | follow (`MAX_FILE_BYTES`, the entry's size) |
| tests at their bounds | the new values |
| `docs/living-spec/README.md`, `docs/memory-budget.md`, `docs/decisions.md` (D85) | the delta |

## Reused
The existing bound tests and the memory tests (`memory_worst_case`, the QEMU script phase).
