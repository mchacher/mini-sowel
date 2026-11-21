//! `motion-light` (spec 012, R3): its parameters and `validate`'s messages, the motion sources,
//! and the state machine driven through the home as the radio loop drives it, with a fake time.
//! "Light ON" is the relay's `state` reported by the test, as the device's echo would be.

mod support;

use mini_sowel_core::home::events::EventValue;
use mini_sowel_core::radio::zcl::Command;
use mini_sowel_core::recipes::motion_light::{info, is_motion_active, resolve_motion_sources};
use serde_json::{Value, json};
use support::*;

const SECOND: u64 = 1000;

// ---- Definition, motion sources ----

#[test]
fn the_recipe_info_is_the_json_of_the_reference() {
    let expected: Value = serde_json::from_str(include_str!("recipe_info.json")).unwrap();
    assert_eq!(info(), expected);
    let mut rig = Rig::new();
    let (status, list) = rig.get("/api/v1/recipes");
    assert_eq!(status, 200);
    assert_eq!(list, json!([expected.clone()]));
}

#[test]
fn motion_is_a_true_a_non_zero_number_or_one_of_five_words() {
    let yes = [
        EventValue::Bool(true),
        EventValue::Number(1.0),
        EventValue::Number(5.0),
        EventValue::Text("true".into()),
        EventValue::Text("TRUE".into()),
        EventValue::Text("on".into()),
        EventValue::Text("On".into()),
        EventValue::Text("1".into()),
        EventValue::Text("occupied".into()),
        EventValue::Text("detected".into()),
        EventValue::Text(" detected ".into()),
    ];
    for value in yes {
        assert!(is_motion_active(&value), "{value:?}");
    }
    let no = [
        EventValue::Bool(false),
        EventValue::Number(0.0),
        EventValue::Text("false".into()),
        EventValue::Text("off".into()),
        EventValue::Text("0".into()),
        EventValue::Text("clear".into()),
        EventValue::Text("".into()),
        EventValue::Null,
    ];
    for value in no {
        assert!(!is_motion_active(&value), "{value:?}");
    }
    // A JSON value is cut and converted the same way.
    assert!(is_motion_active(&EventValue::from_json(&json!("Occupied"))));
    assert!(!is_motion_active(&EventValue::from_json(&json!(null))));
}

#[test]
fn a_motion_source_is_a_device_and_a_key_once_and_two_devices_are_two_sources() {
    let mut rig = Rig::new();
    let g = rig.garage();
    let second = rig.motion_sensor(0x51);
    // Two equipments on the same sensor: the same (device, key).
    rig.equipment("Again", "sensor", &g.zone, &[&g.motion_device]);
    rig.equipment("Other", "sensor", &g.zone, &[&second]);
    let zone = mini_sowel_core::devices::Id::parse(&g.zone).unwrap();
    let sources = resolve_motion_sources(&rig.home, &zone);
    assert_eq!(sources.len(), 2);
    let keys: Vec<&str> = sources.iter().map(|(_, k)| &**k).collect();
    assert_eq!(keys, ["occupancy", "occupancy"]);
    assert_ne!(sources[0].0, sources[1].0);
}

#[test]
fn the_motion_sources_of_a_zone_include_a_child_zone_and_skip_other_data_and_disabled_equipments() {
    let mut rig = Rig::new();
    let g = rig.garage(); // the zone's own motion sensor (1 source)
    let (status, shelf) = rig.post(
        "/api/v1/zones",
        json!({"name": "Shelf", "parentId": g.zone}),
    );
    assert_eq!(status, 201);
    let shelf = shelf["id"].as_str().unwrap().to_string();
    let child = rig.motion_sensor(0x51);
    rig.equipment("Shelf motion", "sensor", &shelf, &[&child]);
    // A zone outside the tree does not count.
    let other_zone = rig.zone("Elsewhere");
    let outside = rig.motion_sensor(0x52);
    rig.equipment("Outside", "sensor", &other_zone, &[&outside]);
    // A sensor with temperature only: no motion binding.
    let climate = rig.sensor(0x53);
    rig.equipment("Climate", "sensor", &g.zone, &[&climate]);
    // A disabled equipment is skipped.
    let disabled = rig.motion_sensor(0x54);
    let disabled_eq = rig.equipment("Disabled", "sensor", &g.zone, &[&disabled]);
    let (status, _) = rig.put(
        &format!("/api/v1/equipments/{disabled_eq}"),
        json!({"enabled": false}),
    );
    assert_eq!(status, 200);
    // And a second motion sensor in the zone itself: 3 sources in all.
    let more = rig.motion_sensor(0x55);
    rig.equipment("More", "sensor", &g.zone, &[&more]);
    let zone = mini_sowel_core::devices::Id::parse(&g.zone).unwrap();
    let sources = resolve_motion_sources(&rig.home, &zone);
    assert_eq!(sources.len(), 3);
}

