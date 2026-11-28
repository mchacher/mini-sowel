//! `motion-light`, Sowel's recipe 1.1.2 as the script `recipes/scripts/motion-light.js` (spec 046,
//! R7; the former parity suite of spec 041, with the native recipe gone). The behaviour scenarios
//! play a home at the same times and keep the trace of each step: the orders sent, the instance's
//! state and the lines its log gained; each says what its trace must contain (so a scenario where
//! nothing happens cannot pass by emptiness). Then the recipe's info, Sowel's validation texts
//! (R3), the behaviours that are the engine's adaptations (R4), the instance bound (R5), a home
//! saved with instances whose recipe is not installed (R6) and a boot by day.
//!
//! These tests need the `script-recipes` feature (the host program turns it on, so
//! `cargo test --workspace` runs them).

mod mem_storage;
mod support;

use serde_json::{Value, json};
use support::*;

const SECOND: u64 = 1000;
const BASE: &str = "/api/v1/recipe-instances";

/// One run of a scenario: a home with the garage (and the script loaded), one instance of the
/// recipe being tested, and the trace of what each step did.
struct Run {
    rig: Rig,
    g: Garage,
    id: String,
    trace: Vec<String>,
    seen: usize,
}

impl Run {
    fn new(now: u64, located: bool) -> Run {
        let mut rig = Rig::new();
        rig.now = now;
        let g = rig.garage();
        rig.load_motion_light();
        if located {
            let (status, body) = rig.put(
                "/api/v1/settings",
                json!({
                    "home.latitude": "48.8566",
                    "home.longitude": "2.3522",
                    "home.timezone": "Europe/Paris",
                }),
            );
            assert_eq!(status, 200, "{body}");
        }
        Run {
            rig,
            g,
            id: String::new(),
            trace: Vec::new(),
            seen: 0,
        }
    }

    /// Creates the instance: the garage's zone and light, a timeout of 20 s, and `extra`.
    fn create(&mut self, extra: Value) -> (u16, Value) {
        let mut params = json!({
            "zone": self.g.zone,
            "lights": [self.g.light],
            "timeout": "20s",
        });
        if let (Some(params), Some(extra)) = (params.as_object_mut(), extra.as_object()) {
            for (key, value) in extra {
                params.insert(key.clone(), value.clone());
            }
        }
        self.rig
            .post(BASE, json!({"recipeId": "motion-light", "params": params}))
    }

    fn start(&mut self, extra: Value) {
        let (status, body) = self.create(extra);
        assert_eq!(status, 201, "{body}");
        self.id = body["id"].as_str().unwrap().to_string();
        self.step("start");
    }

    /// Records what the last step did.
    fn step(&mut self, label: &str) {
        let sent = self.rig.sent();
        let (state, new) = if self.id.is_empty() {
            ("null".to_string(), Vec::new())
        } else {
            let messages = self.rig.messages(&self.id);
            let from = self.seen.min(messages.len());
            self.seen = messages.len();
            (self.state_text(), messages[from..].to_vec())
        };
        self.trace.push(format!(
            "{label} | sent {sent:?} | state {state} | log {new:?}"
        ));
    }

    fn report(&mut self, label: String, event: &solite_core::radio::MeasureEvent) {
        self.rig.report(event);
        self.step(&label);
    }

    fn motion(&mut self, on: bool) {
        self.report(format!("motion {on}"), &motion(PIR, on));
    }

    fn relay(&mut self, on: bool) {
        self.report(format!("relay {on}"), &state(RELAY, on));
    }

    fn lux(&mut self, value: f64) {
        self.report(format!("lux {value}"), &luminosity(PIR, value));
    }

    fn press(&mut self) {
        self.report("press".into(), &temperature(0x60, 20.0));
    }

    /// A press of a real button (spec 049): the toggle command of a remote.
    fn toggle(&mut self) {
        self.report(
            "press".into(),
            &action(0x70, solite_core::radio::zcl::Action::Toggle),
        );
    }

    /// A button of the garage that is a remote with its `action` data.
    fn remote_button(&mut self) -> String {
        let remote = self.rig.remote(0x70);
        let data = self.rig.home.devices.get(&remote).unwrap().data[0]
            .id
            .to_string();
        let id = self.rig.equipment("Remote", "button", &self.g.zone, &[]);
        let (status, body) = self.rig.post(
            &format!("/api/v1/equipments/{id}/data-bindings"),
            json!({"deviceDataId": data, "alias": "action"}),
        );
        assert_eq!(status, 201, "{body}");
        id
    }

    fn advance(&mut self, ms: u64) {
        self.rig.advance(ms);
        self.step(&format!("advance {ms}"));
    }

    /// A button of the garage whose `action` is the temperature of a sensor.
    fn button(&mut self) -> String {
        let sensor = self.rig.sensor(0x60);
        let data = self.rig.home.devices.get(&sensor).unwrap().data[0]
            .id
            .to_string();
        let id = self.rig.equipment("Button", "button", &self.g.zone, &[]);
        let (status, body) = self.rig.post(
            &format!("/api/v1/equipments/{id}/data-bindings"),
            json!({"deviceDataId": data, "alias": "action"}),
        );
        assert_eq!(status, 201, "{body}");
        id
    }

