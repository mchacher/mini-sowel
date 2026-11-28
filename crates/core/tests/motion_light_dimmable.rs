//! `motion-light-dimmable`, Sowel's recipe 1.0.2 as the script `recipes/scripts/motion-light-dimmable.js`
//! (spec 054): its info (pinned from Sowel's source), Sowel's validation texts, the brightness
//! of the time slot (read in the home's time zone, `api.now().localTime`), the off timer, the
//! manual brightness override, and the memory of 4 instances.
//!
//! These tests need the `script-recipes` feature (the host program turns it on, so
//! `cargo test --workspace` runs them).

mod support;

use std::sync::Arc;

use serde_json::{Value, json};
use solite_core::devices::Id;
use solite_core::radio::measures::{Measure, Value as MeasureValue};
use solite_core::radio::zcl::Command;
use solite_core::radio::{MeasureEvent, Sender};
use solite_core::recipes::native::{NativeHost, RecipeName};
use solite_core::recipes::script::{
    NewRuntime, SCRIPT_MEMORY_LIMIT, ScriptRecipe, ScriptRuntimes, SystemRuntimes,
};
use solite_core::recipes::{MAX_INSTANCES_PER_SCRIPT, ScriptClock};
use solite_recipe_sdk::{Clock, Event, EventValue, Recipe};
use support::*;

const DIMMABLE: &str = include_str!("../../../recipes/scripts/motion-light-dimmable.js");
const SECOND: u64 = 1000;
const BASE: &str = "/api/v1/recipe-instances";
const GHOST: &str = "11111111-2222-4333-8444-555555555555";
const DIMMER: u8 = 0x7B;
/// 2026-11-21 00:00:00 UTC.
const MIDNIGHT: u64 = T0 - (9 * HOUR + 12 * MINUTE + 3 * SECOND + 120);

struct World {
    rig: Rig,
    g: Garage,
    lamp: String,
    button: String,
}

fn brightness_report(level: f64) -> MeasureEvent {
    MeasureEvent {
        sender: Sender::Eui64(eui(DIMMER)),
        endpoint: 1,
        measures: vec![Measure {
            kind: "light_brightness",
            value: MeasureValue::Number(level),
            unit: "",
            device_value: None,
        }],
    }
}

impl World {
    /// The garage with a dimmable lamp (state and brightness) and a remote, the script loaded;
    /// the home is in Paris (an hour ahead of UTC in November), at `now`.
    fn new(now: u64) -> World {
        let mut rig = Rig::new();
        rig.now = now;
        let g = rig.garage();
        let clock: ScriptClock = Arc::new(|| 0);
        rig.home
            .load_script_recipe(DIMMABLE, Arc::new(SystemRuntimes), clock)
            .unwrap();
        let (status, _) = rig.put("/api/v1/settings", json!({"home.timezone": "Europe/Paris"}));
        assert_eq!(status, 200);
        let e = eui(DIMMER);
        rig.home.devices.announced(e, 0x8C, rig.now);
        rig.home
            .devices
            .identified(e, Some("TEST"), Some("DIM01"), rig.now, &mut *rig.random)
            .unwrap();
        rig.home
            .devices
            .reporting_configured(e, &[0x0006, 0x0008], rig.now, &mut *rig.random)
            .unwrap();
        let device = rig.device(DIMMER);
        let lamp = rig.equipment("Garage lamp", "light_dimmable", &g.zone, &[&device]);
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
        rig.report(&state(DIMMER, false));
        rig.sent();
        World {
            rig,
            g,
            lamp,
            button,
        }
    }

    fn create(&mut self, params: Value) -> (u16, Value) {
        self.rig.post(
            BASE,
            json!({"recipeId": "motion-light-dimmable", "params": params}),
        )
    }

