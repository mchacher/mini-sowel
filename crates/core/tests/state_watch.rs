//! `state-watch`, Sowel's recipe 3.1.0 as the script `recipes/scripts/state-watch.js` (spec 055):
//! its info (pinned from Sowel's source, with the `data-key` and `text` slots), Sowel's
//! validation texts, the alarm raised and cleared with the watched value, with a delay, and
//! inside a check window in the home's time zone (one across midnight), the summary, and the
//! memory of 4 instances.
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

const SCRIPT: &str = include_str!("../../../recipes/scripts/state-watch.js");
const SECOND: u64 = 1000;
const BASE: &str = "/api/v1/recipe-instances";
const GHOST: &str = "11111111-2222-4333-8444-555555555555";
const GATE: u8 = 0x7B;
/// 2026-11-21 00:00:00 UTC.
const MIDNIGHT: u64 = T0 - (9 * HOUR + 12 * MINUTE + 3 * SECOND + 120);

/// The instant of a Paris time of the day (an hour ahead of UTC in November); hours from 1 to 23.
fn paris(day: u64, hour: u64, minute: u64) -> u64 {
    MIDNIGHT + day * 24 * HOUR + (hour - 1) * HOUR + minute * MINUTE
}

struct World {
    rig: Rig,
    g: Garage,
    gate: String,
}

impl World {
    /// The garage with a gate (a relay, OFF, `state` data) beside its light; the script loaded; the
    /// home in Paris.
    fn new(now: u64) -> World {
        let mut rig = Rig::new();
        rig.now = now;
        let g = rig.garage();
        let clock: ScriptClock = Arc::new(|| 0);
        rig.home
            .load_script_recipe(SCRIPT, Arc::new(SystemRuntimes), clock)
            .unwrap();
        let (status, _) = rig.put("/api/v1/settings", json!({"home.timezone": "Europe/Paris"}));
        assert_eq!(status, 200);
        let device = rig.relay(GATE);
        let gate = rig.equipment("Gate", "switch", &g.zone, &[&device]);
        rig.report(&state(GATE, false));
        rig.sent();
        World { rig, g, gate }
    }

    fn create(&mut self, params: Value) -> (u16, Value) {
        self.rig
            .post(BASE, json!({"recipeId": "state-watch", "params": params}))
    }

