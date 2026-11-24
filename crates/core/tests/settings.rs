//! The home's settings (spec 026 R1, R2.2, R2.3): `GET/PUT /settings` and `/settings/timezones`,
//! their events, `settings.json` through the saver and `load_home`, and the root zone's sun data in
//! the aggregation.

mod mem_storage;
mod support;

use std::sync::Mutex;

use mem_storage::MemStorage;
use serde_json::{Value, json};
use solite_core::api::{Reach, Request, handle};
use solite_core::devices::{lock, parse_iso};
use solite_core::engine::{DEFINITION_DELAY_MS, SETTINGS_FILE, Saver, load_home};
use solite_core::home::Home;
use solite_core::live::{ClientId, LiveHandle};
use solite_core::settings::tz::TimeZone;
use support::{HOST, PORT, ROOT, Rig};

const SETTINGS: &str = "/api/v1/settings";

fn at(iso: &str) -> u64 {
    parse_iso(iso).unwrap()
}

const NOON: &str = "2026-06-21T12:00:00.000Z";

fn paris() -> Value {
    json!({
        "home.latitude": "48.8566",
        "home.longitude": "2.3522",
        "home.timezone": "Europe/Paris",
    })
}

fn put(rig: &mut Rig, body: Value) -> (u16, Value) {
    rig.put(SETTINGS, body)
}

fn get(rig: &mut Rig) -> Value {
    let (status, body) = rig.get(SETTINGS);
    assert_eq!(status, 200);
    body
}

fn put_refused(rig: &mut Rig, body: Value) -> (u16, String) {
    rig.refused("PUT", SETTINGS, &body.to_string())
}

// ---- The API ----

#[test]
fn a_first_start_has_no_settings() {
    let mut rig = Rig::new();
    assert_eq!(get(&mut rig), json!({}));
}

#[test]
fn the_six_keys_are_stored_as_given() {
    let mut rig = Rig::new();
    let (status, body) = put(
        &mut rig,
        json!({
            "home.name": "  Annexe  ",
            "home.latitude": "48.8566",
            "home.longitude": "2.3522",
            "home.timezone": "Europe/Paris",
            "home.sunriseOffset": "15",
            "home.sunsetOffset": "-20",
        }),
    );
    assert_eq!((status, body), (200, json!({"success": true})));
    assert_eq!(
        get(&mut rig),
        json!({
            "home.name": "  Annexe  ",
            "home.latitude": "48.8566",
            "home.longitude": "2.3522",
            "home.timezone": "Europe/Paris",
            "home.sunriseOffset": "15",
            "home.sunsetOffset": "-20",
        })
    );
    // A number with spaces does not match its pattern (Sowel's parseFloat would take it).
    let (status, _) = put(&mut rig, json!({"home.latitude": " 48.8566"}));
    assert_eq!(status, 400);
}

#[test]
fn a_put_keeps_the_keys_it_does_not_give() {
    let mut rig = Rig::new();
    put(&mut rig, json!({"home.name": "Annexe"}));
    put(&mut rig, json!({"home.latitude": "45"}));
    assert_eq!(
        get(&mut rig),
        json!({"home.name": "Annexe", "home.latitude": "45"})
    );
}

#[test]
fn the_empty_string_is_stored_and_reads_as_unset() {
    let mut rig = Rig::new();
    put(
        &mut rig,
        json!({"home.name": "Annexe", "home.latitude": "45"}),
    );
    let (status, _) = put(&mut rig, json!({"home.name": "", "home.latitude": ""}));
    assert_eq!(status, 200);
    // As Sowel: the keys keep `""`, which reads as unset.
    assert_eq!(get(&mut rig), json!({"home.name": "", "home.latitude": ""}));
}

