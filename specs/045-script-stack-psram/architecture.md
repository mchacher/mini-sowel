# Architecture: Spec 045

## Overview
Firmware only, plus one admission setting in the core.

## Data
None stored.

## Files changed
| File | Change |
| --- | --- |
| `firmware/src/job_thread.rs:47-150` | `run` and `spawn` take where the stack lives (internal or PSRAM); callers say so |
| `firmware/src/http_server.rs:174-181` | `start_script_thread`: PSRAM stack; a `start_recipe_thread` beside it, a rendezvous channel as `SCRIPT_JOBS`; `:995-1001` sends `RecipeJob`/`CatalogueRefresh` to it (503 `Busy` when absent) |
| `firmware/src/storage.rs` | the PSRAM-stack guard (R2) at the top of each flash call |
| `crates/core/src/recipes/install/mod.rs:43-55` | `RecipeInstalls` gets `job_thread_kept: bool` (false by default) and a setter |
| `crates/core/src/home/install_ops.rs:176-196` | `internal_refusal`: the 20 KB stack counted only when the thread is not kept |
| `firmware/src/scripts.rs:79-90` | sets `job_thread_kept` on the box (radio image) when the thread started |
| `firmware/sdkconfig.defaults` (or the QEMU image's config) | `CONFIG_SPIRAM_ALLOW_STACK_EXTERNAL_MEMORY=y` |
| `firmware/src/script_phase.rs` | the QEMU phase's `script` thread in PSRAM |
| docs | living spec, memory budget, D83, roadmap |

## Reused
The API workers' PSRAM stacks (`http_server.rs:1047-1088`, `MallocCap::Spiram | Cap8bit`); the
permanent `script` thread's pattern (`script_loop`, `SCRIPT_JOBS`, the mutex).
