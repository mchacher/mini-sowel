//! The sun and the time zones of spec 026 (R2): the port of suncalc 2 against values computed with
//! Sowel's `suncalc` (`getTimes`, `getPosition`), the daylight rule with its offsets, and the POSIX
//! rules of the time zone list.

use serde_json::{Map, Value, json};
use solite_core::devices::parse_iso;
use solite_core::settings::Settings;
use solite_core::settings::sun::{altitude_at, sun_times, sunlight};
use solite_core::settings::tz::{TIME_ZONES, TimeZone, find};

const PARIS: (f64, f64) = (48.8566, 2.3522);
const TROMSO: (f64, f64) = (69.6492, 18.9553);
const SYDNEY: (f64, f64) = (-33.8688, 151.2093);
const MINUTE: i64 = 60_000;

fn at(iso: &str) -> u64 {
    parse_iso(iso).unwrap()
}

fn settings(entries: Value) -> Settings {
    let Value::Object(map) = entries else {
        panic!("an object")
    };
    let map: Map<String, Value> = map;
    Settings::from_entries(Settings::check(&map).unwrap())
}

fn paris(extra: Value) -> Settings {
    let mut base = json!({
        "home.latitude": "48.8566",
        "home.longitude": "2.3522",
        "home.timezone": "Europe/Paris",
    });
    for (k, v) in extra.as_object().unwrap() {
        base[k] = v.clone();
    }
    settings(base)
}

/// `now`, (latitude, longitude), suncalc's sunrise and sunset (UTC), the case's name.
type Case = (
    &'static str,
    (f64, f64),
    &'static str,
    &'static str,
    &'static str,
);

/// suncalc's sunrise and sunset, UTC, for each row of the plan's table.
const TABLE: [Case; 5] = [
    (
        "2026-06-21T12:00:00.000Z",
        PARIS,
        "2026-06-21T03:46:57.073Z",
        "2026-06-21T19:57:54.267Z",
        "Paris summer",
    ),
    (
        "2026-12-21T12:00:00.000Z",
        PARIS,
        "2026-12-21T07:41:15.737Z",
        "2026-12-21T15:56:04.091Z",
        "Paris winter",
    ),
    (
        "2026-03-20T12:00:00.000Z",
        (0.0, 0.0),
        "2026-03-20T06:04:12.481Z",
        "2026-03-20T18:10:43.375Z",
        "Equator equinox",
    ),
    (
        "2026-12-21T02:00:00.000Z",
        SYDNEY,
        "2026-12-20T18:40:39.039Z",
        "2026-12-21T09:05:25.857Z",
        "Sydney summer",
    ),
    (
        "2026-07-01T17:00:00.000Z",
        (40.7128, -74.006),
        "2026-07-01T09:28:40.051Z",
        "2026-07-02T00:31:06.432Z",
        "New York summer",
    ),
];

// ---- suncalc ----

#[test]
fn sunrise_and_sunset_match_suncalc_within_a_minute() {
    for (now, (lat, lon), rise, set, name) in TABLE {
        let (r, s) = sun_times(at(now), lat, lon);
        let (r, s) = (r.unwrap(), s.unwrap());
        assert!((r - at(rise) as i64).abs() < 60_000, "{name} sunrise {r}");
        assert!((s - at(set) as i64).abs() < 60_000, "{name} sunset {s}");
        assert!(altitude_at(at(now), lat, lon) > 0.0, "{name}");
    }
}

#[test]
fn the_port_is_suncalc_to_the_second() {
    // Not required by the spec (one minute), but a faithful port is far closer.
    for (now, (lat, lon), rise, set, name) in TABLE {
        let (r, s) = sun_times(at(now), lat, lon);
        assert!((r.unwrap() - at(rise) as i64).abs() < 1_000, "{name}");
        assert!((s.unwrap() - at(set) as i64).abs() < 1_000, "{name}");
    }
}

