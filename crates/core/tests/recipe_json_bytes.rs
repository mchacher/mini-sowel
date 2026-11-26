//! The bytes of the recipe routes, pinned (spec 041): `GET /recipes`, `GET /recipes/motion-light`
//! and `GET /recipe-instances` with the instance state, compared as text, so that moving the
//! recipe onto the SDK (and running scripts beside it) cannot change a byte, key order included.

mod support;

use serde_json::json;
use support::*;

const SECOND: u64 = 1000;

fn body(rig: &mut Rig, path: &str) -> String {
    let response = rig.raw("GET", path, "");
    assert_eq!(response.status, 200);
    response.body.unwrap()
}

#[test]
fn the_recipe_list_and_the_recipe_are_served_byte_for_byte() {
    let mut rig = Rig::new();
    let list = body(&mut rig, "/api/v1/recipes");
    let one = body(&mut rig, "/api/v1/recipes/motion-light");
    assert_eq!(list, include_str!("pinned/recipes.json"));
    assert_eq!(one, include_str!("pinned/recipe_motion_light.json"));
}

/// Where a test of the instances' bytes is: the id of the instance and the garage's ids are the
/// deterministic ones of the rig.
fn instances_after(setup: impl FnOnce(&mut Rig, &Garage) -> String) -> String {
    let mut rig = Rig::new();
    let g = rig.garage();
    let id = setup(&mut rig, &g);
    let text = body(&mut rig, "/api/v1/recipe-instances");
    // The instance's own id, to make the expectation readable.
    assert!(text.contains(&id));
    text
}

#[test]
fn an_instance_with_no_state_key_is_served_with_an_empty_state() {
    let text = instances_after(|rig, g| rig.garage_instance(g, json!({})));
    assert_eq!(text, include_str!("pinned/instances_no_keys.json"));
}

#[test]
fn an_instance_with_one_state_key_is_served_with_it() {
    let text = instances_after(|rig, g| {
        let id = rig.garage_instance(g, json!({"maxOnDuration": "5m"}));
        rig.report(&motion(PIR, true));
        id
    });
    assert_eq!(text, include_str!("pinned/instances_one_key.json"));
}

#[test]
fn an_instance_with_the_three_state_keys_is_served_in_their_order() {
    let text = instances_after(|rig, g| {
        let id = rig.garage_instance(g, json!({"luxThreshold": 50, "maxOnDuration": "5m"}));
        rig.sent();
        rig.report(&luminosity(PIR, 40.0));
        rig.report(&state(RELAY, true));
        rig.report(&luminosity(PIR, 60.0));
        rig.advance(SECOND);
        id
    });
    assert_eq!(text, include_str!("pinned/instances_three_keys.json"));
}
