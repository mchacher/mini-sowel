//! Recipes (spec 012, R2, R5, R7, R8): Sowel's routes with their statuses and bodies, the bounds
//! (instances, log, params, lights), what survives a restart, and the bodies and queries that are
//! refused.

mod support;

use serde_json::{Value, json};
use solite_core::api::{Reach, Reply, Request, handle_stream};
use solite_core::devices::Id;
use solite_core::home::Home;
use solite_core::radio::zcl::Command;
use solite_core::recipes::RecipeInstance;
use support::*;

const GHOST: &str = "11111111-2222-4333-8444-555555555555";

fn base() -> String {
    "/api/v1/recipe-instances".to_string()
}

#[test]
fn an_instance_is_created_listed_read_changed_disabled_enabled_and_deleted_as_sowel_does() {
    let mut rig = Rig::new();
    let g = rig.garage();
    // Create: 201, the instance without a state, enabled.
    let (status, created) = rig.instance(json!({
        "zone": g.zone, "lights": [g.light], "timeout": "20s"
    }));
    assert_eq!(status, 201, "{created}");
    let id = created["id"].as_str().unwrap().to_string();
    assert_eq!(created["recipeId"], "motion-light");
    assert_eq!(created["enabled"], true);
    assert_eq!(created["params"]["timeout"], "20s");
    assert!(created["createdAt"].as_str().unwrap().ends_with('Z'));
    assert!(created.get("state").is_none());
    assert_eq!(
        created.as_object().unwrap().len(),
        5,
        "id, recipeId, params, enabled, createdAt"
    );
    // List: each with its state, oldest first.
    let (_, second) = rig.instance(json!({"zone": g.zone, "lights": [g.light]}));
    let (status, list) = rig.get(&base());
    assert_eq!(status, 200);
    let list = list.as_array().unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0]["id"], id.as_str());
    assert_eq!(list[1]["id"], second["id"]);
    assert_eq!(list[0]["state"], json!({}));
    assert_eq!(list[0]["recipeId"], "motion-light");
    // Update: the new params, 200.
    let (status, updated) = rig.put(
        &format!("{}/{id}", base()),
        json!({"params": {"zone": g.zone, "lights": [g.light], "timeout": "1h"}}),
    );
    assert_eq!(status, 200, "{updated}");
    assert_eq!(updated["params"]["timeout"], "1h");
    assert_eq!(updated["id"], id.as_str());
    assert_eq!(updated["createdAt"], created["createdAt"]);
    // Actions: accepted and ignored while running.
    let (status, body) = rig.post(
        &format!("{}/{id}/actions", base()),
        json!({"action": "anything", "payload": {"a": 1}}),
    );
    assert_eq!((status, body), (200, json!({"ok": true})));
    // Disable, then actions are refused; enable again.
    let (status, body) = rig.post(&format!("{}/{id}/disable", base()), json!({}));
    assert_eq!((status, body), (200, json!({"success": true})));
    let (_, list) = rig.get(&base());
    assert_eq!(list[0]["enabled"], false);
    assert_eq!(
        rig.refused(
            "POST",
            &format!("{}/{id}/actions", base()),
            r#"{"action":"x"}"#
        ),
        (400, "Instance not running".to_string())
    );
    let (status, body) = rig.post(&format!("{}/{id}/enable", base()), json!({}));
    assert_eq!((status, body), (200, json!({"success": true})));
    let (_, list) = rig.get(&base());
    assert_eq!(list[0]["enabled"], true);
    // Enabling an instance that is enabled is a no-op that still answers 200.
    let (status, body) = rig.post(&format!("{}/{id}/enable", base()), json!({}));
    assert_eq!((status, body), (200, json!({"success": true})));
    // Delete: 204 and its log is gone.
    let (status, body) = rig.delete(&format!("{}/{id}", base()));
    assert_eq!((status, body), (204, Value::Null));
    let (_, list) = rig.get(&base());
    assert_eq!(list.as_array().unwrap().len(), 1);
    let (status, log) = rig.get(&format!("{}/{id}/log", base()));
    assert_eq!((status, log), (200, json!([])));
}

