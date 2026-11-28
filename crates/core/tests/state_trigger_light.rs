//! `state-trigger-light`, Sowel's recipe 0.2.3 as the script `recipes/scripts/state-trigger-light.js`
//! (spec 055): its info (pinned from Sowel's source), Sowel's validation texts, the lights on for
//! a duration when the watched state turns to the target, the night rule, the re-report rule, the
//! external switch-off, and the memory of 4 instances.
//!
//! These tests need the `script-recipes` feature (the host program turns it on, so
//! `cargo test --workspace` runs them).

mod support;

use std::sync::Arc;

use serde_json::{Value, json};
use solite_core::devices::Id;
use solite_core::radio::zcl::Command;
use solite_core::recipes::native::{NativeHost, RecipeName};
use solite_core::recipes::script::{
    NewRuntime, SCRIPT_MEMORY_LIMIT, ScriptRecipe, ScriptRuntimes, SystemRuntimes,
};
use solite_core::recipes::{MAX_INSTANCES_PER_SCRIPT, ScriptClock};
use solite_recipe_sdk::{Clock, Event, EventValue, Recipe};
use support::*;

const SCRIPT: &str = include_str!("../../../recipes/scripts/state-trigger-light.js");
const SECOND: u64 = 1000;
const BASE: &str = "/api/v1/recipe-instances";
const GHOST: &str = "11111111-2222-4333-8444-555555555555";
const GATE: u8 = 0x7B;
/// 2026-11-21 00:00:00 UTC.
const MIDNIGHT: u64 = T0 - (9 * HOUR + 12 * MINUTE + 3 * SECOND + 120);

struct World {
    rig: Rig,
    g: Garage,
    gate: String,
}

impl World {
    /// The garage with a gate (a relay, OFF) beside its light; the script loaded; the home in
    /// Paris (an hour ahead of UTC in November) and at its latitude, so that the sun is known.
    fn new(now: u64) -> World {
        let mut rig = Rig::new();
        rig.now = now;
        let g = rig.garage();
        let clock: ScriptClock = Arc::new(|| 0);
        rig.home
            .load_script_recipe(SCRIPT, Arc::new(SystemRuntimes), clock)
            .unwrap();
        let (status, _) = rig.put(
            "/api/v1/settings",
            json!({"home.timezone": "Europe/Paris", "home.latitude": "48.8566", "home.longitude": "2.3522"}),
        );
        assert_eq!(status, 200);
        let device = rig.relay(GATE);
        let gate = rig.equipment("Gate", "switch", &g.zone, &[&device]);
        rig.report(&state(GATE, false));
        rig.sent();
        World { rig, g, gate }
    }

    fn create(&mut self, params: Value) -> (u16, Value) {
        self.rig.post(
            BASE,
            json!({"recipeId": "state-trigger-light", "params": params}),
        )
    }

    fn start(&mut self, extra: Value) -> String {
        let mut params = json!({
            "zone": self.g.zone,
            "trigger": self.gate,
            "stateValue": "true",
            "lights": [self.g.light],
            "duration": "20s",
        });
        for (key, value) in extra.as_object().unwrap() {
            params[key] = value.clone();
        }
        let (status, body) = self.create(params);
        assert_eq!(status, 201, "{body}");
        body["id"].as_str().unwrap().to_string()
    }

    fn gate(&mut self, on: bool) {
        self.rig.report(&state(GATE, on));
    }

    fn lamp(&mut self, on: bool) {
        self.rig.report(&state(RELAY, on));
    }

    fn sent(&mut self) -> Vec<Command> {
        self.rig.sent()
    }

    fn log(&mut self, id: &str) -> String {
        self.rig.messages(id).join("\n")
    }

    fn state(&mut self, id: &str) -> Value {
        self.rig.state(id)
    }
}

fn refused(w: &mut World, params: Value) -> (u16, String) {
    let (status, body) = w.create(params);
    (status, body["error"].as_str().unwrap_or("").to_string())
}

fn invalid(message: &str) -> (u16, String) {
    (400, format!("Invalid params: {message}"))
}

// ---- info ----

#[test]
fn the_recipe_is_served_with_sowels_info_byte_for_byte() {
    let mut w = World::new(T0);
    let response = w.rig.raw("GET", "/api/v1/recipes/state-trigger-light", "");
    assert_eq!(response.status, 200);
    assert_eq!(
        response.body.unwrap(),
        include_str!("pinned/recipe_state_trigger_light.json")
    );
}

