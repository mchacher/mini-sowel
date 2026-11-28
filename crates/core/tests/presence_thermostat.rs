//! `presence-thermostat`, Sowel's recipe 1.2.0 as the script `recipes/scripts/presence-thermostat.js`
//! (spec 055): its info (pinned from Sowel's source), Sowel's validation texts, the setpoint by
//! the zone's presence, the eco timeout, the manual override, the night window, the weekday and
//! weekend preheat windows (in the home's time zone), the cocoon button, and the memory of 4
//! instances.
//!
//! These tests need the `script-recipes` feature (the host program turns it on, so
//! `cargo test --workspace` runs them).

mod support;

use std::sync::Arc;

use serde_json::{Value, json};
use solite_core::devices::Id;
use solite_core::radio::measures::{Measure, Value as Reading};
use solite_core::radio::zcl::{Action, Command};
use solite_core::radio::{MeasureEvent, Sender};
use solite_core::recipes::native::{NativeHost, RecipeName};
use solite_core::recipes::script::{
    NewRuntime, SCRIPT_MEMORY_LIMIT, ScriptRecipe, ScriptRuntimes, SystemRuntimes,
};
use solite_core::recipes::{MAX_INSTANCES_PER_SCRIPT, ScriptClock};
use solite_recipe_sdk::{Clock, Event, EventValue, Recipe};
use support::*;

const SCRIPT: &str = include_str!("../../../recipes/scripts/presence-thermostat.js");
const SECOND: u64 = 1000;
const BASE: &str = "/api/v1/recipe-instances";
const GHOST: &str = "11111111-2222-4333-8444-555555555555";
const THERMOSTAT: u8 = 0x61;
const BUTTON: u8 = 0x62;
/// 2026-11-21 00:00:00 UTC, a Saturday.
const MIDNIGHT: u64 = T0 - (9 * HOUR + 12 * MINUTE + 3 * SECOND + 120);

/// The instant of a Paris time of the day (an hour ahead of UTC in November), `day` days after
/// Saturday the 21st; hours from 1 to 23.
fn paris(day: u64, hour: u64, minute: u64) -> u64 {
    MIDNIGHT + day * 24 * HOUR + (hour - 1) * HOUR + minute * MINUTE
}

fn setpoint(last: u8, degrees: f64) -> MeasureEvent {
    MeasureEvent {
        sender: Sender::Eui64(eui(last)),
        endpoint: 1,
        measures: vec![Measure {
            kind: "setpoint",
            value: Reading::Number(degrees),
            unit: "°C",
            device_value: None,
        }],
    }
}

struct World {
    rig: Rig,
    g: Garage,
    thermostat: String,
    button: String,
}

