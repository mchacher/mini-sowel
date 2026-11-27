//! Script recipes (spec 041, plan step 4): the loader, the host's validation of the slots, the
//! script API (events, reads, orders), the sandbox, the timers, the state and the log, the errors
//! of a script, the limits and the failure of a recipe as a whole, its return, the bounds and the
//! stack. The scripts are in `fixtures/scripts/`; `cases.js` is every bad script in one file
//! (`case` names what it does, `on-<case>` does it when the zone's motion changes).
//!
//! These tests need the `script-recipes` feature (the host program turns it on, so
//! `cargo test --workspace` runs them).

mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{Value, json};
use solite_core::devices::Id;
use solite_core::live::{ClientId, LiveHandle};
use solite_core::recipes::script::{
    HeldCounter, NewRuntime, RefusedFlag, SYSTEM_MEMORY_LIMIT, ScriptRecipe, ScriptRuntimes,
    SystemRuntimes,
};
use solite_core::recipes::{MAX_INSTANCES_PER_SCRIPT, MAX_SCRIPT_RECIPES, ScriptClock};
use solite_recipe_sdk::{Clock, ErrorKind, Event, Host, Recipe};
use support::*;

const PROBE: &str = include_str!("fixtures/scripts/probe.js");
const CASES: &str = include_str!("fixtures/scripts/cases.js");
const SLOTS: &str = include_str!("fixtures/scripts/slots.js");
const SANDBOX: &str = include_str!("fixtures/scripts/sandbox.js");
const TIMERS: &str = include_str!("fixtures/scripts/timers.js");
const READS: &str = include_str!("fixtures/scripts/reads.js");
const DEPTH: &str = include_str!("fixtures/scripts/depth.js");
const SPIN: &str = include_str!("fixtures/scripts/spin.js");
const GARBAGE: &str = include_str!("fixtures/scripts/garbage.js");

// ---- Helpers ----

/// A monotonic clock that moves one millisecond at each reading: a loop that never ends meets its
/// 200 ms after two hundred readings, whatever the machine.
fn ticking() -> (ScriptClock, Arc<AtomicU64>) {
    let reads = Arc::new(AtomicU64::new(0));
    let counter = reads.clone();
    (
        Arc::new(move || counter.fetch_add(1, Ordering::SeqCst)),
        reads,
    )
}

fn load(
    rig: &mut Rig,
    source: &str,
) -> Result<solite_core::recipes::script::ScriptSummary, String> {
    let (clock, _) = ticking();
    rig.home
        .load_script_recipe(source, Arc::new(SystemRuntimes), clock)
}

fn loaded(source: &str) -> (Rig, Garage) {
    let mut rig = Rig::new();
    let garage = rig.garage();
    load(&mut rig, source).unwrap();
    (rig, garage)
}

/// POST /recipe-instances; (status, body).
fn create(rig: &mut Rig, recipe: &str, params: Value) -> (u16, Value) {
    rig.post(
        "/api/v1/recipe-instances",
        json!({ "recipeId": recipe, "params": params }),
    )
}

fn created(rig: &mut Rig, recipe: &str, params: Value) -> String {
    let (status, body) = create(rig, recipe, params);
    assert_eq!(status, 201, "{body}");
    body["id"].as_str().unwrap().to_string()
}

/// An instance of `cases` doing `case`, on the garage's light.
fn case(rig: &mut Rig, garage: &Garage, case: &str) -> String {
    created(
        rig,
        "cases",
        json!({"zone": garage.zone, "lights": [garage.light], "case": case}),
    )
}

fn reason(rig: &mut Rig, recipe: &str, params: Value) -> (u16, String) {
    let (status, body) = create(rig, recipe, params);
    (status, body["error"].as_str().unwrap_or("").to_string())
}

fn running(rig: &Rig, id: &str) -> bool {
    rig.home.recipes().is_running(&Id::parse(id).unwrap())
}

fn enabled(rig: &mut Rig, id: &str) -> bool {
    let (status, body) = rig.get("/api/v1/recipe-instances");
    assert_eq!(status, 200);
    body.as_array()
        .unwrap()
        .iter()
        .find(|i| i["id"] == id)
        .unwrap()["enabled"]
        == true
}

/// A client of the hub subscribed to the recipes topic, and its batches.
fn listening(rig: &Rig) -> (LiveHandle, ClientId) {
    let hub = rig.home.live();
    let client = hub.connect().expect("a free slot");
    assert!(hub.subscribe(
        client,
        &json!({"type": "subscribe", "topics": ["recipes"]}).to_string()
    ));
    (hub, client)
}

fn event_types(hub: &LiveHandle, client: ClientId) -> Vec<String> {
    match hub.take_batch(client) {
        None => Vec::new(),
        Some(text) => serde_json::from_str::<Vec<Value>>(&text)
            .unwrap()
            .iter()
            .map(|e| e["type"].as_str().unwrap().to_string())
            .collect(),
    }
}

/// Moves the garage's motion to the opposite of what it was: a `zone.data.changed`.
fn toggle_motion(rig: &mut Rig, on: &mut bool) {
    *on = !*on;
    rig.report(&motion(PIR, *on));
}

/// The state of an instance as `GET /recipe-instances` writes it, key order and all.
fn state_text(rig: &mut Rig, id: &str) -> String {
    #[derive(serde::Deserialize)]
    struct Instance {
        id: String,
        state: Box<serde_json::value::RawValue>,
    }
    let body = rig.raw("GET", "/api/v1/recipe-instances", "").body.unwrap();
    let list: Vec<Instance> = serde_json::from_str(&body).unwrap();
    list.into_iter()
        .find(|i| i.id == id)
        .unwrap()
        .state
        .get()
        .to_string()
}

fn has(messages: &[String], text: &str) -> bool {
    messages.iter().any(|m| m == text)
}

fn starts(messages: &[String], text: &str) -> bool {
    messages.iter().any(|m| m.starts_with(text))
}

// ---- load ----

#[test]
fn a_valid_script_is_loaded_and_served_with_sowels_shape() {
    let mut rig = Rig::new();
    let summary = load(&mut rig, SLOTS).unwrap();
    assert_eq!(summary.id, "slots");
    assert_eq!(summary.source_bytes, SLOTS.len());
    assert!(summary.runtime_bytes > 0);
    let (status, list) = rig.get("/api/v1/recipes");
    assert_eq!(status, 200);
    let ids: Vec<&str> = list
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["id"].as_str().unwrap())
        .collect();
    // The built-in recipe first, then the script.
    assert_eq!(ids, ["motion-light", "slots"]);
    let (status, one) = rig.get("/api/v1/recipes/slots");
    assert_eq!(status, 200);
    assert_eq!(one["name"], "Slots");
    assert_eq!(one["description"], "Every slot type");
    assert_eq!(one["icon"], "Settings");
    assert_eq!(one["i18n"]["fr"]["name"], "Emplacements");
    // Slots as an array, in declaration order; `zone` is declared first here.
    let slots: Vec<&str> = one["slots"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        slots,
        [
            "zone", "lights", "button", "level", "delay", "mode", "flag", "wake"
        ]
    );
    assert_eq!(
        one["slots"][1],
        json!({"id": "lights", "name": "Lights", "description": "", "type": "equipment",
               "required": true, "list": true, "constraints": {"equipmentType": "light_onoff"}})
    );
    // `default` is read as `defaultValue`, `constraints` carries the several types and the scope.
    assert_eq!(one["slots"][4]["defaultValue"], "5m");
    assert_eq!(
        one["slots"][2]["constraints"],
        json!({"equipmentType": ["button", "sensor"], "crossZone": true})
    );
    assert_eq!(
        one["slots"][5]["options"],
        json!([{"value": "eco", "label": "Eco"}, {"value": "boost", "label": "Boost"}])
    );
}

#[test]
fn the_zone_slot_is_added_first_when_the_script_does_not_declare_it() {
    let mut rig = Rig::new();
    load(&mut rig, PROBE).unwrap();
    let (_, one) = rig.get("/api/v1/recipes/probe");
    let slots: Vec<&str> = one["slots"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap())
        .collect();
    assert_eq!(slots, ["zone", "lights", "timeout"]);
    assert_eq!(
        one["slots"][0],
        json!({"id": "zone", "name": "Zone", "description": "Zone to monitor", "type": "zone", "required": true})
    );
    // `equipmentType` at the slot's top level is read as a constraint.
    assert_eq!(
        one["slots"][1]["constraints"],
        json!({"equipmentType": "light_onoff"})
    );
}

/// A script with this `recipe({...})` call.
fn script(definition: &str) -> String {
    format!("recipe({definition});")
}

const OK_START: &str = "start: function () {}";

#[test]
fn a_script_that_is_not_a_recipe_is_refused_with_its_reason() {
    let long_id = "a".repeat(65);
    let too_big = format!(
        "// {}\n{}",
        "x".repeat(16 * 1024),
        script(&format!("{{api: 1, id: 'big', name: 'Big', {OK_START}}}"))
    );
    let cases: Vec<(String, &str)> = vec![
        (String::new(), "empty"),
        ("   \n ".into(), "empty"),
        (too_big, "too large"),
        ("recipe({".into(), "does not compile"),
        ("var x = 1;".into(), "no recipe() call"),
        (
            "recipe({api: 1, id: 'a', name: 'A', slots: {}, start: function () {}}); recipe({api: 1, id: 'b', name: 'B', slots: {}, start: function () {}});".into(),
            "recipe() is called more than once",
        ),
        (script(&format!("{{api: 2, id: 'a', name: 'A', {OK_START}}}")), "api must be 1"),
        (script(&format!("{{id: 'a', name: 'A', {OK_START}}}")), "api must be 1"),
        (script(&format!("{{api: 1, id: 'Upper', name: 'A', {OK_START}}}")), "id must be 1 to 64 characters among a-z, 0-9 and -"),
        (script(&format!("{{api: 1, id: '', name: 'A', {OK_START}}}")), "id must be 1 to 64 characters among a-z, 0-9 and -"),
        (script(&format!("{{api: 1, id: '{long_id}', name: 'A', {OK_START}}}")), "id must be 1 to 64 characters among a-z, 0-9 and -"),
        (script(&format!("{{api: 1, id: 'a_b', name: 'A', {OK_START}}}")), "id must be 1 to 64 characters among a-z, 0-9 and -"),
        (script(&format!("{{api: 1, id: 'a', {OK_START}}}")), "name is required"),
        (script("{api: 1, id: 'a', name: 'A'}"), "no start function"),
        (script(&format!("{{api: 1, id: 'a', name: 'A', slots: {{t: {{type: 'text'}}}}, {OK_START}}}")), "slot t: type \"text\" is not supported"),
        (script(&format!("{{api: 1, id: 'a', name: 'A', slots: {{t: {{type: 'data-key'}}}}, {OK_START}}}")), "slot t: type \"data-key\" is not supported"),
        (script(&format!("{{api: 1, id: 'a', name: 'A', slots: {{t: {{type: 'color'}}}}, {OK_START}}}")), "slot t: unknown type \"color\""),
        (script(&format!("{{api: 1, id: 'a', name: 'A', slots: 5, {OK_START}}}")), "slots must be an object"),
        (script(&format!("{{api: 1, id: 'a', name: 'A', slots: {{zone: {{type: 'number'}}}}, {OK_START}}}")), "slot zone must be of type zone"),
        (script(&format!("{{api: 1, id: 'motion-light', name: 'A', {OK_START}}}")), "id motion-light is a built-in recipe"),
        ("throw new Error('at load');".into(), "the script failed when loaded: at load"),
    ];
    for (source, expected) in cases {
        let mut rig = Rig::new();
        let error = load(&mut rig, &source).unwrap_err();
        assert!(error.starts_with("Script recipe invalid: "), "{error}");
        assert!(
            error.contains(expected),
            "{error} should contain {expected}"
        );
        // Nothing was loaded.
        let (_, list) = rig.get("/api/v1/recipes");
        assert_eq!(list.as_array().unwrap().len(), 1);
    }
}