// ---- validation ----

#[test]
fn every_refused_setting_answers_sowels_text_in_sowels_order() {
    let mut w = World::new(T0);
    let (zone, light, gate) = (w.g.zone.clone(), w.g.light.clone(), w.gate.clone());
    let dimmable = w.rig.equipment("Lamp", "light_dimmable", &zone, &[]);
    let motion = w.g.motion_equipment.clone();
    let ok = json!({"zone": zone, "trigger": gate, "stateValue": "true", "lights": [light]});
    let with = |extra: Value| {
        let mut params = ok.clone();
        for (key, value) in extra.as_object().unwrap() {
            params[key] = value.clone();
        }
        params
    };
    let cases: Vec<(Value, String)> = vec![
        (json!({}), "Zone is required".into()),
        (json!({"zone": 5}), "Zone is required".into()),
        (json!({"zone": GHOST}), "Zone not found".into()),
        (
            json!({"zone": zone}),
            "Trigger equipment is required".into(),
        ),
        (
            json!({"zone": zone, "trigger": GHOST}),
            "Trigger equipment not found".into(),
        ),
        (
            json!({"zone": zone, "trigger": motion.clone(), "stateValue": "x", "lights": [light]}),
            "Trigger \"Garage motion\" has no \"state\" data binding".into(),
        ),
        (
            json!({"zone": zone, "trigger": gate}),
            "Target state value is required".into(),
        ),
        (
            json!({"zone": zone, "trigger": gate, "stateValue": "  "}),
            "Target state value is required".into(),
        ),
        (
            json!({"zone": zone, "trigger": gate, "stateValue": "true"}),
            "At least one light is required".into(),
        ),
        (
            with(json!({"lights": [GHOST]})),
            format!("Light {GHOST} not found"),
        ),
        (
            with(json!({"lights": [dimmable]})),
            "\"Lamp\" is not a light_onoff equipment".into(),
        ),
        (
            with(json!({"duration": "soon"})),
            "Invalid duration format: soon. Use e.g. \"10m\", \"30s\", \"1h\"".into(),
        ),
        (with(json!({"duration": 0})), "Duration must be > 0".into()),
    ];
    for (params, message) in cases {
        assert_eq!(
            refused(&mut w, params.clone()),
            invalid(&message),
            "{params}"
        );
    }
    // The order of the checks: the first failure wins (the lights before the duration).
    assert_eq!(
        refused(&mut w, with(json!({"lights": [GHOST], "duration": "soon"}))),
        invalid(&format!("Light {GHOST} not found"))
    );
    // A text longer than the host's bound is refused by the host.
    assert_eq!(
        refused(&mut w, with(json!({"stateValue": "x".repeat(65)}))),
        invalid("stateValue: at most 64 bytes")
    );
}

// ---- behaviour ----

#[test]
fn the_trigger_turning_to_the_target_lights_the_lamp_for_the_duration() {
    let mut w = World::new(MIDNIGHT + 22 * HOUR);
    let id = w.start(json!({}));
    w.gate(true);
    assert_eq!(w.sent(), vec![Command::On]);
    assert!(w.state(&id).get("expiresAt").is_some());
    assert!(
        w.log(&id)
            .contains("State trigger fired — 1 light(s) on for 20s")
    );
    w.lamp(true);
    w.rig.advance(10 * SECOND);
    assert_eq!(w.sent(), vec![], "not yet");
    w.rig.advance(10 * SECOND);
    assert_eq!(w.sent(), vec![Command::Off]);
    assert!(w.state(&id).get("expiresAt").is_none());
    assert!(w.log(&id).contains("State trigger off after 20s"));
}

#[test]
fn a_target_reported_again_does_not_fire_and_a_new_transition_does() {
    let mut w = World::new(MIDNIGHT + 22 * HOUR);
    w.start(json!({}));
    w.gate(true);
    assert_eq!(w.sent(), vec![Command::On]);
    w.lamp(true);
    w.rig.advance(20 * SECOND);
    assert_eq!(w.sent(), vec![Command::Off]);
    w.lamp(false);
    w.rig.advance(3 * SECOND);
    // Still open, reported again: not a transition.
    w.gate(true);
    assert_eq!(w.sent(), vec![]);
    w.gate(false);
    w.gate(true);
    assert_eq!(w.sent(), vec![Command::On]);
}