    /// The instance's state as `GET /recipe-instances` writes it, key order and all.
    fn state_text(&mut self) -> String {
        #[derive(serde::Deserialize)]
        struct Instance {
            id: String,
            state: Box<serde_json::value::RawValue>,
        }
        let body = self.rig.raw("GET", BASE, "").body.unwrap();
        let list: Vec<Instance> = serde_json::from_str(&body).unwrap();
        list.into_iter()
            .find(|i| i.id == self.id)
            .unwrap()
            .state
            .get()
            .to_string()
    }
}

/// Plays the scenario: each of `expect` is in the trace (a scenario where nothing happens cannot
/// pass by emptiness).
fn both_at(now: u64, located: bool, scenario: fn(&mut Run), expect: &[&str]) {
    let mut run = Run::new(now, located);
    scenario(&mut run);
    for text in expect {
        assert!(
            run.trace.iter().any(|line| line.contains(text)),
            "nothing says {text:?}:\n{}",
            run.trace.join("\n")
        );
    }
}

fn both(scenario: fn(&mut Run), expect: &[&str]) {
    both_at(T0, false, scenario, expect);
}

// ---- The machine ----

#[test]
fn a_start_with_no_motion_sends_off() {
    both(
        |r| r.start(json!({})),
        &["Off", "Recipe activated — no motion, lights off"],
    );
}

#[test]
fn a_start_with_motion_sends_on() {
    both(
        |r| {
            r.motion(true);
            r.start(json!({}));
        },
        &["Motion detected — 1 light(s) turned on", "On"],
    );
}

#[test]
fn motion_switches_on_and_the_timeout_without_motion_switches_off() {
    both(
        |r| {
            r.start(json!({}));
            r.advance(10 * SECOND);
            r.motion(true);
            r.relay(true);
            r.advance(SECOND);
            r.motion(false);
            r.advance(19_999);
            r.advance(1);
        },
        &[
            "timerExpiresAt",
            "No motion for 20s — lights turned off",
            "sent [Off]",
        ],
    );
}

#[test]
fn motion_back_before_the_timeout_cancels_the_timer() {
    both(
        |r| {
            r.start(json!({}));
            r.motion(true);
            r.relay(true);
            r.advance(SECOND);
            r.motion(false);
            r.advance(10 * SECOND);
            r.motion(true);
            r.advance(60 * SECOND);
        },
        &["timerExpiresAt", "sent [On]"],
    );
}

#[test]
fn an_external_on_starts_the_countdown_and_a_heartbeat_changes_nothing() {
    both(
        |r| {
            r.start(json!({"timeout": "2m"}));
            r.relay(true);
            r.advance(60 * SECOND);
            r.relay(true);
            r.advance(59_999);
            r.advance(1);
        },
        &[
            "Light turned on externally — turning off in 2min",
            "No motion for 2min — lights turned off",
        ],
    );
}

#[test]
fn after_the_timer_a_new_external_on_starts_a_fresh_countdown() {
    both(
        |r| {
            r.start(json!({"timeout": "2m"}));
            r.relay(true);
            r.advance(120 * SECOND);
            r.advance(SECOND);
            r.relay(false);
            r.advance(10 * SECOND);
            r.relay(true);
            r.advance(60 * SECOND);
            r.relay(true);
            r.advance(59_999);
            r.advance(1);
        },
        &["No motion for 2min — lights turned off"],
    );
}

#[test]
fn an_empty_lux_threshold_means_none() {
    both(
        |r| {
            r.start(json!({"luxThreshold": ""}));
            r.lux(1.0);
            r.motion(true);
        },
        &["sent [On]"],
    );
}

#[test]
fn above_the_lux_threshold_the_light_stays_off_and_at_the_threshold_it_goes_on() {
    both(
        |r| {
            r.start(json!({"luxThreshold": 50}));
            r.lux(120.0);
            r.motion(true);
            r.lux(50.0);
        },
        &[
            "Motion detected but luminosity 120 exceeds threshold 50 — not turning on",
            "sent [On]",
        ],
    );
}

#[test]
fn the_light_goes_off_only_above_the_threshold_plus_ten_percent() {
    both(
        |r| {
            r.start(json!({"luxThreshold": 50}));
            r.lux(40.0);
            r.motion(true);
            r.relay(true);
            r.lux(55.0);
            r.lux(56.0);
        },
        &["Luminosity above threshold — lights turned off"],
    );
}

#[test]
fn a_light_switched_on_by_hand_above_the_hysteresis_is_an_override() {
    both(
        |r| {
            r.start(json!({"luxThreshold": 50}));
            r.lux(40.0);
            r.advance(5 * SECOND);
            r.relay(true);
            r.lux(60.0);
        },
        &[
            "Light turned on manually above lux threshold — entering override mode",
            "overrideMode",
        ],
    );
}

#[test]
fn a_light_switched_off_by_hand_during_motion_is_an_override_cleared_after_the_timeout() {
    both(
        |r| {
            r.start(json!({}));
            r.motion(true);
            r.relay(true);
            r.advance(5 * SECOND);
            r.relay(false);
            r.motion(false);
            r.motion(true);
            r.motion(false);
            r.advance(19_999);
            r.advance(1);
            r.advance(5 * SECOND);
            r.motion(true);
        },
        &[
            "Light turned off manually while motion active — entering override mode",
            "Motion detected but override mode active — ignoring",
            "No motion for 20s — override cleared, lights off",
            "sent [On]",
        ],
    );
}