// ---- validate ----

fn refused(rig: &mut Rig, params: Value) -> (u16, String) {
    let (status, body) = rig.instance(params);
    (status, body["error"].as_str().unwrap_or("").to_string())
}

fn invalid(message: &str) -> (u16, String) {
    (400, format!("Invalid params: {message}"))
}

#[test]
fn every_validate_message_of_the_reference_in_its_order() {
    let mut rig = Rig::new();
    let g = rig.garage();
    let other_zone = rig.zone("Elsewhere");
    let dimmable = {
        let e = eui(0x61);
        rig.home.devices.announced(e, 0x8C, rig.now);
        rig.home
            .devices
            .identified(e, Some("T"), Some("DIM"), rig.now, &mut *rig.random)
            .unwrap();
        rig.home
            .devices
            .reporting_configured(e, &[0x0006, 0x0008], rig.now, &mut *rig.random)
            .unwrap();
        rig.device(0x61)
    };
    let lamp = rig.equipment("Lamp", "light_dimmable", &g.zone, &[&dimmable]);
    let relay2 = rig.relay(0x62);
    let far = rig.equipment("Far light", "light_onoff", &other_zone, &[&relay2]);
    let relay3 = rig.relay(0x63);
    let no_state = rig.equipment("Bare light", "light_onoff", &g.zone, &[]);
    let _ = relay3;
    let ghost = "11111111-2222-4333-8444-555555555555";
    let zone = &g.zone;
    let light = &g.light;

    let cases: Vec<(Value, String)> = vec![
        (json!({}), "Zone parameter is required".into()),
        (json!({"zone": ""}), "Zone parameter is required".into()),
        (json!({"zone": 5}), "Zone parameter is required".into()),
        (json!({"zone": ghost}), format!("Zone not found: {ghost}")),
        (
            json!({"zone": zone}),
            "At least one light is required".into(),
        ),
        (
            json!({"zone": zone, "lights": []}),
            "At least one light is required".into(),
        ),
        (
            json!({"zone": zone, "lights": [ghost]}),
            format!("Light equipment not found: {ghost}"),
        ),
        (
            json!({"zone": zone, "lights": [lamp]}),
            "Light \"Lamp\" is type \"light_dimmable\" but this recipe requires light_onoff".into(),
        ),
        (
            json!({"zone": zone, "lights": [far]}),
            "Light \"Far light\" does not belong to the selected zone".into(),
        ),
        (
            json!({"zone": zone, "lights": [no_state]}),
            "Light \"Bare light\" has no \"state\" order binding".into(),
        ),
        (
            json!({"zone": zone, "lights": [light], "timeout": "10x"}),
            "Invalid duration format: 10x. Use e.g. \"10m\", \"30s\", \"1h\"".into(),
        ),
        (
            json!({"zone": zone, "lights": [light], "timeout": true}),
            "Invalid duration: true".into(),
        ),
        (
            json!({"zone": zone, "lights": [light], "luxThreshold": "abc"}),
            "luxThreshold must be a non-negative number".into(),
        ),
        (
            json!({"zone": zone, "lights": [light], "luxThreshold": -1}),
            "luxThreshold must be a non-negative number".into(),
        ),
        (
            json!({"zone": zone, "lights": [light], "maxOnDuration": "soon"}),
            "Invalid duration format: soon. Use e.g. \"10m\", \"30s\", \"1h\"".into(),
        ),
        (
            json!({"zone": zone, "lights": [light], "buttons": [ghost]}),
            format!("Button equipment not found: {ghost}"),
        ),
        (
            json!({"zone": zone, "lights": [light], "buttons": [g.motion_equipment]}),
            "Button \"Garage motion\" has no \"action\" data binding".into(),
        ),
    ];
    for (params, message) in cases {
        assert_eq!(
            refused(&mut rig, params.clone()),
            invalid(&message),
            "{params}"
        );
    }
    // The order of the checks: the first failure wins.
    assert_eq!(
        refused(
            &mut rig,
            json!({"zone": zone, "lights": [light], "timeout": "x", "luxThreshold": "y", "maxOnDuration": "z", "buttons": [ghost]})
        ),
        invalid("Invalid duration format: x. Use e.g. \"10m\", \"30s\", \"1h\"")
    );
}

