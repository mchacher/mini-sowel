# Plan: Spec 028

## Steps

- [x] 1. Living spec updated (delta of `spec.md`)
- [x] 2. Types of the core: none (no new Sowel type or field)
- [x] 3. Logic in the core:
  - [x] 3.1 `Devices::from_list` resets 1970 dates; `Devices::shift_before_clock(jump)`;
        `Home::observe_wall(now)` with the last wall time kept in `Home` (R1)
  - [x] 3.2 `engine::run` and `Saver::tick` call `observe_wall` first (R1.2)
  - [x] 3.3 motion-light's stale buttons and their prune in `settle` (R2)
  - [x] 3.4 `Runner::refresh(home, params)`, motion-light's sources and buttons taken again;
        `settle` restarts only when the lights lost an id, else refreshes; settle after an
        equipment created, a data binding added and a zone updated (R2.2, R3)
- [x] 4. Adapters (host, firmware): not applicable (the engine loop is in the core; the host and
      the firmware already give the clocks)
- [x] 5. API: not applicable beyond passing `now` to the two equipment ops (R3.3)
- [x] 6. Interface: not applicable
- [x] 7. Tests (below), one per scenario
- [x] 8. Docs: D45, D46, roadmap (five lines out, one in, log), specs index; "No longer true"
      searched and completed

## Test plan

| Module | Scenario | Expected |
| ------ | -------- | -------- |
| devices (`tests/devices.rs`) | R1.2 shift: a device with `lastSeen`, `createdAt`, `updatedAt`, a data `lastUpdated` and `lastChanged` at 4 600 ms, one data never set; `Home::observe_wall(5 000)` then `observe_wall(T0)` | the five dates + (T0 − 5 000); the unset `lastUpdated` stays `None`; values flag set, definitions flag not set |
| devices | R1.2 a date already after 2020 (read from storage) on a device, another device in 1970 | the first unchanged, the second shifted |
| devices | R1.2 edge: the clock crosses with no device | nothing changes, no flag set |
| devices | R1.2 first call only records: `observe_wall(T0)` with no last wall time on a device dated 4 600 | nothing shifted |
| devices | R1.5 clock back below 2020 then crossing again: dates taken while back are shifted by the second jump, the earlier ones (already after 2020) untouched | as said |
| devices | R1.1 load: `Devices::from_list` with `lastSeen`, `lastUpdated`, `lastChanged` and `createdAt` in 1970 | the three `None`, `createdAt` unchanged, values flag set; a list with nothing in 1970: no flag |
| engine (`tests/engine.rs`) | R1.2/R1.3 nominal: the wall clock starts at ~0 (1970), a device reports, the clock jumps to T0 + elapsed | after the run, the device's `lastSeen` is ≥ T0 and ≤ the wall time at the end; status `online`; no `device.status_changed` to `offline` |
| engine | R1.2 across a restart of `run`: the clock is set between two calls of `engine::run` (the first stops while in 1970, a `saver.tick` at T0 in between, then `run` again) | the device's dates shifted, status `online` |
| engine | R1.1 load: a stored `devices.json` with a `lastSeen` in 1970, loaded through `load_home`, then a crossing | `lastSeen` `null`, status `unknown` |
| engine | unchanged: the existing 56-year jump test (wall starts at T0) still passes | as today |
| recipe_settle (`tests/recipe_settle.rs`) | R2 nominal: the button's device removed (`remove_device`) under a running garage motion-light | instance enabled and running, not restarted (one start line in its log); `buttons` empty in its params; log has `Button removed from settings: <button name>`; saved `recipes.json` without the button; no `recipe.instance.error` |
| recipe_settle | R2: the button's `action` data binding removed | same as above |
| recipe_settle | R2.2 the light on with its off timer running, a button loses its binding | `timerExpiresAt` unchanged, no order sent |
| recipe_settle | R2.1 a disabled instance with that button | button pruned, logged, written; stays disabled |
| recipe_settle | R2.3 the button equipment deleted | one log line `Equipment removed from settings: <name>`, no `Button removed` line; not restarted |
| recipe_settle | R2 edge: the same button in two instances | pruned from both, one line each |
| recipe_settle | R2 edge: the same button id twice in one instance's `buttons` (written in the stored params) | every copy taken out, one log line |
| recipe_settle | R2.4 unchanged: `POST /api/v1/recipe-instances` with a button without `action` | 400 (existing test in `motion_light.rs` still passes) |
| recipe_settle | R2.5 unchanged: the light's `state` order binding removed | instance disabled (existing test still passes) |
| recipe_settle | R2 after the prune: a press on the pruned button's equipment (event of alias `action`, binding added back) | nothing happens (the button is no longer listened to) |
| recipe_settle | R3 nominal: running motion-light with `maxOnDuration`; the motion sensor's data binding removed; then a device event of that sensor's motion | `failsafeExpiresAt` not moved by the event; not restarted |
| recipe_settle | R3.3: a second motion sensor on a new equipment of the zone (equipment created, binding added); its motion event | the safety auto-off is reset; not restarted |
| recipe_settle | R3.3: a sensor's equipment moved out of the zone (`PUT /equipments/:id` `zoneId`) or disabled (`enabled: false`) | its motion no longer resets the safety auto-off |
| recipe_settle | R3.3: a child zone holding a motion sensor moved out of the instance's zone (`PUT /zones/:id` `parentId`) | its motion no longer resets the safety auto-off; moved back in, it does |
| recipe_settle | R3 edge: the zone's last motion sensor unbound | sources empty, no failure, the instance keeps running |
| recipe_settle | R3.1 the state is kept: lights on with the off timer running; a sensor binding removed | `timerExpiresAt` unchanged; no order sent |
| api (`tests/api.rs`) | R4: `DELETE /api/v1/devices/:id`, then a measure from that address | 204; the device is not in `GET /api/v1/devices` after the measure |
| api | R4: an announce of the removed device outside a pairing window | not added back |
| api / live | R5: `PUT /api/v1/devices/:id` `{"zoneId": ...}` with a client listening to `devices` | 200 with the device; no live event |
| devices | Code review: an API request after the clock is set, before the loop's next turn | the dates moved first, the device stays `online` |
| recipe_settle | Code review: a backup import that moves a child zone out of the instance's zone | its sensor no longer resets the safety auto-off; not restarted |

## Validation

- [x] Validate command green
- [x] Agent review done, blocking findings fixed
- [ ] Demo seen and approved by the user (autonomous run: the tests and the review stand in; the API demo of spec.md is the user's)