    fn start(&mut self, extra: Value) -> String {
        let mut params = json!({
            "zone": self.g.zone,
            "lights": [self.lamp],
            "timeout": "20s",
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

    fn lamp(&mut self, on: bool) {
        self.rig.report(&state(DIMMER, on));
    }

    fn sent(&mut self) -> Vec<Command> {
        self.rig.sent()
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
    let mut w = World::new(T0);
    let response = w
        .rig
        .raw("GET", "/api/v1/recipes/motion-light-dimmable", "");
    assert_eq!(response.status, 200);
    assert_eq!(
        response.body.unwrap(),
        include_str!("pinned/recipe_motion_light_dimmable.json")
    );
}

// ---- validation ----

#[test]
fn every_refused_setting_answers_sowels_text_in_sowels_order() {
    let mut w = World::new(T0);
    let (zone, lamp) = (w.g.zone.clone(), w.lamp.clone());
    let plain = w.g.light.clone();
    let bare = w.rig.equipment("Bare lamp", "light_dimmable", &zone, &[]);
    let other_zone = w.rig.zone("Elsewhere");
    let device = w.rig.device(DIMMER);
    let far = w
        .rig
        .equipment("Far lamp", "light_dimmable", &other_zone, &[&device]);
    let motion = w.g.motion_equipment.clone();
    let ok = json!({"zone": zone, "lights": [lamp]});
    let with = |extra: Value| {
        let mut params = ok.clone();
        for (key, value) in extra.as_object().unwrap() {
            params[key] = value.clone();
        }
        params
    };
    let cases: Vec<(Value, String)> = vec![
        (json!({}), "Zone parameter is required".into()),
        (json!({"zone": 5}), "Zone parameter is required".into()),
        (
            json!({"zone": GHOST, "lights": [lamp]}),
            format!("Zone not found: {GHOST}"),
        ),
        (
            json!({"zone": zone}),
            "At least one light is required".into(),
        ),
        (
            json!({"zone": zone, "lights": [GHOST]}),
            format!("Light equipment not found: {GHOST}"),
        ),
        (
            json!({"zone": zone, "lights": [plain]}),
            "Light \"Garage light\" is type \"light_onoff\" but this recipe requires light_dimmable or light_color".into(),
        ),
        (
            json!({"zone": zone, "lights": [far]}),
            "Light \"Far lamp\" does not belong to the selected zone".into(),
        ),
        (
            json!({"zone": zone, "lights": [bare]}),
            "Light \"Bare lamp\" has no \"state\" order binding".into(),
        ),
        (
            with(json!({"timeout": "soon"})),
            "Invalid duration format: soon. Use e.g. \"10m\", \"30s\", \"1h\"".into(),
        ),
        (
            with(json!({"luxThreshold": -1})),
            "luxThreshold must be a non-negative number".into(),
        ),
        (
            with(json!({"maxOnDuration": "x"})),
            "Invalid duration format: x. Use e.g. \"10m\", \"30s\", \"1h\"".into(),
        ),
        (
            with(json!({"buttons": [GHOST]})),
            format!("Button equipment not found: {GHOST}"),
        ),
        (
            with(json!({"buttons": [motion]})),
            "Button \"Garage motion\" has no \"action\" data binding".into(),
        ),
        (
            with(json!({"brightness": 0})),
            "brightness must be between 1 and 254".into(),
        ),
        (
            with(json!({"brightness": 255})),
            "brightness must be between 1 and 254".into(),
        ),
        (
            with(json!({"slot2Start": "08:00", "slot2End": "09:00"})),
            "Slot 2: start, end, and brightness must all be provided or all omitted".into(),
        ),
        (
            with(json!({"slot1Start": "8:00", "slot1End": "09:00", "slot1Brightness": 50})),
            "slot1Start must be in HH:MM format".into(),
        ),
        (
            with(json!({"slot3Start": "08:00", "slot3End": "9h", "slot3Brightness": 50})),
            "slot3End must be in HH:MM format".into(),
        ),
        (
            with(json!({"slot1Start": "08:00", "slot1End": "09:00", "slot1Brightness": 300})),
            "slot1Brightness must be between 1 and 254".into(),
        ),
    ];
    for (params, message) in cases {
        assert_eq!(
            refused(&mut w, params.clone()),
            invalid(&message),
            "{params}"
        );
    }
    // The order of the checks: the first failure wins (the lights before the timeout).
    assert_eq!(
        refused(
            &mut w,
            json!({"zone": zone, "lights": [GHOST], "timeout": "soon"})
        ),
        invalid(&format!("Light equipment not found: {GHOST}"))
    );
}

// ---- behaviour ----

/// The home's three slots of the tests, in Paris time: 22:00 to 06:00 (past midnight) at 30,
/// 06:00 to 18:00 at 200, and 100 outside the slots.
fn slots() -> Value {
    json!({
        "brightness": 100,
        "slot1Start": "22:00", "slot1End": "06:00", "slot1Brightness": 30,
        "slot2Start": "06:00", "slot2End": "18:00", "slot2Brightness": 200,
    })
}

#[test]
fn motion_turns_the_lamp_on_at_the_brightness_of_the_homes_local_slot() {
    // 21:30 UTC is 22:30 in Paris: the night slot, which a read in UTC would miss (it would give
    // the default); and 17:30 UTC is 18:30 in Paris: the default, where UTC says the day slot.
    for (utc, level) in [
        (21 * HOUR + 30 * MINUTE, 30),
        (17 * HOUR + 30 * MINUTE, 100),
        (23 * HOUR + 30 * MINUTE, 30),
        (4 * HOUR + 59 * MINUTE, 30),
        (5 * HOUR + 30 * MINUTE, 200),
    ] {
        let mut w = World::new(MIDNIGHT + utc);
        let id = w.start(slots());
        assert_eq!(w.sent(), vec![Command::Off], "the start sync");
        w.motion(true);
        assert_eq!(
            w.sent(),
            vec![Command::On, Command::MoveToLevel(level)],
            "{utc}"
        );
        assert!(
            w.log(&id)
                .contains(&format!("1 light(s) turned on at brightness {level}"))
        );
    }
}

#[test]
fn without_a_brightness_the_lamp_is_only_turned_on() {
    let mut w = World::new(T0);
    let id = w.start(json!({}));
    w.sent();
    w.motion(true);
    assert_eq!(w.sent(), vec![Command::On]);
    assert!(
        w.log(&id)
            .contains("Motion detected — 1 light(s) turned on")
    );
}

#[test]
fn no_motion_for_the_delay_turns_the_lamp_off() {
    let mut w = World::new(MIDNIGHT + 10 * HOUR);
    let id = w.start(slots());
    w.sent();
    w.motion(true);
    w.lamp(true);
    w.sent();
    w.motion(false);
    w.rig.advance(10 * SECOND);
    assert_eq!(w.sent(), vec![], "not yet");
    assert!(w.state(&id).get("timerExpiresAt").is_some());
    w.rig.advance(10 * SECOND);
    assert_eq!(w.sent(), vec![Command::Off]);
    assert!(w.log(&id).contains("No motion for 20s — lights turned off"));
}

#[test]
fn the_slot_is_read_when_the_lamp_is_turned_on_not_while_it_stays_on() {
    // 16:59 UTC is 17:59 in Paris: the day slot, 200. Two minutes later the default applies, but
    // the lamp stays as it is; the next turn-on takes the new brightness.
    let mut w = World::new(MIDNIGHT + 16 * HOUR + 59 * MINUTE);
    let id = w.start(slots());
    w.sent();
    w.motion(true);
    assert_eq!(w.sent(), vec![Command::On, Command::MoveToLevel(200)]);
    w.lamp(true);
    w.rig.report(&brightness_report(200.0));
    w.rig.advance(2 * MINUTE);
    w.motion(false);
    w.motion(true);
    assert_eq!(w.sent(), vec![], "the lamp stays at 200");
    w.motion(false);
    w.rig.advance(20 * SECOND);
    assert_eq!(w.sent(), vec![Command::Off]);
    w.lamp(false);
    w.rig.advance(10 * SECOND);
    w.motion(true);
    assert_eq!(w.sent(), vec![Command::On, Command::MoveToLevel(100)]);
    assert!(w.state(&id).get("overrideMode").is_none());
}

#[test]
fn a_manual_brightness_change_enters_override_after_the_grace_period() {
    let mut w = World::new(MIDNIGHT + 10 * HOUR);
    let id = w.start(slots());
    w.sent();
    w.motion(true);
    w.lamp(true);
    w.sent();
    // The lamp's own echo, within the grace period, and the same level: nothing.
    w.rig.report(&brightness_report(200.0));
    w.rig.advance(2 * SECOND);
    w.rig.report(&brightness_report(90.0));
    assert!(w.state(&id).get("overrideMode").is_none());
    w.rig.advance(6 * SECOND);
    w.rig.report(&brightness_report(200.0));
    assert!(w.state(&id).get("overrideMode").is_none());
    w.rig.report(&brightness_report(90.0));
    assert_eq!(w.state(&id)["overrideMode"], json!(true));
    assert!(
        w.log(&id)
            .contains("Manual brightness change detected — entering override mode")
    );
}

#[test]
fn a_press_of_a_button_switches_the_lamp_off_in_override_and_the_next_one_on() {
    let mut w = World::new(MIDNIGHT + 10 * HOUR);
    let id = w.start(json!({"brightness": 80, "buttons": [w.button.clone()]}));
    w.sent();
    w.motion(true);
    w.lamp(true);
    w.sent();
    w.rig
        .report(&action(0x70, solite_core::radio::zcl::Action::Toggle));
    assert_eq!(w.sent(), vec![Command::Off]);
    assert_eq!(w.state(&id)["overrideMode"], json!(true));
    w.lamp(false);
    w.rig
        .report(&action(0x70, solite_core::radio::zcl::Action::Toggle));
    assert_eq!(w.sent(), vec![Command::On, Command::MoveToLevel(80)]);
    assert!(w.state(&id).get("overrideMode").is_none());
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
    let w = World::new(MIDNIGHT + 10 * HOUR);
    let runtimes: Arc<dyn ScriptRuntimes> = Arc::new(Roomy);
    let clock: ScriptClock = Arc::new(|| 0);
    let (mut recipe, _) = ScriptRecipe::load(DIMMABLE, &runtimes, &clock).unwrap();
    let names = RecipeName {
        id: "motion-light-dimmable".into(),
        name: "Motion Light (Dimmable)".into(),
    };
    let clock = Clock {
        mono: w.rig.now,
        wall: w.rig.now,
    };
    let mut params = slots();
    params["zone"] = json!(w.g.zone);
    params["lights"] = json!([w.lamp]);
    params["buttons"] = json!([w.button]);
    params["timeout"] = json!(600_000);
    params["maxOnDuration"] = json!(3_600_000);
    params["luxThreshold"] = json!(50);
    params["slot3Start"] = json!("18:00");
    params["slot3End"] = json!("22:00");
    params["slot3Brightness"] = json!(120);
    let empty = recipe.held_bytes();
    let host = NativeHost::new(&w.rig.home, Id::ROOT_ZONE, &names, clock.mono, clock.wall);
    for handle in 0..MAX_INSTANCES_PER_SCRIPT as u32 {
        recipe.start(&host, handle, &params, clock).unwrap();
    }
    assert_eq!(recipe.instance_count(), MAX_INSTANCES_PER_SCRIPT);
    let events = [
        Event::Zone {
            zone: w.g.zone.clone(),
            motion: true,
            luminosity: Some(10),
        },
        Event::Equipment {
            equipment: w.lamp.clone(),
            alias: "state".into(),
            value: EventValue::Bool(true),
        },
        Event::Equipment {
            equipment: w.lamp.clone(),
            alias: "brightness".into(),
            value: EventValue::Number(90.0),
        },
        Event::Zone {
            zone: w.g.zone.clone(),
            motion: false,
            luminosity: Some(10),
        },
    ];
    for handle in 0..MAX_INSTANCES_PER_SCRIPT as u32 {
        for event in &events {
            recipe.on_event(&host, handle, event, clock).unwrap();
        }
    }
    let held = recipe.held_bytes();
    println!(
        "motion-light-dimmable: {} bytes of source, {empty} bytes loaded, {held} bytes with {MAX_INSTANCES_PER_SCRIPT} instances (the box's limit is {SCRIPT_MEMORY_LIMIT})",
        DIMMABLE.len()
    );
    assert!(DIMMABLE.len() < 32 * 1024, "{}", DIMMABLE.len());
    assert!(held < SCRIPT_MEMORY_LIMIT, "{held}");
}