#[test]
fn the_state_keys_keep_sowels_order_after_an_override_is_set_cleared_and_set_again() {
    both(
        |r| {
            r.start(json!({"luxThreshold": 50, "maxOnDuration": "5m"}));
            r.lux(40.0);
            r.relay(true); // by hand: both timers
            r.lux(60.0); // override: the three keys
            let text = r.state_text();
            assert!(
                text.find("overrideMode") < text.find("timerExpiresAt")
                    && text.find("timerExpiresAt") < text.find("failsafeExpiresAt"),
                "{text}"
            );
            r.advance(20 * SECOND);
            r.lux(30.0);
            r.relay(false);
            r.advance(5 * SECOND);
            r.relay(true);
            r.lux(60.0); // set again
            r.advance(SECOND);
        },
        &["overrideMode", "failsafeExpiresAt"],
    );
}

#[test]
fn the_override_clear_switches_a_light_that_is_on_off() {
    both(
        |r| {
            r.start(json!({}));
            r.motion(true);
            r.relay(true);
            r.advance(5 * SECOND);
            r.relay(false);
            r.relay(true);
            r.motion(false);
            r.advance(20 * SECOND);
        },
        &[
            "No motion for 20s — override cleared, lights off",
            "sent [Off]",
        ],
    );
}

#[test]
fn the_echo_of_the_recipes_own_off_is_not_a_manual_change() {
    both(
        |r| {
            r.start(json!({}));
            r.motion(true);
            r.relay(true);
            r.motion(false);
            r.advance(20 * SECOND);
            r.advance(500);
            r.relay(false);
            r.advance(500);
            r.motion(true);
            r.motion(false);
            r.advance(2 * SECOND);
            r.motion(true);
        },
        &["sent [Off]", "sent [On]"],
    );
}

#[test]
fn the_stale_race_of_sowels_recipe_is_kept() {
    both(
        |r| {
            r.start(json!({}));
            r.motion(true);
            r.lux(30.0);
        },
        &["Light turned off manually while motion active — entering override mode"],
    );
}

#[test]
fn a_button_press_switches_a_light_that_is_on_off_with_an_override_and_one_that_is_off_on() {
    both(
        |r| {
            let b = r.button();
            r.start(json!({"buttons": [b]}));
            r.press();
            r.relay(true);
            r.advance(5 * SECOND);
            r.press();
            r.advance(20 * SECOND);
        },
        &[
            "Motion detected — 1 light(s) turned on",
            "Button pressed — lights off, entering override mode",
            "No motion for 20s — override cleared, lights off",
        ],
    );
}

#[test]
fn a_remotes_toggle_command_switches_the_lights_like_a_button_press() {
    both(
        |r| {
            let b = r.remote_button();
            r.start(json!({"buttons": [b]}));
            r.toggle();
            r.relay(true);
            r.advance(5 * SECOND);
            // The same command a second time is a second press.
            r.toggle();
            r.advance(20 * SECOND);
        },
        &[
            "Motion detected — 1 light(s) turned on",
            "Button pressed — lights off, entering override mode",
            "No motion for 20s — override cleared, lights off",
        ],
    );
}

#[test]
fn the_failsafe_fires_and_motion_impulses_keep_it_from_firing() {
    both(
        |r| {
            r.start(json!({"maxOnDuration": "5m"}));
            r.motion(true);
            r.relay(true);
            for _ in 0..3 {
                r.advance(4 * 60 * SECOND);
                r.report("impulse".into(), &motion(PIR, true));
            }
            r.advance(5 * 60 * SECOND - 1);
            r.advance(1);
        },
        &[
            "failsafeExpiresAt",
            "Failsafe: lights forced off after 5min max on duration",
        ],
    );
}

#[test]
fn the_zones_motion_restarts_the_failsafe_too() {
    both(
        |r| {
            r.start(json!({"maxOnDuration": "5m"}));
            r.motion(true);
            r.relay(true);
            r.advance(4 * 60 * SECOND);
            r.motion(false);
            r.motion(true);
            r.advance(4 * 60 * SECOND);
            r.advance(60 * SECOND);
        },
        &["Failsafe: lights forced off after 5min max on duration"],
    );
}

#[test]
fn a_timer_of_more_than_24_days_fires_at_its_time_not_at_once() {
    both(
        |r| {
            r.lux(5.0);
            r.start(json!({"maxOnDuration": "700h"}));
            r.motion(true);
            r.relay(true);
            for _ in 0..699 {
                r.rig.advance(3600 * SECOND);
                r.rig.report(&state(RELAY, true));
                r.rig.report(&luminosity(PIR, 5.0));
                assert_eq!(r.rig.sent(), []);
            }
            r.step("700 hours of heartbeats");
            r.advance(3600 * SECOND);
        },
        &["Failsafe: lights forced off after 700h max on duration"],
    );
}

#[test]
fn disable_when_daylight_has_no_effect_while_the_daylight_is_unknown() {
    both(
        |r| {
            r.start(json!({"disableWhenDaylight": true}));
            r.motion(true);
        },
        &["sent [On]"],
    );
}

