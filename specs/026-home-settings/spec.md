# Spec 026: home-settings

## Context

Roadmap item J6 (milestone J, the autonomous box): "Home settings: the location (sun times, so
'inactive during day' works), the time zone; export and import of the configuration". Done when:
"the light no longer switches on in daylight".

Today the root zone's `isDaylight` is always `null` (read as night), so motion-light's
`disableWhenDaylight` ("Inactive during day") has no effect, and the tool has no idea of the
home's local time. A flashed box in an annex needs the home's location and time zone, set once from
the browser, kept in its flash, and a way to keep a copy of what it was set up with.

Decided in an autonomous run (the user away): the choices are recorded as D41 and D42 in
`docs/decisions.md`, for the user's review.

## Goals

1. The home's settings (name, location, time zone, sunrise and sunset offsets) set through Sowel's
   `GET/PUT /api/v1/settings`, kept in `settings.json` through the saver, like zones.
2. Sunrise, sunset and daylight computed in the core from those settings, as Sowel's
   `SunlightManager` (suncalc), shown on the root zone's aggregation; motion-light's "Inactive
   during day" works.
3. A Settings page in the interface: the home's settings, today's sun times, export and import.
4. Export and import of the home's settings and zones as one bounded JSON document.

## Out of scope

- Equipments, recipe instances, devices and the radio network's keys in the backup (they depend
  on the devices paired to this box; their size passes the API's 16 KB body bound): roadmap line.
- Sowel's `system.restart_required` (Solite applies a new location or time zone at once) and
  `GET /api/v1/system/timezone`.
- Finding the time zone from the location (Sowel's `tz-lookup`: its database does not fit the
  box): the time zone is picked from a list.
- A "current time" pill in the header; local time in the logs; schedules (milestone C).
- The box run (one pass, later: see "How the user will check it").

## Requirements

### R1: The home's settings