#[test]
fn the_legacy_light_is_used_only_when_lights_is_not_a_list_and_a_comma_string_is_not_split() {
    let mut rig = Rig::new();
    let g = rig.garage();
    let (status, body) = rig.instance(json!({"zone": g.zone, "light": g.light}));
    assert_eq!(status, 201, "{body}");
    // `lights` a string, even a comma one: not a list, so `light` is looked at, and it is absent.
    let both = format!("{},{}", g.light, g.light);
    assert_eq!(
        refused(&mut rig, json!({"zone": g.zone, "lights": both})),
        invalid("At least one light is required")
    );
    // A list wins over the legacy `light`, and what is not a string is dropped.
    let ghost = "11111111-2222-4333-8444-555555555555";
    assert_eq!(
        refused(
            &mut rig,
            json!({"zone": g.zone, "lights": [3, null], "light": g.light})
        ),
        invalid("At least one light is required")
    );
    assert_eq!(
        refused(
            &mut rig,
            json!({"zone": g.zone, "lights": [ghost], "light": g.light})
        ),
        invalid(&format!("Light equipment not found: {ghost}"))
    );
}

#[test]
fn a_zone_without_a_motion_sensor_logs_a_warning_at_every_start() {
    let mut rig = Rig::new();
    let zone = rig.zone("Blind");
    let relay = rig.relay(RELAY);
    let light = rig.equipment("Blind light", "light_onoff", &zone, &[&relay]);
    let (status, body) = rig.instance(json!({"zone": zone, "lights": [light], "timeout": "20s"}));
    assert_eq!(status, 201, "{body}");
    let id = body["id"].as_str().unwrap().to_string();
    let warning = "Zone has no motion sensors — recipe will never trigger";
    assert_eq!(
        rig.messages(&id),
        [
            "Instance created",
            warning,
            "Recipe activated — no motion, lights off"
        ]
    );
    let (_, log) = rig.get(&format!("/api/v1/recipe-instances/{id}/log"));
    assert_eq!(log[1]["level"], "warn");
    // And again each time it starts.
    rig.post(&format!("/api/v1/recipe-instances/{id}/disable"), json!({}));
    rig.post(&format!("/api/v1/recipe-instances/{id}/enable"), json!({}));
    let messages = rig.messages(&id);
    assert_eq!(messages.iter().filter(|m| *m == warning).count(), 2);
}

// ---- The machine ----

#[test]
fn a_start_with_no_motion_sends_off_and_a_start_with_motion_sends_on() {
    let mut rig = Rig::new();
    let g = rig.garage();
    let id = rig.garage_instance(&g, json!({}));
    assert_eq!(rig.sent(), [Command::Off]);
    assert_eq!(
        rig.messages(&id),
        [
            "Instance created",
            "Recipe activated — no motion, lights off"
        ]
    );

    let mut rig = Rig::new();
    let g = rig.garage();
    rig.report(&motion(PIR, true));
    let id = rig.garage_instance(&g, json!({}));
    assert_eq!(rig.sent(), [Command::On]);
    assert_eq!(
        rig.messages(&id),
        ["Instance created", "Motion detected — 1 light(s) turned on"]
    );
}