#[test]
fn polar_day_has_no_times_and_the_sun_above_the_horizon() {
    let noon = at("2026-06-21T12:00:00.000Z");
    for ((lat, lon), altitude) in [(TROMSO, 42.51315), ((90.0, 0.0), 23.47660)] {
        assert_eq!(sun_times(noon, lat, lon), (None, None));
        let a = altitude_at(noon, lat, lon);
        assert!(a > 0.0 && (a - altitude).abs() < 0.05, "{a}");
    }
}

#[test]
fn polar_night_has_no_times_and_the_sun_below_the_horizon() {
    let cases = [
        ("2026-12-21T12:00:00.000Z", TROMSO, -3.64634),
        ("2026-06-21T12:00:00.000Z", (-90.0, 0.0), -22.95409),
    ];
    for (now, (lat, lon), altitude) in cases {
        assert_eq!(sun_times(at(now), lat, lon), (None, None));
        let a = altitude_at(at(now), lat, lon);
        assert!(a < 0.0 && (a - altitude).abs() < 0.05, "{a}");
    }
}

#[test]
fn paris_at_night_has_the_same_day_and_the_sun_below() {
    for now in ["2026-06-21T01:00:00.000Z", "2026-06-21T23:30:00.000Z"] {
        let (r, s) = sun_times(at(now), PARIS.0, PARIS.1);
        assert!((r.unwrap() - at("2026-06-21T03:46:57.073Z") as i64).abs() < 60_000);
        assert!((s.unwrap() - at("2026-06-21T19:57:54.267Z") as i64).abs() < 60_000);
        assert!(altitude_at(at(now), PARIS.0, PARIS.1) < 0.0);
    }
}

// ---- The daylight rule ----

fn day(settings: &Settings, now: u64) -> Option<bool> {
    sunlight(settings, now).is_daylight
}

#[test]
fn paris_at_noon_in_june_is_local_times_and_day() {
    let sun = sunlight(&paris(json!({})), at("2026-06-21T12:00:00.000Z"));
    assert_eq!(sun.sunrise.as_deref(), Some("05:46"));
    assert_eq!(sun.sunset.as_deref(), Some("21:57"));
    assert_eq!(sun.is_daylight, Some(true));
}

/// Spec 037, R5: the sunrise and sunset shown for 2026-11-25 (Paris reads 08:14 and 17:00; the
/// 07:58 once noted is Paris around 14 November), with the home's own time zone.
#[test]
fn late_november_sun_times_are_local_for_paris_and_new_york() {
    let now = at("2026-11-25T12:00:00.000Z");
    let sun = sunlight(&paris(json!({})), now);
    assert_eq!(sun.sunrise.as_deref(), Some("08:14"));
    assert_eq!(sun.sunset.as_deref(), Some("17:00"));
    let new_york = settings(json!({
        "home.latitude": "40.7128",
        "home.longitude": "-74.006",
        "home.timezone": "America/New_York",
    }));
    let sun = sunlight(&new_york, now);
    assert_eq!(sun.sunrise.as_deref(), Some("06:54"));
    assert_eq!(sun.sunset.as_deref(), Some("16:31"));
}

#[test]
fn paris_before_sunrise_and_after_sunset_is_night() {
    let home = paris(json!({}));
    assert_eq!(day(&home, at("2026-06-21T01:00:00.000Z")), Some(false));
    assert_eq!(day(&home, at("2026-06-21T23:30:00.000Z")), Some(false));
}

/// Paris's sunrise and sunset of 2026-06-21, in ms (what the port gives).
fn paris_times() -> (u64, u64) {
    let (r, s) = sun_times(at("2026-06-21T12:00:00.000Z"), PARIS.0, PARIS.1);
    (r.unwrap() as u64, s.unwrap() as u64)
}

#[test]
fn the_day_starts_at_sunrise_plus_the_default_30_minutes() {
    let home = paris(json!({}));
    let (rise, _) = paris_times();
    assert_eq!(day(&home, rise + 29 * MINUTE as u64), Some(false));
    assert_eq!(day(&home, rise + 30 * MINUTE as u64), Some(true));
}

