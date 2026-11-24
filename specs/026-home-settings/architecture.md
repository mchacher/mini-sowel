# Architecture: Spec 026

## Overview

```
PUT /api/v1/settings ──► api/settings.rs ──► Home::put_settings ──► Settings (6 keys, checked)
                                                   │ settings_changed flag ──► Saver ──► settings.json
                                                   └► sunlight cache invalidated
Home::aggregate(now) ──► Home::sunlight(now) ──► settings::sun (suncalc port) + settings::tz (POSIX rule)
                              │ cached per minute       └► root zone: sunrise, sunset, isDaylight
                              └► sunlight.changed (if different)
motion-light is_daytime ──► Home::sunlight(now).is_daylight
GET/POST /api/v1/backup ──► api/backup.rs ──► Home::export / Home::import (checked on a copy, then swapped)
ui: #/settings ──► SettingsPage (HomeSettingsCard, BackupCard) ──► api/settings.ts
```

Everything new is in the core (`crates/core/src/settings/`), tested on the Mac. The host and the
firmware need no change: `settings.json` goes through the existing `Storage` trait by name, and
`load_home` and the `Saver` gain the file.

## Data

| Name | Type | Where |
| ---- | ---- | ----- |
| `Settings` | the six optional values (`name: Option<String>`, `latitude: Option<String>`...), with `get`, `to_map`, `apply(entries) -> Outcome<Vec<key>>` (all checked first) | `crates/core/src/settings/mod.rs`; bound: six keys, `MAX_VALUE_CHARS = 64`, `MAX_NUMBER_CHARS = 32` |
| `settings.json` | JSON object, key → string | the storage, beside `zones.json` (`SETTINGS_FILE`) |
| `Sunlight` | `{ sunrise: Option<String>, sunset: Option<String>, is_daylight: Option<bool> }` | `crates/core/src/settings/sun.rs` |
| `SunCache` | the last `Sunlight` and the minute it was computed for (cleared by a settings change); the last value the aggregation saw, for `sunlight.changed` | fields of `Home` (one value each) |
| `TimeZone` | parsed POSIX rule: std offset, optional dst offset and two `Mm.w.d/time` rules | `crates/core/src/settings/tz.rs` |
| `TIME_ZONES` | `&[(&str, &str)]`, IANA name → POSIX rule, about 50, `UTC` first | `crates/core/src/settings/tz.rs`; fixed |
| `Backup` | `{ version: 1, settings: Map, zones: Vec<Zone> }` | `crates/core/src/settings/backup.rs`; bound: 32 zones, the 16 KB body |