impl World {
    /// The garage with a thermostat (a Thermostat cluster device) and a remote beside its motion
    /// sensor; the script loaded; the home in Paris.
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
        let e = eui(THERMOSTAT);
        rig.home.devices.announced(e, 0x8C, rig.now);
        rig.home
            .devices
            .identified(e, Some("TEST"), Some("THM01"), rig.now, &mut *rig.random)
            .unwrap();
        rig.home
            .devices
            .reporting_configured(e, &[0x0201], rig.now, &mut *rig.random)
            .unwrap();
        let device = rig.device(THERMOSTAT);
        let thermostat = rig.equipment("Thermostat", "thermostat", &g.zone, &[&device]);
        let remote = rig.remote(BUTTON);
        let button = rig.equipment("Button", "button", &g.zone, &[&remote]);
        rig.report(&setpoint(THERMOSTAT, 19.0));
        rig.sent();
        World {
            rig,
            g,
            thermostat,
            button,
        }
    }

    fn create(&mut self, params: Value) -> (u16, Value) {
        self.rig.post(
            BASE,
            json!({"recipeId": "presence-thermostat", "params": params}),
        )
    }

    fn start(&mut self, extra: Value) -> String {
        let mut params = json!({
            "zone": self.g.zone,
            "thermostat": self.thermostat,
            "comfortTemp": 21,
            "ecoTemp": 17,
            "timeout": "10m",
        });
        for (key, value) in extra.as_object().unwrap() {
            params[key] = value.clone();
        }
        let (status, body) = self.create(params);
        assert_eq!(status, 201, "{body}");
        body["id"].as_str().unwrap().to_string()
    }

    fn motion(&mut self, on: bool) {
        self.rig.report(&motion(PIR, on));
    }

    /// Time moves to `at` (a turn at the end, a second after).
    fn go(&mut self, at: u64) {
        self.rig.now = at;
        self.rig.advance(SECOND);
    }

    fn sent(&mut self) -> Vec<Command> {
        self.rig.sent()
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

fn degrees(value: i16) -> Vec<Command> {
    vec![Command::WriteHeatingSetpoint(value)]
}

// ---- info ----

#[test]
fn the_recipe_is_served_with_sowels_info_byte_for_byte() {
    let mut w = World::new(T0);
    let response = w.rig.raw("GET", "/api/v1/recipes/presence-thermostat", "");
    assert_eq!(response.status, 200);
    assert_eq!(
        response.body.unwrap(),
        include_str!("pinned/recipe_presence_thermostat.json")
    );
}

// ---- validation ----

#[test]
fn every_refused_setting_answers_sowels_text_in_sowels_order() {
    let mut w = World::new(T0);
    let (zone, thermostat) = (w.g.zone.clone(), w.thermostat.clone());
    let other = w.rig.zone("Hall");
    let elsewhere = w.rig.equipment("Away", "thermostat", &other, &[]);
    let bare = w.rig.equipment("Bare", "thermostat", &zone, &[]);
    let motion = w.g.motion_equipment.clone();
    let button = w.button.clone();
    let ok = json!({"zone": zone, "thermostat": thermostat, "comfortTemp": 21, "ecoTemp": 17});
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
            "Thermostat parameter is required".into(),
        ),
        (
            json!({"zone": zone, "thermostat": GHOST}),
            format!("Thermostat equipment not found: {GHOST}"),
        ),
        (
            json!({"zone": zone, "thermostat": elsewhere}),
            "Thermostat \"Away\" does not belong to the selected zone".into(),
        ),
        (
            json!({"zone": zone, "thermostat": bare}),
            "Thermostat \"Bare\" has no \"setpoint\" order binding".into(),
        ),
        (
            json!({"zone": zone, "thermostat": thermostat}),
            "comfortTemp is required".into(),
        ),
        (
            with(json!({"comfortTemp": "warm"})),
            "comfortTemp must be a number".into(),
        ),
        (
            json!({"zone": zone, "thermostat": thermostat, "comfortTemp": 21}),
            "ecoTemp is required".into(),
        ),
        (
            with(json!({"ecoTemp": "cold"})),
            "ecoTemp must be a number".into(),
        ),
        (
            with(json!({"timeout": "soon"})),
            "Invalid duration format: soon. Use e.g. \"10m\", \"30s\", \"1h\"".into(),
        ),
        (
            with(json!({"nightTemp": 16})),
            "nightTemp requires nightStart and nightEnd".into(),
        ),
        (
            with(json!({"nightTemp": 16, "nightStart": "22:00"})),
            "nightTemp requires nightStart and nightEnd".into(),
        ),
        (
            with(json!({"nightStart": "22:00", "nightEnd": "06:00"})),
            "nightStart/nightEnd require nightTemp".into(),
        ),
        (
            with(json!({"nightTemp": 16, "nightStart": "9:00", "nightEnd": "06:00"})),
            "nightStart must be in HH:MM format".into(),
        ),
        (
            with(json!({"nightTemp": 16, "nightStart": "22:00", "nightEnd": "6h"})),
            "nightEnd must be in HH:MM format".into(),
        ),
        (
            with(json!({"nightTemp": "cool", "nightStart": "22:00", "nightEnd": "06:00"})),
            "nightTemp must be a number".into(),
        ),
        (
            with(json!({"preheatStart": "06:00"})),
            "preheatStart and preheatEnd must both be provided or both omitted".into(),
        ),
        (
            with(json!({"preheatStart": "06:00", "preheatEnd": "8h"})),
            "preheatEnd must be in HH:MM format".into(),
        ),
        (
            with(json!({"weekendPreheatEnd": "09:00"})),
            "weekendPreheatStart and weekendPreheatEnd must both be provided or both omitted"
                .into(),
        ),
        (
            with(json!({"weekendPreheatStart": "7h", "weekendPreheatEnd": "09:00"})),
            "weekendPreheatStart must be in HH:MM format".into(),
        ),
        (
            with(json!({"buttons": [button]})),
            "cocoonTemp is required when buttons are configured".into(),
        ),
        (
            with(json!({"cocoonTemp": 24})),
            "buttons are required when cocoonTemp is configured".into(),
        ),
        (
            with(json!({"buttons": [button], "cocoonTemp": "hot"})),
            "cocoonTemp must be a number".into(),
        ),
        (
            with(json!({"buttons": [GHOST], "cocoonTemp": 24})),
            format!("Button equipment not found: {GHOST}"),
        ),
        (
            with(json!({"buttons": [motion], "cocoonTemp": 24})),
            "Button \"Garage motion\" has no \"action\" data binding".into(),
        ),
    ];
    for (params, message) in cases {
        assert_eq!(
            refused(&mut w, params.clone()),
            invalid(&message),
            "{params}"
        );
    }
    // All the options at once are accepted.
    let (status, body) = w.create(with(json!({
        "timeout": "5m", "nightTemp": 16, "nightStart": "22:00", "nightEnd": "06:00",
        "preheatStart": "06:00", "preheatEnd": "08:00",
        "weekendPreheatStart": "07:00", "weekendPreheatEnd": "09:00",
        "buttons": [button], "cocoonTemp": 24,
    })));
    assert_eq!(status, 201, "{body}");
    // The host's own checks of the slots: a thermostat slot takes a thermostat only.
    let light = w.g.light.clone();
    assert_eq!(refused(&mut w, with(json!({"thermostat": light}))).0, 400);
}