#[test]
fn motion_switches_on_and_the_timeout_without_motion_switches_off() {
    let mut rig = Rig::new();
    let g = rig.garage();
    let id = rig.garage_instance(&g, json!({}));
    assert_eq!(rig.sent(), [Command::Off]);
    rig.advance(10 * SECOND);
    rig.report(&motion(PIR, true));
    assert_eq!(rig.sent(), [Command::On]);
    // The echo of the light: nothing to do.
    rig.report(&state(RELAY, true));
    assert_eq!(rig.sent(), []);
    rig.advance(SECOND);
    rig.report(&motion(PIR, false));
    // The timer is armed and visible in the state.
    let state_keys = rig.state(&id);
    assert!(state_keys["timerExpiresAt"].is_string(), "{state_keys}");
    rig.advance(19_999);
    assert_eq!(rig.sent(), []);
    rig.advance(1);
    assert_eq!(rig.sent(), [Command::Off]);
    assert_eq!(
        rig.messages(&id)[2..],
        [
            "Motion detected — 1 light(s) turned on",
            "No motion for 20s — lights turned off"
        ]
    );
    assert_eq!(rig.state(&id), json!({}));
}

#[test]
fn motion_back_before_the_timeout_cancels_the_timer_and_the_light_stays_on() {
    let mut rig = Rig::new();
    let g = rig.garage();
    let id = rig.garage_instance(&g, json!({}));
    rig.sent();
    rig.report(&motion(PIR, true));
    rig.report(&state(RELAY, true));
    rig.sent();
    rig.advance(SECOND);
    rig.report(&motion(PIR, false));
    rig.advance(10 * SECOND);
    rig.report(&motion(PIR, true));
    assert_eq!(rig.state(&id), json!({}), "the timer is gone");
    rig.advance(60 * SECOND);
    assert_eq!(rig.sent(), []);
}

#[test]
fn an_external_on_with_no_motion_starts_the_countdown_and_a_heartbeat_changes_nothing() {
    let mut rig = Rig::new();
    let g = rig.garage();
    let id = rig.garage_instance(&g, json!({"timeout": "2m"}));
    rig.sent();
    rig.report(&state(RELAY, true));
    assert_eq!(
        rig.messages(&id).last().unwrap(),
        "Light turned on externally — turning off in 2min"
    );
    rig.advance(60 * SECOND);
    rig.report(&state(RELAY, true)); // the periodic re-report
    assert_eq!(rig.sent(), []);
    rig.advance(59_999);
    assert_eq!(rig.sent(), []);
    rig.advance(1);
    assert_eq!(rig.sent(), [Command::Off]);
    assert_eq!(
        rig.messages(&id).last().unwrap(),
        "No motion for 2min — lights turned off"
    );
}

#[test]
fn after_the_timer_a_new_external_on_starts_a_fresh_countdown() {
    let mut rig = Rig::new();
    let g = rig.garage();
    let id = rig.garage_instance(&g, json!({"timeout": "2m"}));
    rig.sent();
    rig.report(&state(RELAY, true));
    rig.advance(120 * SECOND);
    assert_eq!(rig.sent(), [Command::Off]);
    rig.advance(SECOND);
    rig.report(&state(RELAY, false)); // the echo of the recipe's OFF
    rig.advance(10 * SECOND);
    rig.report(&state(RELAY, true)); // someone switches it on
    rig.advance(60 * SECOND);
    rig.report(&state(RELAY, true)); // heartbeat
    assert_eq!(rig.sent(), []);
    rig.advance(59_999);
    assert_eq!(rig.sent(), []);
    rig.advance(1);
    assert_eq!(rig.sent(), [Command::Off]);
    let off_lines = rig
        .messages(&id)
        .iter()
        .filter(|m| m.starts_with("No motion for 2min"))
        .count();
    assert_eq!(off_lines, 2);
}

#[test]
fn an_empty_lux_threshold_means_none() {
    let mut rig = Rig::new();
    let g = rig.garage();
    rig.garage_instance(&g, json!({"luxThreshold": ""}));
    rig.sent();
    rig.report(&luminosity(PIR, 1.0));
    rig.report(&motion(PIR, true));
    assert_eq!(rig.sent(), [Command::On]);
}

#[test]
fn above_the_lux_threshold_the_light_stays_off_with_its_line_and_at_the_threshold_it_goes_on() {
    let mut rig = Rig::new();
    let g = rig.garage();
    let id = rig.garage_instance(&g, json!({"luxThreshold": 50}));
    rig.sent();
    rig.report(&luminosity(PIR, 120.0));
    rig.report(&motion(PIR, true));
    assert_eq!(rig.sent(), []);
    assert_eq!(
        rig.messages(&id).last().unwrap(),
        "Motion detected but luminosity 120 exceeds threshold 50 — not turning on"
    );
    // Exactly at the threshold: allowed.
    rig.report(&luminosity(PIR, 50.0));
    assert_eq!(rig.sent(), [Command::On]);
}

