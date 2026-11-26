//! `motion-light-script` against the built-in `motion-light` (spec 041, R5, goal 2): every
//! behaviour scenario of `motion_light.rs` (and the ones of `recipe_settle.rs` that follow the
//! sensors and the lights) the script API can express is played twice, once on each recipe, in the
//! same home at the same times; what is compared is the trace of each step: the orders sent, the
//! instance's state and the lines its log gained. Then one test for each difference of R5.2.
//!
//! Not run both ways (and why):
//! - `the_recipe_list_is_the_json_of_the_reference`, the motion sources unit tests: they test the
//!   built-in's crate, not a behaviour; the script's `RecipeInfo` is compared slot by slot below.
//! - `every_validate_message_of_the_reference_in_its_order`, `the_lux_threshold_follows_javascripts_number`:
//!   validation of the settings is the host's for a script (R5.2); the texts that stay the
//!   recipe's own (a light with no `state` order, a button with no `action` data) are compared.
//! - `a_boot_by_day_starts_the_recipe_with_the_daylight_before_any_aggregate`: needs a saved home
//!   and the program's start-up, where the host loads scripts (host tests).
//! - the settle tests of buttons (spec 028) and of the legacy `light` key: the script's settle is
//!   the generic one (R5.2), each a difference test below.
//!
//! These tests need the `script-recipes` feature (the host program turns it on, so
//! `cargo test --workspace` runs them).

mod support;

use std::sync::Arc;

use serde_json::{Value, json};
use solite_core::recipes::ScriptClock;
use solite_core::recipes::script::SystemRuntimes;
use support::*;

const SCRIPT: &str = include_str!("../../../recipes/scripts/motion-light-script.js");
const SECOND: u64 = 1000;
const BASE: &str = "/api/v1/recipe-instances";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Flavor {
    Builtin,
    Script,
}

impl Flavor {
    fn recipe(self) -> &'static str {
        match self {
            Flavor::Builtin => "motion-light",
            Flavor::Script => "motion-light-script",
        }
    }
}

fn load_script(rig: &mut Rig) {
    let clock: ScriptClock = Arc::new(|| 0);
    rig.home
        .load_script_recipe(SCRIPT, Arc::new(SystemRuntimes), clock)
        .unwrap();
}

/// One run of a scenario: a home with the garage (and the script loaded), one instance of the
/// recipe being tested, and the trace of what each step did.
struct Run {
    rig: Rig,
    g: Garage,
    flavor: Flavor,
    id: String,
    trace: Vec<String>,
    seen: usize,
}