// ---- behaviour ----

#[test]
fn the_setpoint_follows_the_presence_and_goes_eco_after_the_timeout() {
    let mut w = World::new(paris(0, 12, 0));
    let id = w.start(json!({}));
    // At the start with no motion: eco.
    assert_eq!(w.sent(), degrees(1700));
    assert_eq!(w.state(&id)["currentMode"], "eco");
    assert!(
        w.log(&id)
            .contains("Recipe activated — no motion — setpoint → 17°C (eco)")
    );
    w.motion(true);
    assert_eq!(w.sent(), degrees(2100));
    assert_eq!(w.state(&id)["currentMode"], "comfort");
    assert!(
        w.log(&id)
            .contains("Motion detected — setpoint → 21°C (comfort)")
    );
    // No motion: the timer runs, the state shows its end.
    w.motion(false);
    assert_eq!(w.sent(), vec![]);
    assert!(w.state(&id)["timerExpiresAt"].is_string());
    w.rig.advance(9 * MINUTE);
    assert_eq!(w.sent(), vec![]);
    w.rig.advance(MINUTE + SECOND);
    assert_eq!(w.sent(), degrees(1700));
    assert_eq!(w.state(&id)["currentMode"], "eco");
    assert!(w.state(&id).get("timerExpiresAt").is_none());
    assert!(
        w.log(&id)
            .contains("No motion for 10min — setpoint → 17°C (eco)")
    );
    // Motion again before the timeout cancels it.
    w.motion(true);
    assert_eq!(w.sent(), degrees(2100));
    w.motion(false);
    w.rig.advance(5 * MINUTE);
    w.motion(true);
    w.rig.advance(20 * MINUTE);
    assert_eq!(w.sent(), vec![]);
}

