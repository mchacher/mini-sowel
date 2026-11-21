# Plan: Spec 012

## Steps

- [x] 1. Living spec updated
- [x] 2. Core types: `EngineEvent`, recipe types, durations
- [x] 3. Core logic: events from the home; the engine; motion-light (params, validate, machine)
- [x] 4. Orders: unattended bound in the queue
- [x] 5. API: the recipe routes
- [x] 6. Interface: not applicable
- [x] 7. Host: events and tick in the radio loop; `recipes.json`
- [x] 8. Simulator: nothing new (the garage scenario has motion and the relay)
- [x] 9. Tests; each module seen failing once
- [x] 10. Docs, decision D17, Sowel issue lines

## Test plan

The machine tests drive `MotionLight` with events and a fake time; "light ON" means the light's
`state` data, updated by the test as the device's echo would.

| Module | Scenario | Expected |
| ------ | -------- | -------- |
| duration | `"10m"`, `"30s"`, `"1h"`, `"5 m"`, `120000`, `"10M"`, `"1d"`, `"1.5m"`, `true` | 600000, 30000, 3600000, 300000, 120000, then the two error texts of the reference §2 |
| duration | `format_duration` 30000, 600000, 5400000, 7200000 | `30s`, `10min`, `2h` (rounded), `2h` |
| motion_light | `is_motion_active` true set and false set of reference §8 tests 1-2 | as listed |
| motion_light | motion source key equal / different (§8 test 3) | as listed |
| motion_light | motion sources of the zone and a child zone, a non-motion binding, a disabled equipment (§8 test 4) | 3 sources |
| motion_light | `RecipeInfo` serialized | equals the reference §1 JSON |
| motion_light | each `validate` message of §2 (zone missing, zone unknown, no light, light unknown, wrong type, other zone, no `state` binding, bad timeout, bad lux, bad maxOn, button unknown, button without `action`) | 400 `Invalid params: <exact text>` |
| motion_light | legacy `light` param; `lights` a comma string | used; not split (no light: error) |
| motion_light | zone without motion sensor | log warn `Zone has no motion sensors — recipe will never trigger` at start |
| motion_light | start with no motion | OFF sent, `Recipe activated — no motion, lights off` |
| motion_light | start with motion | ON sent, `Motion detected — 1 light(s) turned on` |
| motion_light | motion → ON; motion clears → timer; timeout → OFF with `No motion for 20s — lights turned off` | as listed |
| motion_light | motion returns before the timeout | timer cancelled, light stays on |
| motion_light | external ON with no motion, heartbeat ON at 60 s, OFF at 120 s (§8 test 6) | as listed |
| motion_light | OFF then a new external ON: fresh countdown (§8 test 7) | as listed |
| motion_light | `luxThreshold ""` with lux 1 (§8 test 8) | ON |
| motion_light | `luxThreshold 50`, lux 120 (§8 test 9); lux 50 | stays OFF with its log line; ON |
| motion_light | light on by recipe, lux = 55 (50 × 1.1), then 56 | stays on; OFF with `Luminosity above threshold — lights turned off` |
| motion_light | light on by hand above lux × 1.1 | override, `Light turned on manually above lux threshold — entering override mode` |
| motion_light | light switched off by hand while motion | override; motion ignored with its log; cleared after the timeout without motion |
| motion_light | echo OFF of the recipe's own OFF within 2 s | no override |
| motion_light | button with light on; with light off | OFF and override; ON |
| motion_light | failsafe 5m: fires with its warn line; impulses every 4 min keep it from firing; zone motion resets it | as listed |
| motion_light | `disableWhenDaylight` true | no effect |
| motion_light | stop | three state keys gone, no order sent |
| motion_light | durations above 24.8 days | the timer fires at its time, not at once |
| recipes | create, list with `state`, get, update, delete, enable, disable, actions, log with limit | Sowel's statuses and bodies (R5) |
| recipes | unknown recipe; unknown instance on each route; actions on a disabled instance; body not an object | 404 / 404 / 400 `Instance not running` / 400 `Invalid body` |
| recipes | 17th instance; params over 2 KB | 400 with the texts of R2, R7.1 |
| recipes | 51st log line | the oldest dropped, newest first |
| recipes | an instance whose handler fails | the others still run |
| recipes | `recipes.json` round trip; an instance whose light was deleted at restart | restored; `Failed to restore: Light equipment not found: <id>`, stopped |
| events | a report of the same value twice | two `device.data.updated`, two `equipment.data.changed` |
| events | a recompute with no change; with motion changed | no `zone.data.changed`; one per zone changed (the zone and its parents) |
| orders | 3 unattended orders and 2 HTTP orders | none refused; sent in the order queued |
| recipes | PUT with invalid params | 400; the instance still running, its params unchanged |
| recipes | PUT / DELETE unknown id | 404 `Instance not found` |
| recipes | 17 lights; 17 buttons | 400 `Invalid params: At most 16 lights` / `... buttons` |
| motion_light | a disabled light among two | ON sent to the other only, no log line |
| recipes | PUT `params` not an object; `/actions` without `action`, with `payload` a string | 400 `Invalid body` |
| recipes | log `limit` absent, `0`, `-3`, `abc`, `10`, `500` | 50, 50, 50, 50, 10, 50 entries at most |
| recipes | `recipes.json` corrupt; unknown recipeId; 17 entries; duplicate id; params over 2 KB | host stops; kept not started with its log line; 16 kept; second dropped; kept not started |
| recipes | a log message over 200 bytes | cut at 200 bytes |
| duration | `-5`, `1500.5`, `"99999999999999999999h"` | 0, 1500, format error |
| motion_light | `luxThreshold` `"50"`, `" "`, `true`, `"abc"`, `null` | 50, 0, 1, error, no threshold |
| events | only events an instance listens to are queued | an unrelated zone or device gives none |
| events | 129 events | the oldest dropped |
| events | a zone created over the API, then a loop turn | its zone event handed over at that turn |
| events | first recompute after a start | one zone event per zone |
| orders | silent relay: 1 unattended order being sent, then 1 HTTP order | the HTTP order is taken next, before other unattended ones; 502 if past its deadline |
| orders | 33rd unattended order | dropped |
| memory | realistic home + 3 instances with full logs of 200-byte messages + full event queue (128) | peak below 200 KB |
| host | sim garage, instance with timeout 20s: motion → `state` true; after the timeout → false; log route | as the acceptance criterion |
| host | restart with `recipes.json` | instance running again (start sync sends OFF) |

## Validation

- [x] `sh scripts/validate.sh` green
- [x] Every row above has its test
- [x] Code review done, findings fixed