#[test]
fn an_unknown_key_is_refused_and_nothing_changes() {
    let mut rig = Rig::new();
    put(&mut rig, json!({"home.name": "Before"}));
    let refused = put_refused(
        &mut rig,
        json!({"home.name": "After", "home.latitude": "45", "home.foo": "x"}),
    );
    assert_eq!(refused, (400, "Unknown setting: home.foo".to_string()));
    assert_eq!(get(&mut rig), json!({"home.name": "Before"}));
}

fn invalid(rig: &mut Rig, key: &str, value: Value) {
    let refused = put_refused(rig, json!({ key: value }));
    assert_eq!(
        refused,
        (400, format!("Invalid value for {key}")),
        "{key} = {value}"
    );
}

#[test]
fn a_latitude_out_of_its_range_or_pattern_is_refused() {
    let mut rig = Rig::new();
    for value in [
        "91",
        "-90.5",
        "abc",
        "NaN",
        "inf",
        "1e3",
        "48.123456789012345678901234567890",
    ] {
        invalid(&mut rig, "home.latitude", json!(value));
    }
    assert_eq!(get(&mut rig), json!({}));
}

#[test]
fn the_patterns_take_plain_decimals_only() {
    let mut rig = Rig::new();
    for value in ["48.8566", "-0", "0", "90", "-90"] {
        let (status, body) = put(&mut rig, json!({"home.latitude": value}));
        assert_eq!(status, 200, "{value}: {body}");
    }
    for value in ["1e1", "+5", ".5", "5.", "030.", "0x10", "٤٥", "1234"] {
        invalid(&mut rig, "home.latitude", json!(value));
    }
    let (status, _) = put(&mut rig, json!({"home.sunriseOffset": "030"}));
    assert_eq!(status, 200);
    assert_eq!(get(&mut rig)["home.sunriseOffset"], "030");
    invalid(&mut rig, "home.sunriseOffset", json!("30.0"));
}

#[test]
fn a_longitude_of_180_is_accepted_and_180_1_refused() {
    let mut rig = Rig::new();
    assert_eq!(put(&mut rig, json!({"home.longitude": "180"})).0, 200);
    assert_eq!(put(&mut rig, json!({"home.longitude": "-180"})).0, 200);
    invalid(&mut rig, "home.longitude", json!("180.1"));
}

#[test]
fn offsets_are_whole_minutes_from_minus_180_to_180() {
    let mut rig = Rig::new();
    for key in ["home.sunriseOffset", "home.sunsetOffset"] {
        assert_eq!(put(&mut rig, json!({ key: "30" })).0, 200);
        assert_eq!(put(&mut rig, json!({ key: "-180" })).0, 200);
        for value in ["181", "1.5", "x"] {
            invalid(&mut rig, key, json!(value));
        }
    }
}

#[test]
fn a_time_zone_is_a_name_of_the_list_in_its_exact_case() {
    let mut rig = Rig::new();
    assert_eq!(
        put(&mut rig, json!({"home.timezone": "Europe/Paris"})).0,
        200
    );
    invalid(&mut rig, "home.timezone", json!("Mars/Olympus"));
    invalid(&mut rig, "home.timezone", json!("europe/paris"));
}

#[test]
fn a_name_of_64_characters_is_accepted_and_65_refused() {
    let mut rig = Rig::new();
    let name = "n".repeat(64);
    assert_eq!(put(&mut rig, json!({ "home.name": name })).0, 200);
    invalid(&mut rig, "home.name", json!("n".repeat(65)));
}

#[test]
fn a_value_that_is_not_a_string_and_a_body_that_is_not_an_object_are_refused() {
    let mut rig = Rig::new();
    invalid(&mut rig, "home.latitude", json!(48));
    invalid(&mut rig, "home.name", json!(null));
    assert_eq!(
        rig.refused("PUT", SETTINGS, "[]"),
        (400, "Invalid body".to_string())
    );
    assert_eq!(
        rig.refused("PUT", SETTINGS, "{not json"),
        (400, "Invalid body".to_string())
    );
}