    fn start(&mut self, extra: Value) -> String {
        let mut params = json!({
            "zone": self.g.zone,
            "equipment": self.gate,
            "dataKey": "state",
            "watchValue": "true",
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

    /// Time moves to `at` (a turn at the end, a second after).
    fn go(&mut self, at: u64) {
        self.rig.now = at;
        self.rig.advance(SECOND);
    }

    fn state(&mut self, id: &str) -> Value {
        self.rig.state(id)
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
    let mut w = World::new(T0);
    let response = w.rig.raw("GET", "/api/v1/recipes/state-watch", "");
    assert_eq!(response.status, 200);
    assert_eq!(
        response.body.unwrap(),
        include_str!("pinned/recipe_state_watch.json")
    );
}

// ---- validation ----

#[test]
fn every_refused_setting_answers_sowels_text_in_sowels_order() {
    let mut w = World::new(T0);
    let (zone, gate) = (w.g.zone.clone(), w.gate.clone());
    let other = w.rig.zone("Hall");
    let away = w.rig.equipment("Away", "switch", &other, &[]);
    let ok = json!({"zone": zone, "equipment": gate, "dataKey": "state", "watchValue": "true"});
    let with = |extra: Value| {
        let mut params = ok.clone();
        for (key, value) in extra.as_object().unwrap() {
            params[key] = value.clone();
        }
        params
    };
    let cases: Vec<(Value, String)> = vec![
        (json!({}), "Zone parameter is required".into()),
        (json!({"zone": GHOST}), format!("Zone not found: {GHOST}")),
        (
            json!({"zone": zone}),
            "Equipment parameter is required".into(),
        ),
        (
            json!({"zone": zone, "equipment": GHOST}),
            format!("Equipment not found: {GHOST}"),
        ),
        (
            json!({"zone": zone, "equipment": away}),
            "Equipment \"Away\" does not belong to the selected zone".into(),
        ),
        (
            json!({"zone": zone, "equipment": gate}),
            "Data key parameter is required".into(),
        ),
        (
            json!({"zone": zone, "equipment": gate, "dataKey": "contact"}),
            "Equipment \"Gate\" has no data binding with alias \"contact\"".into(),
        ),
        (
            json!({"zone": zone, "equipment": gate, "dataKey": "state"}),
            "Watch value parameter is required".into(),
        ),
        (
            with(json!({"watchValue": ""})),
            "Watch value parameter is required".into(),
        ),
        (
            with(json!({"delay": "soon"})),
            "Invalid duration format: soon. Use e.g. \"10m\", \"30s\", \"1h\"".into(),
        ),
        (
            with(json!({"checkStart": "21:00"})),
            "The check window needs both a start and an end time".into(),
        ),
        (
            with(json!({"checkEnd": "06:00"})),
            "The check window needs both a start and an end time".into(),
        ),
        (
            with(json!({"checkStart": "9h", "checkEnd": "06:00"})),
            "Invalid start time: 9h. Use HH:MM (e.g., \"21:00\")".into(),
        ),
        (
            with(json!({"checkStart": "21:00", "checkEnd": "6"})),
            "Invalid end time: 6. Use HH:MM (e.g., \"06:00\")".into(),
        ),
    ];
    for (params, message) in cases {
        assert_eq!(
            refused(&mut w, params.clone()),
            invalid(&message),
            "{params}"
        );
    }
    // The host's own bounds: a text over 64 bytes.
    assert_eq!(
        refused(&mut w, with(json!({"watchValue": "x".repeat(65)}))),
        invalid("watchValue: at most 64 bytes")
    );
    let (status, body) = w.create(with(
        json!({"delay": "5m", "checkStart": "21:00", "checkEnd": "06:00"}),
    ));
    assert_eq!(status, 201, "{body}");
}

// ---- behaviour ----

#[test]
fn the_alarm_is_raised_while_the_value_is_watched_and_cleared_when_it_leaves() {
    let mut w = World::new(paris(0, 12, 0));
    let id = w.start(json!({}));
    let state = w.state(&id);
    assert_eq!(state["alarm"], false);
    assert_eq!(state["summary"], "state : true");
    w.gate(true);
    let state = w.state(&id);
    assert_eq!(state["alarm"], true);
    assert!(state["alarmSince"].is_string());
    assert_eq!(state["summary"], "state : true en alarme");
    assert!(w.log(&id).contains("ALARM: state=true"));
    w.gate(false);
    let state = w.state(&id);
    assert_eq!(state["alarm"], false);
    assert_eq!(state["alarmSince"], Value::Null);
    assert_eq!(state["summary"], "state : true");
    assert!(w.log(&id).contains("state — alarm cleared"));
}

#[test]
fn a_value_already_watched_at_the_start_raises_the_alarm_at_once() {
    let mut w = World::new(paris(0, 12, 0));
    w.gate(true);
    let id = w.start(json!({}));
    assert_eq!(w.state(&id)["alarm"], true);
    assert!(w.log(&id).contains("Current: state=true"));
}

#[test]
fn with_a_delay_the_alarm_waits_for_it_and_a_return_before_cancels_it() {
    let mut w = World::new(paris(0, 12, 0));
    let id = w.start(json!({"delay": "5m"}));
    assert_eq!(w.state(&id)["summary"], "state : true après 5 min");
    w.gate(true);
    assert!(w.state(&id)["watchStartedAt"].is_string());
    assert!(w.log(&id).contains("state=true — alarm in 5min"));
    w.rig.advance(4 * MINUTE);
    assert_eq!(w.state(&id)["alarm"], false);
    // Back before the delay: nothing raised, the pending delay forgotten.
    w.gate(false);
    assert!(w.state(&id).get("watchStartedAt").is_none());
    w.rig.advance(2 * MINUTE);
    assert_eq!(w.state(&id)["alarm"], false);
    // Again, and this time it stays.
    w.gate(true);
    w.rig.advance(5 * MINUTE + SECOND);
    assert_eq!(w.state(&id)["alarm"], true);
    assert_eq!(w.state(&id)["summary"], "state : true en alarme");
}

#[test]
fn a_check_window_across_midnight_alarms_only_inside_it_in_the_homes_time() {
    let mut w = World::new(paris(0, 12, 0));
    let id = w.start(json!({"checkStart": "21:00", "checkEnd": "06:00"}));
    assert_eq!(w.state(&id)["summary"], "state : true de 21:00 à 06:00");
    // Outside the window: the value is watched, no alarm.
    w.gate(true);
    assert_eq!(w.state(&id)["alarm"], false);
    // The window opens at 21:00 Paris.
    w.go(paris(0, 20, 59));
    assert_eq!(w.state(&id)["alarm"], false);
    w.go(paris(0, 21, 0));
    assert_eq!(w.state(&id)["alarm"], true);
    // It closes at 06:00 the next morning.
    w.go(paris(1, 5, 59));
    assert_eq!(w.state(&id)["alarm"], true);
    w.go(paris(1, 6, 0));
    assert_eq!(w.state(&id)["alarm"], false);
    assert!(w.log(&id).contains("state — alarm cleared"));
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
    let w = World::new(paris(0, 12, 0));
    let runtimes: Arc<dyn ScriptRuntimes> = Arc::new(Roomy);
    let clock: ScriptClock = Arc::new(|| 0);
    let (mut recipe, _) = ScriptRecipe::load(SCRIPT, &runtimes, &clock).unwrap();
    let names = RecipeName {
        id: "state-watch".into(),
        name: "State Watch".into(),
    };
    let clock = Clock {
        mono: w.rig.now,
        wall: w.rig.now,
    };
    let params = json!({
        "zone": w.g.zone, "equipment": w.gate, "dataKey": "state", "watchValue": "true",
        "delay": 300_000, "checkStart": "21:00", "checkEnd": "06:00",
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
            equipment: w.gate.clone(),
            alias: "state".into(),
            value: EventValue::Bool(false),
        },
    ];
    for handle in 0..MAX_INSTANCES_PER_SCRIPT as u32 {
        for event in &events {
            recipe.on_event(&host, handle, event, clock).unwrap();
        }
    }
    for step in 1..=4u64 {
        let clock = Clock {
            mono: clock.mono + step * 7 * HOUR,
            wall: clock.wall + step * 7 * HOUR,
        };
        let host = NativeHost::new(&w.rig.home, Id::ROOT_ZONE, &names, clock.mono, clock.wall);
        for handle in 0..MAX_INSTANCES_PER_SCRIPT as u32 {
            recipe.tick(&host, handle, clock).unwrap();
        }
    }
    let held = recipe.held_bytes();
    println!(
        "state-watch: {} bytes of source, {empty} bytes loaded, {held} bytes with {MAX_INSTANCES_PER_SCRIPT} instances (the box's limit is {SCRIPT_MEMORY_LIMIT})",
        SCRIPT.len()
    );
    assert!(SCRIPT.len() < 32 * 1024, "{}", SCRIPT.len());
    assert!(held < SCRIPT_MEMORY_LIMIT, "{held}");
}