1. The settings are Sowel's flat key-value map of strings. Solite knows six keys; any other is
   refused:

   | Key | Value | Unset means |
   | --- | ----- | ----------- |
   | `home.name` | text, at most 64 characters | no name |
   | `home.latitude` | matches `^-?[0-9]{1,3}(\.[0-9]{1,12})?$`, −90 to 90 | no location |
   | `home.longitude` | matches `^-?[0-9]{1,3}(\.[0-9]{1,12})?$`, −180 to 180 | no location |
   | `home.timezone` | a name of the time zone list (R2.5), exact case, e.g. `Europe/Paris` | `UTC` |
   | `home.sunriseOffset` | matches `^-?[0-9]{1,3}$`, −180 to 180 (whole minutes) | 30 (Sowel's default) |
   | `home.sunsetOffset` | matches `^-?[0-9]{1,3}$`, −180 to 180 | 45 (Sowel's default) |

   The values are stored as given (as Sowel). No exponent, no `+`, no `.5` or `5.`, no `inf` or `nan`: what
   the pattern does not match is refused. `home.timezone` is Solite's own key (D41): Sowel takes
   its time zone from the `TZ` variable or derives it from the location.
2. `GET /api/v1/settings` answers 200 with the object of the keys that are set, values as stored
   (`{}` on a first start).
3. `PUT /api/v1/settings` takes a JSON object of keys to string values. Every entry is checked
   before anything changes: a body that is not an object is 400 `Invalid body`; an unknown key 400
   `Unknown setting: <key>`; a value that is not a string, or breaks the rule of its key, 400
   `Invalid value for <key>`. The empty string is stored as is and reads as unset (as Sowel). Keys not in the body
   are kept (Sowel's `setMany`). Answer 200 `{"success":true}`. Each accepted PUT emits Sowel's
   `settings.changed` event `{"type":"settings.changed","keys":[...]}` (the keys of the body, topic
   `system`). The settings are marked to be written only when a value actually changed, so a PUT
   that changes nothing writes nothing to the flash. After an accepted PUT the zones are
   recomputed at once (as after every change through the API), so the root zone's sun fields and
   `sunlight.changed` come in the same request.
4. The settings are kept in `settings.json` (a JSON object, at most six entries) beside
   `zones.json`, written by the saver with the other definitions: 2 s after the change (one write
   for a burst, D40), at once on stop, held by `saves hold`. On load: no file means no settings; a
   file that is not a JSON object of strings stops the host (as the other files); an unknown key
   or a value that breaks its rule is dropped with a warning in the log, and the file is written
   again. When every key is unset the file holds `{}`. The pairing mode (`--pair`, the saver's
   devices-only mode) never writes it (it reads it with the rest of the home).
5. `GET /api/v1/settings/timezones` (Solite's own route, D41) answers 200 with the list of time
   zone names the box knows, in the list's order, `UTC` first.

### R2: Sun times and daylight

1. Computed in the core, with no hardware dependency, as Sowel's `SunlightManager` with `suncalc`
   2: sunrise and sunset of the day nearest to `now` (suncalc's `getTimes`, sun at −0.833°), the
   sun's altitude (`getPosition`). The result matches suncalc within one minute.
2. The root zone's aggregation carries them (Sowel's `injectSunlightData`); every other zone keeps
   `null`:
   - no latitude or no longitude, or the clock not set (before 2020-01-01, a box that has not
     reached the time server): `sunrise`, `sunset`, `isDaylight` are `null`;
   - else `sunrise` and `sunset` are local `HH:MM` in the home's time zone, and `isDaylight` is
     true from sunrise + `home.sunriseOffset` minutes until sunset − `home.sunsetOffset` minutes
     (end excluded);
   - where the sun neither rises nor sets that day (polar day or night): `sunrise` and `sunset` are
     `"--:--"`, and `isDaylight` is true when the sun's altitude is above 0 (Sowel's rule).
3. The sun data is computed when asked with a `now` (`Home::sunlight(now)`), from a cache of one
   value keyed by the wall-clock minute and cleared by any settings change: at most one
   computation per minute (not one per batch of measures). The zone aggregation (root zone) and
   motion-light both ask it with their `now`, so a recipe reads the right value even before the
   first aggregation after a boot, and right after a PUT. When the aggregation sees a value
   different from the last one it saw, Sowel's `{"type":"sunlight.changed"}` is emitted (topic
   `system`), and the root zone's aggregate changes, so `zone.data.changed` follows as for any
   aggregate change. The periodic check (every 60 s after the last recompute) brings a daylight
   change without any measure: the flip shows on the root zone at most about 2 minutes late (the
   minute's cache, plus the check's period); motion-light reads it with its own `now`, at most one
   minute late.
4. motion-light's "Inactive during day" (`disableWhenDaylight`) reads the root zone's
   `isDaylight` (`Home::sunlight(now)`): when true, motion does not switch the lights on and the
   start sync does not switch them on; `null` and false are night, as before.
5. **Time zones**: the fixed list of `architecture.md` ("Time zone list"), each IANA name with its
   POSIX rule (`CET-1CEST,M3.5.0,M10.5.0/3`), `UTC` first. The parser takes names of letters (at
   least 3) or quoted (`<+03>`, `<-03>`), offsets `[+-]hh[:mm[:ss]]` (POSIX sign: `CET-1` is one
   hour east of UTC), an optional daylight name and offset (one hour more than standard by
   default), and the rules `Mm.w.d[/time]` (time 02:00 by default, possibly negative or above
   24 h). Daylight saving runs from the start rule (read in standard time) to the end rule (read in
   daylight time), in the northern and the southern hemisphere (start after end in the year).
   Rules of the `Jn` and `n` forms are refused. The rules are today's: a past date under another
   law gets today's rule. Zones whose rule is not a fixed yearly rule (Casablanca, Cairo...) are
   not in the list. A stored `home.timezone` that is not in the list (a later version dropped it)
   is dropped at load (R1.4), so the home is back to `UTC`.

### R3: Export and import of the home

1. `GET /api/v1/backup` answers 200 with `{"version":1,"settings":{...},"zones":[...]}` (compact
   JSON): the settings as `GET /api/v1/settings`, and every zone as Sowel's `Zone` (flat list, as
   `zones.json`). This is Solite's own document (D42): Sowel's backup is its SQLite tables. It
   holds no secret (no radio key, no Wi-Fi password). So that every export can be imported back,
   a document over 16384 bytes (`MAX_BODY_BYTES`) is not sent: 413 `Backup too large: <n> bytes,
   16384 at most (shorten the rooms' descriptions)`. Worst case: 32 zones with a 64-character
   name, a 32-character icon and a 500-character description, about 26 KB; a home without long
   descriptions is about 250 bytes per zone, 8 KB for 32 zones. The response is at most 16384
   bytes.
2. `POST /api/v1/backup` takes that document (JSON, at most 16384 bytes as every body: 413
   beyond). It is checked whole before anything changes; any failure is 400 `Invalid backup:
   <reason>` and nothing changes. The reasons:
   - `not an object`, `unknown field <name>`, `missing field <name>`, `unsupported version <v>`
     (only 1);
   - `settings`: `Unknown setting: <key>` or `Invalid value for <key>` (the rules of R1.3);
   - `zones`: a list of at most 32 objects (else `too many zones`) with Sowel's `Zone` fields only
     (`id`, `name`, `parentId`, `icon`, `description`, `displayOrder`, `createdAt`, `updatedAt`;
     the dates optional strings, read and ignored; any other field: `unknown field <name> in a
     zone`); `id` a UUID (`invalid zone id`), unique (`duplicate zone <id>`); `name`, `icon`,
     `description` and `displayOrder` under the rules and texts of `POST /api/v1/zones`
     (`<text> in zone <id>`); the root zone's `parentId` is `null` and no other zone's is
     (`only the root zone has no parent`);
   - applied to a copy of the zones, in this order: first the new zones (ids the home does not
     have), in passes: each pass creates those whose parent is already in the copy, until none is
     left; a pass that creates none ends it with `parent not found for zone <id>` (a parent in
     neither the document nor the home, or new zones that are each other's parent); then the zones
     the home has, updated in the document's order with the checks of `PUT /api/v1/zones/:id`
     (`<text> in zone <id>`, e.g. a zone moved under itself or a descendant); the home may not
     pass 32 zones (`too many zones`).
3. What the import replaces: **the settings, whole** (a key absent from the document is unset);
   **the zones of the document**: created with their ids, or updated when a field differs (name,
   parent, icon, description, display order). `updatedAt` of an updated zone, and `createdAt` and
   `updatedAt` of a created one, are the time of the import; a zone whose fields are all the same
   is left as it is (not updated, no event, no write). Zones of the home absent from the document
   are kept; equipments, devices and recipe instances are not touched. A created or updated zone
   has the side effects of `POST` and `PUT /api/v1/zones` (its event, the zones written, the
   recompute at the end of the request).
4. Answer 200 `{"success":true,"settings":<keys set after the import>,"zonesCreated":<n>,
   "zonesUpdated":<n>,"zonesUnchanged":<n>}`. Events: `settings.changed` with the keys whose value
   changed (none changed: no event), `zone.created` and `zone.updated` per zone. An import that
   changes nothing writes nothing.

### R4: The interface

1. **Settings** (`#/settings`) joins the Administration section of the sidebar and the drawer,
   after Discovery.
2. **Home** card (ported from Sowel's `HomeSettingsSection`, Sowel's texts): name, latitude,
   longitude (`,` typed is turned into `.`), sunrise and sunset offsets, and a time zone select
   filled from `GET /api/v1/settings/timezones`. Sowel shows its time zone read-only with its
   source; Solite makes it a select (D41); unset, it shows `UTC`. No restart toast (Sowel's asks
   for a restart after a new location; Solite applies it at once). Save appears when something
   changed and sends the six keys with `PUT /api/v1/settings`; a refusal shows the host's text.
   Under it, "Today": sunrise and sunset (`--:--` shown as is) and "Day" or "Night", from the root
   zone's aggregation (live); when they are `null`: "Set the location to get the sun times".
3. **Backup** card: "Export" downloads `solite-backup-<YYYY-MM-DD>.json` from
   `GET /api/v1/backup` (a refusal shows its text); "Import" picks a `.json` file; a file over
   16384 bytes is refused in the page with "File too large (16 KB at most)" and nothing is sent;
   otherwise a confirmation says what it replaces (R3.3), then it is sent; the result says
   "Imported: <n> settings, <c> rooms created, <u> rooms updated", or the host's refusal text.
   After an import the zones and the settings are read again.
4. French when the browser is in French, English otherwise; Sowel's tokens and classes only; the
   first load stays within the 170 KB gzip budget (estimate: about 2 KB gzip more than the 57.5 KB
   of spec 024).

### R5: Bounds

| Collection | Bound | Where |
| ---------- | ----- | ----- |
| Settings | 6 keys; a name at most 64 characters, numbers by their pattern (17 characters at most) | next to the settings type |
| Time zone list | fixed, compiled in (the list of `architecture.md`) | next to the list |
| Sun cache | one value | in `Home` |
| Backup zones | 32 | next to the import |
| Backup document, both ways | 16384 bytes (`MAX_BODY_BYTES`) | the API's existing bound; the export refuses beyond |

## Living spec delta

### docs/living-spec/README.md

- New section **"Keeps the home's settings"** (after "Organises the home in zones and
  equipments"): the six keys and their rules, the routes `GET/PUT /api/v1/settings` and
  `GET /api/v1/settings/timezones`, `settings.json` (written within 2 s), the sun times on the root
  zone (rules of R2.2, polar days, clock not set), `settings.changed` and `sunlight.changed`, and
  the export and import (R3: what it replaces, the 16 KB bound both ways, the refusals).
- Changed, "Runs the motion-light recipe" (line 428-430): "'Inactive during day' has no effect:
  the tool does not know the sun's times (the root zone's `isDaylight` is `null`, night)" →
  "'Inactive during day' keeps the lights off while the root zone's `isDaylight` is true (sunrise
  + offset to sunset − offset, from the home's settings); with no location it is `null`, night."
- Changed, "Organises the home in zones and equipments", zone aggregation (line 406-411): adds "the
  root zone also carries `sunrise`, `sunset` and `isDaylight` (see 'Keeps the home's settings')".
- Changed, same section, lines 412-415: "Zones and equipments are kept in `zones.json` and
  `equipments.json`... A `zones.json` or `equipments.json` that cannot be read stops the host" →
  adds `settings.json` (the settings, see their section) to both sentences.
- Changed, "Shows the home in the browser": the Administration list (line 477-478) adds Settings;
  a **Settings** bullet (R4.2, R4.3); line 470 ("pairs the devices... sets the recipes") adds "and
  sets the home's location and time zone".
- History: row 026.

### No longer true

- `docs/living-spec/README.md`: the sentences above (rewritten).
- `crates/core/src/recipes/motion_light/machine.rs:155`: the comment "Solite computes no sunrise
  or sunset" (rewritten with the code).
- `docs/decisions.md:249` ("`isDaylight` is always `null`"): history, kept; D41 states the new
  rule.
- `docs/architecture.md:40` and `:90-91` (the files kept) gain `settings.json`; `:47` (the
  interface's pages) gains Settings; the core's blocks gain `settings/`.
- `docs/design-system.md:51` (the Administration list: Devices, Equipments, Topology, Discovery)
  gains Settings; the budget figure is updated.
- `crates/host/src/store.rs:1-3` (the files of the data folder) gains `settings.json`.
- Searched: "isDaylight", "daylight", "sunrise", "time zone", "timezone", "zones.json",
  "equipments.json", "Administration", "Topology, Discovery", "J6" in `docs/` (roadmap and
  decisions excepted), `README.md`, `CLAUDE.md`, `.claude/`.

## Acceptance criteria

- [x] `PUT /api/v1/settings` with `home.latitude` `48.8566`, `home.longitude` `2.3522`,
      `home.timezone` `Europe/Paris`, then `GET /api/v1/zones/aggregation` with the core's clock at
      2026-06-21T12:00:00Z (tests give `now`): the root zone shows `sunrise` `05:46`, `sunset`
      `21:57`, `isDaylight` true.
- [x] motion-light with "Inactive during day", same settings: motion at 12:00Z (14:00 local)
      does not switch the light on; motion at 21:30Z (23:30 local) does.
- [x] Without a location, the root zone's sun fields are `null` and motion switches the light on.
- [x] An invalid setting, an unknown key, a backup with a bad version, a duplicate zone id, a
      missing parent or a 33rd zone is refused with nothing changed.
- [x] Export then import on a fresh home gives the same settings and zones; a second import of
      the same document changes and writes nothing.
- [x] `settings.json` is written once for a burst of setting changes, and not for a PUT that
      changes nothing.
- [x] The Settings page shows the home's settings, today's sun times, exports and imports; budget
      green.
- [x] `sh scripts/validate.sh` green (four firmware images, QEMU).

## Edge cases

- Latitude without longitude: no location (`null`). A value in `settings.json` that breaks its
  rule is dropped at load (R1.4), so no location when it is a coordinate.
- Clock not set (box before the time server): `null`, never 1970's sun; motion-light reads night.
- Polar day and night (Tromsø, the poles): `"--:--"` and the altitude rule.
- Southern hemisphere time zone (Sydney: daylight saving from October to April).
- The instants of a daylight-saving change.
- A PUT or an import that changes nothing: no write.
- A backup with a zone whose parent comes after it in the list: accepted.
- A backup of more than 16384 bytes: refused at export (413), at import (413), and in the page
  before sending.
- `"NaN"`, `"inf"`, `"1e1"`, `"+5"`, `".5"`, `"5."` as a coordinate: refused (the pattern).

## Sowel contract

- `GET/PUT /api/v1/settings`: `src/api/routes/settings.ts` (flat `Record<string,string>`,
  `{ success: true }`); keys `home.name`, `home.latitude`, `home.longitude`, `home.sunriseOffset`,
  `home.sunsetOffset` and their defaults: `src/zones/sunlight-manager.ts`,
  `ui/src/pages/SettingsPage.tsx`.
- `ZoneAggregatedData.sunrise`, `.sunset`, `.isDaylight` (`src/shared/types.ts`, the zone
  aggregation interface), on the root zone only: `src/zones/zone-aggregator.ts`
  (`injectSunlightData`); the rules of `sunlight-manager.ts` (offsets, `"--:--"`, altitude).
- Events `settings.changed { keys }` and `sunlight.changed` (`src/shared/types.ts`,
  `EngineEvent`).
- `Zone` (`src/shared/types.ts`) in the backup.
- Divergences (decisions): `home.timezone` and `GET /api/v1/settings/timezones` (D41); unknown keys
  refused; strict number patterns (Sowel's `parseFloat` and `parseInt` take more); the empty
  string is stored and read as unset, as Sowel; `settings.changed` from an import
  carries only the changed keys; no `system.restart_required`; `GET/POST /api/v1/backup` carry
  Solite's JSON document, not Sowel's SQLite tables and ZIP, and the import takes a JSON body, not
  a file upload (D42); Sowel's settings and backup routes are for administrators, Solite's are
  open to the local network like every route of its API until J4's password (D13).

## How the user will check it

On the Mac (the user away, this run): `cargo run -p solite-host -- --sim scenarios/garage.json
--serve --speed 10`, then `curl` the settings with Paris's location; `GET
/api/v1/zones/aggregation` shows the root zone's sun times of today (the host runs on the real
clock; the June values are the tests'); the Settings page in the browser (`cd ui && npm run dev`),
the export and import of the garage's zones; the motion-light tests show the light kept off by
day.

**Checked on the box later** (one pass, the user present, after their explicit yes to flash
(D6), D39: at most one flash write per step): with the radio image flashed `--keep-storage`,
inside `box.sh saves --budget 2`: one `PUT /api/v1/settings` with the annex's location and time
zone, one `POST /api/v1/backup` of a document exported from the Mac; then
`GET /api/v1/zones/aggregation` shows the right local sun times; the flush writes `settings.json`
and `zones.json` (2 writes; the sun data is never written). Then by day, motion does not switch
the ZBMINIR2 on with "Inactive during day" ticked. Roadmap line added.

**Run on the Mac, 2026-11-24 22:21 local** (simulated garage, `--serve 8091`): `PUT /api/v1/settings` with Paris and `Europe/Paris` answered `{"success":true}`; the root zone showed `sunrise` `08:12`, `sunset` `17:01`, `isDaylight` false (night); `GET /api/v1/backup` gave the settings and the root zone; a backup of version 2 was refused with `Invalid backup: unsupported version 2`. The Settings page was not opened in a browser in this run (the user away): its Vitest tests cover it.