#[test]
fn a_manual_setpoint_suspends_the_recipe_until_the_room_has_been_still_for_the_timeout() {
    let mut w = World::new(paris(0, 12, 0));
    let id = w.start(json!({}));
    w.motion(true);
    w.sent();
    // Our own echo (within the 5 s window, or the same value) is not a manual change.
    w.rig.report(&setpoint(THERMOSTAT, 21.0));
    assert!(w.state(&id).get("overrideMode").is_none());
    w.rig.advance(10 * SECOND);
    w.rig.report(&setpoint(THERMOSTAT, 23.0));
    assert_eq!(w.state(&id)["overrideMode"], true);
    assert!(
        w.log(&id)
            .contains("Manual setpoint change detected — entering override mode")
    );
    // The motion going away starts the timer; it ends the override with eco.
    w.motion(false);
    assert_eq!(w.sent(), vec![]);
    w.rig.advance(10 * MINUTE + SECOND);
    assert_eq!(w.sent(), degrees(1700));
    assert!(w.state(&id).get("overrideMode").is_none());
    assert!(
        w.log(&id)
            .contains("No motion for 10min — override cleared — setpoint → 17°C (eco)")
    );
}

#[test]
fn the_night_window_holds_its_setpoint_whatever_the_presence_and_ends_by_the_room() {
    let mut w = World::new(paris(0, 12, 0));
    let id = w.start(json!({"nightTemp": 16, "nightStart": "22:00", "nightEnd": "06:00"}));
    assert_eq!(w.sent(), degrees(1700));
    w.go(paris(0, 22, 0));
    assert_eq!(w.sent(), degrees(1600));
    assert_eq!(w.state(&id)["currentMode"], "night");
    assert!(
        w.log(&id)
            .contains("Night window started — setpoint → 16°C (night)")
    );
    // Presence changes are ignored at night.
    w.motion(true);
    w.motion(false);
    assert_eq!(w.sent(), vec![]);
    // The next morning at 06:00 (05:00 UTC), with no motion: eco.
    w.go(paris(1, 6, 0));
    assert_eq!(w.sent(), degrees(1700));
    assert_eq!(w.state(&id)["currentMode"], "eco");
    assert!(w.log(&id).contains("Night ended — no motion"));
}

#[test]
fn the_recipe_started_inside_the_night_window_is_in_night_mode() {
    let mut w = World::new(paris(0, 23, 0));
    let id = w.start(json!({"nightTemp": 16, "nightStart": "22:00", "nightEnd": "06:00"}));
    assert_eq!(w.sent(), degrees(1600));
    assert_eq!(w.state(&id)["currentMode"], "night");
}

#[test]
fn the_weekend_and_the_weekday_have_their_own_preheat_windows() {
    // Saturday the 21st, 05:30 in Paris.
    let mut w = World::new(paris(0, 5, 30));
    let id = w.start(json!({
        "preheatStart": "06:00", "preheatEnd": "08:00",
        "weekendPreheatStart": "07:00", "weekendPreheatEnd": "09:00",
    }));
    assert_eq!(w.sent(), degrees(1700));
    // The weekday window does not apply on a Saturday.
    w.go(paris(0, 6, 30));
    assert_eq!(w.sent(), vec![]);
    w.go(paris(0, 7, 0));
    assert_eq!(w.sent(), degrees(2100));
    assert!(
        w.log(&id)
            .contains("Preheat window started — setpoint → 21°C (comfort)")
    );
    // Preheat protects the comfort from the missing motion; leaving it starts the timer.
    w.motion(true);
    w.motion(false);
    w.go(paris(0, 8, 30));
    assert_eq!(w.sent(), vec![]);
    w.go(paris(0, 9, 0));
    assert_eq!(w.sent(), vec![]);
    w.go(paris(0, 9, 12));
    assert_eq!(w.sent(), degrees(1700));
    // Monday the 23rd: the weekday window, from 06:00.
    w.go(paris(2, 5, 30));
    assert_eq!(w.sent(), vec![]);
    w.go(paris(2, 6, 0));
    assert_eq!(w.sent(), degrees(2100));
    // The weekend window is not the weekday's: 08:30 on a Monday is outside both.
    w.go(paris(2, 8, 30));
    assert_eq!(w.sent(), vec![]);
}