#[test]
fn the_day_ends_at_sunset_minus_the_default_45_minutes_excluded() {
    let home = paris(json!({}));
    let (_, set) = paris_times();
    assert_eq!(day(&home, set - 45 * MINUTE as u64), Some(false));
    assert_eq!(day(&home, set - 46 * MINUTE as u64), Some(true));
}

#[test]
fn offsets_of_zero_make_the_day_start_at_sunrise_exactly() {
    let home = paris(json!({"home.sunriseOffset": "0", "home.sunsetOffset": "0"}));
    let (rise, set) = paris_times();
    assert_eq!(day(&home, rise), Some(true));
    assert_eq!(day(&home, rise - 1), Some(false));
    assert_eq!(day(&home, set - 1), Some(true));
    assert_eq!(day(&home, set), Some(false));
}

#[test]
fn a_negative_offset_starts_the_day_before_sunrise() {
    let home = paris(json!({"home.sunriseOffset": "-10"}));
    let (rise, _) = paris_times();
    assert_eq!(day(&home, rise - 5 * MINUTE as u64), Some(true));
    assert_eq!(day(&home, rise - 11 * MINUTE as u64), Some(false));
}

#[test]
fn tromso_in_summer_is_day_and_in_winter_night_with_no_times() {
    let home = settings(json!({"home.latitude": "69.6492", "home.longitude": "18.9553"}));
    let sun = sunlight(&home, at("2026-06-21T12:00:00.000Z"));
    assert_eq!(sun.sunrise.as_deref(), Some("--:--"));
    assert_eq!(sun.sunset.as_deref(), Some("--:--"));
    assert_eq!(sun.is_daylight, Some(true));
    let sun = sunlight(&home, at("2026-12-21T12:00:00.000Z"));
    assert_eq!(sun.sunrise.as_deref(), Some("--:--"));
    assert_eq!(sun.sunset.as_deref(), Some("--:--"));
    assert_eq!(sun.is_daylight, Some(false));
}

#[test]
fn a_latitude_without_a_longitude_is_no_location() {
    let home = settings(json!({"home.latitude": "48.8566"}));
    let sun = sunlight(&home, at("2026-06-21T12:00:00.000Z"));
    assert_eq!(
        (sun.sunrise, sun.sunset, sun.is_daylight),
        (None, None, None)
    );
}

#[test]
fn a_clock_not_set_gives_no_sun_data() {
    let sun = sunlight(&paris(json!({})), 3_600_000);
    assert_eq!(
        (sun.sunrise, sun.sunset, sun.is_daylight),
        (None, None, None)
    );
}

#[test]
fn sydney_in_summer_gives_its_daylight_saving_local_times() {
    let home = settings(json!({
        "home.latitude": "-33.8688",
        "home.longitude": "151.2093",
        "home.timezone": "Australia/Sydney",
    }));
    let sun = sunlight(&home, at("2026-12-21T02:00:00.000Z"));
    assert_eq!(sun.sunrise.as_deref(), Some("05:40"));
    assert_eq!(sun.sunset.as_deref(), Some("20:05"));
    assert_eq!(sun.is_daylight, Some(true));
}

#[test]
fn without_a_time_zone_the_times_are_utc() {
    let home = settings(json!({"home.latitude": "48.8566", "home.longitude": "2.3522"}));
    let sun = sunlight(&home, at("2026-06-21T12:00:00.000Z"));
    assert_eq!(sun.sunrise.as_deref(), Some("03:46"));
    assert_eq!(sun.sunset.as_deref(), Some("19:57"));
}

// ---- Time zones ----

fn offset(zone: &str, iso: &str) -> i64 {
    find(zone).unwrap().offset_at(at(iso) as i64)
}

#[test]
fn every_time_zone_of_the_list_parses_with_a_unique_name_utc_first() {
    assert_eq!(TIME_ZONES[0].0, "UTC");
    for (i, (name, rule)) in TIME_ZONES.iter().enumerate() {
        assert!(TimeZone::parse(rule).is_ok(), "{name}: {rule}");
        assert!(
            !TIME_ZONES[..i].iter().any(|(other, _)| other == name),
            "{name} twice"
        );
    }
}

