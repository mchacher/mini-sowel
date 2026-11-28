//! `switch-light`, Sowel's recipe 1.0.0 as the script `recipes/scripts/switch-light.js` (spec 052):
//! its info (pinned from Sowel's source), Sowel's validation texts, the toggle by a button's
//! `action` (spec 049), the failsafe, and the memory of 8 instances.
//!
//! These tests need the `script-recipes` feature (the host program turns it on, so
//! `cargo test --workspace` runs them).

mod support;

use std::sync::Arc;

use serde_json::{Value, json};
use solite_core::devices::Id;
use solite_core::recipes::native::{NativeHost, RecipeName};
use solite_core::recipes::script::{
    NewRuntime, SCRIPT_MEMORY_LIMIT, ScriptRecipe, ScriptRuntimes, SystemRuntimes,
};
use solite_core::recipes::{MAX_INSTANCES_PER_SCRIPT, ScriptClock};
use solite_recipe_sdk::{Clock, Event, EventValue, Recipe};
use support::*;

const SWITCH_LIGHT: &str = include_str!("../../../recipes/scripts/switch-light.js");
const SECOND: u64 = 1000;
const BASE: &str = "/api/v1/recipe-instances";
const GHOST: &str = "11111111-2222-4333-8444-555555555555";

struct World {
    rig: Rig,
    g: Garage,
    button: String,
}

impl World {
    fn new() -> World {
        let mut rig = Rig::new();
        rig.now = T0;
        let g = rig.garage();
        let clock: ScriptClock = Arc::new(|| 0);
        rig.home
            .load_script_recipe(SWITCH_LIGHT, Arc::new(SystemRuntimes), clock)
            .unwrap();
        let remote = rig.remote(0x70);
        let data = rig.home.devices.get(&remote).unwrap().data[0]
            .id
            .to_string();
        let button = rig.equipment("Remote", "button", &g.zone, &[]);
        let (status, body) = rig.post(
            &format!("/api/v1/equipments/{button}/data-bindings"),
            json!({"deviceDataId": data, "alias": "action"}),
        );
        assert_eq!(status, 201, "{body}");
        World { rig, g, button }
    }

    fn create(&mut self, params: Value) -> (u16, Value) {
        self.rig
            .post(BASE, json!({"recipeId": "switch-light", "params": params}))
    }

    fn start(&mut self, extra: Value) -> String {
        let mut params = json!({
            "zone": self.g.zone,
            "lights": [self.g.light],
            "buttons": [self.button],
        });
        for (key, value) in extra.as_object().unwrap() {
            params[key] = value.clone();
        }
        let (status, body) = self.create(params);
        assert_eq!(status, 201, "{body}");
        body["id"].as_str().unwrap().to_string()
    }

    fn press(&mut self) {
        self.rig
            .report(&action(0x70, solite_core::radio::zcl::Action::Toggle));
    }

    fn light(&mut self, on: bool) {
        self.rig.report(&state(RELAY, on));
    }

    fn sent(&mut self) -> String {
        format!("{:?}", self.rig.sent())
    }

    fn state(&mut self, id: &str) -> Value {
        let (_, list) = self.rig.get(BASE);
        list.as_array()
            .unwrap()
            .iter()
            .find(|i| i["id"] == id)
            .unwrap()["state"]
            .clone()
    }