#[test]
fn a_zone_without_a_motion_sensor_logs_a_warning_at_every_start() {
    both(
        |r| {
            let zone = r.rig.zone("Blind");
            let relay = r.rig.relay(0x7C);
            let light = r
                .rig
                .equipment("Blind light", "light_onoff", &zone, &[&relay]);
            r.g.zone = zone;
            r.g.light = light;
            r.start(json!({}));
            let id = r.id.clone();
            r.rig.post(&format!("{BASE}/{id}/disable"), json!({}));
            r.step("disable");
            r.rig.post(&format!("{BASE}/{id}/enable"), json!({}));
            r.step("enable");
        },
        &["Zone has no motion sensors — recipe will never trigger"],
    );
}

// ---- Inactive during day ----

/// 2026-06-21T12:00Z, 14:00 in Paris: day.
const JUNE_NOON: u64 = 1_782_043_200_000;
/// 2026-06-21T21:30Z, 23:30 in Paris: night.
const JUNE_NIGHT: u64 = 1_782_077_400_000;

#[test]
fn by_day_motion_does_not_switch_the_light_on() {
    both_at(
        JUNE_NOON,
        true,
        |r| {
            r.start(json!({"disableWhenDaylight": true}));
            r.motion(true);
        },
        &["Motion detected but daytime — not turning on"],
    );
}

#[test]
fn by_night_motion_switches_the_light_on() {
    both_at(
        JUNE_NIGHT,
        true,
        |r| {
            r.start(json!({"disableWhenDaylight": true}));
            r.motion(true);
        },
        &["sent [On]"],
    );
}

#[test]
fn by_day_motion_switches_the_light_on_when_the_option_is_off() {
    both_at(
        JUNE_NOON,
        true,
        |r| {
            r.start(json!({"disableWhenDaylight": false}));
            r.motion(true);
        },
        &["sent [On]"],
    );
}

#[test]
fn the_start_sync_by_day_with_motion_does_not_switch_the_light_on() {
    both_at(
        JUNE_NOON,
        true,
        |r| {
            r.motion(true);
            r.start(json!({"disableWhenDaylight": true}));
        },
        &["Recipe activated — daytime, lights off"],
    );
}

#[test]
fn with_the_clock_not_set_the_daylight_is_unknown() {
    both_at(
        3_600_000,
        true,
        |r| {
            r.start(json!({"disableWhenDaylight": true}));
            r.motion(true);
        },
        &["sent [On]"],
    );
}

#[test]
fn a_location_set_then_motion_in_the_same_minute_reads_day_at_once() {
    both_at(
        JUNE_NOON,
        false,
        |r| {
            r.start(json!({"disableWhenDaylight": true}));
            let (status, _) = r.rig.put(
                "/api/v1/settings",
                json!({
                    "home.latitude": "48.8566",
                    "home.longitude": "2.3522",
                    "home.timezone": "Europe/Paris",
                }),
            );
            assert_eq!(status, 200);
            r.rig.now += 1_000;
            r.motion(true);
        },
        &["Motion detected but daytime"],
    );
}

// ---- Enable, disable, the lights ----

#[test]
fn stop_clears_the_three_state_keys_and_enabling_syncs_again() {
    both(
        |r| {
            r.start(json!({"luxThreshold": 50, "maxOnDuration": "5m"}));
            r.lux(40.0);
            r.relay(true);
            r.lux(60.0);
            let id = r.id.clone();
            r.rig.post(&format!("{BASE}/{id}/disable"), json!({}));
            r.step("disable");
            r.rig.post(&format!("{BASE}/{id}/enable"), json!({}));
            r.step("enable");
        },
        &["overrideMode", "failsafeExpiresAt", "sent [Off]"],
    );
}

#[test]
fn a_disabled_light_among_two_gets_no_order_and_counts_in_the_message() {
    both(
        |r| {
            let relay2 = r.rig.relay(0x7B);
            let second = r
                .rig
                .equipment("Second light", "light_onoff", &r.g.zone, &[&relay2]);
            let (status, _) = r.rig.put(
                &format!("/api/v1/equipments/{second}"),
                json!({"enabled": false}),
            );
            assert_eq!(status, 200);
            let first = r.g.light.clone();
            r.start(json!({"lights": [first, second]}));
            r.motion(true);
        },
        &["Motion detected — 2 light(s) turned on"],
    );
}

#[test]
fn a_light_deleted_while_running_leaves_the_settings_and_the_other_is_served() {
    both(
        |r| {
            let relay2 = r.rig.relay(0x7B);
            let second = r
                .rig
                .equipment("Second light", "light_onoff", &r.g.zone, &[&relay2]);
            r.rig.measure(&state(0x7B, false));
            let first = r.g.light.clone();
            r.start(json!({"lights": [first, second.clone()]}));
            let (status, _) = r.rig.delete(&format!("/api/v1/equipments/{second}"));
            assert_eq!(status, 204);
            r.step("delete the second light");
            r.motion(true);
        },
        &[
            "Equipment removed from settings: Second light",
            "Motion detected — 1 light(s) turned on",
        ],
    );
}

#[test]
fn the_only_light_deleted_disables_the_instance_with_its_reason() {
    let mut r = Run::new(T0, false);
    r.start(json!({}));
    let (status, _) = r.rig.delete(&format!("/api/v1/equipments/{}", r.g.light));
    assert_eq!(status, 204);
    let (_, list) = r.rig.get(BASE);
    assert_eq!(list[0]["enabled"], false);
    r.rig.report(&motion(PIR, true));
    r.rig.advance(60_000);
    assert!(r.rig.sent().is_empty());
    let messages = r.rig.messages(&r.id);
    assert!(
        messages.last().unwrap().starts_with("Disabled: "),
        "{messages:?}"
    );
}