#[test]
fn a_body_that_is_not_declared_json_is_415() {
    let mut rig = Rig::new();
    let response = handle(
        &mut rig.home,
        &Request {
            method: "PUT",
            path: SETTINGS,
            host: Some(HOST),
            port: PORT,
            reach: Reach::Loopback,
            content_type: Some("text/plain"),
            body: br#"{"home.name":"x"}"#,
        },
        rig.now,
        &mut *rig.random,
    );
    assert_eq!(response.status, 415);
    assert_eq!(get(&mut rig), json!({}));
}

#[test]
fn the_time_zones_are_listed_utc_first() {
    let mut rig = Rig::new();
    let (status, body) = rig.get("/api/v1/settings/timezones");
    assert_eq!(status, 200);
    let names = body.as_array().unwrap();
    assert_eq!(names[0], "UTC");
    assert!(names.contains(&json!("Europe/Paris")));
}

#[test]
fn other_methods_are_405() {
    let mut rig = Rig::new();
    for (method, path) in [
        ("POST", SETTINGS),
        ("DELETE", SETTINGS),
        ("PUT", "/api/v1/settings/timezones"),
        ("POST", "/api/v1/settings/timezones"),
        ("PUT", "/api/v1/backup"),
        ("DELETE", "/api/v1/backup"),
    ] {
        assert_eq!(rig.refused(method, path, "{}").0, 405, "{method} {path}");
    }
}

// ---- Events ----

fn client(hub: &LiveHandle, topics: &[&str]) -> ClientId {
    let id = hub.connect().expect("a free slot");
    let message = json!({"type": "subscribe", "topics": topics}).to_string();
    assert!(hub.subscribe(id, &message));
    id
}

fn batch(hub: &LiveHandle, id: ClientId) -> Vec<Value> {
    match hub.take_batch(id) {
        None => Vec::new(),
        Some(text) => serde_json::from_str::<Value>(&text)
            .unwrap()
            .as_array()
            .unwrap()
            .clone(),
    }
}

fn of_type<'a>(events: &'a [Value], name: &str) -> Vec<&'a Value> {
    events.iter().filter(|e| e["type"] == name).collect()
}

#[test]
fn a_put_emits_settings_changed_with_the_keys_of_the_body() {
    let mut rig = Rig::new();
    let hub = rig.home.live();
    let c = client(&hub, &[]);
    put(
        &mut rig,
        json!({"home.name": "Annexe", "home.sunsetOffset": "45"}),
    );
    let events = batch(&hub, c);
    let changed = of_type(&events, "settings.changed");
    assert_eq!(changed.len(), 1, "{events:?}");
    let mut keys: Vec<&str> = changed[0]["keys"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k.as_str().unwrap())
        .collect();
    keys.sort_unstable();
    assert_eq!(keys, ["home.name", "home.sunsetOffset"]);
}

#[test]
fn a_location_gives_sunlight_changed_once_and_the_root_zones_data_changed() {
    let mut rig = Rig::new();
    rig.now = at(NOON);
    rig.home.recompute(rig.now);
    let hub = rig.home.live();
    let c = client(&hub, &["zones"]);
    put(&mut rig, paris());
    rig.get("/api/v1/zones/aggregation");
    rig.home.recompute(rig.now);
    let events = batch(&hub, c);
    assert_eq!(of_type(&events, "sunlight.changed").len(), 1, "{events:?}");
    let root: Vec<_> = of_type(&events, "zone.data.changed")
        .into_iter()
        .filter(|e| e["zoneId"] == ROOT)
        .collect();
    assert_eq!(root.len(), 1, "{events:?}");
    assert_eq!(root[0]["aggregatedData"]["isDaylight"], true);
    // sunlight.changed comes before the root zone's change.
    let position = |name: &str| events.iter().position(|e| e["type"] == name).unwrap();
    assert!(position("sunlight.changed") < position("zone.data.changed"));
}

// ---- The aggregation ----