#[test]
fn a_script_with_a_loaded_id_is_refused_and_a_third_script_too() {
    let mut rig = Rig::new();
    load(&mut rig, PROBE).unwrap();
    assert_eq!(
        load(&mut rig, PROBE).unwrap_err(),
        "Script recipe invalid: id probe is already loaded"
    );
    for n in 2..=MAX_SCRIPT_RECIPES {
        load(
            &mut rig,
            &script(&format!(
                "{{api: 1, id: 'script-{n}', name: 'S', {OK_START}}}"
            )),
        )
        .unwrap();
    }
    assert_eq!(
        load(
            &mut rig,
            &script(&format!(
                "{{api: 1, id: 'one-more', name: 'S', {OK_START}}}"
            )),
        )
        .unwrap_err(),
        format!("Script recipe invalid: too many script recipes (at most {MAX_SCRIPT_RECIPES})")
    );
    // The scripts in load order, after the built-in recipe.
    let (_, list) = rig.get("/api/v1/recipes");
    let ids: Vec<&str> = list
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["motion-light", "probe", "script-2"]);
}

#[test]
fn a_script_recipe_holds_eight_instances_and_a_ninth_is_refused_the_built_in_ones_not_counted() {
    let (mut rig, g) = loaded(PROBE);
    // Built-in instances do not count against the script's bound.
    let _ = rig.garage_instance(&g, json!({}));
    for _ in 0..MAX_INSTANCES_PER_SCRIPT {
        probe_instance(&mut rig, &g);
    }
    assert_eq!(MAX_INSTANCES_PER_SCRIPT, 8);
    assert_eq!(
        reason(
            &mut rig,
            "probe",
            json!({"zone": g.zone, "lights": [g.light]})
        ),
        (400, "Too many instances of probe (max 8)".to_string())
    );
    let (_, list) = rig.get("/api/v1/recipe-instances");
    assert_eq!(list.as_array().unwrap().len(), 9);
}

#[test]
fn on_restore_the_ninth_instance_of_a_script_fails_to_restore_and_stays_stopped_not_dropped() {
    use solite_core::recipes::RecipeInstance;
    let (mut rig, g) = loaded(PROBE);
    let params = json!({"zone": g.zone, "lights": [g.light]}).to_string();
    let list: Vec<RecipeInstance> = (0..=MAX_INSTANCES_PER_SCRIPT as u8)
        .map(|n| RecipeInstance {
            id: Id::from_random([n + 1; 16]),
            recipe_id: "probe".into(),
            params: params.clone().into(),
            enabled: true,
            created_at: rig.now,
        })
        .collect();
    rig.home.load_recipes(list);
    rig.home.start_recipes(rig.now, rig.now);
    for n in 0..MAX_INSTANCES_PER_SCRIPT as u8 {
        assert!(running(&rig, &Id::from_random([n + 1; 16]).to_string()));
    }
    let ninth = Id::from_random([MAX_INSTANCES_PER_SCRIPT as u8 + 1; 16]).to_string();
    assert!(!running(&rig, &ninth));
    assert_eq!(
        rig.messages(&ninth).last().unwrap(),
        "Failed to restore: Too many instances of probe (max 8)"
    );
    let (_, all) = rig.get("/api/v1/recipe-instances");
    assert_eq!(all.as_array().unwrap().len(), 9);
}

// ---- slots ----

fn slots_rig() -> (Rig, Garage) {
    loaded(SLOTS)
}

fn invalid(text: &str) -> (u16, String) {
    (400, format!("Invalid params: {text}"))
}

#[test]
fn the_host_checks_every_slot_with_its_own_texts() {
    let (mut rig, g) = slots_rig();
    let other_zone = rig.zone("Elsewhere");
    let far = {
        let relay = rig.relay(0x62);
        rig.equipment("Far light", "light_onoff", &other_zone, &[&relay])
    };
    let dimmable = rig.equipment("Lamp", "light_dimmable", &g.zone, &[]);
    let ghost = "11111111-2222-4333-8444-555555555555";
    let (zone, light) = (g.zone.clone(), g.light.clone());
    let ok = || json!({"zone": zone, "lights": [light]});
    let with = |extra: Value| {
        let mut params = ok();
        for (k, v) in extra.as_object().unwrap() {
            params[k] = v.clone();
        }
        params
    };
    let cases: Vec<(Value, String)> = vec![
        (json!({"lights": [light]}), "zone is required".into()),
        (json!({"zone": zone}), "lights is required".into()),
        (
            json!({"zone": zone, "lights": []}),
            "lights is required".into(),
        ),
        (
            json!({"zone": ghost, "lights": [light]}),
            format!("zone: zone not found: {ghost}"),
        ),
        (
            with(json!({"level": 101})),
            "level must be a number between 0 and 100".into(),
        ),
        (
            with(json!({"level": -1})),
            "level must be a number between 0 and 100".into(),
        ),
        (
            with(json!({"level": "ten"})),
            "level must be a number between 0 and 100".into(),
        ),
        (
            with(json!({"delay": "10x"})),
            "delay: invalid duration \"10x\"".into(),
        ),
        (
            with(json!({"delay": true})),
            "delay: invalid duration \"true\"".into(),
        ),
        (
            with(json!({"mode": "turbo"})),
            "mode must be one of eco, boost".into(),
        ),
        (
            with(json!({"flag": "yes"})),
            "flag must be true or false".into(),
        ),
        (
            with(json!({"wake": "7h"})),
            "wake: invalid time \"7h\"".into(),
        ),
        (
            json!({"zone": zone, "lights": [ghost]}),
            format!("lights: equipment not found: {ghost}"),
        ),
        (
            json!({"zone": zone, "lights": [dimmable]}),
            "lights: Lamp is not a light_onoff".into(),
        ),
        (
            json!({"zone": zone, "lights": [far]}),
            "lights: Far light is not in the zone".into(),
        ),
        (
            with(
                json!({"button": g.motion_equipment.clone(), "level": 5, "delay": "1m", "mode": "eco", "flag": true, "wake": "07:30"}),
            ),
            String::new(),
        ),
    ];
    for (params, message) in cases {
        let got = reason(&mut rig, "slots", params.clone());
        if message.is_empty() {
            assert_eq!(got.0, 201, "{params}");
        } else {
            assert_eq!(got, invalid(&message), "{params}");
        }
    }
}

#[test]
fn an_equipment_outside_the_zone_is_accepted_with_cross_zone_and_a_list_holds_16() {
    let (mut rig, g) = slots_rig();
    let other_zone = rig.zone("Elsewhere");
    // The `button` slot is `crossZone`: a sensor of another zone passes (and a light type does not).
    let sensor = rig.equipment("Far sensor", "sensor", &other_zone, &[]);
    let (status, body) = create(
        &mut rig,
        "slots",
        json!({"zone": g.zone, "lights": [g.light], "button": sensor}),
    );
    assert_eq!(status, 201, "{body}");
    let far_light = rig.equipment("Far light", "light_onoff", &other_zone, &[]);
    assert_eq!(
        reason(
            &mut rig,
            "slots",
            json!({"zone": g.zone, "lights": [g.light], "button": far_light})
        ),
        invalid("button: Far light is not a button or sensor")
    );
    // 16 lights in a list are accepted, 17 are not.
    let mut lights: Vec<String> = (0..16)
        .map(|n| rig.equipment(&format!("Light {n}"), "light_onoff", &g.zone, &[]))
        .collect();
    assert_eq!(
        create(&mut rig, "slots", json!({"zone": g.zone, "lights": lights})).0,
        201
    );
    lights.push(rig.equipment("Light 16", "light_onoff", &g.zone, &[]));
    assert_eq!(
        reason(&mut rig, "slots", json!({"zone": g.zone, "lights": lights})),
        invalid("lights: at most 16 equipments")
    );
}

#[test]
fn the_scripts_own_validate_comes_after_the_hosts_checks() {
    let (mut rig, g) = slots_rig();
    let base = json!({"zone": g.zone, "lights": [g.light]});
    let mut forty_two = base.clone();
    forty_two["level"] = json!(42);
    assert_eq!(
        reason(&mut rig, "slots", forty_two.clone()),
        invalid("forty-two is taken")
    );
    // The host's checks first: a bad zone wins over the script's text.
    forty_two["zone"] = json!("11111111-2222-4333-8444-555555555555");
    assert!(
        reason(&mut rig, "slots", forty_two)
            .1
            .contains("zone not found")
    );
    let mut throws = base.clone();
    throws["level"] = json!(43);
    assert_eq!(
        reason(&mut rig, "slots", throws),
        invalid("Script error: validate threw")
    );
    // A PUT is checked the same way, and the running instance is untouched.
    let id = created(&mut rig, "slots", base.clone());
    let mut bad = base;
    bad["level"] = json!(42);
    let (status, body) = rig.put(
        &format!("/api/v1/recipe-instances/{id}"),
        json!({"params": bad}),
    );
    assert_eq!(
        (status, body["error"].as_str().unwrap()),
        (400, "Invalid params: forty-two is taken")
    );
    assert!(running(&rig, &id));
}

#[test]
fn the_script_gets_its_params_parsed() {
    let (mut rig, g) = slots_rig();
    let id = created(
        &mut rig,
        "slots",
        json!({"zone": g.zone, "lights": [g.light], "button": g.motion_equipment, "level": 7, "mode": "eco", "flag": false}),
    );
    let messages = rig.messages(&id);
    let params: Value = serde_json::from_str(&messages[1]).unwrap();
    // A duration in milliseconds (the default, "5m"), a list of ids, one id, the zone id.
    assert_eq!(params["delay"], 300_000);
    assert_eq!(params["lights"], json!([g.light]));
    assert_eq!(params["button"], g.motion_equipment.as_str());
    assert_eq!(params["zone"], g.zone.as_str());
    assert_eq!(params["level"], 7);
    assert_eq!(params["flag"], false);
}

// ---- API calls ----

fn probe_instance(rig: &mut Rig, g: &Garage) -> String {
    created(rig, "probe", json!({"zone": g.zone, "lights": [g.light]}))
}