#[test]
fn the_lights_state_order_binding_removed_disables_the_instance() {
    let mut r = Run::new(T0, false);
    r.start(json!({}));
    let (_, light) = r.rig.get(&format!("/api/v1/equipments/{}", r.g.light));
    let binding = light["orderBindings"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let (status, _) = r.rig.delete(&format!(
        "/api/v1/equipments/{}/order-bindings/{binding}",
        r.g.light
    ));
    assert_eq!(status, 204);
    let (_, list) = r.rig.get(BASE);
    assert_eq!(list[0]["enabled"], false);
    assert_eq!(
        r.rig.messages(&r.id).last().unwrap(),
        "Disabled: Light \"Garage light\" has no \"state\" order binding"
    );
}

// ---- Settle: the motion sources follow the home (spec 028) ----

fn unbind_all(r: &mut Run, equipment: &str) {
    let (_, item) = r.rig.get(&format!("/api/v1/equipments/{equipment}"));
    for binding in item["dataBindings"].as_array().unwrap().clone() {
        let id = binding["id"].as_str().unwrap();
        let (status, _) = r.rig.delete(&format!(
            "/api/v1/equipments/{equipment}/data-bindings/{id}"
        ));
        assert_eq!(status, 204);
    }
}

/// The garage's instance with a safety auto-off, the light on and the zone's motion on.
fn lit(r: &mut Run) {
    r.start(json!({"maxOnDuration": "5m"}));
    r.motion(true);
    r.relay(true);
}

/// An impulse of `sensor` six seconds after the last step; the failsafe's state after it.
fn impulse(r: &mut Run, sensor: u8, label: &str) {
    r.advance(SECOND);
    r.advance(5 * SECOND);
    r.report(label.into(), &motion(sensor, true));
}

#[test]
fn a_motion_sensor_unbound_under_a_running_instance_no_longer_resets_the_failsafe() {
    both(
        |r| {
            lit(r);
            let equipment = r.g.motion_equipment.clone();
            unbind_all(r, &equipment);
            r.step("unbind");
            impulse(r, PIR, "impulse of the unbound sensor");
        },
        &["failsafeExpiresAt"],
    );
}

#[test]
fn a_motion_sensor_added_to_the_zone_resets_the_failsafe_without_a_restart() {
    both(
        |r| {
            lit(r);
            let second = r.rig.motion_sensor(0x62);
            let zone = r.g.zone.clone();
            r.rig
                .equipment("Second motion", "sensor", &zone, &[&second]);
            r.step("add a sensor");
            impulse(r, 0x62, "impulse of the new sensor");
        },
        &["failsafeExpiresAt"],
    );
}

#[test]
fn a_sensor_equipment_moved_out_of_the_zone_or_disabled_stops_counting() {
    both(
        |r| {
            lit(r);
            let other = r.rig.zone("Cellar");
            let sensor = format!("/api/v1/equipments/{}", r.g.motion_equipment);
            r.rig.put(&sensor, json!({"zoneId": other}));
            r.step("moved out");
            impulse(r, PIR, "impulse, moved out");
            let zone = r.g.zone.clone();
            r.rig.put(&sensor, json!({"zoneId": zone}));
            r.step("moved back");
            impulse(r, PIR, "impulse, moved back");
            r.rig.put(&sensor, json!({"enabled": false}));
            r.step("disabled");
            impulse(r, PIR, "impulse, disabled");
        },
        &["failsafeExpiresAt"],
    );
}

#[test]
fn the_last_motion_sensor_of_the_zone_unbound_leaves_the_instance_running() {
    both(
        |r| {
            r.start(json!({"maxOnDuration": "5m"}));
            let equipment = r.g.motion_equipment.clone();
            unbind_all(r, &equipment);
            r.step("unbind");
            r.advance(30 * SECOND);
        },
        &["Recipe activated"],
    );
}

#[test]
fn a_sensor_unbound_keeps_the_state_and_the_timers_and_sends_nothing() {
    both(
        |r| {
            r.start(json!({}));
            r.motion(true);
            r.relay(true);
            r.motion(false);
            let equipment = r.g.motion_equipment.clone();
            unbind_all(r, &equipment);
            r.step("unbind");
        },
        &["timerExpiresAt"],
    );
}

// ---- The recipe's info ----

#[test]
fn the_recipe_is_sowels_motion_light_with_its_seven_slots() {
    let mut rig = Rig::new();
    rig.load_motion_light();
    let (status, list) = rig.get("/api/v1/recipes");
    assert_eq!(status, 200);
    let list = list.as_array().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["id"], "motion-light");
    assert_eq!(list[0]["name"], "Motion Light");
    assert_eq!(list[0]["icon"], "Lightbulb");
    let slots: Vec<&str> = list[0]["slots"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        slots,
        [
            "zone",
            "lights",
            "timeout",
            "luxThreshold",
            "maxOnDuration",
            "buttons",
            "disableWhenDaylight"
        ]
    );
    // `min` is written `0`, not `0.0`: the bytes are Sowel's (recipe_json_bytes.rs).
    assert_eq!(
        list[0]["slots"][3]["constraints"],
        json!({"min": 0}),
        "{}",
        list[0]["slots"][3]
    );
}