#[test]
fn the_light_goes_off_only_above_the_threshold_plus_ten_percent() {
    let mut rig = Rig::new();
    let g = rig.garage();
    let id = rig.garage_instance(&g, json!({"luxThreshold": 50}));
    rig.sent();
    rig.report(&luminosity(PIR, 40.0));
    rig.report(&motion(PIR, true));
    assert_eq!(rig.sent(), [Command::On]);
    rig.report(&state(RELAY, true));
    rig.report(&luminosity(PIR, 55.0)); // 50 x 1.1: stays on
    assert_eq!(rig.sent(), []);
    rig.report(&luminosity(PIR, 56.0));
    assert_eq!(rig.sent(), [Command::Off]);
    assert_eq!(
        rig.messages(&id).last().unwrap(),
        "Luminosity above threshold — lights turned off"
    );
}

#[test]
fn a_light_switched_on_by_hand_above_the_hysteresis_is_an_override() {
    let mut rig = Rig::new();
    let g = rig.garage();
    let id = rig.garage_instance(&g, json!({"luxThreshold": 50}));
    rig.sent();
    rig.report(&luminosity(PIR, 40.0));
    rig.advance(5 * SECOND);
    rig.report(&state(RELAY, true));
    rig.report(&luminosity(PIR, 60.0));
    assert_eq!(
        rig.messages(&id).last().unwrap(),
        "Light turned on manually above lux threshold — entering override mode"
    );
    assert_eq!(rig.state(&id)["overrideMode"], true);
    assert_eq!(rig.sent(), [], "the light is left alone");
}

#[test]
fn a_light_switched_off_by_hand_during_motion_is_an_override_cleared_after_the_timeout() {
    let mut rig = Rig::new();
    let g = rig.garage();
    let id = rig.garage_instance(&g, json!({}));
    rig.sent();
    rig.report(&motion(PIR, true));
    rig.report(&state(RELAY, true));
    rig.sent();
    rig.advance(5 * SECOND);
    rig.report(&state(RELAY, false));
    // The light being off changes the zone's aggregate (`lightsOn`): its event comes with the
    // motion still on, and is ignored (as Sowel does).
    let messages = rig.messages(&id);
    assert_eq!(
        messages[messages.len() - 2],
        "Light turned off manually while motion active — entering override mode"
    );
    assert_eq!(
        messages[messages.len() - 1],
        "Motion detected but override mode active — ignoring"
    );
    assert_eq!(rig.state(&id)["overrideMode"], true);
    // Motion goes on: ignored, and the timer waits.
    rig.report(&motion(PIR, false));
    rig.report(&motion(PIR, true));
    assert_eq!(
        rig.messages(&id).last().unwrap(),
        "Motion detected but override mode active — ignoring"
    );
    assert_eq!(rig.sent(), []);
    // Motion ends: after the timeout the override is cleared (the light is off already).
    rig.report(&motion(PIR, false));
    rig.advance(19_999);
    assert_eq!(rig.state(&id)["overrideMode"], true);
    rig.advance(1);
    assert_eq!(
        rig.messages(&id).last().unwrap(),
        "No motion for 20s — override cleared, lights off"
    );
    assert_eq!(rig.state(&id), json!({}));
    assert_eq!(rig.sent(), []);
    // The recipe works again.
    rig.advance(5 * SECOND);
    rig.report(&motion(PIR, true));
    assert_eq!(rig.sent(), [Command::On]);
}

#[test]
fn the_override_clear_switches_a_light_that_is_on_off() {
    let mut rig = Rig::new();
    let g = rig.garage();
    let id = rig.garage_instance(&g, json!({}));
    rig.sent();
    // Override through the lux path leaves the light on: use a button instead (below); here a
    // light on by hand during an override started by a manual off, then switched on again.
    rig.report(&motion(PIR, true));
    rig.report(&state(RELAY, true));
    rig.sent();
    rig.advance(5 * SECOND);
    rig.report(&state(RELAY, false)); // override
    rig.report(&state(RELAY, true)); // on again, tracked only
    rig.report(&motion(PIR, false));
    rig.advance(20 * SECOND);
    assert_eq!(rig.sent(), [Command::Off]);
    assert_eq!(
        rig.messages(&id).last().unwrap(),
        "No motion for 20s — override cleared, lights off"
    );
}

