//! No recipe is built in (spec 046, R1): an engine with nothing loaded lists nothing, and an
//! instance of `motion-light` is refused as a recipe nobody provides. These tests run with and
//! without the `script-recipes` feature: a build without it has no recipe at all.

mod support;

use serde_json::json;
use support::*;

#[test]
fn a_home_with_no_script_lists_no_recipe_and_refuses_an_instance_of_motion_light() {
    let mut rig = Rig::new();
    let g = rig.garage();
    assert_eq!(rig.get("/api/v1/recipes"), (200, json!([])));
    assert_eq!(
        rig.refused("GET", "/api/v1/recipes/motion-light", ""),
        (404, "Recipe not found".to_string())
    );
    let (status, body) = rig.post(
        "/api/v1/recipe-instances",
        json!({"recipeId": "motion-light", "params": {"zone": g.zone, "lights": [g.light]}}),
    );
    assert_eq!(
        (status, body["error"].as_str().unwrap()),
        (404, "Recipe not found: motion-light")
    );
}