#[test]
fn the_button_toggles_the_cocoon_setpoint() {
    let mut w = World::new(paris(0, 12, 0));
    let button = w.button.clone();
    let id = w.start(json!({"buttons": [button], "cocoonTemp": 24}));
    w.sent();
    w.rig.report(&action(BUTTON, Action::Toggle));
    assert_eq!(w.sent(), degrees(2400));
    let state = w.state(&id);
    assert_eq!(state["currentMode"], "cocoon");
    assert_eq!(state["cocoonMode"], true);
    assert!(
        w.log(&id)
            .contains("Button pressed — setpoint → 24°C (cocoon)")
    );
    // The eco timer does not touch the cocoon; a second press leaves it, by the room.
    w.rig.advance(30 * MINUTE);
    assert_eq!(w.sent(), vec![]);
    w.rig.report(&action(BUTTON, Action::Toggle));
    assert_eq!(w.sent(), degrees(1700));
    assert!(w.state(&id).get("cocoonMode").is_none());
    assert!(
        w.log(&id)
            .contains("Cocoon deactivated by button — no motion")
    );
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
    let w = World::new(paris(0, 5, 30));
    let runtimes: Arc<dyn ScriptRuntimes> = Arc::new(Roomy);
    let clock: ScriptClock = Arc::new(|| 0);
    let (mut recipe, _) = ScriptRecipe::load(SCRIPT, &runtimes, &clock).unwrap();
    let names = RecipeName {
        id: "presence-thermostat".into(),
        name: "Presence Thermostat".into(),
    };
    let clock = Clock {
        mono: w.rig.now,
        wall: w.rig.now,
    };
    // Every option used.
    let params = json!({
        "zone": w.g.zone, "thermostat": w.thermostat, "comfortTemp": 21, "ecoTemp": 17,
        "timeout": 600_000, "nightTemp": 16, "nightStart": "22:00", "nightEnd": "06:00",
        "preheatStart": "06:00", "preheatEnd": "08:00",
        "weekendPreheatStart": "07:00", "weekendPreheatEnd": "09:00",
        "buttons": [w.button], "cocoonTemp": 24,
    });
    let empty = recipe.held_bytes();
    let host = NativeHost::new(&w.rig.home, Id::ROOT_ZONE, &names, clock.mono, clock.wall);
    for handle in 0..MAX_INSTANCES_PER_SCRIPT as u32 {
        recipe.start(&host, handle, &params, clock).unwrap();
    }
    assert_eq!(recipe.instance_count(), MAX_INSTANCES_PER_SCRIPT);
    let events = [
        Event::Equipment {
            equipment: w.button.clone(),
            alias: "action".into(),
            value: EventValue::Text("toggle".into()),
        },
        Event::Equipment {
            equipment: w.thermostat.clone(),
            alias: "setpoint".into(),
            value: EventValue::Number(23.0),
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
        "presence-thermostat: {} bytes of source, {empty} bytes loaded, {held} bytes with {MAX_INSTANCES_PER_SCRIPT} instances (the box's limit is {SCRIPT_MEMORY_LIMIT})",
        SCRIPT.len()
    );
    assert!(SCRIPT.len() < 32 * 1024, "{}", SCRIPT.len());
    assert!(held < SCRIPT_MEMORY_LIMIT, "{held}");
}
