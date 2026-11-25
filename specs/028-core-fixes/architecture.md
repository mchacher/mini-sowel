# Architecture: Spec 028

## Overview

```
load_home ──> Devices::from_list: lastSeen / lastUpdated / lastChanged < 2020 -> null (values flag)
engine::run turn, Saver::tick ──> Home::observe_wall(now)
      (last wall < 2020 <= now) ──> Devices::shift_before_clock(now - last)   (values flag only)
      then activity, measures, refresh_status, recompute, recipes, saver (unchanged order)

Home::{create_equipment, add_data_binding, ... existing settle callers}
   └─> settle_recipes(Settle) ─> RecipeEngine::settle
          0. (existing) zone deleted: its instances deleted
          1. (existing) deleted equipment pruned from every instance
          1b. NEW: motion-light buttons without an `action` binding pruned (motion_light::stale_buttons)
          2-3. (existing) enabled instances validated; disabled when invalid;
               CHANGED: restarted only when its lights (`lights`, legacy `light`) lost an id
          4. NEW: every running instance not restarted: Runner::refresh(home, params)
               (motion-light re-resolves its sources and takes its buttons again)
       ─> Home::with_engine recomputes the interest (existing)
```

## Data

No new stored data, no new field. `Runner` gains, a no-op by default, `fn refresh(&mut self, _home: &Home, _params: &Value) {}`. `MotionLight::sources` and
`MotionLight::p.buttons` are replaced on refresh (same bounds: at most 64 x 16 pairs, 16
buttons). `Home` keeps one `Option<u64>`, the last wall time it was given.

## Files changed

| File | Change |
| ---- | ------ |
| `crates/core/src/devices/registry.rs` | `from_list` resets 1970 `lastSeen`/`lastUpdated`/`lastChanged` (values flag); `shift_before_clock(jump) -> bool` walks the devices and their data, sets the values flag when something moved |
| `crates/core/src/home/mod.rs` | `last_wall: Option<u64>`; `Home::observe_wall(now)` (room for other dates later) |
| `crates/core/src/engine/mod.rs` | `run`: `observe_wall(now)` first in the guarded block, before activity |
| `crates/core/src/engine/saver.rs` | `tick`: `observe_wall(now)` first, under the home's lock |
| `crates/core/src/recipes/mod.rs` | `Runner::refresh`; `settle`: step 1b (buttons) and step 4 (refresh of running instances not restarted) |
| `crates/core/src/recipes/motion_light/params.rs` (or `mod.rs`) | `stale_buttons(params: &Value, home) -> Vec<(String /*id*/, String /*name or id*/)>` and the prune of `buttons` |
| `crates/core/src/recipes/motion_light/machine.rs` | `impl Runner::refresh`: `self.sources = resolve_motion_sources(home, &self.p.zone)`, `self.p.buttons` from the params' `buttons` (ids that parse) |
| `crates/core/src/home/equipment_ops.rs` | `create_equipment` and `add_data_binding` call `settle_recipes(Settle::default(), now)` (they need a `now`: take it from the API caller, as the other ops do) |
| `crates/core/src/home/mod.rs` | `update_zone` calls `settle_recipes(Settle::default(), now)` |
| `crates/core/src/api/equipments.rs` | pass `now` where `create_equipment`/`add_data_binding` gain it |
| `crates/core/tests/engine.rs` | the clock-jump tests (R1) |
| `crates/core/tests/devices.rs` | the registry's shift and forget (R1, unit level through the public API) |
| `crates/core/tests/recipe_settle.rs` | R2, R3 tests |
| `crates/core/tests/api.rs` | R4 (removed device not added back by a measure), R5 (no live event on a zone change) where the existing helpers are |
| `docs/living-spec/README.md` | the delta |
| `docs/decisions.md` | D45 (dates before the clock), D46 (buttons pruned, sources re-resolved; narrows D38) |
| `docs/roadmap.md` | five inbox lines removed, one added (other 1970 dates), log line |
| `docs/specs-index.md` | row 028 |

## Reused

- `prune_equipment` in `recipes/mod.rs` for removing an id from a list in params text (call it
  per stale button id with the key `buttons`).
- `resolve_motion_sources` (`motion_light/sources.rs`), unchanged.
- `CLOCK_SET_AFTER_MS` (`home/mod.rs:49`), as `aggregate` uses it for `motionSince`.
- The rigs of `tests/recipe_settle.rs` (`Garage`, `listening`, `types`, `saved`) and of
  `tests/engine.rs` (`Rig`, `bench`, the jumping `Clocks` of
  `a_wall_clock_that_jumps_forward_by_56_years_does_not_stop_the_engine`).