#[test]
fn on_and_on_zone_and_on_motion_report_call_the_handlers_with_the_values() {
    let (mut rig, g) = loaded(PROBE);
    let id = probe_instance(&mut rig, &g);
    let mut on = false;
    toggle_motion(&mut rig, &mut on);
    let messages = rig.messages(&id);
    // The sensor's report, then the zone's aggregate: every `onZone` handler, in order.
    assert!(
        has(&messages, &format!("report true {}", g.motion_equipment)),
        "{messages:?}"
    );
    let at = messages
        .iter()
        .position(|m| m == "zone motion true")
        .unwrap();
    assert_eq!(
        &messages[at..at + 3],
        ["zone motion true", "zone luminosity null", "zone sensors 1"]
    );
    // The light's own `state` data.
    rig.report(&state(RELAY, true));
    let messages = rig.messages(&id);
    assert!(
        has(&messages, &format!("light {} true", g.light)),
        "{messages:?}"
    );
}

#[test]
fn a_person_staying_in_the_room_calls_the_motion_report_handler_at_each_report() {
    let (mut rig, g) = loaded(PROBE);
    let id = probe_instance(&mut rig, &g);
    for _ in 0..3 {
        rig.report(&motion(PIR, true));
    }
    let reports = rig
        .messages(&id)
        .iter()
        .filter(|m| m.starts_with("report true"))
        .count();
    // The zone's aggregate did not change after the first one; the reports still arrive.
    assert_eq!(reports, 3);
    let zone_events = rig
        .messages(&id)
        .iter()
        .filter(|m| *m == "zone motion true")
        .count();
    assert_eq!(zone_events, 1);
    rig.report(&motion(PIR, false));
    assert!(has(
        &rig.messages(&id),
        &format!("report false {}", g.motion_equipment)
    ));
}

#[test]
fn an_aggregate_that_is_not_motion_calls_every_on_zone_handler_once_with_current_values() {
    let (mut rig, g) = loaded(PROBE);
    let climate = rig.sensor(0x53);
    rig.equipment("Climate", "sensor", &g.zone, &[&climate]);
    let id = probe_instance(&mut rig, &g);
    let before = rig.messages(&id).len();
    rig.report(&temperature(0x53, 21.5));
    let messages = rig.messages(&id);
    assert_eq!(
        &messages[before..],
        [
            "zone motion false",
            "zone luminosity null",
            "zone sensors 1"
        ]
    );
}

#[test]
fn an_on_zone_alias_that_is_not_motion_luminosity_or_motion_sensors_throws() {
    let (mut rig, g) = loaded(CASES);
    let id = case(&mut rig, &g, "badzone");
    assert!(has(&rig.messages(&id), "Unknown zone data: temperature"));
}

#[test]
fn get_reads_one_equipment_a_list_and_the_zone() {
    let (mut rig, g) = loaded(READS);
    let second = rig.relay(0x7B);
    let second_light = rig.equipment("Second light", "light_onoff", &g.zone, &[&second]);
    let id = created(
        &mut rig,
        "reads",
        json!({"zone": g.zone, "lights": [g.light, second_light], "first": g.light}),
    );
    rig.report(&state(0x7B, true));
    let mut on = false;
    toggle_motion(&mut rig, &mut on);
    let messages = rig.messages(&id);
    assert!(has(&messages, "first false"), "{messages:?}");
    assert!(has(&messages, "lights [false,true]"), "{messages:?}");
    assert!(has(&messages, "zone [true,null,1]"), "{messages:?}");
    // An alias the equipment has not: null.
    assert!(has(&messages, "power [null,null]"), "{messages:?}");
}

/// An equipment whose light state is a `light_state` data bound under the alias `level` and whose
/// switch is a `light_toggle` order bound under the alias `flip`: no `state` alias at all.
fn odd_light(rig: &mut Rig, zone: &str) -> String {
    let mut device = device_json(
        0x31,
        "Odd relay",
        false,
        &[("power", "light_state", "boolean", json!(true), Some(T0))],
    );
    let order = "d0310000-0000-4000-8000-0000000000aa";
    device["orders"] = json!([{
        "id": order, "deviceId": device["id"], "key": "flip", "type": "boolean",
        "category": "light_toggle"
    }]);
    let data = data_id(&device, 0);
    rig.add_devices(vec![device]);
    let equipment = rig.equipment("Odd light", "light_onoff", zone, &[]);
    let (status, body) = rig.post(
        &format!("/api/v1/equipments/{equipment}/data-bindings"),
        json!({"deviceDataId": data, "alias": "level"}),
    );
    assert_eq!(status, 201, "{body}");
    let (status, body) = rig.post(
        &format!("/api/v1/equipments/{equipment}/order-bindings"),
        json!({"deviceOrderId": order, "alias": "flip"}),
    );
    assert_eq!(status, 201, "{body}");
    equipment
}

#[test]
fn get_and_order_use_the_light_state_and_the_toggle_by_category_before_the_alias() {
    // The device is added first: adding devices makes the home again.
    let mut rig = Rig::new();
    let g = rig.garage();
    let odd = odd_light(&mut rig, &g.zone);
    load(&mut rig, READS).unwrap();
    let id = created(
        &mut rig,
        "reads",
        json!({"zone": g.zone, "lights": [odd], "first": odd}),
    );
    let mut on = false;
    toggle_motion(&mut rig, &mut on);
    // `get(.., "state")` finds the `light_state` data under another alias.
    let messages = rig.messages(&id);
    assert!(has(&messages, "first true"), "{messages:?}");
    assert!(has(&messages, "lights [true]"), "{messages:?}");
}

#[test]
fn order_finds_a_toggle_binding_by_category_under_another_alias() {
    let mut rig = Rig::new();
    let g = rig.garage();
    let odd = odd_light(&mut rig, &g.zone);
    load(&mut rig, CASES).unwrap();
    let id = created(
        &mut rig,
        "cases",
        json!({"zone": g.zone, "lights": [odd], "case": "orders"}),
    );
    // The `light_toggle` binding is found under its own alias (`flip`): nothing is refused, and
    // without the category the answer would be "no order state".
    let messages = rig.messages(&id);
    assert!(has(&messages, "refused 0 []"), "{messages:?}");
}

