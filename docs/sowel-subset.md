# Sowel subset

What Solite has of Sowel's model and API, and why; and the names it has that Sowel does not, each
kept by a decision. Solite is a subset of Sowel: what it has is Sowel's (names, values,
semantics, JSON; Sowel's `src/shared/types.ts` wins), and what is outside the subset is refused
or kept untouched (decision D43).

`crates/core/tests/sowel_contract.rs` reads this file: the first column of each table below must
list exactly what the code has, and every name of Solite outside Sowel must be in
**Deviations**. It compares Solite with `contract/sowel-snapshot.json` (taken from Sowel's
repository) and, when it is present, with Sowel's repository itself (`SOWEL_REPO`, else
`../sowel`). The JSON fields of devices, zones, the zone aggregation, equipments, bindings and
recipes are checked against Sowel's interfaces from the API's real answers; they need no table.
Every one of Sowel's 31 equipment types is accepted, stored and served; Sowel's optional
equipment fields are kept.

To rewrite the snapshot after Sowel moved: `SOWEL_SNAPSHOT=write cargo test -p solite-core
--test sowel_contract` with Sowel's repository beside this one.

## Data categories

The categories the device catalog gives a device's data (Sowel's `DataCategory`).

| Category | Why |
| -------- | --- |
| `temperature` | Temperature cluster (0x0402) |
| `humidity` | Humidity cluster (0x0405) |
| `pressure` | Pressure cluster (0x0403) |
| `luminosity` | Illuminance cluster (0x0400) |
| `motion` | Occupancy cluster (0x0406) and IAS motion zones |
| `contact_door` | IAS contact zones (Sowel's Zigbee2MQTT plugin gives `contact` this category) |
| `water_leak` | IAS water leak zones |
| `smoke` | IAS fire zones |
| `light_state` | On/off cluster (0x0006) |
| `light_brightness` | Level cluster (0x0008) |
| `light_color_temp` | Color Control cluster (0x0300), colour temperature in mireds |
| `light_color` | Color Control cluster (0x0300), the `color` object (type `json`) |
| `battery` | Power configuration cluster (0x0001), percentage |
| `voltage` | Power configuration cluster (0x0001), battery voltage |
| `generic` | IAS zone flags (`tamper`, `battery_low`) |

## Order categories

The categories of a device's orders (Sowel's `OrderCategory`).

| Category | Why |
| -------- | --- |
| `light_toggle` | On/off cluster: on, off, toggle |
| `set_brightness` | Level cluster: move to level |
| `set_color_temp` | Color Control cluster: move to color temperature |
| `set_color` | Color Control cluster: move to color, move to hue and saturation |

## Events