#[test]
fn the_log_lists_newest_first_with_sowels_fields_and_integer_ids_counting_from_one() {
    let mut rig = Rig::new();
    let g = rig.garage();
    let id = rig.garage_instance(&g, json!({}));
    let (status, log) = rig.get(&format!("{}/{id}/log", base()));
    assert_eq!(status, 200);
    let log = log.as_array().unwrap();
    assert_eq!(log.len(), 2);
    assert_eq!(
        log[0]["message"],
        "Recipe activated — no motion, lights off"
    );
    assert_eq!(log[1]["message"], "Instance created");
    assert_eq!(
        (log[1]["id"].as_u64(), log[0]["id"].as_u64()),
        (Some(1), Some(2))
    );
    assert_eq!(log[0]["instanceId"], id.as_str());
    assert_eq!(log[0]["level"], "info");
    assert_eq!(log[0]["timestamp"], "2026-11-21T09:12:03.120Z");
    assert_eq!(log[0].as_object().unwrap().len(), 5);
}

#[test]
fn recipes_are_listed_and_read_and_unknown_ones_refused_on_every_route() {
    let mut rig = Rig::new();
    // No recipe is built in: nothing is listed until a script is loaded (spec 046, R1).
    assert_eq!(rig.get("/api/v1/recipes"), (200, json!([])));
    assert_eq!(
        rig.refused("GET", "/api/v1/recipes/motion-light", ""),
        (404, "Recipe not found".to_string())
    );
    rig.load_motion_light();
    let (status, list) = rig.get("/api/v1/recipes");
    assert_eq!(status, 200);
    assert_eq!(list.as_array().unwrap().len(), 1);
    let (status, one) = rig.get("/api/v1/recipes/motion-light");
    assert_eq!(status, 200);
    assert_eq!(one, list[0]);
    assert_eq!(
        rig.refused("GET", "/api/v1/recipes/nope", ""),
        (404, "Recipe not found".to_string())
    );
    assert_eq!(
        rig.refused("POST", &base(), r#"{"recipeId":"nope","params":{}}"#),
        (404, "Recipe not found: nope".to_string())
    );
    // An unknown instance, route by route.
    let unknown = (404, "Instance not found".to_string());
    assert_eq!(
        rig.refused("PUT", &format!("{}/{GHOST}", base()), r#"{"params":{}}"#),
        unknown
    );
    assert_eq!(
        rig.refused("DELETE", &format!("{}/{GHOST}", base()), ""),
        unknown
    );
    assert_eq!(
        rig.refused("POST", &format!("{}/{GHOST}/enable", base()), "{}"),
        unknown
    );
    assert_eq!(
        rig.refused("POST", &format!("{}/{GHOST}/disable", base()), "{}"),
        unknown
    );
    // Actions on an id that does not exist: as for one that does not run (Sowel checks only the
    // running ones).
    assert_eq!(
        rig.refused(
            "POST",
            &format!("{}/{GHOST}/actions", base()),
            r#"{"action":"x"}"#
        ),
        (400, "Instance not running".to_string())
    );
    assert_eq!(
        rig.get(&format!("{}/{GHOST}/log", base())),
        (200, json!([]))
    );
    assert_eq!(
        rig.get(&format!("{}/not-an-id/log", base())),
        (200, json!([]))
    );
    // Methods that are not there.
    assert_eq!(rig.refused("DELETE", "/api/v1/recipes", "").0, 405);
    assert_eq!(rig.refused("PUT", &base(), "{}").0, 405);
}

#[test]
fn bodies_that_are_not_what_the_route_takes_are_a_400_invalid_body() {
    let mut rig = Rig::new();
    let g = rig.garage();
    let id = rig.garage_instance(&g, json!({}));
    let invalid = (400, "Invalid body".to_string());
    for body in [
        "[]",
        "not json",
        r#"{"params":{}}"#,
        r#"{"recipeId":"","params":{}}"#,
        r#"{"recipeId":3,"params":{}}"#,
        r#"{"recipeId":"motion-light"}"#,
        r#"{"recipeId":"motion-light","params":[]}"#,
        r#"{"recipeId":"motion-light","params":"x"}"#,
    ] {
        assert_eq!(rig.refused("POST", &base(), body), invalid, "{body}");
    }
    let put = format!("{}/{id}", base());
    for body in [r#"{}"#, r#"{"params":3}"#, r#"{"params":null}"#, "[1]"] {
        assert_eq!(rig.refused("PUT", &put, body), invalid, "{body}");
    }
    let actions = format!("{}/{id}/actions", base());
    for body in [
        r#"{}"#,
        r#"{"action":""}"#,
        r#"{"action":3}"#,
        r#"{"action":"x","payload":"text"}"#,
        r#"{"action":"x","payload":null}"#,
        r#"{"action":"x","payload":[1]}"#,
    ] {
        assert_eq!(rig.refused("POST", &actions, body), invalid, "{body}");
    }
    // The body is checked before the instance: an unknown id with a bad body is the body's error.
    assert_eq!(
        rig.refused("PUT", &format!("{}/{GHOST}", base()), "{}"),
        invalid
    );
    // The instance was not touched.
    let (_, log) = rig.get(&format!("{}/{id}/log", base()));
    assert_eq!(log.as_array().unwrap().len(), 2);
}

#[test]
fn the_seventeenth_instance_and_params_over_2_kb_are_refused() {
    // 16 instances in all: 13 stored ones of a recipe nobody provides (kept stopped) and 3 of the
    // script, which holds 4 at most (D85): the fourth hits the bound of all recipes together.
    let mut rig = Rig::new();
    let g = rig.garage();
    let stored: Vec<RecipeInstance> = (1..=13u8)
        .map(|n| instance(n, "nope", json!({"zone": g.zone})))
        .collect();
    rig.home.load_recipes(stored);
    rig.home.start_recipes(rig.now, rig.now);
    for _ in 0..3 {
        rig.garage_instance(&g, json!({}));
    }
    let (status, body) = rig.instance(json!({"zone": g.zone, "lights": [g.light]}));
    assert_eq!(
        (status, body),
        (400, json!({"error": "Too many recipe instances (max 16)"}))
    );
    // Deleting one makes room.
    let (_, list) = rig.get(&base());
    let first = list[0]["id"].as_str().unwrap().to_string();
    rig.delete(&format!("{}/{first}", base()));
    let (status, _) = rig.instance(json!({"zone": g.zone, "lights": [g.light]}));
    assert_eq!(status, 201);

    let mut rig = Rig::new();
    let g = rig.garage();
    let big = "x".repeat(2100);
    assert_eq!(
        refused(
            &mut rig,
            json!({"zone": g.zone, "lights": [g.light], "note": big})
        ),
        (400, "Invalid params: params too large".to_string())
    );
    // Just under the limit is accepted.
    let fits = "x".repeat(1800);
    let (status, _) = rig.instance(json!({"zone": g.zone, "lights": [g.light], "note": fits}));
    assert_eq!(status, 201);
    // The same on an update.
    let id = rig.garage_instance(&g, json!({}));
    assert_eq!(
        rig.refused(
            "PUT",
            &format!("{}/{id}", base()),
            &json!({"params": {"zone": g.zone, "lights": [g.light], "note": big}}).to_string()
        ),
        (400, "Invalid params: params too large".to_string())
    );
}

/// Params of about 2 KB whose `deep` key nests lists so that the params are `levels` levels deep
/// (the params object is the first level): the settings, a filler text, and the nesting.
fn nested_params(g: &Garage, levels: usize) -> Value {
    let mut deep = json!([]);
    for _ in 0..levels.saturating_sub(2) {
        deep = json!([deep]);
    }
    let mut params = json!({"zone": g.zone, "lights": [g.light], "deep": deep});
    let used = params.to_string().len();
    params["note"] = json!("x".repeat(1990 - used));
    assert!(params.to_string().len() > 1900 && params.to_string().len() <= 2048);
    params
}

#[test]
fn params_nested_8_levels_deep_are_accepted_and_9_or_100_are_refused_before_validation() {
    let mut rig = Rig::new();
    let g = rig.garage();
    let too_deep = (400, "Invalid params: params nested too deeply".to_string());
    // 8 levels, 2 KB: accepted (created, then changed).
    let (status, body) = rig.instance(nested_params(&g, 8));
    assert_eq!(status, 201, "{body}");
    let id = body["id"].as_str().unwrap().to_string();
    let (status, _) = rig.put(
        &format!("{}/{id}", base()),
        json!({"params": nested_params(&g, 8)}),
    );
    assert_eq!(status, 200);
    // 9 and 100 levels, 2 KB: refused, on a create and on an update, the instance untouched.
    for levels in [9, 100] {
        assert_eq!(
            refused(&mut rig, nested_params(&g, levels)),
            too_deep,
            "{levels}"
        );
        assert_eq!(
            rig.refused(
                "PUT",
                &format!("{}/{id}", base()),
                &json!({"params": nested_params(&g, levels)}).to_string()
            ),
            too_deep,
            "{levels}"
        );
    }
    let (_, list) = rig.get(&base());
    assert_eq!(list.as_array().unwrap().len(), 1);
    // The depth comes before the recipe's own checks: params with no zone at all say it too.
    assert_eq!(
        refused(&mut rig, json!({"deep": nested_params(&g, 10)})),
        too_deep
    );
}

fn refused(rig: &mut Rig, params: Value) -> (u16, String) {
    let (status, body) = rig.instance(params);
    (status, body["error"].as_str().unwrap_or("").to_string())
}

#[test]
fn seventeen_lights_or_seventeen_buttons_are_refused() {
    let mut rig = Rig::new();
    let g = rig.garage();
    let seventeen: Vec<&str> = (0..17).map(|_| g.light.as_str()).collect();
    assert_eq!(
        refused(&mut rig, json!({"zone": g.zone, "lights": seventeen})),
        (400, "Invalid params: At most 16 lights".to_string())
    );
    assert_eq!(
        refused(
            &mut rig,
            json!({"zone": g.zone, "lights": [g.light], "buttons": seventeen})
        ),
        (400, "Invalid params: At most 16 buttons".to_string())
    );
    // Sixteen lights are fine.
    let sixteen: Vec<&str> = (0..16).map(|_| g.light.as_str()).collect();
    let (status, _) = rig.instance(json!({"zone": g.zone, "lights": sixteen}));
    assert_eq!(status, 201);
}

#[test]
fn the_log_keeps_the_last_20_lines_newest_first_and_cuts_a_message_at_200_bytes() {
    let mut rig = Rig::new();
    let g = rig.garage();
    let id = rig.garage_instance(&g, json!({}));
    // Each external ON / OFF pair writes lines: 60 "Light turned on externally" lines.
    for _ in 0..60 {
        rig.report(&state(RELAY, true));
        rig.report(&state(RELAY, false));
        rig.advance(2 * 1000 + 1);
    }
    let (_, log) = rig.get(&format!("{}/{id}/log", base()));
    let log = log.as_array().unwrap();
    assert_eq!(log.len(), 20, "20 at most, the default");
    // Newest first, ids going down, the oldest ones gone.
    let ids: Vec<u64> = log.iter().map(|e| e["id"].as_u64().unwrap()).collect();
    assert!(ids.windows(2).all(|w| w[0] == w[1] + 1));
    assert!(ids[19] > 11, "the first lines were dropped: {ids:?}");
    assert!(!log.iter().any(|e| e["message"] == "Instance created"));

    // A message over 200 bytes: a light named with 64 characters, 3-byte ones, deleted (spec
    // 024: its name goes to the warning line, 33 + 2 + 62 x 3 = 221 bytes, cut at 200 on a
    // character boundary).
    let mut rig = Rig::new();
    let g = rig.garage();
    let relay = rig.relay(0x80);
    let name = format!("ab{}", "€".repeat(62));
    let long_light = rig.equipment(&name, "light_onoff", &g.zone, &[&relay]);
    let id = rig.garage_instance(&g, json!({"lights": [g.light, long_light]}));
    let (status, _) = rig.delete(&format!("/api/v1/equipments/{long_light}"));
    assert_eq!(status, 204);
    let (_, log) = rig.get(&format!("{}/{id}/log?limit=2", base()));
    let long = log[1]["message"].as_str().unwrap();
    assert!(long.starts_with("Equipment removed from settings: ab€"));
    assert_eq!(long.len(), 200, "cut at 200 bytes");
    assert_eq!(
        log[0]["message"],
        "Recipe activated — no motion, lights off"
    );
}

#[test]
fn the_log_limit_is_a_positive_whole_number_of_at_most_20() {
    let mut rig = Rig::new();
    let g = rig.garage();
    let id = rig.garage_instance(&g, json!({}));
    for _ in 0..60 {
        rig.report(&state(RELAY, true));
        rig.report(&state(RELAY, false));
        rig.advance(2001);
    }
    for (query, count) in [
        ("", 20),
        ("?limit=0", 20),
        ("?limit=-3", 20),
        ("?limit=abc", 20),
        ("?limit=10", 10),
        ("?limit=5", 5),
        ("?limit=50", 20),
        ("?limit=500", 20),
        ("?limit=1", 1),
        ("?limit=", 20),
    ] {
        let (status, log) = rig.get(&format!("{}/{id}/log{query}", base()));
        assert_eq!(status, 200, "{query}");
        assert_eq!(log.as_array().unwrap().len(), count, "{query}");
    }
}

#[test]
fn an_update_with_params_that_do_not_validate_is_a_400_and_the_instance_keeps_running() {
    let mut rig = Rig::new();
    let g = rig.garage();
    let id = rig.garage_instance(&g, json!({}));
    rig.sent();
    let (status, body) = rig.put(
        &format!("{}/{id}", base()),
        json!({"params": {"zone": g.zone, "lights": [GHOST]}}),
    );
    assert_eq!(
        (status, body),
        (
            400,
            json!({"error": format!("Invalid params: Light equipment not found: {GHOST}")})
        )
    );
    // Still running with its params: the motion still switches the light on, after 20 s.
    let (_, list) = rig.get(&base());
    assert_eq!(list[0]["params"]["timeout"], "20s");
    rig.report(&motion(PIR, true));
    assert_eq!(rig.sent(), [Command::On]);
    // No restart: no new sync line.
    let activated = rig
        .messages(&id)
        .iter()
        .filter(|m| m.starts_with("Recipe activated"))
        .count();
    assert_eq!(activated, 1);
    // A valid update restarts it: the sync runs again.
    let (status, _) = rig.put(
        &format!("{}/{id}", base()),
        json!({"params": {"zone": g.zone, "lights": [g.light], "timeout": "5s"}}),
    );
    assert_eq!(status, 200);
    assert_eq!(rig.sent(), [Command::On], "the motion is still there: on");
}

#[test]
fn an_update_of_a_disabled_instance_saves_it_without_starting_it() {
    let mut rig = Rig::new();
    let g = rig.garage();
    let id = rig.garage_instance(&g, json!({}));
    rig.sent();
    rig.post(&format!("{}/{id}/disable", base()), json!({}));
    let (status, _) = rig.put(
        &format!("{}/{id}", base()),
        json!({"params": {"zone": g.zone, "lights": [g.light], "timeout": "5s"}}),
    );
    assert_eq!(status, 200);
    assert_eq!(rig.sent(), []);
    assert_eq!(
        rig.refused(
            "POST",
            &format!("{}/{id}/actions", base()),
            r#"{"action":"x"}"#
        ),
        (400, "Instance not running".to_string())
    );
}

// ---- recipes.json ----

/// What the host writes to `recipes.json`, from the home's cursor.
fn saved(home: &mut Home) -> Option<String> {
    let mut cursor = home.take_recipes_cursor()?;
    let mut text = String::new();
    while let Some(chunk) = cursor.next_chunk(home).unwrap() {
        text.push_str(&chunk);
    }
    Some(text)
}

#[test]
fn the_instances_are_written_at_once_on_each_change_and_come_back_with_their_params_only() {
    let mut rig = Rig::new();
    let g = rig.garage();
    assert!(saved(&mut rig.home).is_none(), "nothing to write yet");
    let id = rig.garage_instance(&g, json!({}));
    let text = saved(&mut rig.home).expect("a create is written");
    assert!(saved(&mut rig.home).is_none(), "taken once");
    let list: Vec<RecipeInstance> = serde_json::from_str(&text).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id.to_string(), id);
    let json: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(json[0].as_object().unwrap().len(), 5, "no state, no log");
    for change in [
        (
            "PUT",
            format!("{}/{id}", base()),
            json!({"params": {"zone": g.zone, "lights": [g.light]}}),
        ),
        ("POST", format!("{}/{id}/disable", base()), json!({})),
        ("POST", format!("{}/{id}/enable", base()), json!({})),
        ("DELETE", format!("{}/{id}", base()), json!(null)),
    ] {
        let body = if change.2.is_null() {
            String::new()
        } else {
            change.2.to_string()
        };
        rig.raw(change.0, &change.1, &body);
        assert!(
            saved(&mut rig.home).is_some(),
            "{} {} is written",
            change.0,
            change.1
        );
    }
    // A refused change writes nothing.
    rig.instance(json!({"zone": "nope"}));
    assert!(saved(&mut rig.home).is_none());
}

/// A new home with the same devices, zones and equipments, as after a restart.
fn restart(rig: &mut Rig) {
    rig.add_devices(Vec::new());
}

#[test]
fn after_a_restart_the_instances_run_again_and_one_whose_light_is_gone_stays_stopped() {
    let mut rig = Rig::new();
    let g = rig.garage();
    let relay2 = rig.relay(0x7B);
    let second = rig.equipment("Second", "light_onoff", &g.zone, &[&relay2]);
    let kept = rig.garage_instance(&g, json!({}));
    let lost = rig.garage_instance(&g, json!({"lights": [second]}));
    let disabled = rig.garage_instance(&g, json!({}));
    rig.post(&format!("{}/{disabled}/disable", base()), json!({}));
    let text = saved(&mut rig.home).unwrap();
    rig.delete(&format!("/api/v1/equipments/{second}"));
    restart(&mut rig);
    rig.load_motion_light();
    rig.home.load_recipes(serde_json::from_str(&text).unwrap());
    rig.home.start_recipes(rig.now, rig.now);
    // The first one started: its sync switched the light off, and its log starts here.
    assert_eq!(rig.sent(), [Command::Off]);
    assert_eq!(
        rig.messages(&kept),
        ["Recipe activated — no motion, lights off"]
    );
    // The second one failed to restore, and stays stopped.
    assert_eq!(
        rig.messages(&lost),
        [format!(
            "Failed to restore: Light equipment not found: {second}"
        )]
    );
    let (_, log) = rig.get(&format!("{}/{lost}/log", base()));
    assert_eq!(log[0]["level"], "error");
    assert_eq!(
        rig.refused(
            "POST",
            &format!("{}/{lost}/actions", base()),
            r#"{"action":"x"}"#
        ),
        (400, "Instance not running".to_string())
    );
    // The disabled one stays disabled and silent.
    assert!(rig.messages(&disabled).is_empty());
    let (_, list) = rig.get(&base());
    assert_eq!(list.as_array().unwrap().len(), 3);
    assert_eq!(list[2]["enabled"], false);
    // The log numbering restarts: "since the start of the program".
    let (_, log) = rig.get(&format!("{}/{kept}/log", base()));
    assert_eq!(log[0]["id"], 1);
    // Loading changed nothing to write.
    assert!(saved(&mut rig.home).is_none());
}

fn instance(n: u8, recipe: &str, params: Value) -> RecipeInstance {
    RecipeInstance {
        id: Id::from_random([n; 16]),
        recipe_id: recipe.to_string(),
        params: params.to_string().into(),
        enabled: true,
        created_at: T0,
    }
}

#[test]
fn what_recipes_json_holds_that_cannot_run_is_kept_stopped_dropped_or_refused_as_the_spec_says() {
    let mut rig = Rig::new();
    let g = rig.garage();
    let good = json!({"zone": g.zone, "lights": [g.light], "timeout": "20s"});
    let mut list = vec![
        instance(1, "nope", good.clone()),
        instance(
            2,
            "motion-light",
            json!({"zone": g.zone, "lights": [g.light], "note": "x".repeat(2100)}),
        ),
        instance(3, "motion-light", good.clone()),
        // The same id twice: the second is dropped.
        instance(3, "motion-light", json!({"zone": "other"})),
    ];
    // 16 in all can stay: the 17th is dropped.
    for n in 10..24u8 {
        list.push(instance(n, "motion-light", good.clone()));
    }
    assert_eq!(list.len(), 18);
    restart(&mut rig);
    rig.load_motion_light();
    rig.home.load_recipes(list);
    assert_eq!(rig.home.recipes().len(), 16, "16 kept");
    rig.home.start_recipes(rig.now, rig.now);
    let first = Id::from_random([1; 16]).to_string();
    let second = Id::from_random([2; 16]).to_string();
    let third = Id::from_random([3; 16]).to_string();
    assert_eq!(
        rig.messages(&first),
        ["Failed to restore: Recipe not found: nope"]
    );
    assert_eq!(
        rig.messages(&second),
        ["Failed to restore: params too large"]
    );
    assert_eq!(
        rig.messages(&third),
        ["Recipe activated — no motion, lights off"],
        "the first of the two with that id"
    );
    // The two that could not start are kept: still listed, still enabled.
    let (_, listed) = rig.get(&base());
    assert_eq!(listed.as_array().unwrap().len(), 16);
    assert_eq!(listed[0]["recipeId"], "nope");
    assert_eq!(listed[0]["enabled"], true);
    // The last two entries (ids 22 and 23) were beyond 16.
    let beyond = Id::from_random([23; 16]).to_string();
    assert!(
        !listed
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["id"] == beyond.as_str())
    );
}

