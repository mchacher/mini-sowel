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
