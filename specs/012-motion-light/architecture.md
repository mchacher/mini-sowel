# Architecture: Spec 012

## Overview

```
radio loop (host)
  measures ──► Home: devices update ──► events queue (128) ──┐
               recompute ─────────────► zone.data.changed ──┤
                                                            ▼
               RecipeEngine.handle(event, now) ──► MotionLight instance
               RecipeEngine.tick(now)  (timers)           │ ON/OFF
                                                          ▼
               OrderQueue.push_unattended (own bound 32) ──► runner ──► radio
HTTP worker ──► api::recipes (lock, answer at once) ──► RecipeEngine (create/stop/start)
```

The engine lives in the core and touches no hardware: it receives events and the time, and returns
nothing but orders pushed to the home's queue and log lines kept in itself. The instance's
behaviour is a plain state machine (`MotionLight`) whose inputs are the four handlers of the
reference (`on_zone_changed`, `on_light_changed`, `on_button_action`, `on_impulse`) and
`tick(now)`; its reads of the home (zone aggregate, light state, equipments) go through a small
read-only view the engine passes in, so the machine is tested without a radio.

## Data

- `recipes/mod.rs`: `RecipeInfo`, `RecipeSlotDef`, `RecipeInstance`, `RecipeLogEntry` (serde,
  Sowel's names, optional fields skipped when absent), `RecipeEngine` (instances ≤ 16, each with
  its runner, its state keys, its log ring of 50).
- `recipes/duration.rs`: `parse_duration`, `format_duration` (Sowel's `duration.ts`).
- `recipes/motion_light/`: `info.rs` (the definition and i18n), `params.rs` (parsing and
  `validate` with the exact messages), `machine.rs` (the state machine), `sources.rs` (motion
  sources, `is_motion_active`).
- `home/events.rs`: `EngineEvent` (the three variants of R1, Sowel's JSON with
  `#[serde(tag = "type")]`), the bounded queue.
- `recipes.json`: `[{id, recipeId, params, enabled, createdAt}]`.

## Files changed

| File | Change |
| ---- | ------ |
| `crates/core/src/recipes/` | new: engine, durations, motion-light |
| `crates/core/src/home/mod.rs`, `home/events.rs` | events produced on measures and recompute; the engine held by the home; `take_recipes_json` like zones and equipments |
| `crates/core/src/devices/`, `equipments/` | the update path reports which data changed, for the events |
| `crates/core/src/orders/queue.rs` | unattended orders: their own bound (32), taken after the API's |
| `crates/core/src/api/recipes.rs`, `api/mod.rs` | the routes of R5 |
| `crates/host/src/serve.rs`, `store.rs` | events and `tick` in the radio loop; `recipes.json` loaded and saved; instances started after the home is loaded |
| `crates/core/tests/recipes.rs` (new), `motion_light.rs` (new), `api.rs`, `memory_ceiling.rs`; `crates/host/tests/serve.rs` | tests of the plan |
| `docs/living-spec/README.md`, `docs/architecture.md`, `docs/decisions.md` (D17), `docs/roadmap.md` | docs |

## Reused

- Zones and equipments' storage pattern (`take_*_json`, atomic write, 0600) for `recipes.json`.
- Spec 011's `resolveWireValue` port for `ON`/`OFF`, and its queue and runner for sending.
- The zone aggregator (spec 010) for `motion`, `luminosity` and the change test of
  `zone.data.changed`.
- The routes' error texts and JSON helpers of `api/` (specs 008, 010).
- The reference's nine TypeScript tests (§8), ported one for one.