#[test]
fn with_a_location_the_root_zone_has_the_sun_times_and_other_zones_null() {
    let mut rig = Rig::new();
    let garage = rig.zone("Garage");
    rig.now = at(NOON);
    put(&mut rig, paris());
    let (status, body) = rig.get("/api/v1/zones/aggregation");
    assert_eq!(status, 200);
    assert_eq!(body[ROOT]["sunrise"], "05:46");
    assert_eq!(body[ROOT]["sunset"], "21:57");
    assert_eq!(body[ROOT]["isDaylight"], true);
    for field in ["sunrise", "sunset", "isDaylight"] {
        assert_eq!(body[&garage][field], Value::Null, "{field}");
    }
}

#[test]
fn without_a_location_the_root_zone_has_nulls() {
    let mut rig = Rig::new();
    rig.now = at(NOON);
    put(&mut rig, json!({"home.timezone": "Europe/Paris"}));
    let (_, body) = rig.get("/api/v1/zones/aggregation");
    for field in ["sunrise", "sunset", "isDaylight"] {
        assert_eq!(body[ROOT][field], Value::Null, "{field}");
    }
}

#[test]
fn two_aggregates_in_the_same_minute_compute_the_sun_once() {
    let mut rig = Rig::new();
    rig.now = at(NOON);
    put(&mut rig, paris());
    let before = rig.home.sun_computations();
    rig.home.aggregate(rig.now + 1_000);
    rig.home.aggregate(rig.now + 59_000);
    assert_eq!(rig.home.sun_computations(), before);
    rig.home.aggregate(rig.now + 60_000);
    assert_eq!(rig.home.sun_computations(), before + 1);
}

#[test]
fn the_periodic_check_brings_the_end_of_the_day_without_any_measure() {
    let mut rig = Rig::new();
    // 30 s before sunset − 45 min (Paris sets at 19:57:54Z): the day ends at 19:12:54Z.
    rig.now = at("2026-06-21T19:12:24.000Z");
    put(&mut rig, paris());
    let hub = rig.home.live();
    let c = client(&hub, &["zones"]);
    let (_, body) = rig.get("/api/v1/zones/aggregation");
    assert_eq!(body[ROOT]["isDaylight"], true);
    batch(&hub, c);
    rig.home.tick(rig.now + 60_000);
    let events = batch(&hub, c);
    let root: Vec<_> = of_type(&events, "zone.data.changed")
        .into_iter()
        .filter(|e| e["zoneId"] == ROOT)
        .collect();
    assert_eq!(root.len(), 1, "{events:?}");
    assert_eq!(root[0]["aggregatedData"]["isDaylight"], false);
    assert_eq!(of_type(&events, "sunlight.changed").len(), 1);
}

// ---- settings.json ----

fn call(home: &Mutex<Home>, body: Value, now: u64) -> u16 {
    let text = body.to_string();
    let mut random = |b: &mut [u8]| {
        b.fill(9);
        Ok(())
    };
    handle(
        &mut lock(home),
        &Request {
            method: "PUT",
            path: SETTINGS,
            host: Some(HOST),
            port: PORT,
            reach: Reach::Loopback,
            content_type: Some("application/json"),
            body: text.as_bytes(),
        },
        now,
        &mut random,
    )
    .status
}

const T: u64 = support::T0;

/// A home with everything written once, and its storage's counts cleared.
fn saved(storage: &MemStorage) -> Mutex<Home> {
    let home = Mutex::new(load_home(&storage, T).unwrap());
    Saver::new(storage, T).tick(&home, T, true).unwrap();
    storage.per_file.borrow_mut().clear();
    home
}