#[test]
fn order_sends_to_every_enabled_equipment_and_names_the_ones_that_cannot_take_it() {
    let (mut rig, g) = loaded(CASES);
    let second = rig.relay(0x7B);
    let second_light = rig.equipment("Second light", "light_onoff", &g.zone, &[&second]);
    let bare = rig.equipment("Bare light", "light_onoff", &g.zone, &[]);
    let disabled_relay = rig.relay(0x7C);
    let disabled = rig.equipment("Disabled light", "light_onoff", &g.zone, &[&disabled_relay]);
    let (status, _) = rig.put(
        &format!("/api/v1/equipments/{disabled}"),
        json!({"enabled": false}),
    );
    assert_eq!(status, 200);
    let id = created(
        &mut rig,
        "cases",
        json!({"zone": g.zone, "lights": [g.light, second_light, bare, disabled], "case": "orders"}),
    );
    let messages = rig.messages(&id);
    // The bare light has no `state` order; the disabled one gets nothing and says nothing.
    assert!(
        has(&messages, r#"refused 1 ["no order state"]"#),
        "{messages:?}"
    );
    assert_eq!(rig.sent().len(), 2);
}

#[test]
fn console_log_is_the_log_and_date_is_the_calls_wall_time() {
    let (mut rig, g) = loaded(CASES);
    let id = case(&mut rig, &g, "console");
    assert!(has(&rig.messages(&id), "via console"));
    let id = case(&mut rig, &g, "date");
    let line = rig.messages(&id)[1].clone();
    // The engine's clock, T0 = 2026-11-21T09:12:03.120Z, not the operating system's.
    assert_eq!(
        line,
        format!("now {T0} 2026-11-21T09:12:03.120Z 2026-11-21T09:12:03.120Z")
    );
}

#[test]
fn a_then_scheduled_in_a_handler_runs_after_the_call_within_its_limits() {
    let (mut rig, g) = loaded(CASES);
    let id = case(&mut rig, &g, "then");
    let messages = rig.messages(&id);
    assert_eq!(&messages[1..], ["a", "b", "then"]);
    assert_eq!(rig.state(&id), json!({"job": "ran"}));
}

#[test]
fn a_33rd_listened_pair_throws() {
    let (mut rig, g) = loaded(CASES);
    let lights: Vec<String> = (0..16)
        .map(|n| rig.equipment(&format!("L{n}"), "light_onoff", &g.zone, &[]))
        .collect();
    let id = created(
        &mut rig,
        "cases",
        json!({"zone": g.zone, "lights": lights, "button": g.motion_equipment, "case": "listen33"}),
    );
    assert!(has(
        &rig.messages(&id),
        "Too many listened equipments (at most 32)"
    ));
}

// ---- sandbox ----

#[test]
fn a_script_reaches_only_the_slots_of_its_settings() {
    let (mut rig, g) = loaded(SANDBOX);
    let id = created(
        &mut rig,
        "sandbox",
        json!({"zone": g.zone, "lights": [g.light]}),
    );
    let messages = rig.messages(&id);
    for call in ["on-slot", "get-slot", "order-slot"] {
        assert!(
            has(&messages, &format!("{call}: Unknown slot: nope")),
            "{messages:?}"
        );
    }
    // A raw equipment id is not a slot; neither is another recipe's slot, nor an inherited name.
    assert!(
        has(&messages, &format!("on-id: Unknown slot: {}", g.light)),
        "{messages:?}"
    );
    assert!(
        has(&messages, &format!("get-id: Unknown slot: {}", g.light)),
        "{messages:?}"
    );
    assert!(
        has(&messages, &format!("order-id: Unknown slot: {}", g.light)),
        "{messages:?}"
    );
    assert!(
        has(&messages, "get-recipe: Unknown slot: buttons"),
        "{messages:?}"
    );
    assert!(
        has(&messages, "get-proto: Unknown slot: constructor"),
        "{messages:?}"
    );
}

#[test]
fn the_engines_functions_are_not_on_the_globals() {
    let (mut rig, g) = loaded(SANDBOX);
    let id = created(
        &mut rig,
        "sandbox",
        json!({"zone": g.zone, "lights": [g.light]}),
    );
    let messages = rig.messages(&id);
    assert!(has(&messages, "host undefined undefined"), "{messages:?}");
    // The only global that has `shim` in it is not enumerable, but it is there by name: frozen.
    assert!(has(&messages, "globals "), "{messages:?}");
    assert!(has(&messages, "shim object true"), "{messages:?}");
}

#[test]
fn mode_energy_and_history_are_not_available() {
    let (mut rig, g) = loaded(CASES);
    let id = case(&mut rig, &g, "na");
    let messages = rig.messages(&id);
    for name in ["mode", "energy", "history"] {
        assert!(
            has(&messages, &format!("Not available in Solite: {name}")),
            "{messages:?}"
        );
    }
}

const SENSOR_A: &str = "00000000-0000-0000-0000-00000000000a";
const SENSOR_B: &str = "00000000-0000-0000-0000-00000000000b";
const LAMP: &str = "00000000-0000-0000-0000-00000000000c";

/// A zone where two equipments hold the same motion sensor (one device, one key): a report of it is
/// two calls of the script, one per equipment.
struct SharedSensor;

impl Host for SharedSensor {
    fn equipment(&self, id: &str) -> Option<solite_recipe_sdk::EquipmentView> {
        let motion = solite_recipe_sdk::DataBindingView {
            id: format!("{id}-motion"),
            alias: "motion".into(),
            category: Some("motion".into()),
            device_id: Some("0x00124b0000000001".into()),
            key: Some("occupancy".into()),
            value: solite_recipe_sdk::EventValue::Bool(false),
        };
        let data = if id == LAMP { Vec::new() } else { vec![motion] };
        Some(solite_recipe_sdk::EquipmentView {
            id: id.into(),
            name: id.into(),
            kind: if id == LAMP { "light_onoff" } else { "sensor" }.into(),
            enabled: true,
            zone_id: ZONE.into(),
            data_bindings: data,
            order_bindings: Vec::new(),
        })
    }
    fn zone_equipment_ids(&self, _: &str) -> Vec<String> {
        vec![SENSOR_A.into(), SENSOR_B.into(), LAMP.into()]
    }
    fn zone_data(&self, id: &str) -> Option<solite_recipe_sdk::ZoneData> {
        NoHome.zone_data(id)
    }
    fn sunlight(&self) -> solite_recipe_sdk::Sunlight {
        solite_recipe_sdk::Sunlight::default()
    }
    fn order(&self, _: &str, _: &str, _: &Value) -> solite_recipe_sdk::OrderStatus {
        solite_recipe_sdk::OrderStatus::NoBinding
    }
    fn log(&self, _: solite_recipe_sdk::Level, _: &str) {}
}

#[test]
fn an_event_of_two_calls_keeps_the_listen_set_the_first_one_changed() {
    let source = format!(
        "recipe({{api: 1, id: 'shared', name: 'S', slots: {{lights: {{type: 'equipment', name: 'L', list: true}}}}, \
         start: function (api) {{ api.onMotionReport(function (v, eq) {{ \
         if (eq === '{SENSOR_A}') api.on('lights', 'state', function () {{}}); \
         else api.state.set('seen', eq); }}); }}}});"
    );
    let mut recipe = direct(&source);
    let params = json!({"zone": ZONE, "lights": [LAMP]});
    recipe
        .start(&SharedSensor, 1, &params, clock_at(0))
        .unwrap();
    let report = Event::Device {
        device: "0x00124b0000000001".into(),
        key: "occupancy".into(),
        value: solite_recipe_sdk::EventValue::Bool(true),
    };
    // The first call (sensor A) listens to the lamp; the second (sensor B) changes only the state.
    let update = recipe
        .on_event(&SharedSensor, 1, &report, clock_at(1))
        .unwrap()
        .expect("an update");
    assert!(update.state.contains(SENSOR_B), "{}", update.state);
    let listen = update.listen.expect("the first call's listen set");
    assert!(
        listen.contains(&solite_recipe_sdk::ListenEntry::Equipment {
            equipment: LAMP.into(),
            alias: "state".into(),
        }),
        "{listen:?}"
    );
}

#[test]
fn a_start_that_ends_without_its_listen_set_does_not_run_deaf() {
    // The script ends its own call early through the shim it can reach by name: the real end of
    // the call then has no listen set, and the start is refused.
    let mut recipe = direct(
        "recipe({api: 1, id: 'early', name: 'E', slots: {}, start: function (api) { \
         api.onZone('motion', function () {}); \
         Promise.resolve().then(function () { globalThis.__solite_shim.end(1); }); }});",
    );
    let error = recipe
        .start(&NoHome, 1, &json!({"zone": ZONE}), clock_at(0))
        .unwrap_err();
    assert_eq!(error.kind, ErrorKind::Recipe);
    assert_eq!(error.message, "bad end of call: no listen set");
}

#[test]
fn a_call_that_changes_only_the_state_leaves_the_instance_listening() {
    let (mut rig, g) = loaded(&script(
        "{api: 1, id: 'counter', name: 'C', slots: {}, start: function (api) { \
         api.onZone('motion', function (m) { var n = (api.state.get('n') || 0) + 1; \
         api.state.set('n', n); api.log('motion ' + n); }); }}",
    ));
    let id = created(&mut rig, "counter", json!({"zone": g.zone}));
    let mut on = false;
    for _ in 0..4 {
        toggle_motion(&mut rig, &mut on);
    }
    // Every call after the first changed the state only: the instance still heard each change.
    assert!(
        has(&rig.messages(&id), "motion 4"),
        "{:?}",
        rig.messages(&id)
    );
}

/// A host with no home: a zone that exists and no equipment, for the calls that need none.
struct NoHome;

impl Host for NoHome {
    fn equipment(&self, _: &str) -> Option<solite_recipe_sdk::EquipmentView> {
        None
    }
    fn zone_equipment_ids(&self, _: &str) -> Vec<String> {
        Vec::new()
    }
    fn zone_data(&self, _: &str) -> Option<solite_recipe_sdk::ZoneData> {
        Some(solite_recipe_sdk::ZoneData {
            motion: false,
            luminosity: None,
            motion_sensors: 0,
        })
    }
    fn sunlight(&self) -> solite_recipe_sdk::Sunlight {
        solite_recipe_sdk::Sunlight::default()
    }
    fn order(&self, _: &str, _: &str, _: &Value) -> solite_recipe_sdk::OrderStatus {
        solite_recipe_sdk::OrderStatus::NoBinding
    }
    fn log(&self, _: solite_recipe_sdk::Level, _: &str) {}
}

const ZONE: &str = "00000000-0000-0000-0000-000000000001";

fn direct(source: &str) -> ScriptRecipe {
    let (clock, _) = ticking();
    let runtimes: Arc<dyn ScriptRuntimes> = Arc::new(SystemRuntimes);
    ScriptRecipe::load(source, &runtimes, &clock).unwrap().0
}

fn clock_at(n: u64) -> Clock {
    Clock {
        mono: n,
        wall: T0 + n,
    }
}

#[test]
fn a_forged_handle_fails_the_recipe() {
    let mut recipe = direct(CASES);
    let params = json!({"zone": ZONE, "case": "ok"});
    recipe.start(&NoHome, 1, &params, clock_at(0)).unwrap();
    // A handle of a real instance is served; any other is refused and the recipe fails.
    let error = recipe.call_host_with_handle(&NoHome, 999).unwrap_err();
    assert_eq!(error.kind, ErrorKind::Recipe);
    assert!(error.message.contains("handle"), "{}", error.message);
    assert!(recipe.call_host_with_handle(&NoHome, 1).is_ok());
}

#[test]
fn two_instances_of_one_script_in_two_zones_see_neither_the_others_events_nor_state() {
    let (mut rig, g) = loaded(PROBE);
    // A second zone with its own sensor and relay.
    let zone = rig.zone("Cellar");
    let pir = rig.motion_sensor(0x51);
    rig.equipment("Cellar motion", "sensor", &zone, &[&pir]);
    let relay = rig.relay(0x7B);
    let light = rig.equipment("Cellar light", "light_onoff", &zone, &[&relay]);
    let first = probe_instance(&mut rig, &g);
    let second = created(&mut rig, "probe", json!({"zone": zone, "lights": [light]}));
    // The garage's light and motion: only the first instance hears.
    let mut on = false;
    toggle_motion(&mut rig, &mut on);
    rig.report(&state(RELAY, true));
    assert!(has(
        &rig.messages(&first),
        &format!("light {} true", g.light)
    ));
    assert!(
        rig.messages(&second)
            .iter()
            .all(|m| !m.starts_with("light") && !m.starts_with("zone") && !m.starts_with("report")),
        "{:?}",
        rig.messages(&second)
    );
    // The cellar's: only the second.
    rig.report(&state(0x7B, true));
    assert!(has(&rig.messages(&second), &format!("light {light} true")));
    assert!(!has(&rig.messages(&first), &format!("light {light} true")));
}

// ---- timers ----

fn timers_rig() -> (Rig, String) {
    let (mut rig, _) = loaded(TIMERS);
    let id = created(&mut rig, "timers", json!({"zone": ROOT}));
    (rig, id)
}

#[test]
fn after_every_and_cancel_fire_on_time_on_the_monotonic_clock() {
    let (mut rig, id) = timers_rig();
    rig.advance(900);
    assert_eq!(rig.messages(&id).len(), 1, "{:?}", rig.messages(&id));
    rig.advance(100);
    assert!(has(&rig.messages(&id), "every 1"));
    rig.advance(1000);
    let messages = rig.messages(&id);
    assert!(
        has(&messages, "every 2") && has(&messages, "after 2s"),
        "{messages:?}"
    );
    rig.advance(1000);
    // The third firing cancels the timer: no fourth, and the cancelled `after` never runs.
    rig.advance(5000);
    let messages = rig.messages(&id);
    assert!(has(&messages, "every 3"));
    assert!(!has(&messages, "every 4"));
    assert!(!has(&messages, "cancelled ran"));
}

#[test]
fn a_timer_too_short_a_17th_timer_and_a_bad_time_throw() {
    let (mut rig, g) = loaded(CASES);
    let id = case(&mut rig, &g, "timers-short");
    let messages = rig.messages(&id);
    assert!(
        has(&messages, "after: the delay must be at least 100 ms"),
        "{messages:?}"
    );
    assert!(
        has(&messages, "every: the period must be at least 1 s"),
        "{messages:?}"
    );
    assert!(
        has(&messages, "at: expected a time like \"07:30\""),
        "{messages:?}"
    );
    let id = case(&mut rig, &g, "timers17");
    assert!(has(
        &rig.messages(&id),
        "16 timers, then: Too many timers (at most 16)"
    ));
}

#[test]
fn the_timers_are_cleared_when_the_instance_stops() {
    let (mut rig, id) = timers_rig();
    rig.advance(1000);
    let (status, _) = rig.post(&format!("/api/v1/recipe-instances/{id}/disable"), json!({}));
    assert_eq!(status, 200);
    let before = rig.messages(&id).len();
    rig.advance(10_000);
    assert_eq!(rig.messages(&id).len(), before);
    // Enabled again: a new start, new timers (the old ones do not fire twice).
    let (status, _) = rig.post(&format!("/api/v1/recipe-instances/{id}/enable"), json!({}));
    assert_eq!(status, 200);
    rig.advance(1000);
    let fired = rig
        .messages(&id)
        .iter()
        .filter(|m| m.starts_with("every"))
        .count();
    assert!(fired <= 4, "{:?}", rig.messages(&id));
}

#[test]
fn the_script_is_called_for_its_timers_only_when_one_is_due() {
    let mut rig = Rig::new();
    let (clock, reads) = ticking();
    rig.home
        .load_script_recipe(TIMERS, Arc::new(SystemRuntimes), clock)
        .unwrap();
    let id = created(&mut rig, "timers", json!({"zone": ROOT}));
    rig.advance(10);
    // A turn with nothing due reads the clock once (the turn's budget); a call reads it more.
    let idle = reads.load(Ordering::SeqCst);
    for _ in 0..20 {
        rig.advance(10);
    }
    assert_eq!(reads.load(Ordering::SeqCst) - idle, 20);
    rig.advance(1000);
    assert!(reads.load(Ordering::SeqCst) - idle > 21);
    assert!(running(&rig, &id));
}

const SEVEN: u64 = 7 * HOUR;

/// 2026-11-26 00:00:00 UTC.
const MIDNIGHT: u64 = 1_795_651_200_000;

fn at_rig(start: u64) -> (Rig, String) {
    let (mut rig, _) = loaded(TIMERS);
    rig.now = start;
    let id = created(&mut rig, "timers", json!({"zone": ROOT}));
    (rig, id)
}

fn fired_at(rig: &Rig, id: &str) -> usize {
    rig.home
        .recipes()
        .log(id, 50)
        .iter()
        .filter(|e| &*e.message == "at 07:00")
        .count()
}

#[test]
fn an_at_timer_fires_once_a_day_at_the_time_of_the_homes_zone() {
    let (mut rig, id) = at_rig(MIDNIGHT + 6 * HOUR + 59 * MINUTE);
    rig.advance(30_000);
    assert_eq!(fired_at(&rig, &id), 0);
    rig.advance(31_000);
    assert_eq!(fired_at(&rig, &id), 1);
    rig.advance(10 * MINUTE);
    assert_eq!(fired_at(&rig, &id), 1);
    // The next day, in the home's time zone: Paris is an hour ahead in November.
    let (status, _) = rig.put("/api/v1/settings", json!({"home.timezone": "Europe/Paris"}));
    assert_eq!(status, 200);
    rig.now = MIDNIGHT + 24 * HOUR + 5 * HOUR + 59 * MINUTE;
    rig.advance(1000);
    assert_eq!(fired_at(&rig, &id), 1);
    rig.advance(2 * MINUTE);
    assert_eq!(fired_at(&rig, &id), 2);
}

#[test]
fn an_at_timer_waits_while_the_clock_is_not_set_and_follows_a_change_of_zone() {
    // The clock is not set (1970): nothing fires, whatever the time looks like.
    let (mut rig, id) = at_rig(5 * HOUR);
    rig.advance(3 * HOUR);
    assert_eq!(fired_at(&rig, &id), 0);
    // The clock gets set at 05:30 UTC; the home is in Paris: 07:00 there is 06:00 UTC.
    let (status, _) = rig.put("/api/v1/settings", json!({"home.timezone": "Europe/Paris"}));
    assert_eq!(status, 200);
    rig.now = MIDNIGHT + 5 * HOUR + 30 * MINUTE;
    rig.advance(1000);
    assert_eq!(fired_at(&rig, &id), 0);
    rig.now = MIDNIGHT + 6 * HOUR + 1000;
    rig.advance(1000);
    assert_eq!(fired_at(&rig, &id), 1);
    let _ = SEVEN;
}

// ---- state, log, errors ----

#[test]
fn the_state_is_served_in_the_order_the_keys_were_set_and_stops_at_512_bytes() {
    let (mut rig, g) = loaded(CASES);
    let id = case(&mut rig, &g, "state-order");
    // `overrideMode` was deleted and set again: it is last.
    assert_eq!(
        state_text(&mut rig, &id),
        r#"{"timerExpiresAt":"t","failsafeExpiresAt":"f","overrideMode":true}"#
    );
    // 512 bytes serialized are accepted, one more throws.
    let id = case(&mut rig, &g, "state");
    let messages = rig.messages(&id);
    assert!(has(&messages, "512 accepted"), "{messages:?}");
    assert!(
        has(&messages, "513: state is over 512 bytes"),
        "{messages:?}"
    );
    assert_eq!(state_text(&mut rig, &id).len(), 512);
}

#[test]
fn twenty_one_lines_in_one_call_keep_twenty_and_a_warning_and_a_long_line_is_cut() {
    let (mut rig, g) = loaded(CASES);
    let id = case(&mut rig, &g, "log21");
    let messages = rig.messages(&id);
    // "Instance created", 20 lines and one warning are 22 lines; the log keeps the last 20
    // (`MAX_LOG_ENTRIES`): the creation and "line 1" are gone.
    assert_eq!(messages.len(), 20, "{messages:?}");
    assert_eq!(messages[0], "line 2");
    assert_eq!(messages[18], "line 20");
    assert_eq!(messages[19], "Log lines dropped");
    let (_, log) = rig.get(&format!("/api/v1/recipe-instances/{id}/log"));
    assert_eq!(log[0]["level"], "warn");
    let id = case(&mut rig, &g, "log300");
    assert_eq!(rig.messages(&id)[1].len(), 200);
}

#[test]
fn a_long_promise_rejection_is_a_cut_log_line_not_a_failure_of_the_recipe() {
    let (mut rig, g) = loaded(CASES);
    let id = case(&mut rig, &g, "reject-long");
    assert!(running(&rig, &id), "{:?}", rig.messages(&id));
    assert!(starts(&rig.messages(&id), "Script error: xxx"));
    let id = case(&mut rig, &g, "timer-async-long");
    rig.advance(1000);
    assert!(running(&rig, &id), "{:?}", rig.messages(&id));
    assert!(starts(&rig.messages(&id), "Script error: bad yyy"));
    assert!(failed_text(&mut rig, &id).is_none());
}

#[test]
fn a_lone_surrogate_in_a_log_or_a_state_never_fails_the_recipe() {
    let (mut rig, g) = loaded(CASES);
    // A half emoji in a log line becomes U+FFFD; a cut inside an emoji drops it whole.
    let id = case(&mut rig, &g, "lone-log");
    assert!(
        has(&rig.messages(&id), "on: Lampe \u{FFFD}"),
        "{:?}",
        rig.messages(&id)
    );
    let id = case(&mut rig, &g, "cut-pair");
    assert!(running(&rig, &id), "{:?}", rig.messages(&id));
    assert!(failed_text(&mut rig, &id).is_none());
    // In the state (keys and values, nested too) it is kept as U+FFFD.
    let id = case(&mut rig, &g, "lone-state");
    assert!(running(&rig, &id), "{:?}", rig.messages(&id));
    assert_eq!(
        rig.state(&id),
        json!({"k\u{FFFD}": "v\u{FFFD}", "n": {"m\u{FFFD}": ["\u{FFFD}"]}})
    );
    // An alias that is not text is the script's error: this instance does not start.
    let id = case(&mut rig, &g, "lone-alias");
    assert!(
        starts(
            &rig.messages(&id),
            "Failed to start: Script error: on: the alias is not valid text"
        ),
        "{:?}",
        rig.messages(&id)
    );
    assert!(!running(&rig, &id));
    assert!(instances_listed(&mut rig) >= 1);
}

#[test]
fn a_start_that_throws_leaves_the_instance_stopped_with_its_message() {
    let (mut rig, g) = loaded(CASES);
    let id = case(&mut rig, &g, "throws");
    assert!(has(
        &rig.messages(&id),
        "Failed to start: Script error: boom"
    ));
    assert!(!running(&rig, &id));
    assert!(enabled(&mut rig, &id));
}

#[test]
fn a_handler_or_a_timer_that_throws_is_a_log_line_and_the_instance_runs_on() {
    let (mut rig, g) = loaded(CASES);
    let id = case(&mut rig, &g, "on-throws");
    let mut on = false;
    toggle_motion(&mut rig, &mut on);
    toggle_motion(&mut rig, &mut on);
    let errors = rig
        .messages(&id)
        .iter()
        .filter(|m| *m == "Script error: boom")
        .count();
    assert_eq!(errors, 2);
    assert!(running(&rig, &id));
    let id = case(&mut rig, &g, "timer-throws");
    rig.advance(1000);
    assert!(has(&rig.messages(&id), "Script error: late"));
    assert!(running(&rig, &id));
}

// ---- limits and failures of the recipe ----

fn failed_text(rig: &mut Rig, id: &str) -> Option<String> {
    rig.messages(id)
        .into_iter()
        .find(|m| m.starts_with("Script recipe failed"))
}

#[test]
fn an_infinite_loop_fails_the_recipe_within_its_time_and_the_others_run_on() {
    let (mut rig, g) = loaded(CASES);
    let healthy = rig.garage_instance(&g, json!({}));
    let id = case(&mut rig, &g, "on-loop");
    let (hub, client) = listening(&rig);
    rig.home.take_recipes_cursor();
    let mut on = false;
    toggle_motion(&mut rig, &mut on);
    assert_eq!(
        failed_text(&mut rig, &id).as_deref(),
        Some("Script recipe failed: took more than 200 ms")
    );
    assert!(!running(&rig, &id));
    // The instance stays enabled and nothing was written; the events say it stopped and why.
    assert!(enabled(&mut rig, &id));
    assert!(!rig.home.recipes_changed());
    let events = event_types(&hub, client);
    assert!(
        events.contains(&"recipe.instance.stopped".to_string()),
        "{events:?}"
    );
    assert!(
        events.contains(&"recipe.instance.error".to_string()),
        "{events:?}"
    );
    // The built-in recipe, in the same garage, went on: its light was switched on.
    assert!(running(&rig, &healthy));
    assert!(!rig.sent().is_empty());
}

#[test]
fn an_allocation_bomb_caught_or_not_fails_the_recipe() {
    for (case_name, expected) in [
        ("on-bomb", "out of memory"),
        ("on-bomb-caught", "out of memory"),
    ] {
        let (mut rig, g) = loaded(CASES);
        let id = case(&mut rig, &g, case_name);
        let mut on = false;
        toggle_motion(&mut rig, &mut on);
        let failed = failed_text(&mut rig, &id).unwrap_or_default();
        assert!(
            failed.starts_with("Script recipe failed: ") && failed.contains(expected),
            "{case_name}: {failed}"
        );
        assert!(!running(&rig, &id));
    }
}

#[test]
fn a_deep_recursion_fails_the_recipe() {
    let (mut rig, g) = loaded(CASES);
    let id = case(&mut rig, &g, "on-recursion");
    let mut on = false;
    toggle_motion(&mut rig, &mut on);
    assert_eq!(
        failed_text(&mut rig, &id).as_deref(),
        Some("Script recipe failed: stack overflow")
    );
}

/// The system's runtimes with a memory limit of the test's choice.
struct Limit(usize);

impl ScriptRuntimes for Limit {
    fn new_runtime(&self) -> Result<NewRuntime, String> {
        SystemRuntimes.new_runtime()
    }

    fn memory_limit(&self) -> usize {
        self.0
    }
}

/// The bytes a recipe holds before and after one call that makes `cycles` cycles of garbage, with
/// this limit.
fn garbage_held(limit: usize, cycles: usize) -> (usize, usize) {
    let runtimes: Arc<dyn ScriptRuntimes> = Arc::new(Limit(limit));
    let (clock, _) = ticking();
    let source = GARBAGE.replace("700", &cycles.to_string());
    let mut recipe = ScriptRecipe::load(&source, &runtimes, &clock).unwrap().0;
    recipe
        .start(&NoHome, 1, &json!({"zone": ZONE}), clock_at(0))
        .unwrap();
    let base = recipe.held_bytes();
    let event = Event::Zone {
        zone: ZONE.into(),
        motion: true,
        luminosity: None,
    };
    recipe.on_event(&NoHome, 1, &event, clock_at(1)).unwrap();
    (base, recipe.held_bytes())
}

#[test]
fn garbage_is_collected_after_a_call_that_grew_the_heap_by_32_kb_not_after_every_call() {
    // A little garbage in a roomy runtime stays: the collector is not asked to run after each call...
    let (base, after) = garbage_held(1024 * 1024, 40);
    let small = after - base;
    assert!(
        (4_000..32 * 1024).contains(&small),
        "{small} bytes of garbage made"
    );
    // ... a call that grew the heap by more than 32 KB is collected, far from the limit.
    let (base, after) = garbage_held(1024 * 1024, 700);
    assert!(
        after < base + 8 * 1024,
        "held {after} after the call, {base} before"
    );
}

#[test]
fn a_script_that_loops_when_loaded_is_refused_and_one_that_loops_in_validate_fails_the_recipe() {
    let mut rig = Rig::new();
    assert_eq!(
        load(&mut rig, "for (;;) {}").unwrap_err(),
        "Script recipe invalid: the script took more than 1000 ms when loaded"
    );
    let g = rig.garage();
    load(
        &mut rig,
        &script("{api: 1, id: 'slow-validate', name: 'V', validate: function () { for (;;) {} }, start: function () {}}"),
    )
    .unwrap();
    assert_eq!(
        reason(&mut rig, "slow-validate", json!({"zone": g.zone})),
        (
            500,
            "Script recipe failed: took more than 200 ms".to_string()
        )
    );
}

/// The system's runtimes, with an allocator that can be told to have refused an allocation: one
/// flag per runtime, in the order the runtimes were made.
#[derive(Default)]
struct Refusing(std::sync::Mutex<Vec<RefusedFlag>>);

impl Refusing {
    fn flag(&self, n: usize) -> RefusedFlag {
        self.0.lock().unwrap()[n].clone()
    }
}

impl ScriptRuntimes for Refusing {
    fn new_runtime(&self) -> Result<NewRuntime, String> {
        let flag: RefusedFlag = Arc::new(std::sync::atomic::AtomicBool::new(false));
        self.0.lock().unwrap().push(flag.clone());
        Ok(NewRuntime {
            runtime: SystemRuntimes.new_runtime()?.runtime,
            refused: Some(flag),
            held: None,
        })
    }

    fn memory_limit(&self) -> usize {
        SYSTEM_MEMORY_LIMIT
    }
}

fn zone_event(motion: bool) -> Event {
    Event::Zone {
        zone: ZONE.into(),
        motion,
        luminosity: None,
    }
}

/// A script that reads the zone's motion on each motion change: one host call per call.
const TOUCH: &str = "recipe({api: 1, id: 'touch', name: 'T', slots: {}, start: function (api) { \
    api.onZone('motion', function () { api.get('zone', 'motion'); }); }});";

/// A host whose zone read sets a refusal flag: the allocator refused during that call.
struct Refuses(RefusedFlag);

impl Host for Refuses {
    fn equipment(&self, _: &str) -> Option<solite_recipe_sdk::EquipmentView> {
        None
    }
    fn zone_equipment_ids(&self, _: &str) -> Vec<String> {
        Vec::new()
    }
    fn zone_data(&self, id: &str) -> Option<solite_recipe_sdk::ZoneData> {
        self.0.store(true, Ordering::SeqCst);
        NoHome.zone_data(id)
    }
    fn sunlight(&self) -> solite_recipe_sdk::Sunlight {
        solite_recipe_sdk::Sunlight::default()
    }
    fn order(&self, _: &str, _: &str, _: &Value) -> solite_recipe_sdk::OrderStatus {
        solite_recipe_sdk::OrderStatus::NoBinding
    }
    fn log(&self, _: solite_recipe_sdk::Level, _: &str) {}
}

fn touching(runtimes: &Arc<Refusing>) -> ScriptRecipe {
    let shared: Arc<dyn ScriptRuntimes> = runtimes.clone();
    let (clock, _) = ticking();
    let mut recipe = ScriptRecipe::load(TOUCH, &shared, &clock).unwrap().0;
    recipe
        .start(&NoHome, 1, &json!({"zone": ZONE}), clock_at(0))
        .unwrap();
    recipe
}

#[test]
fn an_allocation_the_allocator_refused_fails_the_recipe_even_if_the_script_caught_the_error() {
    let runtimes = Arc::new(Refusing::default());
    let mut recipe = touching(&runtimes);
    // The allocator refuses during the call (the host's read stands for it): the call failed.
    let host = Refuses(runtimes.flag(0));
    let error = recipe
        .on_event(&host, 1, &zone_event(true), clock_at(1))
        .unwrap_err();
    assert_eq!(error.kind, ErrorKind::Recipe);
    assert!(
        error.message.starts_with("out of memory"),
        "{}",
        error.message
    );
}

/// The system's runtimes, with a count of the held bytes the test sets (as the box's allocator
/// keeps one): the engine reads it in place of QuickJS's accounting.
struct Counted(HeldCounter);

impl ScriptRuntimes for Counted {
    fn new_runtime(&self) -> Result<NewRuntime, String> {
        Ok(NewRuntime {
            runtime: SystemRuntimes.new_runtime()?.runtime,
            refused: None,
            held: Some(self.0.clone()),
        })
    }

    fn memory_limit(&self) -> usize {
        SYSTEM_MEMORY_LIMIT
    }
}

#[test]
fn the_allocators_count_is_the_held_memory_and_nine_tenths_of_the_limit_fails_the_recipe() {
    let held: HeldCounter = Arc::new(std::sync::atomic::AtomicUsize::new(10_000));
    let runtimes: Arc<dyn ScriptRuntimes> = Arc::new(Counted(held.clone()));
    let (clock, _) = ticking();
    let mut recipe = ScriptRecipe::load(TOUCH, &runtimes, &clock).unwrap().0;
    assert_eq!(recipe.held_bytes(), 10_000);
    recipe
        .start(&NoHome, 1, &json!({"zone": ZONE}), clock_at(0))
        .unwrap();
    // The allocator says the runtime holds almost all its limit, whatever QuickJS counts: after
    // the collection it still does, so the call failed the recipe.
    held.store(SYSTEM_MEMORY_LIMIT / 100 * 95, Ordering::SeqCst);
    let error = recipe
        .on_event(&NoHome, 1, &zone_event(true), clock_at(1))
        .unwrap_err();
    assert_eq!(error.kind, ErrorKind::Recipe);
    assert!(
        error.message.starts_with("out of memory"),
        "{}",
        error.message
    );
}

#[test]
fn a_refusal_outside_a_call_or_of_another_runtime_is_not_a_failure() {
    let runtimes = Arc::new(Refusing::default());
    let mut first = touching(&runtimes);
    let mut second = touching(&runtimes);
    // The first runtime's allocator refused something while it loaded or stopped something: the
    // flag is cleared before its next call, and it is not the other recipe's anyway.
    runtimes.flag(0).store(true, Ordering::SeqCst);
    assert!(
        second
            .on_event(&NoHome, 1, &zone_event(true), clock_at(1))
            .is_ok()
    );
    assert!(
        first
            .on_event(&NoHome, 1, &zone_event(true), clock_at(1))
            .is_ok()
    );
    // A load clears it too: a refusal in the middle of loading is that load's.
    runtimes.flag(1).store(true, Ordering::SeqCst);
    assert!(
        second
            .on_event(&NoHome, 1, &zone_event(false), clock_at(2))
            .is_ok()
    );
}

// ---- restarts ----

fn toggle_until_failed(rig: &mut Rig, id: &str, on: &mut bool) {
    toggle_motion(rig, on);
    assert!(!running(rig, id));
}

#[test]
fn a_failed_recipe_comes_back_after_a_minute_three_times_in_24_hours_and_then_waits() {
    let (mut rig, g) = loaded(CASES);
    let id = case(&mut rig, &g, "on-loop");
    let mut on = false;
    for n in 1..=3 {
        toggle_until_failed(&mut rig, &id, &mut on);
        rig.advance(59_999);
        assert!(!running(&rig, &id), "after {n} failures, 59.999 s");
        rig.advance(1);
        assert!(running(&rig, &id), "after {n} failures, 60 s");
        assert!(has(
            &rig.messages(&id),
            &format!("Script recipe restarted ({n} of 3 in 24 h)")
        ));
    }
    // The fourth failure within 24 hours: it stays failed.
    toggle_until_failed(&mut rig, &id, &mut on);
    rig.advance(10 * MINUTE);
    assert!(!running(&rig, &id));
    // A person's action brings it back: enable on the enabled instance.
    let (status, _) = rig.post(&format!("/api/v1/recipe-instances/{id}/enable"), json!({}));
    assert_eq!(status, 200);
    assert!(running(&rig, &id));
    // And after 24 hours the automatic return is again allowed.
    toggle_until_failed(&mut rig, &id, &mut on);
    rig.advance(25 * HOUR);
    assert!(running(&rig, &id));
}

#[test]
fn a_create_or_a_change_while_failed_restarts_the_recipe_and_a_recipe_that_fails_at_once_is_500() {
    let (mut rig, g) = loaded(CASES);
    let first = case(&mut rig, &g, "on-loop");
    let mut on = false;
    toggle_until_failed(&mut rig, &first, &mut on);
    // A create restarts the recipe (and the first instance with it).
    let second = case(&mut rig, &g, "ok");
    assert!(running(&rig, &first) && running(&rig, &second));
    // A recipe that fails again at once: 500, and the instance is not kept.
    let count = rig.home.recipes().len();
    let (status, error) = reason(
        &mut rig,
        "cases",
        json!({"zone": g.zone, "lights": [g.light], "case": "loop"}),
    );
    assert_eq!(
        (status, error.as_str()),
        (500, "Script recipe failed: took more than 200 ms")
    );
    assert_eq!(rig.home.recipes().len(), count);
    assert!(!running(&rig, &first) && !running(&rig, &second));
    // A change of an instance of the recipe restarts it too.
    let (status, _) = rig.put(
        &format!("/api/v1/recipe-instances/{second}"),
        json!({"params": {"zone": g.zone, "lights": [g.light], "case": "ok"}}),
    );
    assert_eq!(status, 200);
    assert!(running(&rig, &first) && running(&rig, &second));
}

#[test]
fn a_settle_while_the_recipe_is_failed_leaves_its_instances_alone() {
    let (mut rig, g) = loaded(CASES);
    let spare = rig.equipment("Spare light", "light_onoff", &g.zone, &[]);
    let id = created(
        &mut rig,
        "cases",
        json!({"zone": g.zone, "lights": [g.light, spare], "case": "on-loop"}),
    );
    let mut on = false;
    toggle_until_failed(&mut rig, &id, &mut on);
    // A light is deleted while the recipe is failed: it leaves the settings, the instance stays
    // enabled (it is not checked against a runtime that is gone).
    let (status, _) = rig.delete(&format!("/api/v1/equipments/{spare}"));
    assert_eq!(status, 204);
    assert!(enabled(&mut rig, &id));
    assert!(!running(&rig, &id));
    let (_, instances) = rig.get("/api/v1/recipe-instances");
    let params = instances
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["id"] == id.as_str())
        .unwrap()["params"]
        .clone();
    assert_eq!(params["lights"], json!([g.light]));
    // A minute later it is back with the rest.
    rig.advance(MINUTE);
    assert!(running(&rig, &id));
}

