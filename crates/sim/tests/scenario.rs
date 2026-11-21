//! Scenario files: parsing, the exact error lines, the bounds.

use std::fs;

use mini_sowel_sim::scenario::{MAX_EVENTS, parse_at};
use mini_sowel_sim::{
    Action, DeviceReport, Kind, MAX_DEVICES, NetworkInit, Scenario, ScenarioClock,
};

mod support;
use support::{ONE_SENSOR, scratch};

fn error(json: &str) -> String {
    Scenario::parse(json, "s.json").expect_err("the scenario is refused")
}

/// A scenario with one device of each kind, and the event given.
fn with_event(event: &str) -> String {
    format!(
        r#"{{ "devices": [
            {{ "id": "th", "kind": "temperature-humidity", "eui64": "00124B0000000001" }},
            {{ "id": "relay", "kind": "relay", "eui64": "00124B0000000002" }}
        ], "events": [ {event} ] }}"#
    )
}

#[test]
fn the_example_of_the_format_parses_to_the_radio_one_device_and_three_events() {
    let scenario = Scenario::parse(ONE_SENSOR, "s.json").unwrap();
    assert_eq!(scenario.radio.ezsp, 13);
    assert_eq!(scenario.radio.stack_version, 0x7440);
    assert_eq!(
        scenario.radio.eui64,
        [0x8C, 0x6F, 0xB9, 0xFF, 0xFE, 0x12, 0x34, 0x56]
    );
    assert_eq!(scenario.radio.network, NetworkInit::None);
    assert_eq!(scenario.devices.len(), 1);
    assert_eq!(scenario.devices[0].id, "th");
    assert_eq!(scenario.devices[0].kind, Kind::TemperatureHumidity);
    assert_eq!(
        scenario.devices[0].eui64,
        [0x00, 0x12, 0x4B, 0x00, 0x12, 0x34, 0x56, 0x78]
    );
    let times: Vec<u64> = scenario.events.iter().map(|e| e.at_ms).collect();
    assert_eq!(times, vec![2_000, 40_000, 90_000]);
    assert_eq!(scenario.events[0].action, Action::Join);
    assert_eq!(
        scenario.events[1].action,
        Action::Report(DeviceReport {
            temperature: Some(19.52),
            humidity: Some(54.1),
            ..DeviceReport::default()
        })
    );
    assert_eq!(
        scenario.events[2].action,
        Action::Report(DeviceReport {
            battery: Some(100),
            ..DeviceReport::default()
        })
    );
}

#[test]
fn a_file_that_is_not_json_names_the_line_and_the_column() {
    let message = error("{\n  \"devices\": [,\n}");
    assert!(
        message.starts_with("scenario: s.json: line 2, column "),
        "{message}"
    );
    assert!(!message.contains('\n'));
    assert_eq!(
        error("not json"),
        "scenario: s.json: line 1, column 2: expected ident"
    );
}

#[test]
fn unknown_fields_kinds_devices_and_duplicates_name_the_field() {
    assert_eq!(
        error(r#"{ "colour": 1 }"#),
        "scenario: s.json: colour: unknown field"
    );
    assert_eq!(
        error(
            r#"{ "devices": [ { "id": "a", "kind": "motion", "eui64": "00124B0000000001", "size": 3 } ] }"#
        ),
        "scenario: s.json: devices[0].size: unknown field"
    );
    assert_eq!(
        error(&with_event(
            r#"{ "at": "1s", "device": "th", "do": "join", "x": 1 }"#
        )),
        "scenario: s.json: events[0].x: unknown field"
    );
    assert_eq!(
        error(r#"{ "devices": [ { "id": "a", "kind": "lamp", "eui64": "00124B0000000001" } ] }"#),
        "scenario: s.json: devices[0].kind: unknown kind \"lamp\" (temperature-humidity, motion or relay)"
    );
    assert_eq!(
        error(&with_event(
            r#"{ "at": "1s", "device": "ghost", "do": "join" }"#
        )),
        "scenario: s.json: events[0].device: unknown device \"ghost\""
    );
    assert_eq!(
        error(
            r#"{ "devices": [
              { "id": "a", "kind": "motion", "eui64": "00124B0000000001" },
              { "id": "a", "kind": "motion", "eui64": "00124B0000000002" } ] }"#
        ),
        "scenario: s.json: devices[1].id: duplicate id \"a\""
    );
    assert_eq!(
        error(
            r#"{ "devices": [
              { "id": "a", "kind": "motion", "eui64": "00124B0000000001" },
              { "id": "b", "kind": "motion", "eui64": "00124b0000000001" } ] }"#
        ),
        "scenario: s.json: devices[1].eui64: duplicate EUI64"
    );
    assert_eq!(
        error(&with_event(
            r#"{ "at": "1s", "device": "th", "do": "leave" }"#
        )),
        "scenario: s.json: events[0].do: unknown action \"leave\" (join or report)"
    );
}