    fn log(&mut self, id: &str) -> String {
        self.rig.messages(id).join("\n")
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
    let mut w = World::new();
    let response = w.rig.raw("GET", "/api/v1/recipes/switch-light", "");
    assert_eq!(response.status, 200);
    assert_eq!(
        response.body.unwrap(),
        include_str!("pinned/recipe_switch_light.json")
    );
}

// ---- validation ----

#[test]
fn every_refused_setting_answers_sowels_text_in_sowels_order() {
    let mut w = World::new();
    let (zone, light, button) = (w.g.zone.clone(), w.g.light.clone(), w.button.clone());
    let no_state = w.rig.equipment("Bare light", "light_onoff", &zone, &[]);
    let other_zone = w.rig.zone("Elsewhere");
    let relay = w.rig.relay(0x62);
    let far = w
        .rig
        .equipment("Far light", "light_onoff", &other_zone, &[&relay]);
    let motion = w.g.motion_equipment.clone();
    let cases: Vec<(Value, String)> = vec![
        (json!({}), "Zone parameter is required".into()),
        (json!({"zone": ""}), "Zone parameter is required".into()),
        (json!({"zone": 5}), "Zone parameter is required".into()),
        (
            json!({"zone": GHOST, "lights": [light]}),
            format!("Zone not found: {GHOST}"),
        ),
        (
            json!({"zone": zone}),
            "At least one light is required".into(),
        ),
        (
            json!({"zone": zone, "lights": []}),
            "At least one light is required".into(),
        ),
        (
            json!({"zone": zone, "lights": [GHOST]}),
            format!("Light equipment not found: {GHOST}"),
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
            json!({"zone": zone, "lights": [light]}),
            "At least one button is required".into(),
        ),
        (
            json!({"zone": zone, "lights": [light], "buttons": []}),
            "At least one button is required".into(),
        ),
        (
            json!({"zone": zone, "lights": [light], "buttons": [GHOST]}),
            format!("Button equipment not found: {GHOST}"),
        ),
        (
            json!({"zone": zone, "lights": [light], "buttons": [motion]}),
            "Button \"Garage motion\" has no \"action\" data binding".into(),
        ),
        (
            json!({"zone": zone, "lights": [light], "buttons": [button], "maxOnDuration": "soon"}),
            "Invalid duration format: soon. Use e.g. \"10m\", \"30s\", \"1h\"".into(),
        ),
    ];
    for (params, message) in cases {
        assert_eq!(
            refused(&mut w, params.clone()),
            invalid(&message),
            "{params}"
        );
    }
    // The order of the checks: the first failure wins.
    assert_eq!(
        refused(
            &mut w,
            json!({"zone": zone, "lights": [light], "buttons": [GHOST], "maxOnDuration": "z"})
        ),
        invalid(&format!("Button equipment not found: {GHOST}"))
    );
}

#[test]
fn a_light_that_is_not_a_light_is_refused_by_the_slot() {
    let mut w = World::new();
    let sensor = w.rig.equipment("A sensor", "sensor", &w.g.zone, &[]);
    let params = json!({"zone": w.g.zone, "lights": [sensor], "buttons": [w.button]});
    let (status, _) = refused(&mut w, params);
    assert_eq!(status, 400);
}

// ---- behaviour ----

#[test]
fn a_press_with_the_lights_off_switches_them_on_and_the_next_press_off() {
    let mut w = World::new();
    let id = w.start(json!({}));
    assert_eq!(w.sent(), "[]", "nothing at the start");
    w.press();
    assert_eq!(w.sent(), "[On]");
    assert!(w.log(&id).contains("Button pressed — lights toggled on"));
    w.light(true);
    w.rig.advance(5 * SECOND);
    w.press();
    assert_eq!(w.sent(), "[Off]");
    assert!(w.log(&id).contains("Button pressed — lights toggled off"));
}

#[test]
fn two_presses_in_a_row_each_toggle() {
    let mut w = World::new();
    w.start(json!({}));
    w.press();
    assert_eq!(w.sent(), "[On]");
    // The light has not reported yet: the second press sees it off still.
    w.press();
    assert_eq!(w.sent(), "[On]");
    w.light(true);
    w.press();
    assert_eq!(w.sent(), "[Off]");
}

#[test]
fn a_press_of_a_button_not_in_the_slot_does_nothing() {
    let mut w = World::new();
    let other = w.rig.remote(0x71);
    let data = w.rig.home.devices.get(&other).unwrap().data[0]
        .id
        .to_string();
    let b2 = w.rig.equipment("Other", "button", &w.g.zone, &[]);
    let (status, body) = w.rig.post(
        &format!("/api/v1/equipments/{b2}/data-bindings"),
        json!({"deviceDataId": data, "alias": "action"}),
    );
    assert_eq!(status, 201, "{body}");
    w.start(json!({}));
    w.rig
        .report(&action(0x71, solite_core::radio::zcl::Action::Toggle));
    assert_eq!(w.sent(), "[]");
}

#[test]
fn the_failsafe_forces_the_lights_off_and_a_manual_off_cancels_it() {
    let mut w = World::new();
    let id = w.start(json!({"maxOnDuration": "10s"}));
    w.press();
    assert_eq!(w.sent(), "[On]");
    assert!(w.state(&id).get("failsafeExpiresAt").is_some());
    w.light(true);
    w.rig.advance(10 * SECOND);
    assert_eq!(w.sent(), "[Off]");
    assert!(
        w.log(&id)
            .contains("Failsafe: lights forced off after 10s max on duration")
    );
    assert!(w.state(&id).get("failsafeExpiresAt").is_none());
    // The light goes off by hand: the armed failsafe is cancelled.
    w.light(false);
    w.press();
    w.sent();
    w.light(true);
    assert!(w.state(&id).get("failsafeExpiresAt").is_some());
    w.light(false);
    assert!(w.state(&id).get("failsafeExpiresAt").is_none());
    w.rig.advance(60 * SECOND);
    assert_eq!(w.sent(), "[]");
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
fn eight_instances_of_the_script_stay_within_its_memory() {
    let w = World::new();
    let runtimes: Arc<dyn ScriptRuntimes> = Arc::new(Roomy);
    let clock: ScriptClock = Arc::new(|| 0);
    let (mut recipe, _) = ScriptRecipe::load(SWITCH_LIGHT, &runtimes, &clock).unwrap();
    let names = RecipeName {
        id: "switch-light".into(),
        name: "Switch Light".into(),
    };
    let clock = Clock {
        mono: w.rig.now,
        wall: w.rig.now,
    };
    let params = json!({
        "zone": w.g.zone,
        "lights": [w.g.light],
        "buttons": [w.button],
        "maxOnDuration": 300_000,
    });
    let empty = recipe.held_bytes();
    let host = NativeHost::new(&w.rig.home, Id::ROOT_ZONE, &names, clock.mono, clock.wall);
    for handle in 0..MAX_INSTANCES_PER_SCRIPT as u32 {
        recipe.start(&host, handle, &params, clock).unwrap();
    }
    assert_eq!(recipe.instance_count(), MAX_INSTANCES_PER_SCRIPT);
    // A press and a light changing, for every instance.
    let press = Event::Equipment {
        equipment: w.button.clone(),
        alias: "action".into(),
        value: EventValue::Text("toggle".into()),
    };
    let on = Event::Equipment {
        equipment: w.g.light.clone(),
        alias: "state".into(),
        value: EventValue::Bool(true),
    };
    for handle in 0..MAX_INSTANCES_PER_SCRIPT as u32 {
        recipe.on_event(&host, handle, &press, clock).unwrap();
        recipe.on_event(&host, handle, &on, clock).unwrap();
    }
    let held = recipe.held_bytes();
    println!(
        "switch-light: {} bytes of source, {empty} bytes loaded, {held} bytes with 8 instances (the box's limit is {SCRIPT_MEMORY_LIMIT})",
        SWITCH_LIGHT.len()
    );
    assert!(held < SCRIPT_MEMORY_LIMIT, "{held}");
}