#[test]
fn paris_is_one_hour_east_in_winter_and_two_in_summer() {
    assert_eq!(offset("Europe/Paris", "2026-01-15T12:00:00.000Z"), 3600);
    assert_eq!(offset("Europe/Paris", "2026-07-15T12:00:00.000Z"), 7200);
}

#[test]
fn paris_switches_at_one_utc_on_the_last_sundays_of_march_and_october() {
    assert_eq!(offset("Europe/Paris", "2026-03-29T00:59:59.999Z"), 3600);
    assert_eq!(offset("Europe/Paris", "2026-03-29T01:00:00.000Z"), 7200);
    assert_eq!(offset("Europe/Paris", "2026-10-25T00:59:59.999Z"), 7200);
    assert_eq!(offset("Europe/Paris", "2026-10-25T01:00:00.000Z"), 3600);
}

#[test]
fn new_york_switches_on_its_second_sunday_of_march_and_first_of_november() {
    let ny = "America/New_York";
    assert_eq!(offset(ny, "2026-03-08T06:59:59.999Z"), -18000);
    assert_eq!(offset(ny, "2026-03-08T07:00:00.000Z"), -14400);
    assert_eq!(offset(ny, "2026-11-01T05:59:59.999Z"), -14400);
    assert_eq!(offset(ny, "2026-11-01T06:00:00.000Z"), -18000);
}

#[test]
fn sydney_has_daylight_saving_in_the_southern_summer() {
    assert_eq!(
        offset("Australia/Sydney", "2026-01-15T12:00:00.000Z"),
        39600
    );
    assert_eq!(
        offset("Australia/Sydney", "2026-07-15T12:00:00.000Z"),
        36000
    );
}

#[test]
fn sydney_switches_exactly_at_its_instants() {
    let sydney = "Australia/Sydney";
    assert_eq!(offset(sydney, "2026-04-04T15:59:59.999Z"), 39600);
    assert_eq!(offset(sydney, "2026-04-04T16:00:00.000Z"), 36000);
    assert_eq!(offset(sydney, "2026-10-03T15:59:59.999Z"), 36000);
    assert_eq!(offset(sydney, "2026-10-03T16:00:00.000Z"), 39600);
}

#[test]
fn kolkata_is_five_and_a_half_hours_east() {
    assert_eq!(offset("Asia/Kolkata", "2026-01-15T12:00:00.000Z"), 19800);
    assert_eq!(offset("Asia/Kolkata", "2026-07-15T12:00:00.000Z"), 19800);
}

#[test]
fn utc_and_phoenix_never_change() {
    for date in ["2026-01-15T12:00:00.000Z", "2026-07-15T12:00:00.000Z"] {
        assert_eq!(offset("UTC", date), 0);
        assert_eq!(offset("America/Phoenix", date), -25200);
    }
}

#[test]
fn quoted_names_parse() {
    let istanbul = TimeZone::parse("<+03>-3").unwrap();
    assert_eq!(
        istanbul.offset_at(at("2026-07-15T12:00:00.000Z") as i64),
        10800
    );
    let miquelon = TimeZone::parse("<-03>3<-02>,M3.2.0,M11.1.0").unwrap();
    assert_eq!(
        miquelon.offset_at(at("2026-01-15T12:00:00.000Z") as i64),
        -10800
    );
    assert_eq!(
        miquelon.offset_at(at("2026-07-15T12:00:00.000Z") as i64),
        -7200
    );
}

#[test]
fn rules_of_other_forms_and_garbage_are_refused() {
    for rule in [
        "CET-1CEST,J60,M10.5.0",
        "CET-1CEST,60,M10.5.0",
        "",
        "CET-1CEST,M13.1.0,M10.5.0",
        "garbage",
        "CET",
        "CE-1",
        "CET-1CEST",
        "CET-1CEST,M3.5.0",
        "CET-1CEST,M3.5.0,M10.5.0/3x",
        "CET-1CEST,M3.0.0,M10.5.0",
        "CET-1CEST,M3.5.7,M10.5.0",
        "<+3>-3",
    ] {
        assert!(TimeZone::parse(rule).is_err(), "{rule}");
    }
}