// ---- Sowel's validation texts (R3) ----

fn refused(rig: &mut Rig, params: Value) -> (u16, String) {
    let (status, body) = rig.instance(params);
    (status, body["error"].as_str().unwrap_or("").to_string())
}

fn invalid(message: &str) -> (u16, String) {
    (400, format!("Invalid params: {message}"))
}

#[test]
fn every_refused_setting_answers_sowels_text_in_sowels_order() {
    let mut rig = Rig::new();
    let g = rig.garage();
    let no_state = rig.equipment("Bare light", "light_onoff", &g.zone, &[]);
    let ghost = "11111111-2222-4333-8444-555555555555";
    let (zone, light) = (&g.zone, &g.light);
    let duration =
        |v: &str| format!("Invalid duration format: {v}. Use e.g. \"10m\", \"30s\", \"1h\"");
    let sixteen_plus_one: Vec<&str> = (0..17).map(|_| light.as_str()).collect();
    let buttons_17: Vec<&str> = (0..17).map(|_| ghost).collect();

    let cases: Vec<(Value, String)> = vec![
        (json!({}), "Zone parameter is required".into()),
        (json!({"zone": ""}), "Zone parameter is required".into()),
        (json!({"zone": 5}), "Zone parameter is required".into()),
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
            json!({"zone": zone, "lights": sixteen_plus_one}),
            "At most 16 lights".into(),
        ),
        (
            json!({"zone": zone, "lights": [no_state]}),
            "Light \"Bare light\" has no \"state\" order binding".into(),
        ),
        (
            json!({"zone": zone, "lights": [light], "timeout": "10x"}),
            duration("10x"),
        ),
        (
            json!({"zone": zone, "lights": [light], "timeout": true}),
            duration("true"),
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
            duration("soon"),
        ),
        (
            json!({"zone": zone, "lights": [light], "buttons": [ghost]}),
            format!("Button equipment not found: {ghost}"),
        ),
        (
            json!({"zone": zone, "lights": [light], "buttons": buttons_17}),
            "At most 16 buttons".into(),
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
        invalid(&duration("x"))
    );
}

#[test]
fn the_zone_the_type_and_the_zone_of_a_light_answer_sowels_texts_in_sowels_order() {
    let mut rig = Rig::new();
    let g = rig.garage();
    let other_zone = rig.zone("Elsewhere");
    let relay2 = rig.relay(0x62);
    let far = rig.equipment("Far light", "light_onoff", &other_zone, &[&relay2]);
    let sensor = rig.equipment("A sensor", "sensor", &g.zone, &[]);
    let ghost = "11111111-2222-4333-8444-555555555555";
    // A zone that does not exist, even with no light (Sowel checks the zone first).
    assert_eq!(
        refused(&mut rig, json!({"zone": ghost, "lights": []})),
        invalid(&format!("Zone not found: {ghost}"))
    );
    assert_eq!(
        refused(&mut rig, json!({"zone": ghost, "lights": [g.light]})),
        invalid(&format!("Zone not found: {ghost}"))
    );
    // A light that is not a light, a light of another zone.
    assert_eq!(
        refused(&mut rig, json!({"zone": g.zone, "lights": [sensor]})),
        invalid("Light \"A sensor\" is type \"sensor\" but this recipe requires light_onoff")
    );
    assert_eq!(
        refused(&mut rig, json!({"zone": g.zone, "lights": [far]})),
        invalid("Light \"Far light\" does not belong to the selected zone")
    );
    // The first light that is wrong answers, in the order of the list.
    assert_eq!(
        refused(&mut rig, json!({"zone": g.zone, "lights": [far, ghost]})),
        invalid("Light \"Far light\" does not belong to the selected zone")
    );
    assert_eq!(
        refused(&mut rig, json!({"zone": g.zone, "lights": [ghost, far]})),
        invalid(&format!("Light equipment not found: {ghost}"))
    );
}

// ---- The adaptations of Solite's engine (R4) ----

#[test]
fn a_deleted_equipment_leaves_the_settings_by_the_generic_settle_and_a_button_without_its_action_disables_the_instance()
 {
    let mut r = Run::new(T0, false);
    // A button with an `action` binding.
    let sensor = r.rig.sensor(0x60);
    let data = r.rig.home.devices.get(&sensor).unwrap().data[0]
        .id
        .to_string();
    let button = r.rig.equipment("Wall button", "button", &r.g.zone, &[]);
    let (status, _) = r.rig.post(
        &format!("/api/v1/equipments/{button}/data-bindings"),
        json!({"deviceDataId": data, "alias": "action"}),
    );
    assert_eq!(status, 201);
    // A second button, to be deleted.
    let other = r.rig.equipment("Other button", "button", &r.g.zone, &[]);
    let (status, _) = r.rig.post(
        &format!("/api/v1/equipments/{other}/data-bindings"),
        json!({"deviceDataId": data, "alias": "action"}),
    );
    assert_eq!(status, 201);
    r.start(json!({"buttons": [button.clone(), other.clone()]}));
    // The equipment deleted: gone from the settings, the instance runs on.
    let (status, _) = r.rig.delete(&format!("/api/v1/equipments/{other}"));
    assert_eq!(status, 204);
    let (_, list) = r.rig.get(BASE);
    assert_eq!(list[0]["enabled"], true);
    assert_eq!(list[0]["params"]["buttons"], json!([button]));
    // The binding removed: the script's validate fails and the instance is disabled (the native
    // recipe pruned the button, D46).
    let (_, item) = r.rig.get(&format!("/api/v1/equipments/{button}"));
    let binding = item["dataBindings"][0]["id"].as_str().unwrap().to_string();
    let (status, _) = r.rig.delete(&format!(
        "/api/v1/equipments/{button}/data-bindings/{binding}"
    ));
    assert_eq!(status, 204);
    let (_, list) = r.rig.get(BASE);
    assert_eq!(list[0]["enabled"], false);
    assert_eq!(
        r.rig.messages(&r.id).last().unwrap(),
        "Disabled: Button \"Wall button\" has no \"action\" data binding"
    );
}