// ---- Orders of a recipe and the API ----

#[test]
fn a_recipe_switching_three_lights_never_makes_the_api_answer_busy() {
    let mut rig = Rig::new();
    let g = rig.garage();
    let mut lights = vec![g.light.clone()];
    for last in [0x7B, 0x7C] {
        let relay = rig.relay(last);
        lights.push(rig.equipment("Light", "light_onoff", &g.zone, &[&relay]));
    }
    rig.garage_instance(&g, json!({"lights": lights}));
    // The start sync queued 3 OFF orders of the core.
    assert_eq!(rig.home.orders().unattended_len(), 3);
    // Two orders of the API are accepted, a third is busy: the core's orders do not count.
    let path = format!("/api/v1/equipments/{}/orders/state", g.light);
    let ask = |rig: &mut Rig| {
        handle_stream(
            &mut rig.home,
            &Request {
                method: "POST",
                path: &path,
                host: Some(HOST),
                port: PORT,
                reach: Reach::Loopback,
                content_type: Some("application/json"),
                authorization: None,
                content_length: None,
                body: br#"{"value":true}"#,
            },
            rig.now,
            &mut *rig.random,
        )
    };
    let first = ask(&mut rig);
    assert!(matches!(first, Reply::Order(_)), "{first:?}");
    let second = ask(&mut rig);
    assert!(matches!(second, Reply::Order(_)), "{second:?}");
    let third = ask(&mut rig);
    assert!(
        matches!(third, Reply::Whole(ref r) if r.status == 503),
        "{third:?}"
    );
}
