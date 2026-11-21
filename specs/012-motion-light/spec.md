# Spec 012: motion-light

## Context

Increment B6 of the roadmap: the first recipe, `motion-light`, the one milestone B's demo needs (the
user's Sonoff motion sensor switches their ZBMINIR2 relay). It is written in Rust in the core
(recipes loaded at run time are milestone F), with **the same id, slots, messages and behaviour as
Sowel's** `sowel-recipe-motion-light` 1.1.2, and Sowel's recipe routes, so a Sowel client creates
and reads an instance the same way.

The reference behaviour, with a source line for every rule, is
[motion-light-reference.md](motion-light-reference.md) (a summary of the TypeScript recipe and of
Sowel's recipe engine, read on 2026-11-21). **Where this spec says "as the reference", that file is
the requirement**, section by section.

Sowel files mirrored: `../sowel/src/shared/types.ts` (`RecipeInfo` T:821, `RecipeSlotDef` T:695,
`RecipeInstance` T:834, `RecipeLogEntry`, `EngineEvent` T:1836-1872),
`../sowel/src/api/routes/recipes.ts`, `../sowel/src/recipes/engine/{recipe-manager,light-helpers,
duration}.ts`, `../sowel/src/equipments/equipment-manager.ts:1954-2000` (`equipment.data.changed`
on every update of a bound data), `../sowel/src/zones/zone-aggregator.ts:530-560`
(`zone.data.changed` only when the aggregate changed).

Specified in autonomy (no questions); choices under "Decided in autonomy".

## Goals

1. A motion in a zone switches its lights on; no motion for the timeout switches them off, with
   Sowel's lux threshold, failsafe, manual override and buttons.
2. Instances are created, changed, enabled, disabled, deleted and read through Sowel's routes, and
   come back after a restart.
3. Each instance keeps its log, as Sowel shows it.

## Out of scope

- Other recipes; recipes loaded at run time (F); the proposal of recipes (later milestones).
- Sunrise and sunset: `isDaylight` stays `null` (R6).
- Recipe events on the WebSocket (`recipe.instance.*`): B7.
- Shadow mode, recipe packages and their updates.

## Requirements

### R1: Engine events inside the core

The home produces Sowel's `EngineEvent`s that recipes listen to, with Sowel's fields:

- `device.data.updated` `{deviceId, deviceName, dataId, key, value, previous, timestamp}`: on
  every update of a device data (a report, a read answer, a zone notification), changed or not;
- `equipment.data.changed` `{equipmentId, alias, value, previous}`: for each data binding of that
  device data, on the same update;
- `zone.data.changed` `{zoneId, aggregatedData}`: for each zone whose aggregate differs from the
  last one computed (Sowel's equality), after a recompute.

They go into a queue inside the home, **bounded at 64 events**; the radio loop hands them to the
running instances right after each batch of measures, before the next poll, so the queue never
holds more than one batch. One past the bound: the oldest event is dropped and `log::warn!`
names its type. The type is the one B7 sends on the WebSocket.

### R2: The recipe engine (core)

- A registry of recipes, with `motion-light` only; `RecipeInfo` exactly as the reference §1 (JSON
  shown there, `i18n.fr` included, `icon` `Lightbulb`, absent fields omitted).
- Instances (`RecipeInstance` `{id, recipeId, params, enabled, createdAt}`), **at most 16**; one
  more: 400 `{"error":"Too many recipe instances (max 16)"}`.
- Each running instance gets the events of R1, the time, and sends orders (R4). Its timers are
  deadlines in milliseconds checked on each loop turn (`tick(now)`), no thread; the time is the
  host's clock, the same as the home's (`--speed` speeds up the simulated devices, not the
  timers).
- Its state keys (`overrideMode`, `timerExpiresAt`, `failsafeExpiresAt`), as the reference §5,
  held in memory and served in `state`.
- Its log (`RecipeLogEntry` `{id, instanceId, timestamp, message, level}`), the **last 50 entries
  per instance**, in memory; `id` an integer counting from 1 since the start of the program; the
  engine's own lines (`Instance created`, `Failed to start: <err>`, `Failed to restore: <err>`,
  `Failed to restart: <err>`) as the reference §6.
- An instance whose handler fails never stops the loop (CLAUDE.md, errors).

### R3: `motion-light`

As the reference, sections 2 to 6, with nothing left out:

- parameters, defaults, the legacy `light`, duration parsing and `formatDuration` (§2);
- `validate`'s order and **exact** messages, wrapped as 400 `Invalid params: <msg>` (§2), and the
  warning `Zone has no motion sensors — recipe will never trigger` at every start;
- the events listened to and their filters, motion sources resolved once at start with their
  device and key, `isMotionActive` (§3);
- orders: `ON`/`OFF` to every configured light, the binding found by category `light_toggle` or
  `toggle_power`, else alias `state`; a light without one reported as `<lightId>: no toggle order
  binding` in `Error turning on some lights: ...` (§3);
- the state machine: constants (grace 2000 ms, hysteresis 10 %), the primitive actions, zone
  change, light change, button action, failsafe impulses, start sync, stop (§4), with every quirk
  of §4 "Known quirks" kept as Sowel does it;
- the 14 messages and their levels, em dashes included (§6).

Durations are held as `u64` milliseconds: no 24.8-day limit (a divergence, R7).

### R4: Orders from a recipe

A recipe's order goes through spec 011's queue with no one waiting (`push_unattended`), the value
`ON` or `OFF` resolved as spec 011's R7.4. Unattended orders have **their own bound** (R8.3), not
counted in `MAX_ORDERS` (so a recipe switching three lights never makes the API answer 503); one
past it is dropped with `log::warn!`. The outcome is not reported to the recipe (Sowel's helpers
ignore dispatch errors). Both kinds are sent in the order they were queued.

### R5: Routes (Sowel's `recipes.ts`)

As the reference §7, statuses, bodies and texts included:

| Route | Answer |
| ----- | ------ |
| `GET /api/v1/recipes` | 200 `RecipeInfo[]` |
| `GET /api/v1/recipes/:recipeId` | 200, or 404 `Recipe not found` |
| `GET /api/v1/recipe-instances` | 200, each instance with its `state`, oldest first |
| `POST /api/v1/recipe-instances` `{recipeId, params}` | 201 the instance (no `state`), started; 404 `Recipe not found: <id>`; 400 `Invalid params: <msg>`; 400 `body/recipeId must NOT have fewer than 1 characters` style errors for a body that is not `{recipeId: non-empty string, params: object}` (text: R7.3) |
| `PUT /api/v1/recipe-instances/:id` `{params}` | 200 the instance; validate (400, the running instance unchanged), stop, save, start if enabled; 404 `Instance not found` |
| `DELETE /api/v1/recipe-instances/:id` | 204; stops it, deletes its state and log; 404 `Instance not found` |
| `POST /api/v1/recipe-instances/:id/enable`, `/disable` | 200 `{"success":true}`; 404 `Instance not found` |
| `POST /api/v1/recipe-instances/:id/actions` `{action, payload?}` | 200 `{"ok":true}` while running; 400 `Instance not running` otherwise (unknown id included, as Sowel) |
| `GET /api/v1/recipe-instances/:id/log?limit=N` | 200 newest first, `limit` default 50; unknown id `[]` |

Each route answers at once: an order a recipe sends is not waited for.

### R6: Daylight

mini-Sowel computes no sunrise or sunset: the root zone's `isDaylight` is `null`, read as night,
as Sowel does without a location. `disableWhenDaylight` is accepted and has no effect.

### R7: Precisions

1. **Storage**: `recipes.json` in the data folder (mode 0600, written atomically, as the other
   files), the instances only (`id`, `recipeId`, `params`, `enabled`, `createdAt`), written at once
   on each create, change, enable, disable or delete. `params` larger than 2 KB once serialized:
   400 `Invalid params: params too large`. At start, every enabled instance is validated and
   started; one that fails gets `Failed to restore: <err>` in its log and stays stopped (as
   Sowel). Logs and state keys are not kept (R2).
2. **Start order on the host**: devices, zones and equipments are loaded, then the instances are
   started; their start sync sends its orders like any other (its `OFF` waits in the queue until
   the radio is ready, within spec 011's 12 s deadline).
3. **Body errors**: `recipeId` missing or empty, or `params` not an object: 400 `{"error":"Invalid
   body"}` (Fastify's exact schema text is not part of the contract; a divergence, R8).
4. **A light or button deleted** while an instance runs: it is no longer found when the instance
   acts; orders to it are skipped and reported as `<lightId>: no toggle order binding`. At the
   next start, `validate` refuses the instance (`Light equipment not found: <id>`).
5. **Memory**: the ceiling test (`tests/memory`, spec 008) adds, to its realistic home, 3
   instances with full logs (50 entries) and the event queue full; the ceiling stays 200 KB.
6. **Where it runs**: core and host; the firmware does not run the engine yet, not affected. The
   real demo uses the Dongle Max over TCP with its original firmware, nothing flashed (D6).

### R8: Precisions after the spec review (they override R1 to R7 where they differ)

1. **Events are compact and filtered** (replaces R1's "Sowel's fields" and its bound). The
   engine keeps the set of what its running instances listen to: zone ids, `(equipmentId,
   alias)` pairs, motion sources `(deviceId, key)`. The home queues only the events that match,
   in a compact internal form: zone `{zoneId, motion, luminosity}`, equipment `{equipmentId,
   alias, value}`, device `{deviceId, key, value}` (value: boolean, number, or text cut at 32
   bytes). Bound: **128 events**, one past it the oldest dropped with `log::warn!`. The WebSocket's
   Sowel events are B7's, built separately.
2. **When events are produced.** The radio loop hands the queue to the engine after each batch
   of measures. A recompute after an API edit (on an HTTP thread) queues its zone events; they
   are handed over at the next loop turn. The first recompute after a start emits a zone event for
   every zone (Sowel: no previous value). Equality is Sowel's `aggregatedDataEqual` (its field
   list, `motionSince` excluded).
3. **HTTP orders first.** The runner takes a waiting HTTP order before a waiting unattended one;
   within each kind, first in first out. `MAX_ORDERS` (2) counts HTTP orders only (waiting plus
   an HTTP order being sent); unattended orders count against their own bound, **32**. An HTTP
   order can still wait behind an unattended order being sent to a silent device and pass its
   12 s deadline: 502 `device did not answer` (stated, tested).
4. **At most 16 lights and 16 buttons** per instance: `Invalid params: At most 16 lights` /
   `Invalid params: At most 16 buttons`. A disabled light gets no order and no log line (Sowel's
   `Equipment is disabled` is swallowed by its helper).
5. **Memory.** A log message is cut at 200 bytes. `GET /recipe-instances` is streamed in chunks
   like the other lists (D15). The ceiling test (200 KB, realistic home) adds 3 instances with full
   logs of 200-byte messages and a full event queue. The worst case of the bounds (16 instances × 50 entries × about 260 bytes, about 210 KB, plus 16 × 2 KB of params kept as
   text, measured 45 KB) is written next to the
   bounds; it fits the box's PSRAM and is part of the inbox item "memory budget of the box".
6. **`recipes.json` at start**: unreadable or not valid JSON stops the host, as `zones.json`;
   an unknown `recipeId`: kept, not started, `Failed to restore: Recipe not found: <id>` in its
   log; an entry beyond 16 or with a duplicate id: dropped with `log::warn!`; params over 2 KB:
   kept, not started, `Failed to restore: params too large`.
7. **Bodies and query**: PUT with `params` not an object, `/actions` with `action` missing,
   empty or not a string, or `payload` present and not an object: 400 `Invalid body`. The log's
   `limit`: not a positive integer → 50; above 50 → 50.
8. **Numbers** (where JavaScript would pass them through): a negative duration counts as 0
   (fires at the next tick), a fractional one is rounded down, a duration string too large for
   `u64` milliseconds is the format error. `luxThreshold` follows JavaScript's `Number()`: `true`
   1, `false` 0, a string trimmed (empty after trimming 0, else a decimal number or NaN, which
   is the error); only `undefined`, `null` and `""` mean no threshold.
9. **Recompute per batch** (a divergence): Sowel recomputes the aggregate on every measure,
   mini-Sowel once per batch; a motion going true then false inside one batch gives no zone event,
   and a handler reads the home as it is at the end of the batch.

### R9: Divergences from Sowel (decision D17)

Compact filtered events (R8.1); HTTP orders before recipe orders (R8.3); recompute per batch
(R8.9); at most 16 lights and buttons; log messages cut at 200 bytes; numbers of R8.8; no 24.8-day timer limit; logs and state keys not kept across a restart; at most 16 instances and
50 log entries each; the `Invalid body` text; `isDaylight` always `null`; no `recipe.instance.*`
events yet; the order's source (`recipe`, instance id) is not recorded.

## Living spec delta

### docs/living-spec/README.md

- New section "Runs the motion-light recipe" after "Organises the home in zones and equipments":
  what it does in the user's terms (motion on, timeout off, lux threshold with hysteresis,
  failsafe, manual override and buttons, daylight with no effect), the routes, the log, what
  survives a restart, the bounds (16 instances, 50 log entries, 16 orders from recipes).
- "Serves Sowel's device API": the routes table gains the recipe routes.
- "Organises the home in zones and equipments", Orders bullet: orders also come from recipes,
  with their own bound.
- Storage: `recipes.json` joins the list of files.
- History row 012.

### No longer true

- `docs/living-spec/README.md`: the closing sentence (recipes join what the host does); "503
  `Busy`" rule (recipe orders do not count).
- `docs/decisions.md` D16 ("At most 2 orders at once"): HTTP orders only now, a new decision
  (D17) says so; D1's "Recipes run in `wasmi`": D17 states that native Rust recipes come first,
  until F.
- `docs/architecture.md`: the core's modules (a `recipes/` module, the event queue in `home`).
- `docs/roadmap.md`: B6's status.
- Searched: `recipe`, `motion-light`, `B6`, `EngineEvent`, `isDaylight`, `D16`, `wasmi`, `Busy`.

## Acceptance criteria

- [x] `--sim scenarios/garage.json --serve --speed 10`, the garage of spec 010's demo, and an
      instance `{"recipeId":"motion-light","params":{"zone":"<garage>","lights":["<light>"],
      "timeout":"20s"}}`; then the host restarted (the scenario's motions are at 1 and 3 minutes
      of scenario time, 6 s and 18 s after the start at `--speed 10`): the light is switched off
      at the start (`Recipe activated — no motion, lights off`), on at about 6 s (`Motion
      detected — 1 light(s) turned on`), off at about 44 s, 20 s after the last motion clears
      (`No motion for 20s — lights turned off`); the log route shows these three lines newest
      first (the log before the restart is not kept).
- [ ] Real Dongle Max, Sonoff motion sensor and ZBMINIR2 (pending): a hand in front of the sensor
      switches the relay on, and it goes off after the timeout.

## Edge cases

- Each `validate` message of the reference §2, one by one.
- Lux exactly at the threshold (turns on), at threshold × 1.1 (stays on), above (turns off).
- The light switched off by hand while motion lasts: override, cleared after the timeout without
  motion.
- The light switched on by hand with no motion: off after the timeout; a heartbeat `ON` in
  between changes nothing.
- The echo of the recipe's own `OFF` within 2 s: not taken as a manual change.
- A button press with the light on: off and override; with the light off: on.
- Failsafe: fires after `maxOnDuration` with no impulse; an impulse from a motion source restarts
  it; the zone's motion restarts it too.
- `disableWhenDaylight` true: no effect (`isDaylight` null).
- 17th instance: 400. 51st log line: the oldest goes. 65th event in one batch: the oldest goes.
  17th unattended order: dropped.
- Disable then enable: the start sync runs again; stop clears the three state keys and does not
  switch the light off.
- A light deleted while running (R7.4); an instance that fails to restore (R7.1, R8.6).
- PUT with invalid params: the running instance unchanged.
- 17 lights; a disabled light.
- An HTTP order behind recipe orders to a silent relay (R8.3).
- Events from an API edit; the first recompute after a start (R8.2).
- Bad bodies and `limit` values (R8.7); negative, fractional and huge durations, `luxThreshold`
  forms (R8.8).
- The stale race of the reference §4 (a zone event before the light's echo): kept as Sowel, it
  can show in the real demo; filed as a Sowel issue.

## Decided in autonomy

- The recipe in native Rust with Sowel's exact behaviour, quirks included: the user is Sowel's
  author; a quirk is fixed in Sowel first, then here. Each quirk is a "Sowel issue:" line in the
  roadmap inbox.
- Timers on the host's clock, not on the simulator's speed: the demo uses a short timeout.
- Logs and state in memory only: writing them would wear the box's flash for informational data.
- HTTP orders before recipe orders, each with its own bound (R8.3): a person waiting at the API
  goes first; a recipe's order is never waited for.
- The stale race of the reference §4 kept as Sowel (it may show in the real demo), filed as a
  Sowel issue.

## Added while building

1. Zone events are produced wherever the aggregate is computed (`Home::aggregate`, so `GET
   /zones/aggregation` queues them too); "the last aggregate" of a zone is kept as a fingerprint
   (FNV-1a of its JSON without `motionSince`, plus `motion` and `luminosity`), for memory.
2. The host recomputes the aggregate before starting the instances, so the start sync reads a
   fresh zone and no zone event follows the start (the first-recompute rule of R8.2 holds for a
   home started without that step, and is tested).
3. `POST /recipe-instances` checks in this order: 404 recipe, 400 instance cap, 400 params size,
   then `validate`; bodies before ids. A start failing on enable logs `Failed to start: <err>`;
   a handler failing logs `Error: <msg>`.
4. When the off timer and the failsafe fall due in the same tick, the off timer fires first.
   `timeout: false` is falsy (the default `10m`). `luxThreshold` accepts decimal forms only
   (`Infinity`, hexadecimal, arrays and objects are the error).
5. The reference's test 5 (`getDescendantIds` throwing) cannot happen in Rust: no test.
6. Memory: the ceiling test adds 3 instances of 16 lights with full logs (worst-case 200-byte
   lines) and a full event queue: peak about 175 KB, worst case 186 KB, ceiling 200 KB. After
   the code review, `params` are kept as their JSON text (2 KB of nested JSON parsed takes about
   112 KB): 16 instances of 2 KB keep 45 KB (test `recipe_params_memory`).
7. Kept as Sowel and tested: after a manual off during motion, the zone event that follows
   logs `Motion detected but override mode active — ignoring` right after the override line; the
   stale race of the reference §4.
8. Demo on the simulator, 2026-11-21 (`--sim scenarios/garage.json --serve --speed 10`, instance
   with `timeout` `20s`, host restarted): the light `false` until about 6 s, `true` from 10 s to
   40 s, `false` from 46 s; log, newest first: `No motion for 20s — lights turned off`, `Motion
   detected — 1 light(s) turned on`, `Recipe activated — no motion, lights off`.

## How the user will check it

With `--sim scenarios/garage.json --serve --speed 10` and spec 010's demo steps done:

```
curl -X POST http://127.0.0.1:8080/api/v1/recipe-instances -H 'Content-Type: application/json' \
  -d '{"recipeId":"motion-light","params":{"zone":"<garage>","lights":["<light>"],"timeout":"20s"}}'
```

Then stop the host (Ctrl-C), start it again with the same command, and read
`GET /api/v1/equipments/<light>` (`state`) at 10 s and at 50 s, then
`GET /api/v1/recipe-instances/<id>/log`.

Run on the real hardware (2026-11-21): The motion-light recipe (timeout 20 s) switched the relay off about 20 s after `motion false`, and back on at the next `motion true`; manual switching from the interface was also seen.