#[test]
fn the_legacy_light_key_is_not_read() {
    let mut rig = Rig::new();
    let g = rig.garage();
    assert_eq!(
        refused(&mut rig, json!({"zone": g.zone, "light": g.light})),
        invalid("At least one light is required")
    );
}

#[test]
fn a_refused_orders_reason_is_logged() {
    // Three instances of sixteen lights start together: 48 orders, the queue keeps 32, so the
    // third instance's are refused (`queue full`).
    let mut r = Run::new(T0, false);
    let lights: Vec<String> = (0..16)
        .map(|n| {
            let relay = r.rig.relay(0x80 + n);
            r.rig
                .equipment(&format!("Light {n}"), "light_onoff", &r.g.zone, &[&relay])
        })
        .collect();
    let mut ids = Vec::new();
    for _ in 0..3 {
        let (status, body) = r.create(json!({"lights": lights}));
        assert_eq!(status, 201, "{body}");
        ids.push(body["id"].as_str().unwrap().to_string());
    }
    assert_eq!(r.rig.sent().len(), 32, "the queue keeps 32 of the 48");
    let log = r.rig.messages(&ids[2]);
    let error = log
        .iter()
        .find(|m| m.starts_with("Error turning off some lights: "))
        .unwrap_or_else(|| panic!("{log:?}"));
    assert!(error.contains(": queue full"), "{error}");
    // The log keeps 200 bytes of a line: the first refusals of the sixteen.
    assert!(error.matches("queue full").count() >= 3, "{error}");
}

#[test]
fn the_echo_window_is_counted_on_the_wall_clock() {
    // The light goes off by the timeout; the next motion arrives 500 ms later on the monotonic
    // clock while the wall clock has jumped 3 s ahead (the clock was set): the script is out of
    // its 2 s window and switches the light on.
    let mut r = Run::new(T0, false);
    r.start(json!({}));
    r.motion(true);
    r.relay(true);
    r.motion(false);
    r.advance(20 * SECOND);
    r.rig.sent();
    let (mono, wall) = (r.rig.now + 500, r.rig.now + 3500);
    let mut random = counter();
    r.rig
        .home
        .measures(&state(RELAY, false), mono, &mut *random)
        .unwrap();
    r.rig.home.recompute(mono);
    r.rig.home.run_recipes(mono, wall);
    r.rig
        .home
        .measures(&motion(PIR, true), mono, &mut *random)
        .unwrap();
    r.rig.home.recompute(mono);
    r.rig.home.run_recipes(mono, wall);
    r.rig.now = mono;
    let sent = r.rig.sent();
    assert_eq!(sent.len(), 1, "the script switches on: {sent:?}");
}

// ---- At most 4 instances (R5) ----

#[test]
fn a_fifth_instance_of_motion_light_is_refused_in_any_zone() {
    let mut rig = Rig::new();
    let g = rig.garage();
    for _ in 0..4 {
        rig.garage_instance(&g, json!({}));
    }
    let other_zone = rig.zone("Elsewhere");
    let relay = rig.relay(0x62);
    let light = rig.equipment("Other light", "light_onoff", &other_zone, &[&relay]);
    assert_eq!(
        refused(&mut rig, json!({"zone": other_zone, "lights": [light]})),
        (
            400,
            "Too many instances of motion-light (max 4)".to_string()
        )
    );
}

// ---- A home saved with instances (R5, R6) ----

fn stored(id: &str, params: Value) -> solite_core::recipes::RecipeInstance {
    solite_core::recipes::RecipeInstance {
        id: solite_core::devices::Id::parse(id).unwrap(),
        recipe_id: "motion-light".into(),
        params: params.to_string().into(),
        enabled: true,
        created_at: T0,
    }
}

fn nth_id(n: usize) -> String {
    format!("22222222-2222-4222-8222-{n:012}")
}

fn running(rig: &Rig, id: &str) -> bool {
    rig.home
        .recipes()
        .is_running(&solite_core::devices::Id::parse(id).unwrap())
}