The events Solite sends on `/ws` (Sowel's `EngineEvent`); the others are not sent (D18).

| Event | Why |
| ----- | --- |
| `device.discovered` | A device joined |
| `device.removed` | A device deleted |
| `device.status_changed` | Online, offline |
| `device.data.updated` | A measure |
| `zone.created` | Zone routes |
| `zone.updated` | Zone routes |
| `zone.removed` | Zone routes |
| `zone.data.changed` | Zone aggregation |
| `equipment.created` | Equipment routes |
| `equipment.updated` | Equipment routes, bindings |
| `equipment.removed` | Equipment routes |
| `equipment.data.changed` | A bound measure |
| `equipment.status.changed` | Equipment status |
| `equipment.order.executed` | An order answered |
| `equipment.order.failed` | An order failed |
| `recipe.instance.created` | Recipe routes |
| `recipe.instance.removed` | Recipe routes |
| `recipe.instance.started` | Recipe routes |
| `recipe.instance.stopped` | Recipe routes |
| `recipe.instance.state.changed` | motion-light's state |
| `recipe.instance.error` | A recipe that cannot run |
| `settings.changed` | Settings routes |
| `sunlight.changed` | Sun times on the root zone |

## Settings keys

| Key | Why |
| --- | --- |
| `home.name` | The home's name (Sowel's interface writes it) |
| `home.latitude` | Sun times |
| `home.longitude` | Sun times |
| `home.timezone` | Local time (Sowel's global key; D41, D44) |
| `home.sunriseOffset` | motion-light's "Inactive during day" |
| `home.sunsetOffset` | motion-light's "Inactive during day" |
| `update.manifestUrl` | Where the box reads the latest release's manifest (Solite's own, D67) |
| `update.autoCheck` | The automatic check for a release, off by default (Solite's own, D67) |

## Routes

The routes of `api::ROUTES` (the host and the box also serve `GET /ws`, Sowel's, and the
interface's files).

| Route | Why |
| ----- | --- |
| `GET /api/v1/devices` | Devices |
| `GET /api/v1/devices/:id` | Devices |
| `PUT /api/v1/devices/:id` | Devices |
| `DELETE /api/v1/devices/:id` | Devices |
| `GET /api/v1/zones` | Zones |
| `POST /api/v1/zones` | Zones |
| `GET /api/v1/zones/aggregation` | Zone aggregation |
| `GET /api/v1/zones/:id` | Zones |
| `PUT /api/v1/zones/:id` | Zones |
| `DELETE /api/v1/zones/:id` | Zones |
| `GET /api/v1/equipments` | Equipments |
| `POST /api/v1/equipments` | Equipments |
| `GET /api/v1/equipments/:id` | Equipments |
| `PUT /api/v1/equipments/:id` | Equipments |
| `DELETE /api/v1/equipments/:id` | Equipments |
| `POST /api/v1/equipments/:id/data-bindings` | Bindings |
| `DELETE /api/v1/equipments/:id/data-bindings/:bindingId` | Bindings |
| `POST /api/v1/equipments/:id/order-bindings` | Bindings |
| `DELETE /api/v1/equipments/:id/order-bindings/:bindingId` | Bindings |
| `POST /api/v1/equipments/:id/orders/:alias` | Orders |
| `GET /api/v1/recipes` | Recipes |
| `GET /api/v1/recipes/:id` | Recipes |
| `GET /api/v1/recipe-instances` | Recipe instances |
| `POST /api/v1/recipe-instances` | Recipe instances |
| `PUT /api/v1/recipe-instances/:id` | Recipe instances |
| `DELETE /api/v1/recipe-instances/:id` | Recipe instances |
| `POST /api/v1/recipe-instances/:id/enable` | Recipe instances |
| `POST /api/v1/recipe-instances/:id/disable` | Recipe instances |
| `POST /api/v1/recipe-instances/:id/actions` | motion-light's actions |
| `GET /api/v1/recipe-instances/:id/log` | Recipe journal |
| `GET /api/v1/settings` | Settings |
| `PUT /api/v1/settings` | Settings |
| `GET /api/v1/settings/timezones` | The time zones the box knows (deviation, D41) |
| `GET /api/v1/system/timezone` | The home's time zone |
| `GET /api/v1/integrations/solite/permit-join` | Pairing window (deviation, D28; behind the token, D49) |
| `POST /api/v1/integrations/solite/permit-join` | Pairing window (deviation, D28; behind the token, D49) |
| `GET /api/v1/auth/status` | First start or login (Sowel's, D47) |
| `POST /api/v1/auth/setup` | The one account, confirmed at the box (Sowel's, D47, D48) |
| `POST /api/v1/auth/login` | Tokens (Sowel's, D47) |
| `POST /api/v1/auth/refresh` | New tokens (Sowel's, D47) |
| `POST /api/v1/auth/logout` | Closes the session (Sowel's, D47) |
| `POST /api/v1/system/factory-reset` | Erases the home and the account (deviation, D48) |
| `GET /api/v1/system/backup` | The full backup archive (deviation, D56) |
| `POST /api/v1/system/backup` | Restores the full backup archive (deviation, D56) |
| `GET /api/v1/system/version` | The running version and the last check's latest release (Sowel's `VersionInfo`, a subset, D53, D65) |
| `POST /api/v1/system/version/check` | Checks for a release now (Sowel's route, D65) |
| `POST /api/v1/system/update` | Installs the signed update file in its body, or, with no body, pulls the latest release (Sowel's route takes no body, D53, D65) |
| `POST /api/v1/system/restart` | Restarts the box (Sowel's route, Solite's answer, D79) |
| `GET /api/v1/system/supervisor` | The supervisor's status: mode, boot cause, last incident, restarts (deviation, D58) |

## Equipment types with behaviour

The types with Sowel's binding rules (`equipments::rules`) and proposed by recognition; the other
23 are accepted, stored and served, without rules.

| Type | Why |
| ---- | --- |
| `light_onoff` | Relays and on/off lights (ZBMINIR2) |
| `light_dimmable` | Dimmers |
| `light_color` | Colour bulbs, the most common Zigbee lights (Philips, IKEA, Innr, Ledvance) |
| `shutter` | Roller shutters |
| `switch` | Plugs |
| `sensor` | Temperature, motion, contact, leak, smoke sensors |
| `button` | Remotes |
| `thermostat` | Thermostats |

## Deviations

Names Solite has and Sowel does not, each kept by a decision. Differences of value (bounds,
error texts, fields refused) are listed in the decisions themselves (D13, D14, D16 to D18, D41 to
D44, D47 to D49, D53, D56, D58, D65 to D67, D79).

| Kind | Name | Decision |
| ---- | ---- | -------- |
| route | `GET /api/v1/settings/timezones` | D41 |
| route | `GET /api/v1/integrations/solite/permit-join` | D28 |
| route | `POST /api/v1/integrations/solite/permit-join` | D28 |
| route | `POST /api/v1/system/factory-reset` | D48 |
| route | `GET /api/v1/system/backup` | D56 |
| route | `POST /api/v1/system/backup` | D56 |
| route | `GET /api/v1/system/supervisor` | D58 |
| setting | `update.manifestUrl` | D67 |
| setting | `update.autoCheck` | D67 |

Differences of value in the authentication (D47 to D49), none a new name: one account, always
`admin` (no other user, no `/me`, no `/users`, no API token, no MFA: `trustedDeviceToken` is
accepted and ignored); `preferences` has `language` only; opaque tokens (32 hex characters), not
JWTs, and logout closes the access token too; passwords of 8 characters or more (Sowel: 6); setup
answers 403 `{"error", "confirmRequired": true}` until a person at the box confirmed (D48); one
rate limit for the box (10 attempts per minute on setup and login together); `/ws` reads the token
from the first `bearer.` subprotocol other than `bearer.none`, and the box echoes `bearer.none`
only, so a client that offers `bearer.<token>` alone gets no subprotocol echoed there (D49).

Differences of value in the updates (D53, D65), none a new name: `POST /api/v1/system/update` takes
the signed update file as its body (`application/octet-stream`) **or** an empty body, which pulls
the latest release; Sowel's route takes no body. It answers Sowel's `{success, message}`, its
`{error}` shape, its 409 `Update already in progress` and its 400 `No update available`, when the
update is installed (Sowel answers at once and reports its progress by events, which Solite does
not have). `POST /api/v1/system/version/check` answers Sowel's `VersionInfo`, its 429 (60 s between
two checks, Sowel 10 s, no `Retry-After` header) and its 500 `Update check failed: ...`;
`GET /api/v1/system/version` answers `current`, `latest`, `updateAvailable` and `releaseUrl` from
the last check (`null` and `false` before one), without Sowel's two Docker fields
(`dockerAvailable`, `composeManaged`); `updateAvailable` is false when the release's `minVersion` is
above the running version (Sowel has no such rule). The settings `update.manifestUrl` and
`update.autoCheck` are Solite's own (D67).

Differences of value in the restart (D79), none a new name or shape: `POST /api/v1/system/restart` is Sowel's
route (it restarts Sowel's container through docker, answers `{success, message}` and refuses with 400, 403, 409 and 503
of its own); Solite's restarts the box, answers the same shape, 200 `{"success": true, "message": "Restarting"}`, and refuses with 409 while an update, an
update check, a backup or a probation is on (401 without a session, as every route).

Differences of value in the safe mode (D58), none a new name: while the supervisor holds the engine
stopped, every request that is not a `GET` answers 503 `{"error": "Safe mode: the engine is
stopped"}`, except the account's routes (`/auth/*`), `POST /api/v1/system/update` and `POST
/api/v1/system/version/check` and `POST /api/v1/system/restart` (D79). Sowel has no
such mode.

## Script recipes

Script recipes (spec 041, D73 to D76) follow Sowel's script-recipe draft (`roadmap/ai-script-recipes/spec.md`
in Sowel's repository: "The idea", "Script API (first cut)", "Rules enforced by the host"), which has no
code in Sowel yet. They add no route, field or event: a script recipe is served through Sowel's
recipe routes, with Sowel's `RecipeInfo`, `RecipeSlotDef`, `RecipeInstance` and `RecipeLogEntry`.
What differs from the draft is the script API's own (this section is prose, not a table the contract
test reads), each kept by D75 or D74:

- **Added to the draft**: the script's `id` (1 to 64 characters among `a-z`, `0-9`, `-`; the draft
  has none) and an optional `version` (shown in the start line only); `onMotionReport(fn)`, called
  with the value and the equipment's id on every motion report of a motion sensor of the zone, so
  that a safety auto-off can be reset by fresh motion; `cancel(id)` for the timers;
  `console.log` as `log`; `validate(params, home)` with a second argument, `home.equipment(id)` giving
  the `name`, `orders` and `data` of an equipment named in the params (the draft's `validate` has
  `params` only, and no way to check bindings). A Sowel issue proposes `id`, the zone slot rule,
  `onMotionReport` and `validate(params, home)` for the draft.
- **Left out**: `mode()` (no modes), the tariff period in `now()`, `energy.claim/release`, history
  reads, the observation mode, a per-instance order rate limit, `state.alarm` semantics; a script
  that calls `mode`, `energy` or `history` gets `Not available in Solite: <name>`. Slots of type
  `text` and `data-key` are refused.
- **Functions have no source**: QuickJS is built without keeping the source text of functions
  (`JS_NO_FUNCTION_SOURCE`, D74): `Function.prototype.toString` of a script's function answers
  `function f() { [native code] }`, where Sowel's engine gives the source.
- **Synchronous host calls**: `get`, `order` and the others answer at once (a superset of the draft,
  where they resolve through promises); a promise's `then` still runs after the call.
- **One runtime per recipe**: the instances of a script recipe share one QuickJS runtime and context
  (the draft: one per instance), so a script's top-level variables are shared and what belongs to
  one instance lives inside `start`.
- **An overrun fails the recipe**: a call over its time, memory or stack stops every instance of the
  recipe, which comes back after a minute (the draft stops the instance with `state.alarm`).
- **Validation is the host's**: the texts are Solite's own, `Invalid params: <text>` (Sowel has no
  generic slot validation: each of its recipes checks itself), then the script's `validate`.
- **`onZone(alias, fn)`** is called on every `zone.data.changed` of the instance's zone with the
  aggregate's current value, for `motion`, `luminosity` and `motionSensors`; **`get` and `order`**
  take a slot name only (never an id), read the data of category `light_state` first for the alias
  `state`, and send through a `light_toggle` or `toggle_power` binding first, as Sowel's helpers do.
