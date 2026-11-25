//! A recipe settles when what it uses changes (spec 024, R4, R5): a deleted equipment leaves the
//! settings, an instance that can no longer run is disabled with its reason, a running one whose
//! settings changed restarts, the instances of a deleted zone go with it, an instance that does
//! not validate is not enabled, and a removed device tells the equipments that lost bindings.

mod support;

use serde_json::{Value, json};
use solite_core::devices::Id;
use solite_core::home::Home;
use solite_core::live::{ClientId, LiveHandle};
use solite_core::radio::zcl::Command;
use solite_core::recipes::RecipeInstance;
use support::*;

const BASE: &str = "/api/v1/recipe-instances";

fn listening(rig: &Rig) -> (LiveHandle, ClientId) {
    let hub = rig.home.live();
    let id = hub.connect().expect("a free slot");
    let message = json!({"type": "subscribe", "topics": ["equipments", "recipes"]}).to_string();
    assert!(hub.subscribe(id, &message));
    (hub, id)
}

/// The types of the client's next batch, in order.
fn types(hub: &LiveHandle, id: ClientId) -> Vec<String> {
    hub.take_batch(id)
        .map(|text| {
            serde_json::from_str::<Value>(&text)
                .unwrap()
                .as_array()
                .unwrap()
                .iter()
                .map(|e| e["type"].as_str().unwrap().to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// What the host writes to `recipes.json`, if anything changed.
fn saved(home: &mut Home) -> Option<Value> {
    let mut cursor = home.take_recipes_cursor()?;
    let mut text = String::new();
    while let Some(chunk) = cursor.next_chunk(home).unwrap() {
        text.push_str(&chunk);
    }
    Some(serde_json::from_str(&text).unwrap())
}

fn listed(rig: &mut Rig, id: &str) -> Value {
    let (_, list) = rig.get(BASE);
    list.as_array()
        .unwrap()
        .iter()
        .find(|i| i["id"] == id)
        .cloned()
        .unwrap_or(Value::Null)
}

fn not_running(rig: &mut Rig, id: &str) -> bool {
    rig.refused("POST", &format!("{BASE}/{id}/actions"), r#"{"action":"x"}"#)
        == (400, "Instance not running".to_string())
}

/// A second light in the garage, on its own relay.
fn second_light(rig: &mut Rig, g: &Garage) -> String {
    let relay = rig.relay(0x7B);
    let light = rig.equipment("Second light", "light_onoff", &g.zone, &[&relay]);
    rig.measure(&state(0x7B, false));
    light
}

#[test]
fn the_only_light_deleted_disables_the_instance_with_its_reason() {
    let mut rig = Rig::new();
    let g = rig.garage();
    let id = rig.garage_instance(&g, json!({}));
    rig.sent();
    saved(&mut rig.home);
    let (hub, c) = listening(&rig);
    let (status, _) = rig.delete(&format!("/api/v1/equipments/{}", g.light));
    assert_eq!(status, 204);
    let instance = listed(&mut rig, &id);
    assert_eq!(instance["enabled"], false);
    assert_eq!(instance["params"]["lights"], json!([]));
    assert!(not_running(&mut rig, &id));
    let messages = rig.messages(&id);
    assert_eq!(
        messages[messages.len() - 2..],
        [
            "Equipment removed from settings: Garage light".to_string(),
            "Disabled: At least one light is required".to_string()
        ]
    );
    let (_, log) = rig.get(&format!("{BASE}/{id}/log?limit=2"));
    assert_eq!(
        (log[0]["level"].as_str(), log[1]["level"].as_str()),
        (Some("error"), Some("warn"))
    );
    let events = types(&hub, c);
    let recipes: Vec<&str> = events
        .iter()
        .map(String::as_str)
        .filter(|t| t.starts_with("recipe."))
        .collect();
    assert_eq!(
        recipes,
        ["recipe.instance.stopped", "recipe.instance.error"]
    );
    let written = saved(&mut rig.home).expect("recipes.json is written");
    assert_eq!(written[0]["enabled"], false);
    assert_eq!(written[0]["params"]["lights"], json!([]));
    // Nothing is sent afterwards: motion and time pass.
    rig.report(&motion(PIR, true));
    rig.advance(60_000);
    assert!(rig.sent().is_empty());
}

#[test]
fn one_of_two_lights_deleted_restarts_the_instance_with_the_other() {
    let mut rig = Rig::new();
    let g = rig.garage();
    let second = second_light(&mut rig, &g);
    let id = rig.garage_instance(&g, json!({"lights": [g.light, second]}));
    rig.sent();
    let (status, _) = rig.delete(&format!("/api/v1/equipments/{second}"));
    assert_eq!(status, 204);
    let instance = listed(&mut rig, &id);
    assert_eq!(instance["enabled"], true);
    assert_eq!(instance["params"]["lights"], json!([g.light]));
    assert!(!not_running(&mut rig, &id));
    let messages = rig.messages(&id);
    assert_eq!(
        messages[messages.len() - 2..],
        [
            "Equipment removed from settings: Second light".to_string(),
            "Recipe activated — no motion, lights off".to_string()
        ]
    );
    // The restart synced the remaining light; motion now switches only it, no error line.
    assert_eq!(rig.sent(), [Command::Off]);
    rig.report(&motion(PIR, true));
    assert_eq!(rig.sent(), [Command::On]);
    assert_eq!(
        rig.messages(&id).last().unwrap(),
        "Motion detected — 1 light(s) turned on"
    );
}

#[test]
fn a_disabled_instance_is_pruned_but_neither_validated_nor_told() {
    let mut rig = Rig::new();
    let g = rig.garage();
    let id = rig.garage_instance(&g, json!({}));
    rig.post(&format!("{BASE}/{id}/disable"), json!({}));
    saved(&mut rig.home);
    let (hub, c) = listening(&rig);
    rig.delete(&format!("/api/v1/equipments/{}", g.light));
    let instance = listed(&mut rig, &id);
    assert_eq!(instance["enabled"], false);
    assert_eq!(instance["params"]["lights"], json!([]));
    assert_eq!(
        rig.messages(&id).last().unwrap(),
        "Equipment removed from settings: Garage light"
    );
    assert!(!types(&hub, c).iter().any(|t| t.starts_with("recipe.")));
    assert!(
        saved(&mut rig.home).is_some(),
        "the pruned params are written"
    );
}

#[test]
fn a_deleted_button_leaves_the_buttons_and_the_instance_keeps_running() {
    let mut rig = Rig::new();
    let g = rig.garage();
    let sensor = rig.sensor(0x60);
    let data = rig.home.devices.get(&sensor).unwrap().data[0]
        .id
        .to_string();
    let button = rig.equipment("Button", "button", &g.zone, &[]);
    rig.post(
        &format!("/api/v1/equipments/{button}/data-bindings"),
        json!({"deviceDataId": data, "alias": "action"}),
    );
    let id = rig.garage_instance(&g, json!({"buttons": [button]}));
    rig.delete(&format!("/api/v1/equipments/{button}"));
    let instance = listed(&mut rig, &id);
    assert_eq!(instance["params"]["buttons"], json!([]));
    assert_eq!(instance["enabled"], true);
    assert!(!not_running(&mut rig, &id));
}

#[test]
fn an_equipment_no_instance_names_changes_nothing() {
    let mut rig = Rig::new();
    let g = rig.garage();
    let other = second_light(&mut rig, &g);
    let id = rig.garage_instance(&g, json!({}));
    let before = rig.messages(&id);
    saved(&mut rig.home);
    let (hub, c) = listening(&rig);
    rig.delete(&format!("/api/v1/equipments/{other}"));
    assert_eq!(rig.messages(&id), before);
    assert!(saved(&mut rig.home).is_none());
    assert!(!types(&hub, c).iter().any(|t| t.starts_with("recipe.")));
}

#[test]
fn the_legacy_light_naming_the_deleted_light_becomes_empty_and_the_instance_is_disabled() {
    let mut rig = Rig::new();
    let g = rig.garage();
    let (status, body) = rig.instance(json!({"zone": g.zone, "light": g.light}));
    assert_eq!(status, 201, "{body}");
    let id = body["id"].as_str().unwrap().to_string();
    rig.delete(&format!("/api/v1/equipments/{}", g.light));
    let instance = listed(&mut rig, &id);
    assert_eq!(instance["params"]["light"], "");
    assert_eq!(instance["enabled"], false);
    assert_eq!(
        rig.messages(&id).last().unwrap(),
        "Disabled: At least one light is required"
    );
}

#[test]
fn a_light_moved_to_another_zone_disables_the_instance() {
    let mut rig = Rig::new();
    let g = rig.garage();
    let other = rig.zone("Cellar");
    let id = rig.garage_instance(&g, json!({}));
    let (status, _) = rig.put(
        &format!("/api/v1/equipments/{}", g.light),
        json!({"zoneId": other}),
    );
    assert_eq!(status, 200);
    assert_eq!(listed(&mut rig, &id)["enabled"], false);
    assert_eq!(
        rig.messages(&id).last().unwrap(),
        "Disabled: Light \"Garage light\" does not belong to the selected zone"
    );
}

#[test]
fn a_light_renamed_leaves_the_instance_untouched() {
    let mut rig = Rig::new();
    let g = rig.garage();
    let id = rig.garage_instance(&g, json!({}));
    let before = rig.messages(&id);
    rig.sent();
    saved(&mut rig.home);
    rig.put(
        &format!("/api/v1/equipments/{}", g.light),
        json!({"name": "Workbench"}),
    );
    assert_eq!(rig.messages(&id), before);
    assert!(rig.sent().is_empty(), "no restart, no sync");
    assert!(saved(&mut rig.home).is_none());
    assert_eq!(listed(&mut rig, &id)["enabled"], true);
}

#[test]
fn the_lights_state_order_binding_removed_disables_the_instance() {
    let mut rig = Rig::new();
    let g = rig.garage();
    let id = rig.garage_instance(&g, json!({}));
    let (_, light) = rig.get(&format!("/api/v1/equipments/{}", g.light));
    let binding = light["orderBindings"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let (status, _) = rig.delete(&format!(
        "/api/v1/equipments/{}/order-bindings/{binding}",
        g.light
    ));
    assert_eq!(status, 204);
    assert_eq!(listed(&mut rig, &id)["enabled"], false);
    assert_eq!(
        rig.messages(&id).last().unwrap(),
        "Disabled: Light \"Garage light\" has no \"state\" order binding"
    );
}

#[test]
fn a_removed_relay_tells_its_equipment_then_disables_the_instance() {
    let mut rig = Rig::new();
    let g = rig.garage();
    let id = rig.garage_instance(&g, json!({}));
    let (hub, c) = listening(&rig);
    let (status, _) = rig.delete(&format!("/api/v1/devices/{}", g.relay_device));
    assert_eq!(status, 204);
    // The equipment's status follows (computed when it is read): not part of the order checked.
    let events: Vec<String> = types(&hub, c)
        .into_iter()
        .filter(|t| t != "equipment.status.changed")
        .collect();
    assert_eq!(
        events,
        [
            "equipment.updated",
            "recipe.instance.stopped",
            "recipe.instance.error"
        ]
    );
    assert_eq!(listed(&mut rig, &id)["enabled"], false);
}

#[test]
fn a_device_no_equipment_binds_is_removed_without_equipment_events() {
    let mut rig = Rig::new();
    let g = rig.garage();
    let lone = rig.sensor(0x61);
    let id = rig.garage_instance(&g, json!({}));
    let before = rig.messages(&id);
    let (hub, c) = listening(&rig);
    rig.delete(&format!("/api/v1/devices/{lone}"));
    assert!(types(&hub, c).is_empty());
    assert_eq!(rig.messages(&id), before);
}

#[test]
fn the_instances_of_a_deleted_zone_are_deleted_with_their_log() {
    let mut rig = Rig::new();
    let g = rig.garage();
    let id = rig.garage_instance(&g, json!({}));
    // The zone empties: its light goes (the instance is disabled), then its sensor.
    rig.delete(&format!("/api/v1/equipments/{}", g.light));
    rig.delete(&format!("/api/v1/equipments/{}", g.motion_equipment));
    saved(&mut rig.home);
    let (hub, c) = listening(&rig);
    let (status, body) = rig.delete(&format!("/api/v1/zones/{}", g.zone));
    assert_eq!(status, 204, "{body}");
    assert_eq!(listed(&mut rig, &id), Value::Null);
    let (_, log) = rig.get(&format!("{BASE}/{id}/log"));
    assert_eq!(log, json!([]));
    assert!(types(&hub, c).contains(&"recipe.instance.removed".to_string()));
    assert_eq!(saved(&mut rig.home), Some(json!([])));
}

#[test]
fn an_instance_that_failed_to_restore_is_disabled_at_the_next_change_and_the_request_still_answers()
{
    let mut rig = Rig::new();
    let g = rig.garage();
    let other = second_light(&mut rig, &g);
    let broken = RecipeInstance {
        id: Id::from_random([7; 16]),
        recipe_id: "motion-light".into(),
        params: "[]".into(),
        enabled: true,
        created_at: rig.now,
    };
    rig.home.load_recipes(vec![broken]);
    rig.home.start_recipes(rig.now, rig.now);
    let id = Id::from_random([7; 16]).to_string();
    assert!(not_running(&mut rig, &id));
    assert_eq!(listed(&mut rig, &id)["enabled"], true);
    let (status, _) = rig.delete(&format!("/api/v1/equipments/{other}"));
    assert_eq!(status, 204);
    assert_eq!(listed(&mut rig, &id)["enabled"], false);
    assert_eq!(
        rig.messages(&id).last().unwrap(),
        "Disabled: Zone parameter is required"
    );
}

#[test]
fn enable_refuses_an_instance_that_no_longer_validates() {
    let mut rig = Rig::new();
    let g = rig.garage();
    let id = rig.garage_instance(&g, json!({}));
    rig.delete(&format!("/api/v1/equipments/{}", g.light));
    saved(&mut rig.home);
    let before = rig.messages(&id);
    assert_eq!(
        rig.refused("POST", &format!("{BASE}/{id}/enable"), "{}"),
        (
            400,
            "Invalid params: At least one light is required".to_string()
        )
    );
    assert_eq!(listed(&mut rig, &id)["enabled"], false);
    assert_eq!(rig.messages(&id), before);
    assert!(saved(&mut rig.home).is_none());
}

#[test]
fn enable_still_starts_a_disabled_instance_that_validates() {
    let mut rig = Rig::new();
    let g = rig.garage();
    let id = rig.garage_instance(&g, json!({}));
    rig.post(&format!("{BASE}/{id}/disable"), json!({}));
    let (status, body) = rig.post(&format!("{BASE}/{id}/enable"), json!({}));
    assert_eq!((status, body), (200, json!({"success": true})));
    assert!(!not_running(&mut rig, &id));
}

#[test]
fn a_light_named_by_sixteen_instances_of_sixteen_lights_is_pruned_from_all_in_one_change() {
    let mut rig = Rig::new();
    let g = rig.garage();
    let mut lights = vec![g.light.clone()];
    for n in 0..15u8 {
        let relay = rig.relay(0x90 + n);
        lights.push(rig.equipment(&format!("Light {n}"), "light_onoff", &g.zone, &[&relay]));
    }
    let ids: Vec<String> = (0..16)
        .map(|_| rig.garage_instance(&g, json!({"lights": lights})))
        .collect();
    let (status, _) = rig.delete(&format!("/api/v1/equipments/{}", lights[15]));
    assert_eq!(status, 204);
    for id in &ids {
        let instance = listed(&mut rig, id);
        assert_eq!(instance["params"]["lights"].as_array().unwrap().len(), 15);
        assert_eq!(instance["enabled"], true);
    }
}

// ---- Spec 028: buttons pruned, sources followed ----

const SECOND: u64 = 1000;

/// A `button` equipment of the zone with an `action` binding on the first data of a new SNZB-02
/// (EUI ending `last`); (equipment id, device id, binding id).
fn action_button(rig: &mut Rig, zone: &str, name: &str, last: u8) -> (String, String, String) {
    let sensor = rig.sensor(last);
    let data = rig.home.devices.get(&sensor).unwrap().data[0]
        .id
        .to_string();
    let button = rig.equipment(name, "button", zone, &[]);
    let (status, body) = rig.post(
        &format!("/api/v1/equipments/{button}/data-bindings"),
        json!({"deviceDataId": data, "alias": "action"}),
    );
    assert_eq!(status, 201, "{body}");
    let (_, details) = rig.get(&format!("/api/v1/equipments/{button}"));
    let binding = details["dataBindings"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    (button, sensor, binding)
}

fn unbind(rig: &mut Rig, equipment: &str, binding: &str) {
    let (status, _) = rig.delete(&format!(
        "/api/v1/equipments/{equipment}/data-bindings/{binding}"
    ));
    assert_eq!(status, 204);
}

fn starts(rig: &mut Rig, id: &str) -> usize {
    rig.messages(id)
        .iter()
        .filter(|m| m.starts_with("Recipe activated"))
        .count()
}

fn count(messages: &[String], text: &str) -> usize {
    messages.iter().filter(|m| *m == text).count()
}

#[test]
fn a_button_whose_device_is_removed_leaves_the_settings_and_the_instance_keeps_running() {
    let mut rig = Rig::new();
    let g = rig.garage();
    let (button, sensor, _) = action_button(&mut rig, &g.zone, "Wall button", 0x60);
    let id = rig.garage_instance(&g, json!({"buttons": [button]}));
    rig.sent();
    saved(&mut rig.home);
    let (hub, c) = listening(&rig);
    let (status, _) = rig.delete(&format!("/api/v1/devices/{sensor}"));
    assert_eq!(status, 204);
    let instance = listed(&mut rig, &id);
    assert_eq!(instance["enabled"], true);
    assert_eq!(instance["params"]["buttons"], json!([]));
    assert!(!not_running(&mut rig, &id));
    assert_eq!(starts(&mut rig, &id), 1, "not restarted");
    assert_eq!(
        rig.messages(&id).last().unwrap(),
        "Button removed from settings: Wall button"
    );
    assert!(!types(&hub, c).contains(&"recipe.instance.error".to_string()));
    let written = saved(&mut rig.home).expect("recipes.json is written");
    assert_eq!(written[0]["params"]["buttons"], json!([]));
    assert_eq!(written[0]["enabled"], true);
}

#[test]
fn a_button_whose_action_binding_is_removed_leaves_the_settings_and_the_instance_keeps_running() {
    let mut rig = Rig::new();
    let g = rig.garage();
    let (button, _, binding) = action_button(&mut rig, &g.zone, "Wall button", 0x60);
    let id = rig.garage_instance(&g, json!({"buttons": [button]}));
    rig.sent();
    saved(&mut rig.home);
    unbind(&mut rig, &button, &binding);
    let instance = listed(&mut rig, &id);
    assert_eq!(instance["enabled"], true);
    assert_eq!(instance["params"]["buttons"], json!([]));
    assert!(!not_running(&mut rig, &id));
    assert_eq!(starts(&mut rig, &id), 1);
    assert_eq!(
        rig.messages(&id).last().unwrap(),
        "Button removed from settings: Wall button"
    );
    let written = saved(&mut rig.home).expect("recipes.json is written");
    assert_eq!(written[0]["params"]["buttons"], json!([]));
    // The light still follows motion.
    rig.report(&motion(PIR, true));
    assert_eq!(rig.sent(), [Command::On]);
}

#[test]
fn a_button_pruned_while_the_light_is_on_keeps_its_timer_and_sends_nothing() {
    let mut rig = Rig::new();
    let g = rig.garage();
    let (button, _, binding) = action_button(&mut rig, &g.zone, "Wall button", 0x60);
    let id = rig.garage_instance(&g, json!({"buttons": [button]}));
    rig.sent();
    rig.report(&motion(PIR, true));
    rig.report(&state(RELAY, true));
    rig.report(&motion(PIR, false));
    rig.sent();
    let before = rig.state(&id);
    assert!(before["timerExpiresAt"].is_string());
    unbind(&mut rig, &button, &binding);
    assert_eq!(rig.state(&id), before);
    assert!(rig.sent().is_empty());
    assert_eq!(starts(&mut rig, &id), 1);
    // The timer still fires.
    rig.advance(20 * SECOND);
    assert_eq!(rig.sent(), [Command::Off]);
}

#[test]
fn a_disabled_instance_has_its_stale_button_pruned_logged_and_written_and_stays_disabled() {
    let mut rig = Rig::new();
    let g = rig.garage();
    let (button, _, binding) = action_button(&mut rig, &g.zone, "Wall button", 0x60);
    let id = rig.garage_instance(&g, json!({"buttons": [button]}));
    rig.post(&format!("{BASE}/{id}/disable"), json!({}));
    saved(&mut rig.home);
    unbind(&mut rig, &button, &binding);
    let instance = listed(&mut rig, &id);
    assert_eq!(instance["enabled"], false);
    assert_eq!(instance["params"]["buttons"], json!([]));
    assert_eq!(
        rig.messages(&id).last().unwrap(),
        "Button removed from settings: Wall button"
    );
    assert!(saved(&mut rig.home).is_some());
}

#[test]
fn a_deleted_button_equipment_is_logged_once_by_the_deletion_and_does_not_restart() {
    let mut rig = Rig::new();
    let g = rig.garage();
    let (button, _, _) = action_button(&mut rig, &g.zone, "Wall button", 0x60);
    let id = rig.garage_instance(&g, json!({"buttons": [button]}));
    rig.delete(&format!("/api/v1/equipments/{button}"));
    let messages = rig.messages(&id);
    assert_eq!(
        count(&messages, "Equipment removed from settings: Wall button"),
        1
    );
    assert!(!messages.iter().any(|m| m.starts_with("Button removed")));
    assert_eq!(starts(&mut rig, &id), 1, "not restarted");
    assert_eq!(listed(&mut rig, &id)["params"]["buttons"], json!([]));
    assert!(!not_running(&mut rig, &id));
}

#[test]
fn a_button_in_two_instances_is_pruned_from_both_with_one_line_each() {
    let mut rig = Rig::new();
    let g = rig.garage();
    let (button, _, binding) = action_button(&mut rig, &g.zone, "Wall button", 0x60);
    let first = rig.garage_instance(&g, json!({"buttons": [button]}));
    let second = rig.garage_instance(&g, json!({"buttons": [button]}));
    unbind(&mut rig, &button, &binding);
    for id in [&first, &second] {
        assert_eq!(listed(&mut rig, id)["params"]["buttons"], json!([]));
        let messages = rig.messages(id);
        assert_eq!(
            count(&messages, "Button removed from settings: Wall button"),
            1
        );
        assert!(!not_running(&mut rig, id));
    }
}

#[test]
fn the_same_button_twice_in_the_stored_buttons_is_taken_out_with_one_line() {
    let mut rig = Rig::new();
    let g = rig.garage();
    let (button, _, binding) = action_button(&mut rig, &g.zone, "Wall button", 0x60);
    let params = json!({"zone": g.zone, "lights": [g.light], "timeout": "20s",
        "buttons": [button, button]});
    let stored = RecipeInstance {
        id: Id::from_random([9; 16]),
        recipe_id: "motion-light".into(),
        params: params.to_string().into(),
        enabled: true,
        created_at: rig.now,
    };
    rig.home.load_recipes(vec![stored]);
    rig.home.start_recipes(rig.now, rig.now);
    let id = Id::from_random([9; 16]).to_string();
    assert!(!not_running(&mut rig, &id));
    unbind(&mut rig, &button, &binding);
    assert_eq!(listed(&mut rig, &id)["params"]["buttons"], json!([]));
    let messages = rig.messages(&id);
    assert_eq!(
        count(&messages, "Button removed from settings: Wall button"),
        1
    );
}

#[test]
fn creating_or_changing_an_instance_with_a_button_without_action_is_still_refused() {
    let mut rig = Rig::new();
    let g = rig.garage();
    let (button, _, binding) = action_button(&mut rig, &g.zone, "Wall button", 0x60);
    let id = rig.garage_instance(&g, json!({"buttons": [button]}));
    unbind(&mut rig, &button, &binding);
    let params = json!({"zone": g.zone, "lights": [g.light], "buttons": [button]});
    let body = json!({"recipeId": "motion-light", "params": params}).to_string();
    assert_eq!(
        rig.refused("POST", BASE, &body),
        (
            400,
            "Invalid params: Button \"Wall button\" has no \"action\" data binding".to_string()
        )
    );
    let body = json!({"params": params}).to_string();
    assert_eq!(rig.refused("PUT", &format!("{BASE}/{id}"), &body).0, 400);
}

#[test]
fn a_press_on_a_pruned_button_does_nothing_even_when_its_binding_comes_back() {
    let mut rig = Rig::new();
    let g = rig.garage();
    let (button, sensor, binding) = action_button(&mut rig, &g.zone, "Wall button", 0x60);
    let id = rig.garage_instance(&g, json!({"buttons": [button]}));
    rig.sent();
    unbind(&mut rig, &button, &binding);
    let data = rig.home.devices.get(&sensor).unwrap().data[0]
        .id
        .to_string();
    let (status, _) = rig.post(
        &format!("/api/v1/equipments/{button}/data-bindings"),
        json!({"deviceDataId": data, "alias": "action"}),
    );
    assert_eq!(status, 201);
    let before = rig.messages(&id);
    rig.report(&temperature(0x60, 20.0));
    assert!(rig.sent().is_empty());
    assert_eq!(rig.messages(&id), before);
    assert_eq!(listed(&mut rig, &id)["params"]["buttons"], json!([]));
}

/// An instance of the garage with a safety auto-off, its light on and its zone's motion on;
/// the sensor's raw reports are impulses that reset the auto-off. Returns the instance's id.
fn lit_garage(rig: &mut Rig, g: &Garage) -> String {
    let id = rig.garage_instance(g, json!({"maxOnDuration": "5m"}));
    rig.sent();
    rig.report(&motion(PIR, true));
    rig.report(&state(RELAY, true));
    rig.sent();
    assert!(rig.state(&id)["failsafeExpiresAt"].is_string());
    id
}

fn failsafe(rig: &mut Rig, id: &str) -> Value {
    rig.state(id)["failsafeExpiresAt"].clone()
}

#[test]
fn a_motion_sensor_unbound_under_a_running_instance_no_longer_resets_the_safety_auto_off() {
    let mut rig = Rig::new();
    let g = rig.garage();
    let id = lit_garage(&mut rig, &g);
    let (_, equipment) = rig.get(&format!("/api/v1/equipments/{}", g.motion_equipment));
    let bindings = equipment["dataBindings"].as_array().unwrap().clone();
    for binding in &bindings {
        unbind(
            &mut rig,
            &g.motion_equipment,
            binding["id"].as_str().unwrap(),
        );
    }
    rig.advance(SECOND);
    let before = failsafe(&mut rig, &id);
    rig.advance(5 * SECOND);
    // A raw impulse of the sensor, no longer a source of the zone.
    rig.report(&motion(PIR, true));
    assert_eq!(failsafe(&mut rig, &id), before);
    assert_eq!(starts(&mut rig, &id), 1, "not restarted");
}

#[test]
fn a_motion_sensor_added_to_the_zone_resets_the_safety_auto_off_without_a_restart() {
    let mut rig = Rig::new();
    let g = rig.garage();
    let id = lit_garage(&mut rig, &g);
    let second = rig.motion_sensor(0x62);
    rig.equipment("Second motion", "sensor", &g.zone, &[&second]);
    rig.advance(SECOND);
    let before = failsafe(&mut rig, &id);
    rig.advance(5 * SECOND);
    rig.report(&motion(0x62, true));
    let after = failsafe(&mut rig, &id);
    assert_ne!(after, before, "the new source resets the auto-off");
    assert_eq!(starts(&mut rig, &id), 1, "not restarted");
}

#[test]
fn a_motion_data_binding_added_to_an_equipment_of_the_zone_makes_it_a_source_without_a_restart() {
    let mut rig = Rig::new();
    let g = rig.garage();
    let id = lit_garage(&mut rig, &g);
    let second = rig.motion_sensor(0x62);
    let empty = rig.equipment("Second motion", "sensor", &g.zone, &[]);
    let data = rig
        .home
        .devices
        .get(&second)
        .unwrap()
        .data
        .iter()
        .find(|d| &*d.category == "motion")
        .unwrap()
        .id
        .to_string();
    rig.advance(SECOND);
    let before = failsafe(&mut rig, &id);
    rig.advance(5 * SECOND);
    rig.report(&motion(0x62, true));
    assert_eq!(failsafe(&mut rig, &id), before, "not bound yet");
    let (status, body) = rig.post(
        &format!("/api/v1/equipments/{empty}/data-bindings"),
        json!({"deviceDataId": data, "alias": "occupancy"}),
    );
    assert_eq!(status, 201, "{body}");
    rig.advance(SECOND);
    let before = failsafe(&mut rig, &id);
    rig.advance(5 * SECOND);
    rig.report(&motion(0x62, true));
    assert_ne!(failsafe(&mut rig, &id), before, "bound: it counts");
    assert_eq!(starts(&mut rig, &id), 1);
}

#[test]
fn a_sensor_equipment_moved_out_of_the_zone_or_disabled_stops_counting() {
    let mut rig = Rig::new();
    let g = rig.garage();
    let id = lit_garage(&mut rig, &g);
    let other = rig.zone("Cellar");
    // Moved out.
    let (status, _) = rig.put(
        &format!("/api/v1/equipments/{}", g.motion_equipment),
        json!({"zoneId": other}),
    );
    assert_eq!(status, 200);
    rig.advance(SECOND);
    let before = failsafe(&mut rig, &id);
    rig.advance(5 * SECOND);
    rig.report(&motion(PIR, true));
    assert_eq!(failsafe(&mut rig, &id), before, "moved out");
    // Moved back in: it counts again.
    rig.put(
        &format!("/api/v1/equipments/{}", g.motion_equipment),
        json!({"zoneId": g.zone}),
    );
    rig.advance(SECOND);
    let before = failsafe(&mut rig, &id);
    rig.advance(5 * SECOND);
    rig.report(&motion(PIR, true));
    let back = failsafe(&mut rig, &id);
    assert_ne!(back, before, "moved back");
    // Disabled.
    rig.put(
        &format!("/api/v1/equipments/{}", g.motion_equipment),
        json!({"enabled": false}),
    );
    rig.advance(SECOND);
    let back = failsafe(&mut rig, &id);
    rig.advance(5 * SECOND);
    rig.report(&motion(PIR, true));
    assert_eq!(failsafe(&mut rig, &id), back, "disabled");
    assert_eq!(starts(&mut rig, &id), 1);
}

#[test]
fn a_child_zone_moved_out_of_the_instance_zone_stops_counting_and_moved_back_counts_again() {
    let mut rig = Rig::new();
    let g = rig.garage();
    let shelf = rig.zone("Shelf");
    let (status, body) = rig.put(
        &format!("/api/v1/zones/{shelf}"),
        json!({"parentId": g.zone}),
    );
    assert_eq!(status, 200, "{body}");
    let second = rig.motion_sensor(0x62);
    rig.equipment("Shelf motion", "sensor", &shelf, &[&second]);
    let id = lit_garage(&mut rig, &g);
    // Moved out of the garage (under the root).
    let (status, body) = rig.put(&format!("/api/v1/zones/{shelf}"), json!({"parentId": ROOT}));
    assert_eq!(status, 200, "{body}");
    // The zone's own aggregate may change with the move (the recipe's own rule); read after it.
    rig.advance(SECOND);
    let before = failsafe(&mut rig, &id);
    rig.advance(5 * SECOND);
    rig.report(&motion(0x62, true));
    assert_eq!(failsafe(&mut rig, &id), before, "moved out");
    // Moved back in.
    rig.put(
        &format!("/api/v1/zones/{shelf}"),
        json!({"parentId": g.zone}),
    );
    rig.advance(SECOND);
    let before = failsafe(&mut rig, &id);
    rig.advance(5 * SECOND);
    rig.report(&motion(0x62, true));
    assert_ne!(failsafe(&mut rig, &id), before, "moved back in");
    assert_eq!(starts(&mut rig, &id), 1);
}

#[test]
fn the_last_motion_sensor_of_the_zone_unbound_leaves_the_instance_running() {
    let mut rig = Rig::new();
    let g = rig.garage();
    let id = rig.garage_instance(&g, json!({"maxOnDuration": "5m"}));
    let (_, equipment) = rig.get(&format!("/api/v1/equipments/{}", g.motion_equipment));
    for binding in equipment["dataBindings"].as_array().unwrap().clone() {
        unbind(
            &mut rig,
            &g.motion_equipment,
            binding["id"].as_str().unwrap(),
        );
    }
    assert_eq!(listed(&mut rig, &id)["enabled"], true);
    assert!(!not_running(&mut rig, &id));
    rig.advance(30 * SECOND);
    assert_eq!(starts(&mut rig, &id), 1);
}

#[test]
fn a_sensor_unbound_keeps_the_state_and_the_timers_and_sends_nothing() {
    let mut rig = Rig::new();
    let g = rig.garage();
    let id = rig.garage_instance(&g, json!({}));
    rig.sent();
    rig.report(&motion(PIR, true));
    rig.report(&state(RELAY, true));
    rig.report(&motion(PIR, false));
    rig.sent();
    let before = rig.state(&id);
    assert!(before["timerExpiresAt"].is_string());
    let (_, equipment) = rig.get(&format!("/api/v1/equipments/{}", g.motion_equipment));
    // Only the luminosity or the occupancy binding changes the sources; remove them all.
    for binding in equipment["dataBindings"].as_array().unwrap().clone() {
        unbind(
            &mut rig,
            &g.motion_equipment,
            binding["id"].as_str().unwrap(),
        );
    }
    assert_eq!(rig.state(&id)["timerExpiresAt"], before["timerExpiresAt"]);
    assert!(rig.sent().is_empty());
    assert_eq!(starts(&mut rig, &id), 1);
}