impl Run {
    fn new(flavor: Flavor, now: u64, located: bool) -> Run {
        let mut rig = Rig::new();
        rig.now = now;
        let g = rig.garage();
        load_script(&mut rig);
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
            flavor,
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
        self.rig.post(
            BASE,
            json!({"recipeId": self.flavor.recipe(), "params": params}),
        )
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

/// The trace of a scenario on one recipe.
fn play(flavor: Flavor, now: u64, located: bool, scenario: fn(&mut Run)) -> Vec<String> {
    let mut run = Run::new(flavor, now, located);
    scenario(&mut run);
    run.trace
}

/// Plays the scenario on both recipes: the traces are the same, and each of `expect` is in them
/// (so a scenario where nothing happens cannot pass by emptiness).
fn both_at(now: u64, located: bool, scenario: fn(&mut Run), expect: &[&str]) {
    let built_in = play(Flavor::Builtin, now, located, scenario);
    let script = play(Flavor::Script, now, located, scenario);
    assert_eq!(
        built_in.len(),
        script.len(),
        "the traces differ in length:\n{}\n--\n{}",
        built_in.join("\n"),
        script.join("\n")
    );
    for (step, (a, b)) in built_in.iter().zip(&script).enumerate() {
        assert_eq!(a, b, "step {step} differs (built in, then script)");
    }
    for text in expect {
        assert!(
            built_in.iter().any(|line| line.contains(text)),
            "nothing says {text:?}:\n{}",
            built_in.join("\n")
        );
    }
}

fn both(scenario: fn(&mut Run), expect: &[&str]) {
    both_at(T0, false, scenario, expect);
}

// ---- The machine (motion_light.rs) ----

#[test]
fn parity_a_start_with_no_motion_sends_off() {
    both(
        |r| r.start(json!({})),
        &["Off", "Recipe activated — no motion, lights off"],
    );
}

#[test]
fn parity_a_start_with_motion_sends_on() {
    both(
        |r| {
            r.motion(true);
            r.start(json!({}));
        },
        &["Motion detected — 1 light(s) turned on", "On"],
    );
}

#[test]
fn parity_motion_switches_on_and_the_timeout_without_motion_switches_off() {
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
fn parity_motion_back_before_the_timeout_cancels_the_timer() {
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
fn parity_an_external_on_starts_the_countdown_and_a_heartbeat_changes_nothing() {
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
fn parity_after_the_timer_a_new_external_on_starts_a_fresh_countdown() {
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
fn parity_an_empty_lux_threshold_means_none() {
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
fn parity_above_the_lux_threshold_the_light_stays_off_and_at_the_threshold_it_goes_on() {
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
fn parity_the_light_goes_off_only_above_the_threshold_plus_ten_percent() {
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
fn parity_a_light_switched_on_by_hand_above_the_hysteresis_is_an_override() {
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
fn parity_a_light_switched_off_by_hand_during_motion_is_an_override_cleared_after_the_timeout() {
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
fn parity_the_state_keys_keep_the_built_ins_order_after_an_override_is_set_cleared_and_set_again() {
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
fn parity_the_override_clear_switches_a_light_that_is_on_off() {
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
fn parity_the_echo_of_the_recipes_own_off_is_not_a_manual_change() {
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
fn parity_the_stale_race_of_the_reference_is_kept() {
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
fn parity_a_button_press_switches_a_light_that_is_on_off_with_an_override_and_one_that_is_off_on() {
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
fn parity_the_failsafe_fires_and_motion_impulses_keep_it_from_firing() {
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
fn parity_the_zones_motion_restarts_the_failsafe_too() {
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
fn parity_a_timer_of_more_than_24_days_fires_at_its_time_not_at_once() {
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
fn parity_disable_when_daylight_has_no_effect_while_the_daylight_is_unknown() {
    both(
        |r| {
            r.start(json!({"disableWhenDaylight": true}));
            r.motion(true);
        },
        &["sent [On]"],
    );
}

#[test]
fn parity_a_zone_without_a_motion_sensor_logs_a_warning_at_every_start() {
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
fn parity_by_day_motion_does_not_switch_the_light_on() {
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
fn parity_by_night_motion_switches_the_light_on() {
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
fn parity_by_day_motion_switches_the_light_on_when_the_option_is_off() {
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
fn parity_the_start_sync_by_day_with_motion_does_not_switch_the_light_on() {
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
fn parity_with_the_clock_not_set_the_daylight_is_unknown() {
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
fn parity_a_location_set_then_motion_in_the_same_minute_reads_day_at_once() {
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
fn parity_stop_clears_the_three_state_keys_and_enabling_syncs_again() {
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
fn parity_a_disabled_light_among_two_gets_no_order_and_counts_in_the_message() {
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
fn parity_a_light_deleted_while_running_leaves_the_settings_and_the_other_is_served() {
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
fn parity_the_only_light_deleted_disables_the_instance_with_its_reason() {
    // The reason is the host's (`lights is required`) for the script, the recipe's for the
    // built-in: what is compared is that the instance stops and sends nothing more.
    for flavor in [Flavor::Builtin, Flavor::Script] {
        let mut r = Run::new(flavor, T0, false);
        r.start(json!({}));
        let (status, _) = r.rig.delete(&format!("/api/v1/equipments/{}", r.g.light));
        assert_eq!(status, 204);
        let (_, list) = r.rig.get(BASE);
        assert_eq!(list[0]["enabled"], false, "{flavor:?}");
        r.rig.report(&motion(PIR, true));
        r.rig.advance(60_000);
        assert!(r.rig.sent().is_empty(), "{flavor:?}");
        let messages = r.rig.messages(&r.id);
        assert!(
            messages.last().unwrap().starts_with("Disabled: "),
            "{flavor:?}: {messages:?}"
        );
    }
}

#[test]
fn parity_the_lights_state_order_binding_removed_disables_the_instance() {
    let mut texts = Vec::new();
    for flavor in [Flavor::Builtin, Flavor::Script] {
        let mut r = Run::new(flavor, T0, false);
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
        assert_eq!(list[0]["enabled"], false, "{flavor:?}");
        texts.push(r.rig.messages(&r.id).last().unwrap().clone());
    }
    assert_eq!(
        texts[0],
        "Disabled: Light \"Garage light\" has no \"state\" order binding"
    );
    assert_eq!(texts[0], texts[1], "the script's own validate says it too");
}

// ---- Settle: the motion sources follow the home (recipe_settle.rs, spec 028) ----

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
fn parity_a_motion_sensor_unbound_under_a_running_instance_no_longer_resets_the_failsafe() {
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
fn parity_a_motion_sensor_added_to_the_zone_resets_the_failsafe_without_a_restart() {
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
fn parity_a_sensor_equipment_moved_out_of_the_zone_or_disabled_stops_counting() {
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
fn parity_the_last_motion_sensor_of_the_zone_unbound_leaves_the_instance_running() {
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
fn parity_a_sensor_unbound_keeps_the_state_and_the_timers_and_sends_nothing() {
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

// ---- The slots ----

#[test]
fn the_scripts_slots_are_the_built_ins_in_ids_types_defaults_and_constraints() {
    let mut rig = Rig::new();
    load_script(&mut rig);
    let (_, list) = rig.get("/api/v1/recipes");
    let list = list.as_array().unwrap();
    assert_eq!(
        list.iter()
            .map(|r| r["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["motion-light", "motion-light-script"]
    );
    assert_eq!(list[1]["name"], "Motion Light (script)");
    assert_eq!(list[1]["icon"], "Lightbulb");
    let (built_in, script) = (
        list[0]["slots"].as_array().unwrap(),
        list[1]["slots"].as_array().unwrap(),
    );
    assert_eq!(built_in.len(), script.len());
    for (a, b) in built_in.iter().zip(script) {
        assert_eq!(a["id"], b["id"]);
        for key in [
            "type",
            "required",
            "list",
            "defaultValue",
            "description",
            "name",
        ] {
            assert_eq!(a[key], b[key], "slot {}, {key}", a["id"]);
        }
        // `min` is 0 in the built-in's JSON and 0.0 once a script's number has gone through its
        // float: the same number.
        assert_eq!(
            a["constraints"]["equipmentType"], b["constraints"]["equipmentType"],
            "slot {}",
            a["id"]
        );
        assert_eq!(
            a["constraints"]["min"].as_f64(),
            b["constraints"]["min"].as_f64(),
            "slot {}",
            a["id"]
        );
    }
}

// ---- The differences of R5.2, one test each ----

#[test]
fn difference_the_settings_are_checked_by_the_host_with_its_texts() {
    let mut rig = Rig::new();
    let g = rig.garage();
    load_script(&mut rig);
    let ghost = "11111111-2222-4333-8444-555555555555";
    let refused = |rig: &mut Rig, recipe: &str, params: Value| {
        let (status, body) = rig.post(BASE, json!({"recipeId": recipe, "params": params}));
        (status, body["error"].as_str().unwrap_or("").to_string())
    };
    let cases = [
        (
            json!({"lights": [g.light]}),
            "Invalid params: zone is required",
            "Invalid params: Zone parameter is required",
        ),
        (
            json!({"zone": g.zone}),
            "Invalid params: lights is required",
            "Invalid params: At least one light is required",
        ),
        (
            json!({"zone": g.zone, "lights": [ghost]}),
            &format!("Invalid params: lights: equipment not found: {ghost}"),
            &format!("Invalid params: Light equipment not found: {ghost}"),
        ),
        (
            json!({"zone": g.zone, "lights": [g.light], "timeout": "soon"}),
            "Invalid params: timeout: invalid duration \"soon\"",
            "Invalid params: Invalid duration format: soon. Use e.g. \"10m\", \"30s\", \"1h\"",
        ),
    ];
    for (params, script, built_in) in cases {
        assert_eq!(
            refused(&mut rig, "motion-light-script", params.clone()),
            (400, script.to_string())
        );
        assert_eq!(
            refused(&mut rig, "motion-light", params),
            (400, built_in.to_string())
        );
    }
}

#[test]
fn difference_a_light_without_a_state_order_and_a_button_without_an_action_are_refused_by_its_own_validate_in_the_built_ins_words()
 {
    let mut rig = Rig::new();
    let g = rig.garage();
    load_script(&mut rig);
    let bare = rig.equipment("Bare light", "light_onoff", &g.zone, &[]);
    let no_action = rig.equipment("Dumb button", "button", &g.zone, &[]);
    for (params, text) in [
        (
            json!({"zone": g.zone, "lights": [bare]}),
            "Invalid params: Light \"Bare light\" has no \"state\" order binding",
        ),
        (
            json!({"zone": g.zone, "lights": [g.light], "buttons": [no_action]}),
            "Invalid params: Button \"Dumb button\" has no \"action\" data binding",
        ),
    ] {
        for recipe in ["motion-light", "motion-light-script"] {
            let (status, body) = rig.post(BASE, json!({"recipeId": recipe, "params": params}));
            assert_eq!(
                (status, body["error"].as_str().unwrap()),
                (400, text),
                "{recipe}"
            );
        }
    }
}

#[test]
fn a_validate_looks_at_the_equipments_its_own_params_name_and_no_others() {
    let mut rig = Rig::new();
    let g = rig.garage();
    let source = format!(
        r#"recipe({{ api: 1, id: "peek", name: "Peek", slots: {{
             lights: {{ type: "equipment", list: true, required: true, equipmentType: "light_onoff" }} }},
           validate: function (params, home) {{
             var named = home.equipment(params.lights[0]), other = home.equipment("{other}");
             return "named " + (named !== null ? named.name + " " + named.orders.join() : "nothing") +
               ", other " + (other !== null ? "seen" : "nothing");
           }},
           start: function () {{}} }});"#,
        other = g.motion_equipment
    );
    let clock: ScriptClock = Arc::new(|| 0);
    rig.home
        .load_script_recipe(&source, Arc::new(SystemRuntimes), clock)
        .unwrap();
    let (status, body) = rig.post(
        BASE,
        json!({"recipeId": "peek", "params": {"zone": g.zone, "lights": [g.light]}}),
    );
    assert_eq!(
        (status, body["error"].as_str().unwrap()),
        (
            400,
            "Invalid params: named Garage light state, other nothing"
        )
    );
}

#[test]
fn difference_a_deleted_equipment_leaves_the_settings_by_the_generic_settle_and_a_button_without_its_action_disables_the_instance()
 {
    for flavor in [Flavor::Builtin, Flavor::Script] {
        let mut r = Run::new(flavor, T0, false);
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
        // The equipment deleted: gone from the settings, the instance runs on (both).
        let (status, _) = r.rig.delete(&format!("/api/v1/equipments/{other}"));
        assert_eq!(status, 204);
        let (_, list) = r.rig.get(BASE);
        assert_eq!(list[0]["enabled"], true, "{flavor:?}");
        assert_eq!(list[0]["params"]["buttons"], json!([button]), "{flavor:?}");
        // The binding removed: the built-in prunes the button and runs on, the script's validate
        // fails and the instance is disabled.
        let (_, item) = r.rig.get(&format!("/api/v1/equipments/{button}"));
        let binding = item["dataBindings"][0]["id"].as_str().unwrap().to_string();
        let (status, _) = r.rig.delete(&format!(
            "/api/v1/equipments/{button}/data-bindings/{binding}"
        ));
        assert_eq!(status, 204);
        let (_, list) = r.rig.get(BASE);
        let messages = r.rig.messages(&r.id);
        match flavor {
            Flavor::Builtin => {
                assert_eq!(list[0]["enabled"], true);
                assert_eq!(list[0]["params"]["buttons"], json!([]));
                assert!(
                    messages.contains(&"Button removed from settings: Wall button".to_string()),
                    "{messages:?}"
                );
            }
            Flavor::Script => {
                assert_eq!(list[0]["enabled"], false);
                assert_eq!(
                    messages.last().unwrap(),
                    "Disabled: Button \"Wall button\" has no \"action\" data binding"
                );
            }
        }
    }
}

#[test]
fn difference_the_legacy_light_key_is_not_read() {
    let mut rig = Rig::new();
    let g = rig.garage();
    load_script(&mut rig);
    let params = json!({"zone": g.zone, "light": g.light});
    let (status, body) = rig.post(BASE, json!({"recipeId": "motion-light", "params": params}));
    assert_eq!(status, 201, "{body}");
    let (status, body) = rig.post(
        BASE,
        json!({"recipeId": "motion-light-script", "params": params}),
    );
    assert_eq!(
        (status, body["error"].as_str().unwrap()),
        (400, "Invalid params: lights is required")
    );
}

#[test]
fn difference_a_refused_order_logs_the_hosts_reason_where_the_built_in_drops_it_silently() {
    // Three instances of sixteen lights start together: 48 orders, the queue keeps 32, so the
    // third instance's are refused (`queue full`).
    let mut logs = Vec::new();
    for flavor in [Flavor::Builtin, Flavor::Script] {
        let mut r = Run::new(flavor, T0, false);
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
        logs.push(r.rig.messages(&ids[2]));
    }
    assert!(
        !logs[0].iter().any(|m| m.starts_with("Error")),
        "the built-in is silent: {:?}",
        logs[0]
    );
    let error = logs[1]
        .iter()
        .find(|m| m.starts_with("Error turning off some lights: "))
        .unwrap_or_else(|| panic!("{:?}", logs[1]));
    assert!(error.contains(": queue full"), "{error}");
    // The log keeps 200 bytes of a line: the first refusals of the sixteen.
    assert!(error.matches("queue full").count() >= 3, "{error}");
}

#[test]
fn difference_the_echo_window_is_counted_on_the_wall_clock() {
    // The light goes off by the timeout; the next motion arrives 500 ms later on the monotonic
    // clock while the wall clock has jumped 3 s ahead (the clock was set). The built-in, counting
    // the monotonic clock, is still in its 2 s window and ignores the motion; the script, counting
    // the wall clock, is out of it and switches the light on.
    let mut sent = Vec::new();
    for flavor in [Flavor::Builtin, Flavor::Script] {
        let mut r = Run::new(flavor, T0, false);
        r.start(json!({}));
        r.motion(true);
        r.relay(true);
        r.motion(false);
        r.advance(20 * SECOND);
        r.rig.sent();
        // The echo and the motion, 500 ms later by the monotonic clock, 3.5 s by the wall.
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
        sent.push(r.rig.sent());
    }
    assert_eq!(sent[0], [], "the built-in ignores the motion in its window");
    assert_eq!(sent[1].len(), 1, "the script switches on: {:?}", sent[1]);
}