// ---- bounds ----

#[test]
fn eight_instances_of_a_script_recipe_stay_within_its_memory() {
    let (mut rig, g) = loaded(SPIN);
    let ids: Vec<String> = (0..MAX_INSTANCES_PER_SCRIPT)
        .map(|_| created(&mut rig, "spin", json!({"zone": g.zone})))
        .collect();
    assert!(ids.iter().all(|id| running(&rig, id)));
    // The same through the recipe alone, with its memory read.
    let mut recipe = direct(SPIN);
    let base = recipe.held_bytes();
    for handle in 0..MAX_INSTANCES_PER_SCRIPT as u32 {
        recipe
            .start(&NoHome, handle, &json!({"zone": ZONE}), clock_at(0))
            .unwrap();
    }
    assert_eq!(recipe.instance_count(), MAX_INSTANCES_PER_SCRIPT);
    println!(
        "{MAX_INSTANCES_PER_SCRIPT} instances: {} bytes, {} for the recipe alone",
        recipe.held_bytes(),
        base
    );
    assert!(
        recipe.held_bytes() < SYSTEM_MEMORY_LIMIT,
        "{}",
        recipe.held_bytes()
    );
}

#[test]
fn three_instances_starting_send_48_orders_and_the_queue_keeps_32() {
    let (mut rig, g) = loaded(CASES);
    let lights: Vec<String> = (0..16)
        .map(|n| {
            let relay = rig.relay(0x80 + n);
            rig.equipment(&format!("Light {n}"), "light_onoff", &g.zone, &[&relay])
        })
        .collect();
    let ids: Vec<String> = (0..3)
        .map(|_| {
            created(
                &mut rig,
                "cases",
                json!({"zone": g.zone, "lights": lights, "case": "orders"}),
            )
        })
        .collect();
    let refused: Vec<String> = ids.iter().map(|id| rig.messages(id)[1].clone()).collect();
    assert!(refused[0].starts_with("refused 0"), "{refused:?}");
    assert!(refused[1].starts_with("refused 0"), "{refused:?}");
    assert!(refused[2].starts_with("refused 16"), "{refused:?}");
    assert!(refused[2].contains("queue full"));
    assert_eq!(rig.sent().len(), 32);
}