#[test]
fn a_burst_of_three_puts_is_one_write_of_the_object() {
    let storage = MemStorage::default();
    let home = saved(&storage);
    let mut saver = Saver::new(&storage, T).with_definition_delay(DEFINITION_DELAY_MS);
    for (i, name) in ["One", "Two", "Three"].into_iter().enumerate() {
        let now = T + i as u64 * 300;
        assert_eq!(call(&home, json!({ "home.name": name }), now), 200);
        saver.tick(&home, now, false).unwrap();
    }
    saver.tick(&home, T + 1_999, false).unwrap();
    assert_eq!(storage.writes(SETTINGS_FILE), 0);
    saver.tick(&home, T + 2_000, false).unwrap();
    assert_eq!(storage.writes(SETTINGS_FILE), 1);
    assert_eq!(
        storage.text(SETTINGS_FILE).unwrap(),
        r#"{"home.name":"Three"}"#
    );
}

#[test]
fn a_put_of_the_same_values_writes_nothing() {
    let storage = MemStorage::with(&[(SETTINGS_FILE, r#"{"home.name":"Same"}"#)]);
    let home = saved(&storage);
    let mut saver = Saver::new(&storage, T);
    assert_eq!(call(&home, json!({"home.name": "Same"}), T), 200);
    assert!(!lock(&home).settings_changed());
    saver.tick(&home, T + 5_000, false).unwrap();
    assert_eq!(storage.writes(SETTINGS_FILE), 0);
}

#[test]
fn emptied_keys_are_written_as_empty_strings() {
    let storage = MemStorage::with(&[(SETTINGS_FILE, r#"{"home.name":"A","home.latitude":"1"}"#)]);
    let home = saved(&storage);
    let mut saver = Saver::new(&storage, T);
    call(&home, json!({"home.name": "", "home.latitude": ""}), T);
    saver.tick(&home, T, false).unwrap();
    assert_eq!(
        storage.text(SETTINGS_FILE).unwrap(),
        r#"{"home.latitude":"","home.name":""}"#
    );
}

#[test]
fn load_home_reads_the_settings_back() {
    let storage = MemStorage::with(&[(SETTINGS_FILE, &paris().to_string())]);
    let home = load_home(&&storage, T).unwrap();
    assert_eq!(home.settings().to_map(), *paris().as_object().unwrap());
    assert!(!home.settings_changed());
}

#[test]
fn an_unknown_key_and_a_bad_value_in_the_file_are_dropped_and_the_file_written_again() {
    let text = r#"{"home.name":"Kept","home.foo":"x","home.latitude":"200"}"#;
    let storage = MemStorage::with(&[(SETTINGS_FILE, text)]);
    let home = load_home(&&storage, T).unwrap();
    assert_eq!(
        home.settings().to_map(),
        *json!({"home.name": "Kept"}).as_object().unwrap()
    );
    assert!(home.settings_changed());
    let home = Mutex::new(home);
    Saver::new(&storage, T).tick(&home, T, false).unwrap();
    assert_eq!(
        storage.text(SETTINGS_FILE).unwrap(),
        r#"{"home.name":"Kept"}"#
    );
}

#[test]
fn a_time_zone_the_list_does_not_have_is_dropped_back_to_utc() {
    let text = r#"{"home.timezone":"Mars/Olympus"}"#;
    let storage = MemStorage::with(&[(SETTINGS_FILE, text)]);
    let home = load_home(&&storage, T).unwrap();
    assert_eq!(home.settings().get("home.timezone"), None);
    assert_eq!(home.settings().time_zone(), TimeZone::UTC);
    assert!(home.settings_changed());
    let home = Mutex::new(home);
    Saver::new(&storage, T).tick(&home, T, false).unwrap();
    assert_eq!(storage.text(SETTINGS_FILE).unwrap(), "{}");
}

#[test]
fn a_file_that_is_not_an_object_of_strings_stops_the_load() {
    for text in ["[]", "{not json", r#"{"home.latitude":48}"#] {
        let storage = MemStorage::with(&[(SETTINGS_FILE, text)]);
        let message = load_home(&&storage, T).expect_err("an error");
        assert!(
            message.starts_with("settings: memory://settings.json: cannot read: "),
            "{text}: {message}"
        );
    }
}

#[test]
fn no_file_is_no_settings_and_nothing_written() {
    let storage = MemStorage::default();
    let home = Mutex::new(load_home(&&storage, T).unwrap());
    assert_eq!(lock(&home).settings().to_map().len(), 0);
    Saver::new(&storage, T).tick(&home, T, false).unwrap();
    assert_eq!(storage.writes(SETTINGS_FILE), 0);
}

#[test]
fn the_pairing_saver_does_not_write_the_settings() {
    let storage = MemStorage::default();
    let home = saved(&storage);
    let mut saver = Saver::devices_only(&storage, T);
    assert_eq!(call(&home, json!({"home.name": "Paired"}), T), 200);
    saver.tick(&home, T, false).unwrap();
    saver.tick(&home, T + 1_000, true).unwrap();
    assert_eq!(storage.writes(SETTINGS_FILE), 0);
}

// ---- GET /system/timezone (spec 027 R4) ----

const SYSTEM_TIMEZONE: &str = "/api/v1/system/timezone";

#[test]
fn the_system_timezone_without_a_setting_is_utc_as_a_fallback() {
    let mut rig = Rig::new();
    assert_eq!(
        rig.get(SYSTEM_TIMEZONE),
        (
            200,
            json!({"tz": "UTC", "source": "fallback", "offsetHours": 0})
        )
    );
}

#[test]
fn the_system_timezone_follows_the_offset_of_the_date() {
    let mut rig = Rig::new();
    assert_eq!(
        put(&mut rig, json!({"home.timezone": "Europe/Paris"})).0,
        200
    );
    rig.now = at("2026-01-15T12:00:00.000Z");
    assert_eq!(
        rig.get(SYSTEM_TIMEZONE),
        (
            200,
            json!({"tz": "Europe/Paris", "source": "env", "offsetHours": 1})
        )
    );
    rig.now = at("2026-07-15T12:00:00.000Z");
    let (_, body) = rig.get(SYSTEM_TIMEZONE);
    assert_eq!(body["offsetHours"], 2, "daylight saving time");
    assert_eq!(body["source"], "env");
    assert_eq!(body["tz"], "Europe/Paris");
    // A whole number is written without a fraction.
    assert!(
        rig.raw("GET", SYSTEM_TIMEZONE, "")
            .body
            .unwrap()
            .contains(r#""offsetHours":2}"#)
    );
}

#[test]
fn the_system_timezone_has_fractions_of_an_hour_and_negative_offsets() {
    let mut rig = Rig::new();
    assert_eq!(
        put(&mut rig, json!({"home.timezone": "Asia/Kolkata"})).0,
        200
    );
    let (_, body) = rig.get(SYSTEM_TIMEZONE);
    assert_eq!(body["offsetHours"], 5.5);
    assert_eq!(
        put(&mut rig, json!({"home.timezone": "America/Sao_Paulo"})).0,
        200
    );
    let (_, body) = rig.get(SYSTEM_TIMEZONE);
    assert_eq!(body["offsetHours"], -3);
}

#[test]
fn the_system_timezone_accepts_get_only() {
    let mut rig = Rig::new();
    for method in ["POST", "PUT", "DELETE"] {
        assert_eq!(
            rig.refused(method, SYSTEM_TIMEZONE, "{}").0,
            405,
            "{method}"
        );
    }
}

#[test]
fn a_location_without_a_time_zone_is_still_the_fallback() {
    let mut rig = Rig::new();
    assert_eq!(
        put(
            &mut rig,
            json!({"home.latitude": "48.8566", "home.longitude": "2.3522"})
        )
        .0,
        200
    );
    assert_eq!(
        rig.get(SYSTEM_TIMEZONE),
        (
            200,
            json!({"tz": "UTC", "source": "fallback", "offsetHours": 0})
        )
    );
}