#[test]
fn the_echo_of_the_recipes_own_off_is_not_a_manual_change_and_motion_in_its_two_seconds_is_ignored()
{
    let mut rig = Rig::new();
    let g = rig.garage();
    let id = rig.garage_instance(&g, json!({}));
    rig.sent();
    rig.report(&motion(PIR, true));
    rig.report(&state(RELAY, true));
    assert_eq!(rig.sent(), [Command::On]);
    rig.report(&motion(PIR, false));
    rig.advance(20 * SECOND);
    assert_eq!(rig.sent(), [Command::Off]);
    rig.advance(500);
    rig.report(&state(RELAY, false)); // the echo
    assert_eq!(rig.state(&id), json!({}), "no override");
    // A motion within the 2 s grace is not acted on.
    rig.advance(500);
    rig.report(&motion(PIR, true));
    assert_eq!(rig.sent(), []);
    assert_eq!(rig.state(&id), json!({}));
    // Past the grace, a motion is.
    rig.report(&motion(PIR, false));
    rig.advance(2 * SECOND);
    rig.report(&motion(PIR, true));
    assert_eq!(rig.sent(), [Command::On]);
}

#[test]
fn the_stale_race_of_the_reference_is_kept_a_zone_event_before_the_lights_echo_is_a_manual_off() {
    let mut rig = Rig::new();
    let g = rig.garage();
    let id = rig.garage_instance(&g, json!({}));
    rig.sent();
    rig.report(&motion(PIR, true));
    assert_eq!(rig.sent(), [Command::On]);
    // The light has not reported yet: another change of the zone arrives first.
    rig.report(&luminosity(PIR, 30.0));
    assert_eq!(
        rig.messages(&id).last().unwrap(),
        "Light turned off manually while motion active — entering override mode"
    );
    assert_eq!(rig.state(&id)["overrideMode"], true);
}

/// A button equipment in the garage zone whose `action` is the temperature of a sensor (the test
/// reports a temperature to press it).
fn button(rig: &mut Rig, g: &Garage) -> String {
    let sensor = rig.sensor(0x60);
    let data = rig.home.devices.get(&sensor).unwrap().data[0]
        .id
        .to_string();
    let id = rig.equipment("Button", "button", &g.zone, &[]);
    let (status, body) = rig.post(
        &format!("/api/v1/equipments/{id}/data-bindings"),
        json!({"deviceDataId": data, "alias": "action"}),
    );
    assert_eq!(status, 201, "{body}");
    id
}

fn press(rig: &mut Rig) {
    rig.report(&temperature(0x60, 20.0));
}

#[test]
fn a_button_press_switches_a_light_that_is_on_off_with_an_override_and_one_that_is_off_on() {
    let mut rig = Rig::new();
    let g = rig.garage();
    let b = button(&mut rig, &g);
    let id = rig.garage_instance(&g, json!({"buttons": [b]}));
    rig.sent();
    // Light off: the press turns it on, whatever the motion.
    press(&mut rig);
    assert_eq!(rig.sent(), [Command::On]);
    assert_eq!(
        rig.messages(&id).last().unwrap(),
        "Motion detected — 1 light(s) turned on"
    );
    rig.report(&state(RELAY, true));
    // Light on: the press turns it off and enters the override mode.
    rig.advance(5 * SECOND);
    press(&mut rig);
    assert_eq!(rig.sent(), [Command::Off]);
    assert_eq!(
        rig.messages(&id).last().unwrap(),
        "Button pressed — lights off, entering override mode"
    );
    assert_eq!(rig.state(&id)["overrideMode"], true);
    // And the override clears after the timeout.
    rig.advance(20 * SECOND);
    assert_eq!(rig.state(&id), json!({}));
}