#[test]
fn a_script_instance_is_called_only_for_what_it_listens_to() {
    let (mut rig, g) = loaded(PROBE);
    let id = probe_instance(&mut rig, &g);
    let interest = rig.home.recipes().interest();
    drop(interest);
    // The button-less garage: a luminosity report moves the aggregate (a zone event), a
    // temperature report of a sensor outside the zone does not.
    let other_zone = rig.zone("Elsewhere");
    let sensor = rig.sensor(0x53);
    rig.equipment("Elsewhere climate", "sensor", &other_zone, &[&sensor]);
    let before = rig.messages(&id).len();
    rig.report(&temperature(0x53, 20.0));
    assert_eq!(rig.messages(&id).len(), before);
}

#[test]
fn a_turn_gives_scripts_300_ms_and_carries_the_rest_to_the_next_turn() {
    let mut rig = Rig::new();
    let g = rig.garage();
    // A clock that moves 1 ms at each reading: one call of the script reads it about 60 times,
    // so 8 instances take about half a second, far over the 300 ms of a turn.
    let (clock, _) = ticking();
    rig.home
        .load_script_recipe(SPIN, Arc::new(SystemRuntimes), clock)
        .unwrap();
    let ids: Vec<String> = (0..MAX_INSTANCES_PER_SCRIPT)
        .map(|_| created(&mut rig, "spin", json!({"zone": g.zone})))
        .collect();
    let spun = |rig: &mut Rig| {
        ids.iter()
            .filter(|id| has(&rig.messages(id), "spun"))
            .count()
    };
    rig.report(&motion(PIR, true));
    let first = spun(&mut rig);
    assert!(
        first > 0 && first < MAX_INSTANCES_PER_SCRIPT,
        "{first} instances handled in the first turn"
    );
    // The next turns finish the event, each instance once.
    for _ in 0..30 {
        rig.advance(10);
    }
    assert_eq!(spun(&mut rig), MAX_INSTANCES_PER_SCRIPT);
    for id in &ids {
        let times = rig.messages(id).iter().filter(|m| *m == "spun").count();
        assert_eq!(times, 1, "{id}");
    }
}

