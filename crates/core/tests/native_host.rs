//! `NativeHost` (spec 041, R3.2): the SDK's `Host` over the home, as a recipe sees it: the views, the zones, and the statuses of `order`.

mod support;

use serde_json::json;
use solite_core::devices::Id;
use solite_core::orders::MAX_UNATTENDED;
use solite_core::radio::zcl::Command;
use solite_core::recipes::native::{NativeHost, RecipeName};
use solite_recipe_sdk::{EventValue, Host, Level, OrderStatus};
use support::*;

fn names() -> RecipeName {
    RecipeName {
        id: "motion-light".into(),
        name: "Motion Light".into(),
    }
}

#[test]
fn an_equipment_is_seen_with_its_bindings_and_found_by_an_id_in_capitals() {
    let mut rig = Rig::new();
    let g = rig.garage();
    let recipe = names();
    let host = NativeHost::new(&rig.home, Id::ROOT_ZONE, &recipe, rig.now, rig.now);
    let light = host.equipment(&g.light.to_uppercase()).unwrap();
    assert_eq!(light.id, g.light, "the id comes back in lower case");
    assert_eq!((light.kind.as_str(), light.enabled), ("light_onoff", true));
    assert_eq!(light.zone_id, g.zone);
    let state = light
        .data_bindings
        .iter()
        .find(|b| b.alias == "state")
        .unwrap();
    assert_eq!(state.category.as_deref(), Some("light_state"));
    assert_eq!(state.device_id.as_deref(), Some(g.relay_device.as_str()));
    assert_eq!(state.value, EventValue::Bool(false));
    assert!(light.order_bindings.iter().any(|b| b.alias == "state"));
    assert!(host.equipment("not an id").is_none());
    assert!(
        host.equipment(&g.zone).is_none(),
        "a zone is not an equipment"
    );
}

#[test]
fn a_zone_reads_its_motion_and_its_equipments_enabled_ones_only() {
    let mut rig = Rig::new();
    let g = rig.garage();
    rig.report(&motion(PIR, true));
    let recipe = names();
    let host = NativeHost::new(&rig.home, Id::ROOT_ZONE, &recipe, rig.now, rig.now);
    let zone = host.zone_data(&g.zone).unwrap();
    assert!(zone.motion);
    assert_eq!(zone.motion_sensors, 1);
    assert_eq!(
        host.zone_equipment_ids(&g.zone),
        [g.motion_equipment.clone(), g.light.clone()]
    );
    assert!(
        host.zone_data("11111111-2222-4333-8444-555555555555")
            .is_none()
    );
    assert!(host.zone_data("nonsense").is_none());
    assert!(host.zone_equipment_ids("nonsense").is_empty());
    drop(host);
    let (status, _) = rig.put(
        &format!("/api/v1/equipments/{}", g.motion_equipment),
        json!({"enabled": false}),
    );
    assert_eq!(status, 200);
    let host = NativeHost::new(&rig.home, Id::ROOT_ZONE, &recipe, rig.now, rig.now);
    assert_eq!(host.zone_equipment_ids(&g.zone), [g.light]);
}

#[test]
fn an_order_is_sent_through_the_one_binding_it_names_and_each_refusal_has_its_status() {
    let mut rig = Rig::new();
    let g = rig.garage();
    let recipe = names();
    let host = NativeHost::new(&rig.home, Id::ROOT_ZONE, &recipe, rig.now, rig.now);
    let binding = host.equipment(&g.light).unwrap().order_bindings[0]
        .id
        .clone();
    // Sent: in the queue, with its command.
    assert_eq!(
        host.order(&g.light, &binding, &json!("ON")),
        OrderStatus::Sent
    );
    // A binding that is not on the equipment, an equipment that is not there: no binding.
    let ghost = "11111111-2222-4333-8444-555555555555";
    assert_eq!(
        host.order(&g.light, ghost, &json!("ON")),
        OrderStatus::NoBinding
    );
    assert_eq!(
        host.order(ghost, &binding, &json!("ON")),
        OrderStatus::NoBinding
    );
    assert_eq!(
        host.order(&g.light, "nonsense", &json!("ON")),
        OrderStatus::NoBinding
    );
    // A value the device has no command for.
    assert_eq!(
        host.order(&g.light, &binding, &json!("sideways")),
        OrderStatus::Refused
    );
    drop(host);
    assert_eq!(rig.sent(), [Command::On]);
    // The queue of recipe orders is full after MAX_UNATTENDED: the next is dropped.
    let host = NativeHost::new(&rig.home, Id::ROOT_ZONE, &recipe, rig.now, rig.now);
    for _ in 0..MAX_UNATTENDED {
        assert_eq!(
            host.order(&g.light, &binding, &json!("OFF")),
            OrderStatus::Sent
        );
    }
    assert_eq!(
        host.order(&g.light, &binding, &json!("OFF")),
        OrderStatus::QueueFull
    );
    drop(host);
    assert_eq!(rig.sent().len(), MAX_UNATTENDED);
    // A disabled equipment: nothing sent, nothing said.
    let (status, _) = rig.put(
        &format!("/api/v1/equipments/{}", g.light),
        json!({"enabled": false}),
    );
    assert_eq!(status, 200);
    let host = NativeHost::new(&rig.home, Id::ROOT_ZONE, &recipe, rig.now, rig.now);
    assert_eq!(
        host.order(&g.light, &binding, &json!("ON")),
        OrderStatus::Disabled
    );
    drop(host);
    assert_eq!(rig.sent(), []);
}

#[test]
fn the_lines_a_recipe_logs_are_kept_in_order_until_taken() {
    let rig = Rig::new();
    let recipe = names();
    let host = NativeHost::new(&rig.home, Id::ROOT_ZONE, &recipe, rig.now, rig.now);
    host.log(Level::Warn, "first");
    host.log(Level::Info, "second");
    assert_eq!(
        host.take_lines(),
        [
            (Level::Warn, "first".to_string()),
            (Level::Info, "second".to_string())
        ]
    );
    assert!(host.take_lines().is_empty());
}