#[test]
fn the_failsafe_fires_after_the_max_on_duration_and_motion_impulses_keep_it_from_firing() {
    let mut rig = Rig::new();
    let g = rig.garage();
    let id = rig.garage_instance(&g, json!({"maxOnDuration": "5m"}));
    rig.sent();
    rig.report(&motion(PIR, true));
    rig.report(&state(RELAY, true));
    rig.sent();
    assert!(rig.state(&id)["failsafeExpiresAt"].is_string());
    // Impulses every 4 minutes (a raw report of the sensor, changed or not).
    for _ in 0..3 {
        rig.advance(4 * 60 * SECOND);
        assert_eq!(rig.sent(), []);
        rig.report(&motion(PIR, true));
    }
    // No more: it fires 5 minutes after the last.
    rig.advance(5 * 60 * SECOND - 1);
    assert_eq!(rig.sent(), []);
    rig.advance(1);
    assert_eq!(rig.sent(), [Command::Off]);
    let (_, log) = rig.get(&format!("/api/v1/recipe-instances/{id}/log?limit=1"));
    assert_eq!(
        log[0]["message"],
        "Failsafe: lights forced off after 5min max on duration"
    );
    assert_eq!(log[0]["level"], "warn");
    assert_eq!(rig.state(&id), json!({}));
}

#[test]
fn the_zones_motion_restarts_the_failsafe_too() {
    let mut rig = Rig::new();
    let g = rig.garage();
    rig.garage_instance(&g, json!({"maxOnDuration": "5m"}));
    rig.sent();
    rig.report(&motion(PIR, true));
    rig.report(&state(RELAY, true));
    rig.sent();
    rig.advance(4 * 60 * SECOND);
    // Motion ends and comes back: the zone events reset the failsafe.
    rig.report(&motion(PIR, false));
    rig.report(&motion(PIR, true));
    rig.sent();
    rig.advance(4 * 60 * SECOND);
    assert_eq!(rig.sent(), [], "reset 4 minutes ago");
    rig.advance(60 * SECOND);
    assert_eq!(rig.sent(), [Command::Off]);
}

#[test]
fn a_timer_of_more_than_24_days_fires_at_its_time_not_at_once() {
    let mut rig = Rig::new();
    let g = rig.garage();
    rig.report(&luminosity(PIR, 5.0));
    let id = rig.garage_instance(&g, json!({"maxOnDuration": "700h"}));
    rig.sent();
    rig.report(&motion(PIR, true));
    rig.report(&state(RELAY, true));
    rig.sent();
    // The devices keep reporting (a silent one would go offline), without any motion impulse.
    for _ in 0..699 {
        rig.advance(3600 * SECOND);
        rig.report(&state(RELAY, true));
        rig.report(&luminosity(PIR, 5.0));
        assert_eq!(rig.sent(), []);
    }
    assert!(rig.state(&id)["failsafeExpiresAt"].is_string());
    rig.advance(3600 * SECOND);
    assert_eq!(rig.sent(), [Command::Off]);
    assert_eq!(
        rig.messages(&id).last().unwrap(),
        "Failsafe: lights forced off after 700h max on duration"
    );
}

#[test]
fn disable_when_daylight_has_no_effect_because_the_daylight_is_unknown() {
    let mut rig = Rig::new();
    let g = rig.garage();
    let id = rig.garage_instance(&g, json!({"disableWhenDaylight": true}));
    rig.sent();
    rig.report(&motion(PIR, true));
    assert_eq!(rig.sent(), [Command::On]);
    assert!(!rig.messages(&id).iter().any(|m| m.contains("daytime")));
}

#[test]
fn stop_clears_the_three_state_keys_and_sends_no_order_and_enabling_syncs_again() {
    let mut rig = Rig::new();
    let g = rig.garage();
    let id = rig.garage_instance(&g, json!({"luxThreshold": 50, "maxOnDuration": "5m"}));
    rig.sent();
    rig.report(&luminosity(PIR, 40.0));
    rig.report(&state(RELAY, true)); // by hand: both timers
    rig.report(&luminosity(PIR, 60.0)); // override
    let keys = rig.state(&id);
    assert_eq!(keys["overrideMode"], true);
    assert!(keys["timerExpiresAt"].is_string() && keys["failsafeExpiresAt"].is_string());
    let (status, body) = rig.post(&format!("/api/v1/recipe-instances/{id}/disable"), json!({}));
    assert_eq!((status, body), (200, json!({"success": true})));
    assert_eq!(rig.state(&id), json!({}));
    assert_eq!(rig.sent(), [], "the light is not switched off");
    // Enabling starts again: the light is on with no motion, so the sync switches it off.
    let (status, _) = rig.post(&format!("/api/v1/recipe-instances/{id}/enable"), json!({}));
    assert_eq!(status, 200);
    assert_eq!(rig.sent(), [Command::Off]);
    let activated = rig
        .messages(&id)
        .iter()
        .filter(|m| *m == "Recipe activated — no motion, lights off")
        .count();
    assert_eq!(activated, 2);
}