const SLOW_START: &str = r#"
recipe({
  api: 1, id: "slow", name: "Slow", description: "Works in its start",
  slots: { mode: { type: "select", name: "Mode", required: true, options: ["spin", "fail"] } },
  start: function (api) {
    var n = 0;
    for (var i = 0; i < 300000; i++) { n += i; }
    if (api.params.mode === "fail") api.onZone("motion", function () { for (;;) {} });
  },
});
"#;

#[test]
fn the_starts_after_a_restart_stop_when_the_turns_budget_is_spent_and_go_on_next_turn() {
    let mut rig = Rig::new();
    let g = rig.garage();
    // A clock that moves 1 ms at each reading: a start reads it about 60 times.
    let (clock, _) = ticking();
    rig.home
        .load_script_recipe(SLOW_START, Arc::new(SystemRuntimes), clock)
        .unwrap();
    let mut ids = vec![created(
        &mut rig,
        "slow",
        json!({"zone": g.zone, "mode": "fail"}),
    )];
    for _ in 1..MAX_INSTANCES_PER_SCRIPT {
        ids.push(created(
            &mut rig,
            "slow",
            json!({"zone": g.zone, "mode": "spin"}),
        ));
    }
    let mut on = false;
    toggle_until_failed(&mut rig, &ids[0], &mut on);
    assert!(ids.iter().all(|id| !running(&rig, id)));
    // The restart turn: some instances start, in order, then the budget is spent.
    rig.advance(MINUTE);
    let started: Vec<bool> = ids.iter().map(|id| running(&rig, id)).collect();
    let count = started.iter().filter(|r| **r).count();
    assert!(count > 0 && count < MAX_INSTANCES_PER_SCRIPT, "{started:?}");
    assert!(started[..count].iter().all(|r| *r), "in order: {started:?}");
    // The next turns start the others.
    for _ in 0..30 {
        rig.advance(10);
    }
    assert!(ids.iter().all(|id| running(&rig, id)));
    for id in &ids {
        let restarted = rig
            .messages(id)
            .iter()
            .filter(|m| m.starts_with("Script recipe restarted"))
            .count();
        assert_eq!(restarted, 1, "{id}");
    }
}

// ---- stack ----

#[test]
fn the_stack_limit_is_the_same_depth_on_another_thread_than_the_runtimes() {
    let mut recipe = direct(DEPTH);
    recipe
        .start(&NoHome, 1, &json!({"zone": ZONE}), clock_at(0))
        .unwrap();
    // The depth a recursion reaches before the stack limit, read through the log.
    struct Logging(std::sync::Mutex<Vec<String>>);
    impl Host for Logging {
        fn equipment(&self, _: &str) -> Option<solite_recipe_sdk::EquipmentView> {
            None
        }
        fn zone_equipment_ids(&self, _: &str) -> Vec<String> {
            Vec::new()
        }
        fn zone_data(&self, _: &str) -> Option<solite_recipe_sdk::ZoneData> {
            None
        }
        fn sunlight(&self) -> solite_recipe_sdk::Sunlight {
            solite_recipe_sdk::Sunlight::default()
        }
        fn order(&self, _: &str, _: &str, _: &Value) -> solite_recipe_sdk::OrderStatus {
            solite_recipe_sdk::OrderStatus::NoBinding
        }
        fn log(&self, _: solite_recipe_sdk::Level, message: &str) {
            self.0.lock().unwrap().push(message.to_string());
        }
    }
    let event = Event::Zone {
        zone: ZONE.into(),
        motion: true,
        luminosity: None,
    };
    let here = Logging(Default::default());
    recipe.on_event(&here, 1, &event, clock_at(1)).unwrap();
    let there = Logging(Default::default());
    // The same recipe, called from a thread that did not make its runtime.
    let thread_log = std::thread::scope(|scope| {
        scope
            .spawn(|| {
                recipe.on_event(&there, 1, &event, clock_at(2)).unwrap();
                there.0.lock().unwrap().clone()
            })
            .join()
            .unwrap()
    });
    let here_log = here.0.lock().unwrap().clone();
    assert_eq!(here_log.len(), 1, "{here_log:?}");
    assert!(here_log[0].starts_with("depth "));
    assert_eq!(here_log, thread_log);
}

// ---- review: the end of a call, the engine's own functions, the bounds ----

/// What a script does to the shim's end of call: `Object.prototype.toJSON` rewrites the text the
/// shim hands the engine (`kind` says to what); `harmless` rewrites `JSON.stringify` and the
/// entry points, which the engine does not depend on.
const TAMPER: &str = r#"
var FORGED = {
  "bad-state": function () { return { s: '{"broken":', d: { m: null, a: [] } }; },
  "big-state": function () { return { s: '{"a":"' + "x".repeat(20000) + '"}', d: { m: null, a: [] } }; },
  "timers": function () {
    var a = []; for (var i = 0; i < 300; i++) a.push([1, 2, 3]);
    return { d: { m: null, a: a } };
  },
  "overflow": function () { return { d: { m: null, a: [[23, 59, 9223372036854000000]] } }; },
  "alias": function (api) {
    return { l: { e: [[api.params.lights[0], "x".repeat(100)]], z: false, m: false }, d: { m: null, a: [] } };
  },
  "lines": function () {
    var g = []; for (var i = 0; i < 30; i++) g.push([0, "line"]);
    return { g: g, d: { m: null, a: [] } };
  },
};
recipe({
  api: 1, id: "tamper", name: "Tamper", description: "Rewrites what the engine reads",
  slots: {
    lights: { type: "equipment", name: "Lights", list: true, equipmentType: "light_onoff" },
    kind: { type: "select", name: "Kind", required: true, options: ["bad-state", "big-state", "timers", "overflow", "alias", "lines", "harmless"] },
  },
  start: function (api) {
    var kind = api.params.kind;
    if (kind === "harmless") {
      JSON.stringify = function () { return '{"broken":'; };
      JSON.parse = function () { throw new Error("no parse"); };
      try { __solite_shim.end = function () { return '{"s":"{\\"broken\\":","d":{"m":null,"a":[]}}'; }; } catch (e) {}
      api.log("frozen " + Object.isFrozen(__solite_shim));
      api.state.set("a", 1);
      return;
    }
    api.onZone("motion", function () {
      Object.defineProperty(Object.prototype, "toJSON", {
        configurable: true,
        value: function () {
          if (this && this.d && this.d.a !== undefined && this.logs === undefined) return FORGED[kind](api);
          return this;
        },
      });
    });
  },
});
"#;