#[test]
fn the_time_of_an_event_is_hours_minutes_seconds_in_that_order_up_to_24_hours() {
    assert_eq!(parse_at("1m30s"), Ok(90_000));
    assert_eq!(parse_at("2h"), Ok(7_200_000));
    assert_eq!(parse_at("1h1m1s"), Ok(3_661_000));
    assert_eq!(parse_at("24h"), Ok(86_400_000));
    assert!(
        parse_at("2x")
            .unwrap_err()
            .starts_with("invalid time \"2x\"")
    );
    assert_eq!(parse_at("25h"), Err("\"25h\" is over 24 h".to_string()));
    assert!(parse_at("30s1m").is_err());
    assert!(parse_at("1m1m").is_err());
    assert!(parse_at("").is_err());
    assert!(parse_at("m").is_err());
    assert!(parse_at("1.5s").is_err());
    // The same errors reach the file's line.
    assert_eq!(
        error(&with_event(
            r#"{ "at": "30s1m", "device": "th", "do": "join" }"#
        )),
        format!(
            "scenario: s.json: events[0].at: {}",
            parse_at("30s1m").unwrap_err()
        )
    );
}

#[test]
fn a_field_not_allowed_for_the_kind_or_out_of_range_names_the_field() {
    assert_eq!(
        error(&with_event(
            r#"{ "at": "1s", "device": "relay", "do": "report", "temperature": 20 }"#
        )),
        "scenario: s.json: events[0].temperature: not allowed for kind relay"
    );
    assert_eq!(
        error(&with_event(
            r#"{ "at": "1s", "device": "th", "do": "report", "battery": 101 }"#
        )),
        "scenario: s.json: events[0].battery: must be a whole number from 0 to 100"
    );
    assert_eq!(
        error(&with_event(
            r#"{ "at": "1s", "device": "th", "do": "report", "humidity": 120 }"#
        )),
        "scenario: s.json: events[0].humidity: must be a number from 0 to 100"
    );
    assert_eq!(
        error(&with_event(
            r#"{ "at": "1s", "device": "relay", "do": "report", "light_state": "dim" }"#
        )),
        "scenario: s.json: events[0].light_state: must be \"on\" or \"off\""
    );
    assert_eq!(
        error(&with_event(
            r#"{ "at": "1s", "device": "th", "do": "report" }"#
        )),
        "scenario: s.json: events[0].do: a report needs at least one measure"
    );
    assert_eq!(
        error(&with_event(
            r#"{ "at": "1s", "device": "th", "do": "join", "battery": 5 }"#
        )),
        "scenario: s.json: events[0].battery: not allowed with \"join\""
    );
    assert_eq!(
        error(r#"{ "radio": { "ezsp": 12 } }"#),
        "scenario: s.json: radio.ezsp: must be a whole number from 13 to 19"
    );
    assert_eq!(
        error(r#"{ "radio": { "eui64": "8C6FB9FFFE12345" } }"#),
        "scenario: s.json: radio.eui64: must be 16 hexadecimal digits, got \"8C6FB9FFFE12345\""
    );
    assert_eq!(
        error(r#"{ "devices": [ { "id": "a", "kind": "motion", "eui64": "00124B000000000" } ] }"#),
        "scenario: s.json: devices[0].eui64: must be 16 hexadecimal digits, got \"00124B000000000\""
    );
    assert_eq!(
        error(r#"{ "radio": { "network": "maybe" } }"#),
        "scenario: s.json: radio.network: must be \"none\" or \"formed\""
    );
}

#[test]
fn too_many_devices_or_events_or_a_file_over_one_megabyte_name_the_bound() {
    let device = |i: usize| {
        format!(
            r#"{{ "id": "d{i}", "kind": "motion", "eui64": "{:016X}" }}"#,
            i + 1
        )
    };
    let devices = |n: usize| (0..n).map(device).collect::<Vec<_>>().join(",");
    let json = format!(r#"{{ "devices": [ {} ] }}"#, devices(MAX_DEVICES));
    assert_eq!(
        Scenario::parse(&json, "s.json").unwrap().devices.len(),
        MAX_DEVICES
    );
    let json = format!(r#"{{ "devices": [ {} ] }}"#, devices(MAX_DEVICES + 1));
    assert_eq!(
        error(&json),
        "scenario: s.json: devices[64]: too many devices (at most 64)"
    );

    let events = |n: usize| vec![r#"{ "at": "1s", "device": "d0", "do": "join" }"#; n].join(",");
    let json = |n: usize| {
        format!(
            r#"{{ "devices": [ {} ], "events": [ {} ] }}"#,
            device(0),
            events(n)
        )
    };
    assert_eq!(
        Scenario::parse(&json(MAX_EVENTS), "s.json")
            .unwrap()
            .events
            .len(),
        MAX_EVENTS
    );
    assert_eq!(
        error(&json(MAX_EVENTS + 1)),
        "scenario: s.json: events[10000]: too many events (at most 10000)"
    );

    // A file over 1 MB is refused before it is read: its content is never looked at.
    let dir = scratch();
    fs::create_dir_all(&dir).unwrap();
    let file = dir.join("big.json");
    fs::write(&file, vec![b' '; 1_048_577]).unwrap();
    let message = Scenario::load(&file).unwrap_err();
    assert_eq!(
        message,
        format!("scenario: {}: file is over 1 MB", file.display())
    );
    fs::write(&file, vec![b' '; 1_048_576]).unwrap();
    assert!(
        Scenario::load(&file)
            .unwrap_err()
            .contains("line 1, column"),
        "one MB exactly is read (and is not a scenario)"
    );
    let missing = dir.join("missing.json");
    let message = Scenario::load(&missing).unwrap_err();
    assert!(
        message.starts_with(&format!("scenario: {}: ", missing.display())),
        "{message}"
    );
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn events_written_out_of_order_run_sorted_and_ties_keep_the_file_order() {
    let json = with_event("").replace(
        "[  ]",
        r#"[
          { "at": "30s", "device": "th", "do": "report", "temperature": 3 },
          { "at": "10s", "device": "th", "do": "report", "temperature": 1 },
          { "at": "10s", "device": "th", "do": "report", "temperature": 2 },
          { "at": "1m", "device": "th", "do": "report", "temperature": 4 }
        ]"#,
    );
    let scenario = Scenario::parse(&json, "s.json").unwrap();
    let order: Vec<(u64, f64)> = scenario
        .events
        .iter()
        .map(|e| match &e.action {
            Action::Report(r) => (e.at_ms, r.temperature.unwrap()),
            Action::Join => panic!("no join in this scenario"),
        })
        .collect();
    assert_eq!(
        order,
        vec![(10_000, 1.0), (10_000, 2.0), (30_000, 3.0), (60_000, 4.0)]
    );
}

#[test]
fn every_file_in_scenarios_parses() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../scenarios");
    let mut count = 0;
    for entry in fs::read_dir(dir).expect("the scenarios folder exists") {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "json") {
            Scenario::load(&path).unwrap_or_else(|e| panic!("{e}"));
            count += 1;
        }
    }
    assert!(count >= 2, "snzb02.json and garage.json");
}

#[test]
fn scenario_time_at_speed_10_is_reached_at_a_tenth_of_the_clock_time() {
    let time = ScenarioClock { speed: 10 };
    // 40 s of scenario time is due at 4 s of clock time, never before.
    assert_eq!(time.clock_ms(40_000), 4_000);
    assert!(time.scenario_ms(3_999) < 40_000);
    assert_eq!(time.scenario_ms(4_000), 40_000);
    assert_eq!(ScenarioClock { speed: 1 }.clock_ms(40_000), 40_000);
    assert_eq!(time.clock_ms(1), 1);
}

#[test]
fn an_identity_over_32_characters_or_a_top_level_field_of_the_wrong_type_names_the_field() {
    let long = "x".repeat(33);
    let device = |field: &str, value: &str| {
        format!(
            r#"{{ "devices": [ {{ "id": "a", "kind": "motion", "eui64": "00124B0000000001", "{field}": "{value}" }} ] }}"#
        )
    };
    assert_eq!(
        error(&device("manufacturer", &long)),
        "scenario: s.json: devices[0].manufacturer: at most 32 characters"
    );
    assert_eq!(
        error(&device("model", &long)),
        "scenario: s.json: devices[0].model: at most 32 characters"
    );
    assert!(Scenario::parse(&device("model", &"x".repeat(32)), "s.json").is_ok());
    assert_eq!(
        error(r#"{"devices": {"a":1}}"#),
        "scenario: s.json: devices: must be a list"
    );
    assert_eq!(
        error(r#"{"events": 3}"#),
        "scenario: s.json: events: must be a list"
    );
    assert_eq!(
        error(r#"{"radio": []}"#),
        "scenario: s.json: radio: must be an object"
    );
}