#[test]
fn instances_saved_with_the_recipe_not_installed_stay_stopped_with_their_settings_then_start_with_it()
 {
    let mut rig = Rig::new();
    let g = rig.garage();
    let params = json!({"zone": g.zone, "lights": [g.light], "timeout": "20s"});
    let ids: Vec<String> = (1..=3).map(nth_id).collect();
    rig.home
        .load_recipes(ids.iter().map(|id| stored(id, params.clone())).collect());
    // The boot with nothing installed.
    rig.home.start_recipes(rig.now, rig.now);
    let (_, list) = rig.get(BASE);
    assert_eq!(list.as_array().unwrap().len(), 3);
    for id in &ids {
        assert!(!running(&rig, id));
        assert_eq!(
            rig.messages(id).last().unwrap(),
            "Failed to restore: Recipe not found: motion-light"
        );
    }
    for item in list.as_array().unwrap() {
        assert_eq!(item["params"], params, "the settings are kept");
    }
    // The recipe is installed: the next start (or the install itself) brings them up.
    rig.load_motion_light();
    rig.home.start_recipes(rig.now, rig.now);
    for id in &ids {
        assert!(running(&rig, id), "{:?}", rig.messages(id));
    }
    let (_, list) = rig.get(BASE);
    for item in list.as_array().unwrap() {
        assert_eq!(item["params"], params, "the settings are unchanged");
    }
}

#[test]
fn an_instance_with_only_the_legacy_light_key_stays_stopped_with_sowels_text() {
    let mut rig = Rig::new();
    let g = rig.garage();
    rig.load_motion_light();
    let id = nth_id(1);
    rig.home.load_recipes(vec![stored(
        &id,
        json!({"zone": g.zone, "light": g.light, "timeout": "20s"}),
    )]);
    rig.home.start_recipes(rig.now, rig.now);
    assert!(!running(&rig, &id));
    assert_eq!(
        rig.messages(&id).last().unwrap(),
        // The restore path words a refusal without the API's `Invalid params:` prefix.
        "Failed to restore: At least one light is required"
    );
}

#[test]
fn at_boot_the_fifth_instance_and_the_later_ones_stay_stopped() {
    let mut rig = Rig::new();
    let g = rig.garage();
    rig.load_motion_light();
    let params = json!({"zone": g.zone, "lights": [g.light], "timeout": "20s"});
    let ids: Vec<String> = (1..=10).map(nth_id).collect();
    rig.home
        .load_recipes(ids.iter().map(|id| stored(id, params.clone())).collect());
    rig.home.start_recipes(rig.now, rig.now);
    for id in &ids[..4] {
        assert!(running(&rig, id), "{id}");
    }
    for id in &ids[4..] {
        assert!(!running(&rig, id), "{id}");
        assert_eq!(
            rig.messages(id).last().unwrap(),
            "Failed to restore: Too many instances of motion-light (max 4)"
        );
    }
}

// ---- A boot by day ----

#[test]
fn a_boot_by_day_starts_the_recipe_with_the_daylight_before_any_aggregate() {
    use mem_storage::MemStorage;
    use solite_core::engine::{Saver, load_home};
    use solite_core::home::Home;
    use solite_core::radio::zcl::Command;
    let mut rig = Rig::new();
    rig.now = JUNE_NOON;
    let g = rig.garage();
    let (status, body) = rig.put(
        "/api/v1/settings",
        json!({
            "home.latitude": "48.8566",
            "home.longitude": "2.3522",
            "home.timezone": "Europe/Paris",
        }),
    );
    assert_eq!(status, 200, "{body}");
    rig.report(&motion(PIR, true));
    let id = rig.garage_instance(&g, json!({"disableWhenDaylight": true}));
    let storage = MemStorage::default();
    let home = std::sync::Mutex::new(std::mem::replace(&mut rig.home, Home::new(0)));
    Saver::new(&storage, rig.now)
        .tick(&home, rig.now, true)
        .unwrap();
    // The boot: the home read back, the recipe loaded directly (the boot load of an installed
    // file is `recipe_install.rs`'s, with the same scenario), the recipes started before the first
    // aggregate.
    rig.now += 1_000;
    rig.home = load_home(&&storage, rig.now).unwrap();
    rig.load_motion_light();
    rig.home.start_recipes(rig.now, rig.now);
    assert!(!rig.sent().contains(&Command::On));
    assert_eq!(
        rig.messages(&id).last().unwrap(),
        "Recipe activated — daytime, lights off"
    );
}

// ---- The luminosity threshold is a number ----

#[test]
fn the_lux_threshold_is_a_number_and_it_behaves() {
    let mut rig = Rig::new();
    let g = rig.garage();
    for value in [json!(50), json!(0), json!(12.5), json!(null), json!("")] {
        let (status, body) = rig.instance(json!({
            "zone": g.zone, "lights": [g.light], "luxThreshold": value
        }));
        assert_eq!(status, 201, "{value}: {body}");
        // At most 4 instances of the script: each one made here is deleted again.
        let id = body["id"].as_str().unwrap().to_string();
        assert_eq!(rig.delete(&format!("/api/v1/recipe-instances/{id}")).0, 204);
    }
    for value in [json!("abc"), json!(-0.5), json!("5 lux"), json!(true)] {
        assert_eq!(
            refused(
                &mut rig,
                json!({"zone": g.zone, "lights": [g.light], "luxThreshold": value})
            ),
            invalid("luxThreshold must be a non-negative number"),
            "{value}"
        );
    }
    for (value, lux, on) in [(50.0, 120.0, false), (50.0, 50.0, true), (0.0, 1.0, false)] {
        let mut rig = Rig::new();
        let g = rig.garage();
        rig.garage_instance(&g, json!({"luxThreshold": value}));
        rig.sent();
        rig.report(&luminosity(PIR, lux));
        rig.report(&motion(PIR, true));
        assert_eq!(
            rig.sent() == [solite_core::radio::zcl::Command::On],
            on,
            "{value} {lux}"
        );
    }
}