fn tamper(rig: &mut Rig, g: &Garage, kind: &str) -> String {
    created(
        rig,
        "tamper",
        json!({"zone": g.zone, "lights": [g.light], "kind": kind}),
    )
}

/// `GET /recipe-instances` answers 200 with JSON, whatever a script did.
fn instances_listed(rig: &mut Rig) -> usize {
    let reply = rig.raw("GET", "/api/v1/recipe-instances", "");
    assert_eq!(reply.status, 200);
    let list: Vec<Value> = serde_json::from_str(&reply.body.unwrap()).unwrap();
    list.len()
}

#[test]
fn a_script_that_rewrites_the_end_of_its_call_fails_the_recipe_and_nothing_else() {
    let cases = [
        (
            "bad-state",
            "bad end of call: the state is not a JSON object",
        ),
        ("big-state", "bad end of call: the state is over 512 bytes"),
        ("timers", "bad end of call: too many timers"),
        ("overflow", "bad end of call: a time of day is out of range"),
        ("alias", "bad end of call: an alias is too long"),
        ("lines", "bad end of call: too many log lines"),
    ];
    for (kind, text) in cases {
        let (mut rig, g) = loaded(TAMPER);
        let builtin = rig.garage_instance(&g, json!({}));
        let id = tamper(&mut rig, &g, kind);
        let mut on = false;
        toggle_motion(&mut rig, &mut on);
        assert_eq!(
            failed_text(&mut rig, &id),
            Some(format!("Script recipe failed: {text}")),
            "{kind}"
        );
        assert!(!running(&rig, &id), "{kind}");
        // Every instance is still served, the built-in one first, and it still runs.
        assert_eq!(instances_listed(&mut rig), 2, "{kind}");
        assert!(running(&rig, &builtin), "{kind}");
    }
}

#[test]
fn a_script_that_rewrites_json_or_the_entry_points_changes_nothing_of_what_the_engine_reads() {
    let (mut rig, g) = loaded(TAMPER);
    let id = tamper(&mut rig, &g, "harmless");
    assert!(running(&rig, &id));
    assert!(
        has(&rig.messages(&id), "frozen true"),
        "{:?}",
        rig.messages(&id)
    );
    assert_eq!(state_text(&mut rig, &id), r#"{"a":1}"#);
    assert_eq!(instances_listed(&mut rig), 1);
}

const FACTS: &str = r#"
recipe({
  api: 1, id: "facts", name: "Facts", description: "Asks for what its settings do not name",
  slots: { lights: { type: "equipment", name: "Lights", list: true, equipmentType: "light_onoff" } },
  validate: function (params, home) { return home.equipment(params.lights[0]) ? "" : "no light"; },
  start: function (api) {
    // The engine's `validate` entry point is reachable by name: it is given params of the script's
    // making, and the engine knows which equipments its own settings name (none, in a start).
    __solite_shim.validate(JSON.stringify({ lights: [api.params.lights[0]] }), 0, 0);
  },
});
"#;

#[test]
fn a_script_cannot_read_the_facts_of_an_equipment_its_settings_do_not_name() {
    // The start forges a call to `validate` for the garage's light, which no settings under check
    // name: it gets nothing ("no light"); the recipe does not fail (the forged call also ends the
    // start's own call, which is the instance's error, never the recipe's).
    let (mut rig, g) = loaded(FACTS);
    let (status, body) = create(
        &mut rig,
        "facts",
        json!({"zone": g.zone, "lights": [g.light]}),
    );
    assert_eq!(status, 201, "{body}");
    let id = body["id"].as_str().unwrap().to_string();
    assert!(
        failed_text(&mut rig, &id).is_none(),
        "{:?}",
        rig.messages(&id)
    );
    assert_eq!(instances_listed(&mut rig), 1);
}

const LOOKUP: &str = r#"
recipe({
  api: 1, id: "lookup", name: "Lookup", description: "Looks up text that is not an equipment",
  slots: {
    light: { type: "equipment", name: "Light", equipmentType: "light_onoff" },
    mode: { type: "select", name: "Mode", options: ["07:30", "night"] },
  },
  validate: function (params, home) {
    if (home.equipment(params.zone) !== null) return "a zone is not an equipment";
    if (home.equipment(params.mode) !== null) return "an option is not an equipment";
    return home.equipment(params.light) ? "" : "no light";
  },
  start: function (api) {},
});
"#;

#[test]
fn a_validate_that_looks_up_a_zone_or_an_option_gets_nothing_and_the_recipe_does_not_fail() {
    let (mut rig, g) = loaded(LOOKUP);
    let (status, body) = create(
        &mut rig,
        "lookup",
        json!({"light": g.light, "zone": g.zone, "mode": "07:30"}),
    );
    assert_eq!(status, 201, "{body}");
}

#[test]
fn an_equipment_or_zone_slot_with_a_default_is_refused_when_loaded() {
    for (slot, kind) in [
        ("{type: 'equipment', defaultValue: 'anything'}", "equipment"),
        ("{type: 'equipment', default: 'anything'}", "equipment"),
        ("{type: 'zone', defaultValue: 'anything'}", "zone"),
    ] {
        let mut rig = Rig::new();
        let source = script(&format!(
            "{{api: 1, id: 'a', name: 'A', slots: {{s: {slot}}}, {OK_START}}}"
        ));
        assert_eq!(
            load(&mut rig, &source).unwrap_err(),
            format!("Script recipe invalid: slot s: a default is not allowed for type {kind}")
        );
    }
    // A default stays fine where it cannot name anything but a value.
    let mut rig = Rig::new();
    let source = script(&format!(
        "{{api: 1, id: 'a', name: 'A', slots: {{n: {{type: 'number', defaultValue: 3}}}}, {OK_START}}}"
    ));
    load(&mut rig, &source).unwrap();
}

#[test]
fn a_throw_inside_a_then_is_a_script_error_and_a_stack_overflow_there_fails_the_recipe() {
    let (mut rig, g) = loaded(CASES);
    let id = case(&mut rig, &g, "then-throws");
    assert!(
        has(&rig.messages(&id), "Script error: late"),
        "{:?}",
        rig.messages(&id)
    );
    assert!(running(&rig, &id));
    // An error a later `catch` handled is not one.
    let handled = case(&mut rig, &g, "then-handled");
    let messages = rig.messages(&handled);
    assert!(has(&messages, "caught handled"), "{messages:?}");
    assert!(
        messages.iter().all(|m| !m.starts_with("Script error")),
        "{messages:?}"
    );
    // Past the stack limit: the recipe fails (R3.5).
    let (status, error) = reason(
        &mut rig,
        "cases",
        json!({"zone": g.zone, "lights": [g.light], "case": "then-recursion"}),
    );
    assert_eq!(
        (status, error.as_str()),
        (500, "Script recipe failed: stack overflow")
    );
}

#[test]
fn a_restored_ninth_instance_stays_stopped_when_the_recipe_restarts_by_itself() {
    use solite_core::recipes::RecipeInstance;
    let (mut rig, g) = loaded(CASES);
    let params = json!({"zone": g.zone, "lights": [g.light], "case": "on-loop"}).to_string();
    let ids: Vec<Id> = (0..=MAX_INSTANCES_PER_SCRIPT as u8)
        .map(|n| Id::from_random([n + 1; 16]))
        .collect();
    let list: Vec<RecipeInstance> = ids
        .iter()
        .map(|id| RecipeInstance {
            id: *id,
            recipe_id: "cases".into(),
            params: params.clone().into(),
            enabled: true,
            created_at: rig.now,
        })
        .collect();
    rig.home.load_recipes(list);
    rig.home.start_recipes(rig.now, rig.now);
    let ninth = ids[MAX_INSTANCES_PER_SCRIPT].to_string();
    assert!(!running(&rig, &ninth));
    // The recipe fails (the first instance loops on a motion change) and comes back a minute later.
    let mut on = false;
    toggle_motion(&mut rig, &mut on);
    assert!(ids.iter().all(|id| !running(&rig, &id.to_string())));
    rig.advance(60_000);
    let up = ids
        .iter()
        .filter(|id| running(&rig, &id.to_string()))
        .count();
    assert_eq!(up, MAX_INSTANCES_PER_SCRIPT);
    assert!(!running(&rig, &ninth));
    assert_eq!(
        rig.messages(&ninth).last().unwrap(),
        "Failed to restart: Too many instances of cases (max 8)"
    );
}

#[test]
fn a_script_recipe_restarted_inside_the_loop_hears_the_events_it_listens_to() {
    let (mut rig, g) = loaded(CASES);
    let first = case(&mut rig, &g, "listen-motion");
    let mut on = false;
    toggle_motion(&mut rig, &mut on);
    toggle_motion(&mut rig, &mut on);
    let heard = |rig: &mut Rig| {
        rig.messages(&first)
            .iter()
            .filter(|m| m.starts_with("motion"))
            .count()
    };
    assert_eq!(heard(&mut rig), 2);
    // A create that loops fails the recipe through the API path (500), and the first instance with it.
    let (status, _) = create(
        &mut rig,
        "cases",
        json!({"zone": g.zone, "lights": [g.light], "case": "loop"}),
    );
    assert_eq!(status, 500);
    assert!(!running(&rig, &first));
    // A minute later the engine's own loop restarts it ...
    rig.advance(60_000);
    assert!(running(&rig, &first));
    // ... and the instance hears motion again: the interest was read again after the restart.
    for _ in 0..3 {
        toggle_motion(&mut rig, &mut on);
    }
    assert_eq!(heard(&mut rig), 2 + 3);
}

#[test]
fn the_bounds_of_a_script_are_exact_32_slots_and_16_kb() {
    // 32 slots are accepted, 33 refused.
    let slots = |n: usize| {
        (0..n)
            .map(|i| format!("n{i}: {{type: 'number'}}"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let mut rig = Rig::new();
    let with = |n: usize| {
        script(&format!(
            "{{api: 1, id: 'a', name: 'A', slots: {{{}}}, {OK_START}}}",
            slots(n)
        ))
    };
    load(&mut rig, &with(32)).unwrap();
    let mut rig = Rig::new();
    assert_eq!(
        load(&mut rig, &with(33)).unwrap_err(),
        "Script recipe invalid: more than 32 slots"
    );
    // 16,384 bytes of source are accepted, 16,385 refused.
    let base = script(&format!("{{api: 1, id: 'a', name: 'A', {OK_START}}}"));
    let sized = |bytes: usize| format!("{base}//{}", "x".repeat(bytes - base.len() - 2));
    let mut rig = Rig::new();
    let exact = sized(16 * 1024);
    assert_eq!(exact.len(), 16 * 1024);
    load(&mut rig, &exact).unwrap();
    let mut rig = Rig::new();
    assert_eq!(
        load(&mut rig, &sized(16 * 1024 + 1)).unwrap_err(),
        "Script recipe invalid: too large (16385 bytes, at most 16384)"
    );
}
