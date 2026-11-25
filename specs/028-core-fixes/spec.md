# Spec 028: core-fixes

## Context

First item of the roadmap's "Order of work while the box is away" (2026-11-25): a batch of core
fixes, all tested on the Mac, each from a line of the roadmap's inbox:

- 2026-11-22 (spec 023's run on the box): a device that speaks during the clock wait keeps a
  `lastSeen` of 1970 until it speaks again (the ZBMINIR2 at 4.6 s, the clock set at 19.5 s); the
  interface shows it from 1970, and once the clock is set its status turns `offline` (now minus
  1970 is beyond any timeout) until its next message.
- 2026-11-23 (spec 024's review): a motion-light whose button loses its device or its `action`
  binding is disabled whole (D38's validation), so the garage stops lighting on motion until the
  button is taken out.
- 2026-11-23 (spec 024): a motion sensor (or its binding) deleted under a running motion-light is
  not settled: the instance keeps the motion sources it resolved at its start.
- 2026-11-22 (spec 021): a device removal in the API.
- 2026-11-22 (architecture review): `set_device_zone` emits nothing; check what Sowel's interface
  reads instead.

This run is autonomous (the user is away): the open questions are decided here and recorded
(decision D45, and "Decisions taken in this spec" below).

## Goals

1. A device's dates are never left in 1970 once the box's clock is set, and its status is not
   turned `offline` by the jump.
2. A motion-light keeps running when one of its buttons loses its `action` binding (its device
   removed, the binding deleted): the button leaves its settings, the lights keep following motion.
3. A running motion-light follows the motion sensors of its zone as they are now: one deleted,
   unbound, disabled or moved out stops counting; one added or moved in starts counting, without
   a restart.
4. The two other inbox lines are closed with what is true today (device removal) or what Sowel
   does (no event for a device's zone change).

## Out of scope

- Other dates taken during the clock wait: recipe log lines and state keys (`timerExpiresAt`,
  `failsafeExpiresAt`), equipments and zones created through the API in the first seconds, a
  device's `createdAt` and `updatedAt` read in 1970 from storage (R1.1 leaves them; the first crossing of R1.2 then moves them to about this boot's date). A new inbox line.
- A button that gets its `action` binding back is not put back in the settings: the user adds it
  again from the recipe's form.
- An instance already disabled for a button before this change is not enabled again by itself
  (it is pruned, and the user enables it).
- Removing a deleted device from the Zigbee network (a leave request): it stays in the network,
  as today, and comes back as a new device if it announces itself during a pairing window.
- Any change to the interface.

## Requirements

### R1: dates of devices taken before the clock is set

`CLOCK_SET_AFTER_MS` (2020-01-01, `home/mod.rs`) tells a clock that was never set.

1. **Read from storage**: when the devices are loaded (`Devices::from_list`), a device `lastSeen`,
   a data `lastUpdated` or a data `lastChanged` before `CLOCK_SET_AFTER_MS` (written by a former
   boot, before this change) is unknown: set to `null` (the device's status is then `unknown` until
   its next message, as a device never heard). `createdAt` and `updatedAt`, which cannot be `null`
   in Sowel's model, are left as they are. Something reset marks the devices' values as changed.
2. **The jump**: the home remembers the last wall time it was given (`Home::observe_wall(now)`,
   one `Option<u64>` kept in `Home`, so it survives the radio restarts that call `engine::run`
   again). `observe_wall` is called first in each turn of `engine::run` (before the activity, the
   measures, the statuses, the recompute and the recipes) and first in `Saver::tick` (before
   anything is written, in the loop and in the firmware's restart loop), and first in every API
   request (before the statuses it refreshes, so a request between the clock's setting and the
   loop's next turn does not see a device offline). When the last wall time
   was before `CLOCK_SET_AFTER_MS` and `now` is at or after it, the jump is `now − last`, and every
   date of every device that is before `CLOCK_SET_AFTER_MS` is moved forward by it: `lastSeen`,
   `createdAt`, `updatedAt`, each data's `lastUpdated` and `lastChanged`. A date at or after
   `CLOCK_SET_AFTER_MS` is not touched. The dates are right within the time between two calls
   (one loop turn, tens of milliseconds; a radio restart's few seconds at worst, the date then a
   little late, never after `now`), and their order is kept. The first call (no last wall time)
   only records it.
3. **Status**: after the shift, the statuses are computed with the shifted `lastSeen`, so a device
   heard during the clock wait stays `online` (it is not turned `offline` by the jump, and no
   `device.status_changed` is emitted for it).
4. **Flash**: a shift marks the devices' values as changed, never their definitions, so it adds no
   write: the box's saves are held until the clock is set (spec 023), the values' 10-minute
   interval starts again at that moment (spec 025), and the corrected dates go with the next
   values write.
5. The clock going back below `CLOCK_SET_AFTER_MS` after being set (not expected) records the
   time; a second crossing shifts again the dates taken since, with its own jump.
6. The work is one walk of at most 64 devices x their data, once per crossing.

### R2: a motion-light button that loses its `action` binding is pruned

1. When the recipes settle (D38: an equipment deleted or changed, a binding removed, a device
   removed, a zone deleted; and the cases of R3.3), before the validation, every motion-light
   instance, enabled or not, loses from its `buttons` each id whose equipment no longer exists or
   no longer has a data binding of alias `action`. Each button taken out writes the warning
   `Button removed from settings: <name>` in the instance's log (`<name>` is the equipment's name;
   for an equipment that no longer exists, its id), and the change is written (`recipes.json`).
2. The instance is then validated as today (D38): with its other settings valid it is not
   disabled. A running one is **not restarted** for a change of its buttons alone (a stale button,
   or a deleted button equipment): it takes its new button list in place (R3.1's refresh), its
   timers, override and lights untouched. It still restarts, as D38 says, when its lights lost an
   id.
3. A deleted button equipment is pruned as today, by the deletion's own step (`Equipment removed
   from settings: <name>`), once: R2.1 does not log it a second time.
4. `POST`/`PUT /api/v1/recipe-instances` still refuse a button without an `action` binding
   (`Invalid params: Button "<name>" has no "action" data binding`): only the settle prunes.
5. The lights are unchanged: a light that loses its zone or its `state` order binding still
   disables the instance (D38).

### R3: a running motion-light follows its zone's motion sensors

1. A running motion-light that is not restarted by the settle is refreshed: it resolves its
   motion sources again (the enabled equipments of its zone and the zones below it, each data
   binding of category `motion`, as at its start) and takes its `buttons` from its settings as
   they now are, without restarting: its timers, override and state keys are kept, nothing is
   logged, no order is sent. (`Runner::refresh(home, params)`, a no-op by default.)
2. What it listens to is updated at once: a removed source no longer resets the safety auto-off;
   a new source does.
3. The settle also runs after an equipment is created, after a data binding is added
   (`POST /api/v1/equipments`, `POST /api/v1/equipments/:id/data-bindings`) and after a zone is
   updated (`PUT /api/v1/zones/:id`: a zone moved under or out of an instance's zone changes its
   sources, and can move a light out of it, which D38's validation then catches). So the triggers
   of a settle are: an equipment created, changed or deleted; a data binding added or removed; an
   order binding removed; a device removed; a zone updated (`PUT`, or moved by a backup import, `POST /api/v1/backup`) or deleted.
4. The zone's motion (which switches the lights) comes from the zone aggregation, recomputed from
   the equipments as they are: it is unchanged by this requirement.

### R4: device removal in the API (already there)

`DELETE /api/v1/devices/:id` exists since spec 024 (204; 404 `Device not found`; 409 `Device is
being paired`; the bindings on its data and orders removed, the recipes settled; the interface's
Devices page deletes, confirmed). Nothing to build: the inbox line is closed, and one test checks
that a removed device that sends a measure again is not added back (it comes back only by joining
again during a pairing window).

### R5: a device's zone change emits no event (as Sowel)

Sowel's `PUT /api/v1/devices/:id` (`deviceManager.update`, `src/devices/device-manager.ts`) emits
no event, and its `EngineEvent` (`src/shared/types.ts`) has no `device.updated`; Sowel's interface
takes the device from the answer of the `PUT`. Solite does the same: `set_device_zone` emits
nothing, and the living spec says so. A device's zone feeds nothing else in Solite (zones aggregate
from equipments, recipes read equipments), so no settle is needed either.

## Decisions taken in this spec

- R1: shift by the jump (the date is right within one turn) rather than `null` (the status would
  turn `unknown`, and the interface would lose the device's last message), and `null` only for a
  1970 date read from storage, whose boot is gone. The last wall time lives in `Home`, not in
  `engine::run`, so a radio restart does not lose it. Recorded as D45.
- R2: prune the button (the inbox line's own proposal) rather than keep it and skip it at run time:
  a kept button that no longer validates would make the next `PUT` of the form fail and a boot's
  restore fail. And no restart for a button change (spec review's proposal): a restart runs the
  start sync, which would switch a lit garage off at once because a button lost its binding.
  Recorded in D46, which narrows D38.
- R3: diverges from Sowel, which resolves the sources once at the start (`resolveMotionSources`,
  spec 012's reference): a box that runs a month alone must follow a sensor replaced or moved.
  Recorded in D46.
- R4, R5: no code; R5 follows Sowel, so no decision.

## Living spec delta

### docs/living-spec/README.md

- "Devices" (the device's fields): New: a date taken before the box's clock is set is moved
  forward by the clock's jump when the clock is set (`lastSeen`, `createdAt`, `updatedAt`, the
  data's `lastUpdated` and the `lastChanged` of the equipment's data); a `lastSeen`,
  `lastUpdated` or `lastChanged` read in 1970 from storage is `null`.
- "Devices" (the API table, `PUT /api/v1/devices/:id`): New: a device's name or zone change emits
  no live event, as Sowel's (the client reads the answer).
- "Runs the motion-light recipe", "When what it uses changes": Changed: the list of triggers
  becomes "an equipment created, changed or deleted, a data binding added or removed, an order
  binding removed, a device removed, a zone updated or deleted"; "A running one whose settings
  lost an id and still validates starts again with them" → only when its lights lost an id; a
  change of its buttons alone is taken in place; "A motion sensor deleted under a running instance
  is not settled: its motion simply stops." → a running instance not restarted resolves its motion
  sources again at every settle, keeping its timers and state. New: a button that no longer has an
  `action` data binding (its device removed, its binding deleted) leaves the settings of every
  motion-light instance with the warning `Button removed from settings: <name>`, instead of
  disabling it.
- "Live events": unchanged list; the device topic's events stay discovered, data updated, status
  changed, removed.

### No longer true

- `docs/living-spec/README.md`: "A motion sensor deleted under a running instance is not settled:
  its motion simply stops." → rewritten as above.
- `docs/living-spec/README.md` (D38 paragraph): "(`At least one light is required`, a light no
  longer in the zone or without its `state` order binding...)" stays true; the paragraph gains the
  button rule.
- `docs/roadmap.md`: the five inbox lines named in the context are removed (log line added).
- `docs/living-spec/README.md` (the "Devices" fields): the `lastSeen`, `createdAt`, `updatedAt` line
  gains the clock-jump and 1970-from-storage rules; the `PUT /api/v1/devices/:id` row gains "no
  live event".
- `docs/living-spec/README.md` (the specs list): row 028 added.
- `docs/architecture.md` (the "Recipes" row): the settle's list of triggers and effects now
  names the buttons without `action`, the restart only for lost lights, and the refresh of the
  running instances.
- Searched for `lastSeen`, `1970`, `clock`, `button`, `motion sources`, `set_device_zone`,
  `device.updated`, `DELETE /api/v1/devices`, `settle`, `stale` in `docs/` (roadmap and decisions
  excepted), `README.md`, `CLAUDE.md`, `inputs/README.md`, `.claude/`: nothing else says the
  opposite (`docs/sowel-subset.md` lists the routes only; `docs/hardware.md`'s "button" is the
  board's).

## Acceptance criteria

- [x] A device heard at 4.6 s of a box's boot, before the clock is set at 19.5 s, shows a
      `lastSeen` of the real date (within one loop turn) and stays `online` (engine test).
- [x] A device whose stored `lastSeen` is in 1970 shows `lastSeen: null` and status `unknown` at a
      start whose clock is set (engine test).
- [x] A motion-light whose button's device is removed keeps running, with `Button removed from
      settings: <name>` in its log and the button gone from its `params` (API test).
- [x] A motion-light whose button's `action` binding is deleted keeps running, same log line.
- [x] A motion sensor whose binding is deleted under a running motion-light no longer resets the
      safety auto-off; a motion sensor added to the zone afterwards does, without a restart.
- [x] A removed device that sends a measure is not added back.
- [x] `PUT /api/v1/devices/:id` with a `zoneId` emits no live event.
- [x] `sh scripts/validate.sh` green.

## Edge cases

- The clock crosses 2020 between two turns of the loop while no device was heard: nothing to
  shift.
- A device paired during the clock wait: its `createdAt` is shifted too.
- A data that never had a value (`lastUpdated: null`): stays `null`.
- A button that is in two instances: pruned from both, one line in each log.
- A button pruned never makes an instance invalid by itself (a motion-light needs lights, not
  buttons): with an empty `buttons` list it runs as one created without buttons.
- A disabled instance: its button is pruned (logged, written), it stays disabled.
- The same button listed twice in an instance's `buttons` (not possible through validation; read
  from an edited file): every copy is taken out, one log line.
- A motion source re-resolved while the safety auto-off runs: the timer keeps its due time.
- The zone of a running instance has no motion sensor left: its sources are empty, and only the
  zone's motion changes reset the safety auto-off (the recipe's rule for an instance started with
  no source, unchanged).

## Sowel contract

- `Device` (`src/shared/types.ts`): `lastSeen: string | null`, `createdAt`, `updatedAt`;
  `DeviceData.lastUpdated: string | null`; `EquipmentDataBinding.lastChanged` (`types.ts:593`), kept per data: unchanged types, `null` used as Sowel's.
- `EngineEvent` (`src/shared/types.ts`): no new event; R5 mirrors Sowel's `device-manager.ts`
  `update`, which emits none.
- `DELETE /api/v1/devices/:id` (`docs/technical/api-reference.md`): unchanged.
- motion-light (`mchacher/sowel-recipe-motion-light` 1.1.2, `specs/012-motion-light/motion-light-reference.md`):
  diverges by re-resolving its sources (R3) and by pruning buttons (R2); D46.

## How the user will check it

On the Mac, no box: `cargo test -p solite-core --test engine`, `--test devices`, `--test
recipe_settle` and `--test api` show the new tests passing. Then the garage through the API:

1. `cargo run -p solite-host -- --sim scenarios/garage.json --serve --speed 10`, then
   `sh scripts/demo-garage.sh` (the garage, its light, its motion sensor, the motion-light).
2. Create a `button` equipment in the garage (`POST /api/v1/equipments`) and bind any data of the
   scenario's sensor to it with the alias `action` (`POST /api/v1/equipments/<button>/data-bindings`).
3. Add it to the instance: `PUT /api/v1/recipe-instances/<id>` with its params plus
   `"buttons": ["<button>"]`.
4. `DELETE /api/v1/equipments/<button>/data-bindings/<binding>`: `GET /api/v1/recipe-instances`
   shows the instance still enabled, its `buttons` empty, and its log
   (`GET .../log`) reads `Button removed from settings: <name>`; the light still follows motion.

The clock jump is checked on the box at its next session (the roadmap's "Tests to run with the box
in hand": a device heard before the clock shows a real `lastSeen` and stays `online`).
