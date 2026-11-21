# motion-light 1.1.2: the reference behaviour (summary of the TypeScript recipe, with sources)

Sources. `R` = recipe `src/index.ts`, `M` = recipe `manifest.json`, `T` = recipe `src/index.test.ts`
(all under the repository `mchacher/sowel-recipe-motion-light` at version 1.1.2, read on 2026-11-21). Sowel engine under
`../sowel/`: `ty` = `src/shared/types.ts`,
`RM` = `src/recipes/engine/recipe-manager.ts`, `LH` = `src/recipes/engine/light-helpers.ts`,
`DU` = `src/recipes/engine/duration.ts`, `RT` = `src/api/routes/recipes.ts`,
`ST` = `src/recipes/engine/recipe-state-store.ts`. "(inferred)" marks conclusions drawn from the code,
not stated in it.

The recipe uses its **own timers** (`setTimeout`). It does NOT use Sowel's timed-action primitive
(spec 174): no `timedAction`/`dispatchOrder` with a window anywhere in R (inferred from reading all of R).

## 1. Identity

| Field | Value | Source |
|---|---|---|
| id | `motion-light` | R:276, M:2 |
| name (definition, what the API returns) | `Motion Light` | R:277 |
| description (definition, what the API returns) | `Turns on lights when motion is detected, turns off after a timeout with no motion. Supports lux threshold, failsafe max-on duration, and button override.` | R:278-279 |
| manifest description (NOT what the API returns) | `Turns on lights when motion is detected, turns off after a timeout with no motion` | M:6 |
| version | `1.1.2` (manifest only, not in RecipeInfo) | M:4 |
| icon | `Lightbulb` (from manifest, injected as `icon` by the loader) | M:7, recipe-loader.ts:148-150, RM:169 |
| category | none. No `category` field exists in the manifest or RecipeInfo. `tags`: `automation, motion, light` (manifest only) | M:10-14, ty:821-832 |
| author / repo / sowelVersion | `mchacher` / `mchacher/sowel-recipe-motion-light` / `>=0.10.0` | M:8-9, M:21 |
| manifest i18n.fr | name `Lumière sur mouvement`, description `Allume les lumières sur détection de mouvement, éteint après un délai sans mouvement.` (manifest only; the API uses the definition's i18n below) | M:15-19 |
| actions / tile / groups | none declared (no `actions`, no `tile`, no slot `group`, no `hiddenWhen`, no `options`) | R:275-318 |

`RecipeInfo` is built by `registerExternal` from the definition: `id, name, description, slots`, then
`actions` only if non-empty, `tile` only if declared, `i18n` only if non-empty, `icon` only if a
non-empty string (RM:152-170). Absent optional fields are omitted from the JSON (not null).

### Exact `RecipeInfo` returned by `GET /api/v1/recipes` (array element), and `GET /api/v1/recipes/motion-light`

```json
{
  "id": "motion-light",
  "name": "Motion Light",
  "description": "Turns on lights when motion is detected, turns off after a timeout with no motion. Supports lux threshold, failsafe max-on duration, and button override.",
  "slots": [
    { "id": "zone", "name": "Zone", "description": "Zone to monitor", "type": "zone", "required": true },
    { "id": "lights", "name": "Lights", "description": "Lights to control (must belong to the selected zone)",
      "type": "equipment", "required": true, "list": true, "constraints": { "equipmentType": "light_onoff" } },
    { "id": "timeout", "name": "Timeout", "description": "Delay with no motion before turning off",
      "type": "duration", "required": true, "defaultValue": "10m" },
    { "id": "luxThreshold", "name": "Lux Threshold",
      "description": "Lights won't turn on when ambient brightness exceeds this value; turns off if it rises above threshold + 10% hysteresis",
      "type": "number", "required": false, "constraints": { "min": 0 } },
    { "id": "maxOnDuration", "name": "Safety Auto-off",
      "description": "Failsafe for a stuck sensor: force lights off after this duration with no fresh motion. Active presence keeps resetting it, so it won't cut the light while you're in the room.",
      "type": "duration", "required": false },
    { "id": "buttons", "name": "Manual Switches", "description": "Physical switches for manual on/off toggle",
      "type": "equipment", "required": false, "list": true, "constraints": { "equipmentType": "button" } },
    { "id": "disableWhenDaylight", "name": "Inactive During Day",
      "description": "Do not turn on lights during daytime (based on sunrise/sunset and offsets from settings)",
      "type": "boolean", "required": false }
  ],
  "i18n": {
    "fr": {
      "name": "Lumière sur mouvement",
      "description": "Allume les lumières quand un mouvement est détecté, éteint après un délai sans mouvement. Supporte un seuil de luminosité optionnel et une durée max d'allumage.",
      "slots": {
        "zone": { "name": "Zone", "description": "Zone à surveiller" },
        "lights": { "name": "Lumières", "description": "Lumières à contrôler (doivent appartenir à la zone)" },
        "timeout": { "name": "Délai", "description": "Délai sans mouvement avant extinction" },
        "luxThreshold": { "name": "Seuil lux (max)", "description": "Au-dessus de ce seuil, les lumières ne s'allument pas" },
        "maxOnDuration": { "name": "Extinction auto (sécurité)",
          "description": "Anti-oubli pour capteur bloqué : coupe les lumières après cette durée sans nouveau mouvement. Une présence active la réarme en continu, donc elle ne coupe pas tant que tu es dans la pièce." },
        "buttons": { "name": "Interrupteurs", "description": "Interrupteurs physiques pour allumer/éteindre manuellement" },
        "disableWhenDaylight": { "name": "Inactif le jour", "description": "Ne pas allumer pendant la journée (basé sur lever/coucher du soleil)" }
      }
    }
  },
  "icon": "Lightbulb"
}
```
Sources: slots R:195-267 (base zone R:195-205, lights R:259-267, trailing R:207-252), i18n R:287-318,
RecipeInfo shape ty:821-832, RecipeSlotDef ty:695-757 (fields `id,name,description,type,required,list,defaultValue,options,hiddenWhen,constraints{equipmentType,min,max,crossZone,includeDescendants},group`).
Notes: no `i18n.fr.groups`; the fr pack has no entry for no-slot ids (all 7 present). The slot
description for `disableWhenDaylight` says "offsets from settings", but the code only reads the root
zone's `isDaylight` (R:471-476). The `equipmentType` constraint is a UI filter; validate re-checks it
for lights only (not for buttons).

## 2. Parameters

Instance `params` is a free JSON object (`{ "type": "object" }`, RT:19-21). Keys:

| Key | JSON type accepted | Default | Meaning |
|---|---|---|---|
| `zone` | string (zone UUID) | required | zone monitored; also zone every light must belong to (R:324-358) |
| `lights` | array of strings (equipment ids), non-strings dropped | required, >= 1 | R:180-189 |
| `light` | string (legacy) | - | legacy single light, used ONLY when `lights` is not an array: becomes `[light]` (R:184-186). A comma-separated string for `lights` is NOT split (`normalizeStringArray` R:170-178 exists but is unused). |
| `timeout` | string `^(\d+)\s*(s|m|h)$` or number (ms) | `"10m"` | off delay. Falsy (`""`, `0`, null, absent) -> `"10m"` (R:367, 410) |
| `luxThreshold` | number or numeric string; `undefined`/`null`/`""` = no threshold | none | R:371-376, 411-416 |
| `maxOnDuration` | duration as `timeout`; `undefined`/`null`/`""` = disabled | disabled | R:378-381, 417-422 |
| `buttons` | array of strings (equipment ids); non-array ignored (= none) | none | R:383-399, 423-425 |
| `disableWhenDaylight` | only literal `true` enables | false | R:426 |

Duration parsing (engine helper, DU:9-27): number -> returned as ms unchanged (any number, even
negative/0); non-string non-number -> `Invalid duration: <value>`; string not matching
`^(\d+)\s*(s|m|h)$` -> `Invalid duration format: <value>. Use e.g. "10m", "30s", "1h"`. No `d`, no
decimals, no uppercase. `parseInt` then x1000 / x60000 / x3600000. `formatDuration` (DU:32-36): ms >= 3600000 ->
`<round(ms/3600000)>h`; ms >= 60000 -> `<round(ms/60000)>min`; else `<round(ms/1000)>s`.

### `validate(params, ctx)` order and EXACT messages (R:324-400)
Each failure throws `Error(msg)`; the engine wraps it as HTTP 400 `{"error":"Invalid params: <msg>"}` (RM:325-332, RT:75).
1. `zone` missing / falsy / not a string -> `Zone parameter is required`.
2. zone id unknown -> `Zone not found: <zone>`.
3. (warn, not an error) if zone aggregate exists and `motionSensors === 0`: instance log, level warn:
   `Zone has no motion sensors — recipe will never trigger`. Because the engine also calls `validate`
   on every start (RM:570), this line is logged each time an instance starts, not at creation (at
   creation the temp context's instance id `"temp"` violates the log FK; the insert error is swallowed, RM:746-750; inferred).
4. `lights` empty after normalisation -> `At least one light is required`.
5. For each light in order: not found -> `Light equipment not found: <id>`;
   type not in `["light_onoff"]` -> `Light "<name>" is type "<type>" but this recipe requires light_onoff`
   (list joined with " or "; only one allowed type);
   `equipment.zoneId !== zone` (strict, descendants NOT accepted) -> `Light "<name>" does not belong to the selected zone`;
   no order binding with `alias === "state"` -> `Light "<name>" has no "state" order binding`.
6. `timeout` (default `"10m"`) -> `parseDuration` errors above.
7. `luxThreshold` (if not undefined/null/"") -> `Number(x)` NaN or < 0 -> `luxThreshold must be a non-negative number`.
8. `maxOnDuration` (if not undefined/null/"") -> `parseDuration` errors above. (No check that it exceeds `timeout`.)
9. `buttons` (if not undefined/null; only array entries that are strings are checked): not found -> `Button equipment not found: <id>`;
   no data binding with `alias === "action"` -> `Button "<name>" has no "action" data binding`. Button equipment type is NOT checked.

Engine-level errors: unknown recipe -> 404 `Recipe not found: <id>` (RM:318). Body failing the Fastify
schema (`recipeId` non-empty string, `params` object) -> framework 400 (RT:11-21; message text inferred).

## 3. Inputs and outputs

### Inputs (event bus subscriptions, all unsubscribed on stop)
| Event | Filter | Handler | Source |
|---|---|---|---|
| `zone.data.changed` | `event.zoneId === params.zone` (exact zone; the aggregate already merges descendants) | `onZoneChanged(aggregatedData.motion, aggregatedData.luminosity)` | R:851-856; event ty:1857; aggregate fields ty:286-316 |
| `device.data.updated` | only subscribed if `maxOnDuration` set AND >= 1 motion source resolved; `(event.deviceId, event.key)` in `motionSources`, and `isMotionActive(event.value)` | `resetFailsafeTimer()` | R:864-877; event ty:1836-1847 |
| `equipment.data.changed` (lights) | `equipmentId` in lights AND `alias === "state"` | `onLightChanged(value)` | R:880-885; event ty:1863-1872 |
| `equipment.data.changed` (buttons) | only if buttons non-empty; `equipmentId` in buttons AND `alias === "action"` (ANY value, no value check, fires on every event) | `onButtonAction()` | R:888-895 |

Zone aggregate fields read: `motion` (bool, OR of motion data of the zone tree, `?? false` when no aggregate),
`luminosity` (number|null, rounded mean of luminosity data, zone-aggregator.ts:153-154), `motionSensors` (validate only),
`isDaylight` (bool|null) read from the ROOT zone `00000000-0000-0000-0000-000000000001` (R:107, 471-476).
`isDaylight === true` is "day"; `null`/`false`/no data = night (R:474-475).
Pulled (not event) reads: `zoneAggregator.getByZoneId(zone)` for current motion (R:482-485, 830-832);
`helpers.isAnyLightOn` for the light state (LH:11-23: for each light, data binding with category `light_state`, fallback alias `state`; ON iff value `true` or string upper-case `"ON"`).
`onLightChanged` ON test: `value === true || String(value).toUpperCase() === "ON"` (R:742).

Motion sources (`resolveMotionSources`, R:149-168): descendants of the zone via `zoneManager.getDescendantIds`
(on throw: only the zone itself); for each equipment in each of those zones (`getByZone`) skipping `enabled === false`;
for each data binding with `category === "motion"` and truthy `deviceId` and `key`: add `"<deviceId> <key>"`
(single space, R:125-127). Resolved ONCE at instance start. `isMotionActive` (R:133-138): boolean as is; number != 0;
else `String(v).trim().toLowerCase()` in {`true`,`on`,`1`,`occupied`,`detected`}; everything else (incl. null/undefined -> "null"/"undefined") false.

### Outputs (orders)
All via `helpers.turnOnLights / turnOffLights` for ALL configured lights, one order per light, fire-and-forget,
errors from dispatch ignored (LH:49-90). Per light: order binding found by category `light_toggle` or `toggle_power`,
else alias `state` (LH:31-35). Value: the binding's `enumValues` entry equal (case-insensitive) to `on`/`off`, else `"ON"`/`"OFF"` (LH:41-47).
Equivalent in Sowel API terms: `executeOrder(lightId, <binding alias>, "ON"|"OFF", {kind:"recipe", instanceId, recipeName, recipeId})` (RM:722-737).
A light with no resolvable binding is skipped and reported in the returned error list: `"<lightId>: no toggle order binding"`.
No brightness or other order is ever sent. Helper error list handling: if non-empty, recipe logs error
`Error turning on some lights: <errs joined "; ">` / `Error turning off some lights: <...>` (R:543-545, 561-563, 575-577, 806-808).
Exception: the override-clear timer path (R:635-640) and nothing else ignores the error list.

## 4. State machine

### Constants
- `TURN_OFF_GRACE_MS = 2000` (R:118; code comments say "5s" at R:440 but the value is 2000).
- `LUX_HYSTERESIS_FACTOR = 0.1` (R:111).
- `timeout` default 10 min; `maxOnDuration` optional; no other timing constant.
- Timers: `offTimer` (off timeout), `failsafeTimer` (max-on); single instance each, `Date.now()` for grace.

### Variables (R:429-446)
`offTimer`, `failsafeTimer`, `overrideMode` (false), `lightsOnByRecipe` (init = `isAnyLightOn`, then overwritten by the start sync),
`lastKnownLightOn` (init = `isAnyLightOn`), `turnOffGraceUntil` (0, epoch ms), `stopped` (false), `motionSources`.
On creation: `state.delete("overrideMode")` (R:449).

### Lux and daylight predicates
- `isTooBright(l)`: false if no threshold or `l` null; else `l > threshold` (strict). So turn-on allowed when `l <= threshold` (R:455-459).
- `isBrightEnoughToTurnOff(l)`: false if no threshold or `l` null; else `l > threshold * 1.1` (strict) (R:461-465).
- `isDaytime()`: false unless `disableWhenDaylight`; else root zone `isDaylight === true` (R:471-476). Applied ONLY to automatic turn-on from motion and to the start sync; never when turning off, on button press, or on external ON.

### Primitive actions
- `turnOn()`: `lightsOnByRecipe=true; lastKnownLightOn=true;` send ON; log info `Motion detected — <N> light(s) turned on` (N = number of configured lights); `startFailsafeTimer()` (R:541-554).
- `turnOff(reason)`: `lightsOnByRecipe=false; lastKnownLightOn=false; turnOffGraceUntil=now+2000;` send OFF; log info `<reason>`; cancel failsafe timer + delete `failsafeExpiresAt`; `clearOverrideMode()` (delete `overrideMode`) (R:556-568). Does NOT cancel the off timer.
- `turnOffFailsafe()`: same flags and grace, send OFF, log **warn** `Failsafe: lights forced off after <formatDuration(maxOnDuration)> max on duration`; does NOT clear override mode (R:570-582).
- `startFailsafeTimer()`: no-op if `maxOnDuration` null or already armed; else arm, persist `failsafeExpiresAt` (R:588-600). On expiry: clear failsafe state, cancel off timer, delete `timerExpiresAt`, `turnOffFailsafe()`.
- `resetFailsafeTimer()`: no-op if `maxOnDuration` null or no failsafe armed; else restart with full duration, persist (R:602-614). It never arms one.
- `startOffTimer()`: cancel then arm `timeout`, persist `timerExpiresAt`; on expiry delete `timerExpiresAt` and `turnOff("No motion for <formatDuration(timeout)> — lights turned off")` (R:620-628).
- `startOffTimerForOverrideClear()`: cancel then arm `timeout`, persist `timerExpiresAt`; on expiry delete `timerExpiresAt`; if any light is ON: `lightsOnByRecipe=false; lastKnownLightOn=false; grace=now+2000;` send OFF (error list ignored); then `clearOverrideMode()`, cancel failsafe + delete `failsafeExpiresAt`, log info `No motion for <fmt> — override cleared, lights off` (always logged) (R:630-649).
- `clearOverrideMode()`: only if `overrideMode`: set false, delete state key (R:531-535).

### `onZoneChanged(motion, luminosity)` (R:655-737), ignored if `stopped`
1. **Override mode active**: if `motion`: cancel off timer, delete `timerExpiresAt`, `resetFailsafeTimer()`, log info `Motion detected but override mode active — ignoring`. Else if no off timer armed: `startOffTimerForOverrideClear()` (edge guard). Return.
2. Auto mode. `lightsOn = isAnyLightOn()`.
3. **Lux turn-off**: if `lightsOn && isBrightEnoughToTurnOff(lux)`:
   - if `!lightsOnByRecipe` (light ON but not by the recipe, i.e. manual): if `now < graceUntil` return; else `overrideMode=true`, persist `overrideMode=true`, `startFailsafeTimer()`, log info `Light turned on manually above lux threshold — entering override mode`; return.
   - else cancel off timer, delete `timerExpiresAt`, `turnOff("Luminosity above threshold — lights turned off")`; return.
4. `motion && !lightsOn`:
   a. if `now < graceUntil` return (silent).
   b. if `lightsOnByRecipe` (recipe had it on but it is now off, treated as a manual turn-off): `lightsOnByRecipe=false; overrideMode=true`, persist, `startOffTimerForOverrideClear()`, log info `Light turned off manually while motion active — entering override mode`; return. (Failsafe timer is left running here.)
   c. if `isTooBright(lux)`: log info `Motion detected but luminosity <lux> exceeds threshold <threshold> — not turning on`; return (logged on every such zone event, no dedupe; numbers are JS number-to-string).
   d. if `isDaytime()`: log info `Motion detected but daytime — not turning on`; return.
   e. `turnOn()`.
5. `motion && lightsOn`: cancel off timer, delete `timerExpiresAt`, `resetFailsafeTimer()`.
6. `!motion && lightsOn`: `if (!offTimer) startOffTimer()` (armed only once on the motion true->false edge; later zone events do not rearm).
7. `!motion && !lightsOn`: nothing.

### `onLightChanged(value)` (R:739-793), ignored if `stopped`
1. `lightOn` computed (see section 3). If `lightOn === lastKnownLightOn` return (heartbeat/duplicate guard). Else `lastKnownLightOn = lightOn`.
2. If `overrideMode` return (state still tracked).
3. `motion = zone aggregate motion now` (`getByZoneId(zone)?.motion ?? false`).
4. `lightOn && !motion`: `startOffTimer()`, `startFailsafeTimer()`, log info `Light turned on externally — turning off in <fmt(timeout)>`.
5. `lightOn && motion`: cancel off timer, delete `timerExpiresAt` (no log).
6. `!lightOn`: if `now < graceUntil` return (echo of our own OFF). Else if `lightsOnByRecipe && motion`: `lightsOnByRecipe=false; overrideMode=true`, persist, cancel both timers, delete both expiry keys, `startOffTimerForOverrideClear()`, log info `Light turned off manually while motion active — entering override mode`; return. Else `lightsOnByRecipe=false`; if any timer armed: cancel both, delete both keys, log info `Light turned off externally — timers cancelled`.
Note: step 4/5 never set `lightsOnByRecipe`; it stays as it was.

### `onButtonAction()` (R:799-823), on every `action` data change of any listed button
- If any light ON: `lightsOnByRecipe=false; lastKnownLightOn=false; grace=now+2000;` send OFF (error list logged as above); cancel both timers, delete both keys; `overrideMode=true`, persist; log info `Button pressed — lights off, entering override mode`; `startOffTimerForOverrideClear()`.
- Else: `clearOverrideMode()` then `turnOn()` (logs `Motion detected — N light(s) turned on`; ignores lux and daylight; no off timer is armed here, so the light stays on until a zone no-motion edge, an external-light event, or the failsafe: inferred).

### Failsafe impulses
Each raw `device.data.updated` from a motion source with an active value calls `resetFailsafeTimer()` (also while in override mode, if a failsafe is armed). Zone events with `motion` also reset it (auto step 5, override step 1). Failsafe only fires after `maxOnDuration` without any such impulse.

### Start (`createInstance`), activation, stop, disable
- `createInstance` (R:406-924) parses params (section 2), subscribes (zone, impulse, light, button), then `syncLightsOnStart()` then `turnOffGraceUntil = 0` (R:897-901).
- `syncLightsOnStart` (R:829-844): reads current zone `motion`/`luminosity`. If `motion && !isTooBright && !isDaytime` -> `turnOn()` (sends ON, arms failsafe, log `Motion detected — N light(s) turned on`). Else `turnOff(reason)` — ALWAYS sends OFF orders and logs reason: `Recipe activated — daytime, lights off` (if daytime, checked first), else `Recipe activated — luminosity above threshold, lights off` (if motion), else `Recipe activated — no motion, lights off`. Consequence: every start / update / re-enable / process restart forces the lights to the recipe's idea of state, switching a light that was on off when no motion (inferred). No off timer is armed at start.
- Persisted timers are NOT restored: no `ctx.state.get` exists in R (inferred by search). `overrideMode` is deleted at start.
- `stop()` (R:908-922): `stopped=true`; cancel both timers; unsubscribe all; `overrideMode=false; lightsOnByRecipe=false; grace=0`; delete state keys `overrideMode`, `timerExpiresAt`, `failsafeExpiresAt`. Does NOT switch lights off. Disable, delete, update (stop then start new), package update restart all call it.
- The `turnOffLights` result of `syncLightsOnStart` does not suppress override detection because grace is zeroed after sync.

### Known quirks to preserve or consciously decide on (inferred)
- Stale race: after `turnOn()` the light state is read from the equipment, which only updates on device echo; a zone event arriving before the echo with `motion` true sees `lightsOn=false && lightsOnByRecipe=true` and enters override ("manual turn off") (R:694-710).
- `turnOffFailsafe` leaves `overrideMode` set; override then clears on the next no-motion zone event (R:665-669) via the override-clear timer.
- Lux-override (R:682-686) arms the failsafe but no off timer; in override the clear timer is armed on the next zone event without motion.
- `validate` requires an order binding aliased `state`, while the dispatch resolves by category first.
- JS timers cap near 24.8 days (2^31-1 ms); `maxOnDuration`/`timeout` above that would fire immediately in the TS code (inferred; decide a cap in Rust).

## 5. Persistent instance state (`ctx.state`, table `recipe_state`)
Key/value per instance, JSON-serialised, cascades on instance delete (ST:7-65; migrations/001_initial.sql:102-108). Keys written by this recipe, all write-only (never read back by the recipe):

| Key | Value | Written | Deleted |
|---|---|---|---|
| `overrideMode` | `true` | entering override (R:683, 706, 775, 815) | clearOverrideMode, start, stop |
| `timerExpiresAt` | ISO 8601 UTC string `now + timeout` (`toISOString()`, `Z`) | startOffTimer / startOffTimerForOverrideClear (R:491-494) | off-timer expiry, motion reset, external off, button, stop. NOT deleted by `turnOff()` itself or at start (stale value possible, inferred) |
| `failsafeExpiresAt` | ISO 8601 UTC `now + maxOnDuration` | arm/reset failsafe (R:500-503) | turnOff, failsafe expiry, external off, button, stop |

Every set/delete schedules a `recipe.instance.state.changed` event (microtask-coalesced, ST:31-38). `GET /api/v1/recipe-instances` returns it as `state` (RM:270-276, 302-313). What survives a restart in practice: only instance params and enabled flag (`recipe_instances`), the log, and the state keys above as informational data; behaviour restarts from the start sync.

## 6. Log messages (RecipeLogEntry, level info unless noted)
Written by the recipe via `ctx.log` (stored by RM:730-734; also mirrored to the server logger):
1. `Zone has no motion sensors — recipe will never trigger` (warn; at each start)
2. `Error turning on some lights: <errors joined "; ">` (error); `Error turning off some lights: <...>` (error)
3. `Motion detected — <N> light(s) turned on`
4. `<reason>` from turnOff: `No motion for <fmt> — lights turned off`; `Luminosity above threshold — lights turned off`; `Recipe activated — daytime, lights off`; `Recipe activated — luminosity above threshold, lights off`; `Recipe activated — no motion, lights off`
5. `Failsafe: lights forced off after <fmt> max on duration` (warn)
6. `No motion for <fmt> — override cleared, lights off`
7. `Motion detected but override mode active — ignoring`
8. `Light turned on manually above lux threshold — entering override mode`
9. `Light turned off manually while motion active — entering override mode` (two places)
10. `Motion detected but luminosity <lux> exceeds threshold <thr> — not turning on`
11. `Motion detected but daytime — not turning on`
12. `Light turned on externally — turning off in <fmt>`
13. `Light turned off externally — timers cancelled`
14. `Button pressed — lights off, entering override mode`
`<fmt>` = `formatDuration` (`10min`, `30s`, `2h`; rounding as in section 2). Dashes are em dashes (U+2014).
Written by the engine for this instance: `Instance created` (info, RM:346); `Restarted with updated recipe package` (RM:198);
`Failed to start: <err>` / `Failed to restore: <err>` / `Failed to restart: <err>` / `Failed to restart after update: <err>` (error, RM:357, 247, 435, 204).
Log table: `recipe_log(id autoinc, instance_id FK cascade, timestamp default CURRENT_TIMESTAMP, message, level default 'info')` (migrations/001_initial.sql:109-117).

## 7. Instance routes (RT, RM)
Base `/api/v1`. `RecipeInstance` JSON (ty:834-840): `{ "id": uuid, "recipeId": "motion-light", "params": {...}, "enabled": true, "createdAt": "<ISO8601 UTC>" }`
(`createdAt` from sqlite CURRENT_TIMESTAMP via `toISOUtc`, RM:845). Ids are `randomUUID` (RM:334).

| Route | Request | Success | Errors |
|---|---|---|---|
| `GET /recipes` | - | 200 `RecipeInfo[]` | - |
| `GET /recipes/:recipeId` | - | 200 `RecipeInfo` | 404 `{"error":"Recipe not found"}` (RT:48) |
| `GET /recipe-instances` | - | 200 array of `RecipeInstance & {state:{...}}` ordered by `created_at` (RM:111, 270) | - |
| `POST /recipe-instances` | `{ "recipeId": string (non-empty), "params": object }` | 201 `RecipeInstance` (without `state`), `enabled:true`; emits `recipe.instance.created`, logs `Instance created`, then starts. A start failure (after validate passed) does NOT fail the request: logged as error + `recipe.instance.error`, instance stays saved and enabled (RM:349-368) | 404 `{"error":"Recipe not found: <id>"}`; 400 `{"error":"Invalid params: <msg>"}`; schema 400 |
| `PUT /recipe-instances/:id` | `{ "params": object }` (whole replacement) | 200 `RecipeInstance` (new params); validates, stops, saves, restarts only if enabled (RM:393-448); a restart failure is only logged | 404 `{"error":"Instance not found"}`; 404 `Recipe not found: <id>`; 400 `Invalid params: <msg>` |
| `DELETE /recipe-instances/:id` | - | 204, stops, deletes row + state + log, emits `recipe.instance.removed` | 404 `Instance not found` |
| `POST /recipe-instances/:id/enable` | - | 200 `{"success":true}`; no-op if already enabled; sets enabled=1 and starts (start errors only logged, not returned) | 404 `Instance not found` |
| `POST /recipe-instances/:id/disable` | - | 200 `{"success":true}`; stops runner, sets enabled=0 (no-op write if already 0) | 404 `Instance not found` |
| `POST /recipe-instances/:id/actions` | `{ "action": string (non-empty), "payload"?: object }` | 200 `{"ok":true}`. motion-light declares no actions and no `onAction`, so any action is accepted and silently ignored while running (RM:795-797, R:99 handle has only `stop`) | 400 `{"error":"Instance not running"}` (disabled or failed start); 404 unknown id is reported the same as not running (400) since only the running map is checked (RM:527-531) |
| `GET /recipe-instances/:id/log?limit=N` | `limit` default 50 (`parseInt`, base 10) | 200 `RecipeLogEntry[]` `{id:int, instanceId, timestamp ISO UTC, message, level}` newest first (`ORDER BY timestamp DESC LIMIT ?`, second resolution so ties are unordered, inferred) | unknown id gives `[]`; invalid `limit` -> NaN (behaviour unspecified) |
Lifecycle events emitted on the bus: `recipe.instance.created|started|stopped|removed|error|state.changed` (RM:345, 585, 611, 390, 362, 718).
At boot (`init`, RM:221-256): every enabled instance is validated and started; failure writes log `Failed to restore: <err>` and skips it.
Shadow mode (spec 124) makes `startInstance` a no-op (RM:552-558).

## 8. Test plan from `T` (9 tests; the Rust port should reproduce each)
`formatDuration` is mocked as `<ms>ms` and `parseDuration` as `^(\d+)(s|m|h)$` in the harness (T:155-161); `log` is a no-op. Timers are faked (vitest fake timers).
1. `isMotionActive` is true for `true, 1, 5, "true", "TRUE", "on", "On", "1", "occupied", "detected"` (T:10-15).
2. `isMotionActive` is false for `false, 0, "false", "off", "0", "clear", "", null, undefined` (T:17-21).
3. `motionSourceKey` is stable for equal `(device,key)` and differs when device or key differs (T:24-30).
4. `resolveMotionSources` collects motion bindings of the zone and its descendant zone, skips non-motion bindings and disabled equipment; size 3 (T:64-72).
5. `resolveMotionSources` falls back to the zone alone when `getDescendantIds` throws; size 2, descendant excluded (T:74-84).
6. Periodic re-report: external ON (no motion) arms a 2m off timer; an `ON` heartbeat at 60 s is ignored and the light is OFF at 120 s (T:186-205).
7. A genuine OFF (timer) then a new external ON starts a fresh 2m countdown; heartbeat at 60 s ignored; OFF at 120 s after the new ON (T:207-224).
8. Empty `luxThreshold: ""` means no threshold: zone event `{motion:true, luminosity:1}` turns the light ON (T:231-246).
9. `luxThreshold: 50` with `{motion:true, luminosity:120}` keeps the light OFF (T:248-258).
Harness defaults worth keeping: instance created with light OFF and no motion (start sync sends OFF), `getByZone` empty (no motion sources), `isDaylight: null`.
Suggested additional tests (inferred, not in T): hysteresis boundary (`lux == thr*1.1` stays on, above turns off), grace period 2000 ms, button toggle and override clear, failsafe reset by impulses, daylight gate, legacy `light` param, validate messages from section 2, stop clears the three state keys.