#[test]
fn a_disabled_light_among_two_gets_no_order_and_no_log_line_but_counts_in_the_message() {
    let mut rig = Rig::new();
    let g = rig.garage();
    let relay2 = rig.relay(0x7B);
    let second = rig.equipment("Second light", "light_onoff", &g.zone, &[&relay2]);
    let (status, _) = rig.put(
        &format!("/api/v1/equipments/{second}"),
        json!({"enabled": false}),
    );
    assert_eq!(status, 200);
    let id = rig.garage_instance(&g, json!({"lights": [g.light, second]}));
    assert_eq!(rig.sent(), [Command::Off], "the enabled light only");
    rig.report(&motion(PIR, true));
    assert_eq!(rig.sent(), [Command::On]);
    assert_eq!(
        rig.messages(&id),
        [
            "Instance created",
            "Recipe activated — no motion, lights off",
            "Motion detected — 2 light(s) turned on"
        ]
    );
}

#[test]
fn a_light_deleted_while_running_is_reported_as_having_no_toggle_binding() {
    let mut rig = Rig::new();
    let g = rig.garage();
    let relay2 = rig.relay(0x7B);
    let second = rig.equipment("Second light", "light_onoff", &g.zone, &[&relay2]);
    let id = rig.garage_instance(&g, json!({"lights": [g.light, second]}));
    rig.sent();
    let (status, _) = rig.delete(&format!("/api/v1/equipments/{second}"));
    assert_eq!(status, 204);
    rig.report(&motion(PIR, true));
    assert_eq!(rig.sent(), [Command::On], "the other light is switched");
    let messages = rig.messages(&id);
    assert_eq!(
        messages[messages.len() - 2],
        format!("Error turning on some lights: {second}: no toggle order binding")
    );
    assert_eq!(
        messages[messages.len() - 1],
        "Motion detected — 2 light(s) turned on"
    );
    let (_, log) = rig.get(&format!("/api/v1/recipe-instances/{id}/log?limit=2"));
    assert_eq!(log[1]["level"], "error");
}

#[test]
fn the_lux_threshold_follows_javascripts_number() {
    let mut rig = Rig::new();
    let g = rig.garage();
    // Accepted: a number, a numeric string, a blank string (0), true (1), false (0).
    for value in [
        json!(50),
        json!("50"),
        json!(" "),
        json!(true),
        json!(false),
        json!(" 12.5 "),
    ] {
        let (status, body) = rig.instance(json!({
            "zone": g.zone, "lights": [g.light], "luxThreshold": value
        }));
        assert_eq!(status, 201, "{value}: {body}");
    }
    // Refused: text that is not a number.
    for value in [
        json!("abc"),
        json!("5 lux"),
        json!(-0.5),
        json!("-1"),
        json!([]),
        json!({}),
    ] {
        assert_eq!(
            refused(
                &mut rig,
                json!({"zone": g.zone, "lights": [g.light], "luxThreshold": value})
            ),
            invalid("luxThreshold must be a non-negative number"),
            "{value}"
        );
    }
    // No threshold: absent, null, "".
    for value in [json!(null), json!("")] {
        let (status, _) = rig.instance(json!({
            "zone": g.zone, "lights": [g.light], "luxThreshold": value
        }));
        assert_eq!(status, 201);
    }
    // The values behave: "50" is 50 (120 keeps the light off), " " is 0 (any light is too bright),
    // `true` is 1.
    for (value, lux, on) in [
        (json!("50"), 120.0, false),
        (json!("50"), 50.0, true),
        (json!(" "), 1.0, false),
        (json!(true), 1.0, true),
        (json!(true), 2.0, false),
    ] {
        let mut rig = Rig::new();
        let g = rig.garage();
        rig.garage_instance(&g, json!({"luxThreshold": value}));
        rig.sent();
        rig.report(&luminosity(PIR, lux));
        rig.report(&motion(PIR, true));
        assert_eq!(rig.sent() == [Command::On], on, "{value} {lux}");
    }
}
