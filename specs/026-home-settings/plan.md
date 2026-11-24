# Plan: Spec 026

## Steps

- [x] 1. Living spec updated (the delta, History row 026); D41 and D42 in `docs/decisions.md`;
      roadmap (J6 status, box-check line, backup line); `docs/specs-index.md` row 026.
- [x] 2. Core types: `Settings`, `Sunlight`, `TimeZone`, `Backup`, the two events (Sowel's names).
- [x] 3. Core logic, tests first: settings rules, suncalc port, POSIX rules, the sun cache and root
      injection, motion-light's daylight, the backup's check and import, the saver's file.
- [x] 4. Adapters: none (host and firmware unchanged; the files go through `Storage` by name).
- [x] 5. API: `GET/PUT /settings`, `GET /settings/timezones`, `GET/POST /backup`.
- [x] 6. Interface: Settings page, nav entry, API client, i18n; screenshot.
- [x] 7. Docs: `docs/architecture.md`, `docs/design-system.md` (budget figure).
- [x] 8. `sh scripts/validate.sh` green (four images, QEMU).
- [x] 9. Code review (`code-reviewer`); demo on the Mac (the user away).
- [ ] 10. The box: later, one pass (roadmap line).

## Test plan

Reference values from suncalc 2.0 (Sowel's `node_modules/suncalc`), UTC:

| Case | now (UTC) | lat, lon | sunrise | sunset | altitude |
| ---- | --------- | -------- | ------- | ------ | -------- |
| Paris summer | 2026-06-21T12:00Z | 48.8566, 2.3522 | 03:46:57 | 19:57:54 | > 0 |
| Paris winter | 2026-12-21T12:00Z | same | 07:41:15 | 15:56:04 | > 0 |
| Equator equinox | 2026-03-20T12:00Z | 0, 0 | 06:04:12 | 18:10:43 | > 0 |
| Sydney summer | 2026-12-21T02:00Z | −33.8688, 151.2093 | 2026-12-20T18:40:39 | 09:05:25 | > 0 |
| New York summer | 2026-07-01T17:00Z | 40.7128, −74.006 | 09:28:40 | 2026-07-02T00:31:06 | > 0 |
| Tromsø midsummer | 2026-06-21T12:00Z | 69.6492, 18.9553 | none | none | 42.5° |
| Tromsø midwinter | 2026-12-21T12:00Z | same | none | none | −3.6° |
| North pole, June | 2026-06-21T12:00Z | 90, 0 | none | none | 23.5° |
| South pole, June | 2026-06-21T12:00Z | −90, 0 | none | none | −23.0° |
| Paris, 01:00Z and 23:30Z | 2026-06-21 | Paris | as Paris summer | as Paris summer | < 0 |

### Rust, core: sun (`crates/core/tests/sun.rs`, new)

| Module | Scenario | Expected |
| ------ | -------- | -------- |
| sun | Each row of the table with times | sunrise and sunset within 60 s of suncalc |
| sun | Tromsø midsummer, North pole June | no times; altitude > 0 |
| sun | Tromsø midwinter, South pole June | no times; altitude < 0 |
| sunlight | Paris summer, `Europe/Paris`, default offsets, at 12:00Z | `05:46`, `21:57`, daylight true |
| sunlight | Paris summer at 01:00Z and 23:30Z | daylight false |
| sunlight | Offsets: at sunrise + 29 min (default 30) | false; at + 30 min: true |
| sunlight | Offsets: at sunset − 45 min (default 45) | false (end excluded); − 46 min: true |
| sunlight | Custom offsets 0 and 0, at sunrise exactly | true |
| sunlight | Negative offset −10 at sunrise − 5 min | true |
| sunlight | Tromsø midsummer | `"--:--"`, `"--:--"`, true; midwinter: `"--:--"`, false |
| sunlight | No longitude (latitude only) | all `None` |
| sunlight | Clock not set (now = 1970 + 1 h) with a location | all `None` |
| sunlight | Sydney summer, `Australia/Sydney` | local times in AEDT (`05:40`, `20:05`) |
| sunlight | No time zone set | `HH:MM` in UTC (`03:46`, `19:57` for Paris) |

### Rust, core: time zones (`crates/core/tests/sun.rs` or a `tz` section)

| Module | Scenario | Expected |
| ------ | -------- | -------- |
| tz | Every entry of `TIME_ZONES` parses | no error; names unique; `UTC` first |
| tz | `Europe/Paris` on 2026-01-15 and 2026-07-15 | +3600, +7200 |
| tz | `Europe/Paris` around 2026-03-29 01:00Z (switch at 01:00 UTC) | 00:59:59Z: +3600; 01:00:00Z: +7200 |
| tz | `Europe/Paris` around 2026-10-25 01:00Z | 00:59:59Z: +7200; 01:00:00Z: +3600 |
| tz | `America/New_York` 2026-03-08 07:00Z and 2026-11-01 06:00Z | switches at those instants (−18000 ↔ −14400) |
| tz | `Australia/Sydney` January and July (southern) | +39600, +36000 |
| tz | `Asia/Kolkata` (`IST-5:30`) | +19800 |
| tz | `UTC`, a zone without daylight saving (`America/Phoenix`) | 0, −25200 all year |
| tz | Rules refused: `J60`, `60`, empty, `CET-1CEST,M13.1.0,M10.5.0`, garbage | error |

### Rust, core: settings (`crates/core/tests/settings.rs`, new)

| Module | Scenario | Expected |
| ------ | -------- | -------- |
| api | GET on a first start | 200 `{}` |
| api | PUT the six keys valid, then GET | 200 `{"success":true}`; GET gives them as given |
| api | PUT keeps the keys not given | earlier keys still there |
| api | PUT `""` for a key | stored as `""` (as Sowel), read as unset |
| api | PUT unknown key `home.foo` | 400 `Unknown setting: home.foo`, nothing changed (other valid keys in the same body not applied) |
| api | Latitude `91`, `-90.5`, `abc`, `NaN`, `inf`, `1e3`, 33 characters | 400 `Invalid value for home.latitude` each |
| api | Longitude `180` accepted, `180.1` refused | as said |
| api | Offsets `30`, `-180` accepted; `181`, `1.5`, `x` refused | as said |
| api | Time zone `Europe/Paris` accepted; `Mars/Olympus`, `europe/paris` refused | as said |
| api | Name of 64 characters accepted, 65 refused (after trim) | as said |
| api | A number value (`{"home.latitude": 48}`), an array body, invalid JSON | 400 (`Invalid value for home.latitude`, `Invalid body`) |
| api | Non-JSON content type | 415 (unchanged rule) |
| api | `GET /settings/timezones` | 200, a list starting with `"UTC"`, containing `"Europe/Paris"` |
| api | Wrong methods on the three routes | 405 |
| events | PUT with a live client on `system` | `settings.changed` with the body's keys |
| events | PUT location: the next aggregate | `sunlight.changed` once; the root zone's `zone.data.changed` |
| store | PUT then the saver after 2 s | `settings.json` written once for three PUTs in a burst; its text is the object |
| store | PUT with the same values | no write |
| store | `load_home` from a `settings.json` | the settings read back |
| store | `settings.json` with an unknown key and a latitude of `200` | both dropped, warning, file marked to be written; other keys kept |
| store | `settings.json` not an object (`[]`, invalid JSON) | `load_home` fails with `settings: <where>: cannot read: ...` |
| store | No `settings.json` | no settings, nothing written |
| aggregate | Location set, clock 2026-06-21 12:00Z | root zone `05:46`, `21:57`, true; another zone `null`s |
| aggregate | No location | root zone `null`s (unchanged behaviour) |
| aggregate | Two aggregates within the same minute | the sun computed once (a counter in the cache, or same instant reused) |
| aggregate | `tick` 60 s later crossing sunset − offset with no measure | `isDaylight` false, `zone.data.changed` |

### Rust, core: motion-light (`crates/core/tests/motion_light.rs`)

| Module | Scenario | Expected |
| ------ | -------- | -------- |
| recipe | `disableWhenDaylight` true, location set, motion at 12:00Z (day) | no order to the light |
| recipe | Same at 21:30Z (23:30 local, night) | light switched on |
| recipe | `disableWhenDaylight` false, by day | light switched on |
| recipe | Existing test `disable_when_daylight_has_no_effect_because_the_daylight_is_unknown` (no location) | unchanged, passes |
| recipe | Start sync by day with motion, `disableWhenDaylight` true | the light not switched on |

### Rust, core: backup (`crates/core/tests/backup.rs`, new)

| Module | Scenario | Expected |
| ------ | -------- | -------- |
| export | GET on a home with settings and three zones | `version` 1, the settings, the zones (Sowel's `Zone` JSON) |
| import | Export of home A imported into a fresh home B | B's GET /settings and the zones' ids, names, parents, orders equal A's; 200 with `settings` 6, `zonesCreated` 3, `zonesUpdated` 1 (the root) |
| import | Same document imported twice | second: `zonesCreated` 0, all updated; no zone duplicated |
| import | Settings replaced whole | a key set in B before and absent from the document is unset |
| import | Zones of B absent from the document | kept |
| import | A child before its parent in the list | accepted |
| import | Events | `settings.changed`, `zone.created`, `zone.updated` |
| import | Saved | after the delay, `settings.json` and `zones.json` written once each |
| refuse | `version` 2, missing `zones`, extra field `devices` | 400 `Invalid backup: ...`; nothing changed (settings and zones as before) |
| refuse | A bad setting in the document | 400, nothing changed, zones untouched |
| refuse | Duplicate zone id; id not a UUID; empty name; name of 65 characters; unknown zone field | 400, nothing changed |
| refuse | A parent in neither the document nor the home | 400, nothing changed |
| refuse | A cycle (A under B, B under A) | 400, nothing changed |
| refuse | Root zone with a parent; a non-root zone with `parentId` null | 400 |
| refuse | 33 zones in the document; 20 in the home + 13 new in the document | 400 (bound), nothing changed |
| refuse | A body over 16 KB | 413 (existing rule) |
| refuse | Not JSON content type | 415 |
| api | Wrong methods | 405 |

### Added after the spec review

| Module | Scenario | Expected |
| ------ | -------- | -------- |
| settings | Patterns, table-driven: `48.8566`, `-0`, `0`, `90`, `-90` accepted; `1e1`, `+5`, `.5`, `5.`, `030.`, `0x10`, `٤٥` (unicode digits), 1234 refused; offsets `030` accepted (=30), `30.0` refused | as said |
| store | `settings.json` `{"home.latitude":48}` (a non-string) | `load_home` fails (`settings: ... cannot read`) |
| store | Every key unset by PUTs | `settings.json` written as `{}` |
| store | `settings.json` with `home.timezone` `Mars/Olympus` | dropped at load, warning, file rewritten; offset UTC |
| store | `--pair` saver (devices only) with settings changed | `settings.json` not written |
| sun cache | Boot by day with a location, start sync of a motion-light with motion, no aggregate yet | lights not switched on (the cache is cold, `sunlight(now)` computes) |
| sun cache | PUT location then motion in the same minute | motion-light reads day at once |
| recipe | Clock not set (1970), location set, `disableWhenDaylight` | light switched on (null = night) |
| tz | `Australia/Sydney` switch instants: 2026-04-04T16:00:00Z (end, 03:00 AEDT) and 2026-10-03T16:00:00Z (start, 02:00 AEST) | offsets change exactly there |
| tz | Quoted names: `<+03>-3`, `<-03>3<-02>,M3.2.0,M11.1.0` | parse; +10800; −10800/−7200 |
| backup | Valid settings with an invalid zone | 400, settings unchanged |
| backup | `zones: []` | accepted: settings replaced, zones untouched |
| backup | A document without the root zone | accepted (root kept) |
| backup | Two zones with `parentId` null | 400 `only the root zone has no parent` |
| backup | `displayOrder` a string; `createdAt` a number | 400 |
| backup | Two new zones each other's parent | 400 `parent not found for zone <id>` |
| backup | An existing zone moved under its own child | 400, nothing changed |
| backup | Re-import of the same document | `zonesUnchanged` = all, no event, no write |
| backup | Export of a home of 32 zones with 500-character descriptions | 413 `Backup too large: ...` |
| backup | A body of exactly 16384 bytes | not refused for its size |
| ui | Time zone select: change and save | PUT holds `home.timezone`; unset shows `UTC` |
| ui | Root aggregation with `--:--` | shown as is |
| ui | French browser | French texts |
| ui | A file of exactly 16384 bytes | sent; 16385 refused |

### Rust, core: saver and Sowel JSON

| Module | Scenario | Expected |
| ------ | -------- | -------- |
| saves | Held, a settings change, flush | `settings.json` among the files written; `flushed_line` counts it |
| saves | Existing tests of `saves.rs` and `engine.rs` | unchanged, pass |
| sowel_json | `settings.changed` and `sunlight.changed` serialised | equal to JSON written from Sowel's `EngineEvent` |
| sowel_json | Root aggregate with the sun fields | `"sunrise":"05:46","sunset":"21:57","isDaylight":true` |
| memory | Worst case: six settings at their longest | `settings.json` and the backup's size printed; under 16 KB |

### UI (`ui/src/components/settings/SettingsPage.test.tsx`, Vitest)

| Module | Scenario | Expected |
| ------ | -------- | -------- |
| page | Loads settings and time zones | fields filled; select holds the list; Save hidden |
| page | Edit latitude with a comma, Save | `,` turned to `.`; PUT with the six keys |
| page | PUT refused | the host's text shown |
| page | Root aggregation with sun times | "Today" shows them and "Day"; none: the hint text |
| backup | Export | GET /backup called, a download made |
| backup | Import of a file over 16 KB | refused in the page, nothing sent |
| backup | Import confirmed | POST with the file's text; result line with the counts |
| backup | Import refused by the host | the refusal text shown |
| nav | Sidebar | Settings entry, `#/settings` shows the page |

## Validation

- [x] Validate command green
- [x] Agent review done, blocking findings fixed
- [x] Demo on the Mac (the user away); box check left for later
