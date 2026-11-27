# Architecture: Spec 044

## Overview

Four constants change; the code that enforces them stays (refusals, skips and truncations already
exist at the old values). Two loads gain a log line where they cut (devices, equipments). The tests
that pin the old values move to the new ones, the worst-case test reaches the new bounds, and the
docs that state the figures are corrected.

## Data

Nothing stored changes shape. A file written with the old bounds is cut at load (R5) and written
back cut at its next save. The logs are in RAM only.

## Files changed

| File | Change |
| ---- | ------ |
| `crates/core/src/devices/registry.rs` | `MAX_DATA` 8; `from_list` logs a device it cuts (`:127`) |
| `crates/core/src/equipments/mod.rs` | `MAX_BINDINGS` 8; `from_list` logs an equipment it cuts (`:363-364`) |
| `crates/core/src/devices/registry.rs` (again) | the "not added" line once per device and key |
| `crates/core/src/home/mod.rs` | `from_parts`: drop the dangling bindings, then cut at `MAX_BINDINGS`, logged, marked to be written |
| `crates/core/src/recipes/mod.rs` | `MAX_LOG_ENTRIES` 20; the worst-case arithmetic comment (`:50-54`); params unchanged (R4) |
| `ui/src/components/equipments/bindingUtils.ts`, `ui/src/i18n/messages.ts` | the interface's `MAX_BINDINGS` 8 and its warning |
| `scripts/measure-api.sh`, `scripts/box-session.sh` (B6) | 17 requests |
| `crates/core/tests/memory_worst_case.rs` | the new bounds reached; `MAX_LOG_ENTRIES` in place of the literal 50 |
| `crates/core/tests/devices.rs`, `equipments*.rs`, `recipes*.rs`, `api` tests; `ui/src/components/equipments/bindingUtils.test.ts`, `DeviceSelector.test.tsx` | the bounds' tests at the new values; the loads' cuts |
| `docs/living-spec/README.md`, `CLAUDE.md`, `README.md`, `docs/sowel-subset.md`, `docs/decisions.md` (D80), `docs/memory-budget.md`, `docs/specs-index.md`, `docs/roadmap.md` | R6 and the delta |

## Reused

The refusal and skip paths at the bounds (`registry.rs:493`, `home/equipment_ops.rs:225-246, 401,
458`, `equipments/propose.rs:88,109`, `recipes/mod.rs:414, 756, 1679`, `api/recipes.rs:24-32`), the
home's pruning of bindings whose data is gone (`Home::from_parts`), the worst-case and realistic
memory tests.