#[test]
fn by_day_the_trigger_is_ignored_unless_night_only_is_off() {
    // 11:00 UTC is noon in Paris, in the sun's day.
    let mut w = World::new(MIDNIGHT + 11 * HOUR);
    let id = w.start(json!({}));
    w.gate(true);
    assert_eq!(w.sent(), vec![]);
    assert!(w.log(&id).contains("State trigger ignored — daytime"));
    w.gate(false);
    let mut w = World::new(MIDNIGHT + 11 * HOUR);
    w.start(json!({"nightOnly": false}));
    w.gate(true);
    assert_eq!(w.sent(), vec![Command::On]);
}

#[test]
fn a_lamp_already_on_is_left_alone() {
    let mut w = World::new(MIDNIGHT + 22 * HOUR);
    w.lamp(true);
    let id = w.start(json!({}));
    w.gate(true);
    assert_eq!(w.sent(), vec![]);
    assert!(
        w.log(&id)
            .contains("State trigger ignored — at least one light already on")
    );
}

#[test]
fn a_lamp_switched_off_by_hand_cancels_the_timer() {
    let mut w = World::new(MIDNIGHT + 22 * HOUR);
    let id = w.start(json!({}));
    w.gate(true);
    w.sent();
    w.lamp(true);
    w.lamp(false);
    assert!(w.state(&id).get("expiresAt").is_none());
    assert!(
        w.log(&id)
            .contains("Lights turned off externally — timer cancelled")
    );
    w.rig.advance(30 * SECOND);
    assert_eq!(w.sent(), vec![], "no second off order");
}

// ---- limits ----

/// A roomy limit: the Mac counts with 64-bit pointers, the box's figure is measured in QEMU.
struct Roomy;

impl ScriptRuntimes for Roomy {
    fn new_runtime(&self) -> Result<NewRuntime, String> {
        SystemRuntimes.new_runtime()
    }

    fn memory_limit(&self) -> usize {
        1024 * 1024
    }
}

#[test]
fn four_instances_of_the_script_stay_within_its_memory() {
    let w = World::new(MIDNIGHT + 22 * HOUR);
    let runtimes: Arc<dyn ScriptRuntimes> = Arc::new(Roomy);
    let clock: ScriptClock = Arc::new(|| 0);
    let (mut recipe, _) = ScriptRecipe::load(SCRIPT, &runtimes, &clock).unwrap();
    let names = RecipeName {
        id: "state-trigger-light".into(),
        name: "State-Triggered Light".into(),
    };
    let clock = Clock {
        mono: w.rig.now,
        wall: w.rig.now,
    };
    let params = json!({
        "zone": w.g.zone, "trigger": w.gate, "stateValue": "true",
        "lights": [w.g.light], "duration": 300_000, "nightOnly": true,
    });
    let empty = recipe.held_bytes();
    let host = NativeHost::new(&w.rig.home, Id::ROOT_ZONE, &names, clock.mono, clock.wall);
    for handle in 0..MAX_INSTANCES_PER_SCRIPT as u32 {
        recipe.start(&host, handle, &params, clock).unwrap();
    }
    assert_eq!(recipe.instance_count(), MAX_INSTANCES_PER_SCRIPT);
    let events = [
        Event::Equipment {
            equipment: w.gate.clone(),
            alias: "state".into(),
            value: EventValue::Bool(true),
        },
        Event::Equipment {
            equipment: w.g.light.clone(),
            alias: "state".into(),
            value: EventValue::Bool(true),
        },
        Event::Equipment {
            equipment: w.g.light.clone(),
            alias: "state".into(),
            value: EventValue::Bool(false),
        },
    ];
    for handle in 0..MAX_INSTANCES_PER_SCRIPT as u32 {
        for event in &events {
            recipe.on_event(&host, handle, event, clock).unwrap();
        }
    }
    let held = recipe.held_bytes();
    println!(
        "state-trigger-light: {} bytes of source, {empty} bytes loaded, {held} bytes with {MAX_INSTANCES_PER_SCRIPT} instances (the box's limit is {SCRIPT_MEMORY_LIMIT})",
        SCRIPT.len()
    );
    assert!(SCRIPT.len() < 32 * 1024, "{}", SCRIPT.len());
    assert!(held < SCRIPT_MEMORY_LIMIT, "{held}");
}