Sun computation (port of suncalc 2, as used by Sowel's `sunlight-manager.ts`): `to_days`,
`solar_mean_anomaly`, `ecliptic_longitude`, `declination`, `julian_cycle`, `solar_transit_j`,
`hour_angle`, `get_set_j` with h0 = −0.833°, and `altitude` (`getPosition`). f64, `std` math
(available on the box: `std` on ESP-IDF). Times out as UTC milliseconds; `None` when the hour
angle is not defined (polar).

Local time: `TimeZone::offset_at(utc_ms) -> i64 seconds` from the parsed rule; `HH:MM` =
`(utc_ms/1000 + offset)` in seconds of the day, truncated to the minute (Sowel's `getHours` and
`getMinutes`).

## Files changed

| File | Change |
| ---- | ------ |
| `crates/core/src/settings/mod.rs` (new) | `Settings`, key rules, `parse_file` (lenient: drops bad entries), `to_json`; `SETTINGS_KEYS` |
| `crates/core/src/settings/sun.rs` (new) | suncalc port, `Sunlight`, `sunlight(settings, now)` |
| `crates/core/src/settings/tz.rs` (new) | POSIX rule parser, `offset_at`, `TIME_ZONES`, `find` |
| `crates/core/src/settings/backup.rs` (new) | the backup document: parse strict, check, export |
| `crates/core/src/lib.rs` | `pub mod settings;` |
| `crates/core/src/home/mod.rs` | `settings` and `settings_changed` fields; `settings()`, `put_settings`, `take_settings_cursor` (or `take_settings_json`), `mark_settings_changed`; sun cache; root zone injection in `aggregate` and `zone_aggregate`; `sunlight(now)` (cache by minute, usable from `&Home` through a `Cell`, or computed without storing); `import_backup`; `settings_changed` in `changes`-like accessor used by the saver |
| `crates/core/src/home/cursor.rs` | a cursor for `settings.json` if the saver's loop needs one (one chunk) |
| `crates/core/src/zones/mod.rs` | `#[derive(Clone)]` on `Zones` (the import works on a copy); `create` takes the given id (already) |
| `crates/core/src/engine/storage.rs` | `SETTINGS_FILE`, `load_settings`, `load_home` reads it |
| `crates/core/src/engine/saver.rs` | the settings file in the definitions loop and in `definitions_unsaved` |
| `crates/core/src/live/events.rs` | `EngineEvent::SettingsChanged { keys }`, `EngineEvent::SunlightChanged` (topic `system`) |
| `crates/core/src/api/settings.rs` (new) | `GET/PUT /settings`, `GET /settings/timezones` |
| `crates/core/src/api/backup.rs` (new) | `GET/POST /backup` |
| `crates/core/src/api/mod.rs` | the two routes in `dispatch` |
| `crates/core/src/recipes/motion_light/machine.rs` | `root_is_daylight` reads `Home::sunlight(now)` |
| `ui/src/api/settings.ts` (new) | `getSettings`, `updateSettings`, `getTimezones`, `exportBackup`, `importBackup` |
| `ui/src/components/settings/SettingsPage.tsx` (new) | the two cards |
| `ui/src/components/layout/admin-nav-items.ts` | the Settings entry (`Settings` icon) |
| `ui/src/App.tsx` | the page |
| `ui/src/i18n/messages.ts` | Sowel's `settings.*` keys used, and Solite's few new ones |
| `ui/src/types.ts` | `HomeSettings`, `BackupResult` |
| Tests | `crates/core/tests/settings.rs`, `sun.rs`, `backup.rs` (new); `saves.rs`, `motion_light.rs`, `sowel_json.rs`, `memory_worst_case.rs` extended; `ui/src/components/settings/SettingsPage.test.tsx` |
| Docs | living spec; `docs/architecture.md`; `docs/decisions.md` D41, D42; `docs/roadmap.md`; `docs/specs-index.md`; `docs/design-system.md` (budget figure) |

Also touched (the reviewer's list): the saver's file table grows from 3 to 4 and
`definitions_unsaved` reads the settings flag (devices-only mode ignores it); `Home::from_parts`
keeps its signature and `load_home` sets the settings after it (`Home::load_settings`), so the many
tests calling `from_parts` are unchanged; the tests that enumerate the files
(`crates/core/tests/engine.rs:347`, `fixtures.rs:140`, `crates/host/tests/serve.rs:751,800`,
`sim.rs:277`, `crates/host/src/store.rs:259-293,368`) are updated only where they assert the exact
set of files; `firmware/src/simulated.rs` seeds no `settings.json` (no file: no settings). The
firmware's code is otherwise unchanged; its four images are rebuilt by validate.

## Time zone list

Name → POSIX rule (`UTC` first; then in this order):

```
UTC                   UTC0
Europe/London         GMT0BST,M3.5.0/1,M10.5.0
Europe/Dublin         GMT0IST,M3.5.0/1,M10.5.0
Europe/Lisbon         WET0WEST,M3.5.0/1,M10.5.0
Atlantic/Canary       WET0WEST,M3.5.0/1,M10.5.0
Atlantic/Reykjavik    GMT0
Europe/Paris          CET-1CEST,M3.5.0,M10.5.0/3
Europe/Brussels       CET-1CEST,M3.5.0,M10.5.0/3
Europe/Luxembourg     CET-1CEST,M3.5.0,M10.5.0/3
Europe/Amsterdam      CET-1CEST,M3.5.0,M10.5.0/3
Europe/Berlin         CET-1CEST,M3.5.0,M10.5.0/3
Europe/Zurich         CET-1CEST,M3.5.0,M10.5.0/3
Europe/Monaco         CET-1CEST,M3.5.0,M10.5.0/3
Europe/Madrid         CET-1CEST,M3.5.0,M10.5.0/3
Europe/Rome           CET-1CEST,M3.5.0,M10.5.0/3
Europe/Vienna         CET-1CEST,M3.5.0,M10.5.0/3
Europe/Prague         CET-1CEST,M3.5.0,M10.5.0/3
Europe/Warsaw         CET-1CEST,M3.5.0,M10.5.0/3
Europe/Budapest       CET-1CEST,M3.5.0,M10.5.0/3
Europe/Copenhagen     CET-1CEST,M3.5.0,M10.5.0/3
Europe/Oslo           CET-1CEST,M3.5.0,M10.5.0/3
Europe/Stockholm      CET-1CEST,M3.5.0,M10.5.0/3
Europe/Athens         EET-2EEST,M3.5.0/3,M10.5.0/4
Europe/Helsinki       EET-2EEST,M3.5.0/3,M10.5.0/4
Europe/Bucharest      EET-2EEST,M3.5.0/3,M10.5.0/4
Europe/Kyiv           EET-2EEST,M3.5.0/3,M10.5.0/4
Europe/Istanbul       <+03>-3
Europe/Moscow         MSK-3
America/New_York      EST5EDT,M3.2.0,M11.1.0
America/Toronto       EST5EDT,M3.2.0,M11.1.0
America/Chicago       CST6CDT,M3.2.0,M11.1.0
America/Denver        MST7MDT,M3.2.0,M11.1.0
America/Phoenix       MST7
America/Los_Angeles   PST8PDT,M3.2.0,M11.1.0
America/Vancouver     PST8PDT,M3.2.0,M11.1.0
America/Anchorage     AKST9AKDT,M3.2.0,M11.1.0
Pacific/Honolulu      HST10
America/Montreal      EST5EDT,M3.2.0,M11.1.0
America/Mexico_City   CST6
America/Sao_Paulo     <-03>3
America/Guadeloupe    AST4
America/Martinique    AST4
America/Cayenne       <-03>3
America/Miquelon      <-03>3<-02>,M3.2.0,M11.1.0
Indian/Reunion        <+04>-4
Indian/Mayotte        EAT-3
Pacific/Noumea        <+11>-11
Pacific/Tahiti        <-10>10
Africa/Abidjan        GMT0
Africa/Dakar          GMT0
Africa/Johannesburg   SAST-2
Asia/Dubai            <+04>-4
Asia/Kolkata          IST-5:30
Asia/Singapore        <+08>-8
Asia/Shanghai         CST-8
Asia/Tokyo            JST-9
Australia/Perth       AWST-8
Australia/Sydney      AEST-10AEDT,M10.1.0,M4.1.0/3
Pacific/Auckland      NZST-12NZDT,M9.5.0,M4.1.0/3
```

## Reused

- The zone body rules: `api/input.rs` (`name`, `text_fields`, `refuse_unknown`) for the backup's
  zones.
- `Zones::create` and `Zones::update` (their parent, cycle and count checks) on a clone, for the
  import.
- The saver's definitions loop and `ListCursor` pattern for `settings.json`.
- Sowel's `HomeSettingsSection` (`ui/src/pages/SettingsPage.tsx`) and its i18n keys
  (`settings.home`, `settings.homeName`...), ported.
- `CLOCK_SET_AFTER_MS` in `home/mod.rs` for the clock-not-set rule.
