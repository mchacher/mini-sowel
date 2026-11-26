//! `motion-light` against a fake `Host` (spec 041): the functions the engine-level tests of the
//! core used to call directly (`info`, `is_motion_active`, `resolve_motion_sources`), and the
//! recipe's own parts of the contract (state text, listen set, `prune`). The behaviour through the
//! engine (orders, log lines, timers) is tested in `crates/core/tests/motion_light.rs`.

use std::cell::RefCell;

use serde_json::{Value, json};
use solite_recipe_motion_light::{
    ID, MotionLightRecipe, NAME, info, is_motion_active, resolve_motion_sources,
};
use solite_recipe_sdk::{
    Change, ChangedEquipment, Clock, DataBindingView, EquipmentView, EventValue, Host, Level,
    ListenEntry, OrderBindingView, OrderStatus, Recipe, Sunlight, ZoneData,
};

const ZONE: &str = "00000000-0000-4000-8000-00000000000a";
const LIGHT: &str = "00000000-0000-4000-8000-0000000000b1";
const BUTTON: &str = "00000000-0000-4000-8000-0000000000c1";
const SENSOR: &str = "00000000-0000-4000-8000-0000000000d1";
const SENSOR_DEVICE: &str = "00000000-0000-4000-8000-0000000000e1";
const OTHER_DEVICE: &str = "00000000-0000-4000-8000-0000000000e2";

#[derive(Default)]
struct FakeHost {
    equipments: Vec<EquipmentView>,
    zone: Option<ZoneData>,
    orders: RefCell<Vec<(String, String, Value)>>,
    lines: RefCell<Vec<(Level, String)>>,
}

impl Host for FakeHost {
    fn equipment(&self, id: &str) -> Option<EquipmentView> {
        let id = id.to_ascii_lowercase();
        self.equipments.iter().find(|e| e.id == id).cloned()
    }
    fn zone_equipment_ids(&self, _: &str) -> Vec<String> {
        self.equipments
            .iter()
            .filter(|e| e.enabled)
            .map(|e| e.id.clone())
            .collect()
    }
    fn zone_data(&self, zone: &str) -> Option<ZoneData> {
        (zone == ZONE).then(|| self.zone.clone()).flatten()
    }
    fn sunlight(&self) -> Sunlight {
        Sunlight::default()
    }
    fn order(&self, equipment: &str, binding: &str, value: &Value) -> OrderStatus {
        self.orders
            .borrow_mut()
            .push((equipment.into(), binding.into(), value.clone()));
        OrderStatus::Sent
    }
    fn log(&self, level: Level, message: &str) {
        self.lines.borrow_mut().push((level, message.into()));
    }
}

fn data(
    alias: &str,
    category: &str,
    device: &str,
    key: &str,
    value: EventValue,
) -> DataBindingView {
    DataBindingView {
        id: format!("binding-{alias}"),
        alias: alias.into(),
        category: Some(category.into()),
        device_id: Some(device.into()),
        key: Some(key.into()),
        value,
    }
}

fn equipment(id: &str, kind: &str, data: Vec<DataBindingView>, orders: &[&str]) -> EquipmentView {
    EquipmentView {
        id: id.into(),
        name: format!("Equipment {}", &id[id.len() - 2..]),
        kind: kind.into(),
        enabled: true,
        zone_id: ZONE.into(),
        data_bindings: data,
        order_bindings: orders
            .iter()
            .map(|alias| OrderBindingView {
                id: format!("order-{alias}"),
                alias: (*alias).into(),
                category: None,
            })
            .collect(),
    }
}

fn motion_sensor(id: &str, device: &str) -> EquipmentView {
    equipment(
        id,
        "sensor",
        vec![data(
            "motion",
            "motion",
            device,
            "occupancy",
            EventValue::Bool(false),
        )],
        &[],
    )
}

fn light(on: bool) -> EquipmentView {
    equipment(
        LIGHT,
        "light_onoff",
        vec![data(
            "state",
            "light_state",
            "d",
            "state",
            EventValue::Text(if on { "ON" } else { "OFF" }.into()),
        )],
        &["state"],
    )
}

fn button() -> EquipmentView {
    equipment(
        BUTTON,
        "button",
        vec![data("action", "action", "d", "action", EventValue::Null)],
        &[],
    )
}

fn home(motion: bool) -> FakeHost {
    FakeHost {
        equipments: vec![light(false), button(), motion_sensor(SENSOR, SENSOR_DEVICE)],
        zone: Some(ZoneData {
            motion,
            luminosity: None,
            motion_sensors: 1,
        }),
        ..FakeHost::default()
    }
}

const CLOCK: Clock = Clock {
    mono: 1_000,
    wall: 1_795_252_323_120,
};

#[test]
fn the_recipe_info_is_the_json_of_the_reference() {
    let expected: Value = serde_json::from_str(include_str!("recipe_info.json")).unwrap();
    assert_eq!(info(), expected);
    assert_eq!((ID, NAME), ("motion-light", "Motion Light"));
}

#[test]
fn motion_is_a_true_a_non_zero_number_or_one_of_five_words() {
    let yes = [
        EventValue::Bool(true),
        EventValue::Number(1.0),
        EventValue::Number(5.0),
        EventValue::Text("true".into()),
        EventValue::Text("TRUE".into()),
        EventValue::Text("on".into()),
        EventValue::Text("On".into()),
        EventValue::Text("1".into()),
        EventValue::Text("occupied".into()),
        EventValue::Text("detected".into()),
        EventValue::Text(" detected ".into()),
    ];
    for value in yes {
        assert!(is_motion_active(&value), "{value:?}");
    }
    let no = [
        EventValue::Bool(false),
        EventValue::Number(0.0),
        EventValue::Text("false".into()),
        EventValue::Text("off".into()),
        EventValue::Text("0".into()),
        EventValue::Text("clear".into()),
        EventValue::Text(String::new()),
        EventValue::Null,
    ];
    for value in no {
        assert!(!is_motion_active(&value), "{value:?}");
    }
    // A JSON value is cut and converted the same way.
    assert!(is_motion_active(&EventValue::from_json(&json!("Occupied"))));
    assert!(!is_motion_active(&EventValue::from_json(&json!(null))));
}

#[test]
fn a_motion_source_is_a_device_and_a_key_once_and_two_devices_are_two_sources() {
    let mut host = home(false);
    // Two equipments on the same sensor device: the same (device, key). Another device: a second.
    host.equipments.push(motion_sensor(
        "00000000-0000-4000-8000-0000000000d2",
        SENSOR_DEVICE,
    ));
    host.equipments.push(motion_sensor(
        "00000000-0000-4000-8000-0000000000d3",
        OTHER_DEVICE,
    ));
    // A sensor with temperature only, and a binding whose data is gone: no source.
    host.equipments.push(equipment(
        "00000000-0000-4000-8000-0000000000d4",
        "sensor",
        vec![
            data(
                "temperature",
                "temperature",
                "t",
                "temperature",
                EventValue::Number(20.0),
            ),
            DataBindingView {
                id: "gone".into(),
                alias: "motion".into(),
                category: None,
                device_id: None,
                key: None,
                value: EventValue::Null,
            },
        ],
        &[],
    ));
    let sources = resolve_motion_sources(&host, ZONE);
    assert_eq!(sources.len(), 2);
    assert!(sources.iter().all(|(_, key)| &**key == "occupancy"));
    assert_ne!(sources[0].0, sources[1].0);
}

#[test]
fn the_state_text_has_none_one_or_three_keys_in_the_order_of_the_reference() {
    // No key: the light is off, nothing moves.
    let host = home(false);
    let mut recipe = MotionLightRecipe::new();
    let params = json!({"zone": ZONE, "lights": [LIGHT], "timeout": "20s"});
    let update = recipe.start(&host, 1, &params, CLOCK).unwrap();
    assert_eq!(update.state, "{}");
    // One key: motion at the start switches the light on and arms the failsafe.
    let host = home(true);
    let params = json!({"zone": ZONE, "lights": [LIGHT], "timeout": "20s", "maxOnDuration": "5m"});
    let update = recipe.start(&host, 2, &params, CLOCK).unwrap();
    assert_eq!(
        update.state,
        r#"{"failsafeExpiresAt":"2026-11-21T09:17:03.120Z"}"#
    );
    assert_eq!(host.orders.borrow().len(), 1);
    // Three keys: a button press turns the light off (override), arming the off timer; the
    // failsafe is cancelled by the press, so the override is armed again by the manual light.
    let mut host = home(false);
    host.equipments[0] = light(true);
    let params = json!({"zone": ZONE, "lights": [LIGHT], "buttons": [BUTTON], "maxOnDuration": "5m", "timeout": "20s"});
    let update = recipe.start(&host, 3, &params, CLOCK).unwrap();
    assert_eq!(update.state, "{}");
    let press = solite_recipe_sdk::Event::Equipment {
        equipment: BUTTON.into(),
        alias: "action".into(),
        value: EventValue::Text("single".into()),
    };
    let update = recipe.on_event(&host, 3, &press, CLOCK).unwrap().unwrap();
    assert_eq!(
        update.state,
        r#"{"overrideMode":true,"timerExpiresAt":"2026-11-21T09:12:23.120Z"}"#
    );
    // The same event again changes nothing: no update.
    assert_eq!(recipe.on_event(&host, 3, &press, CLOCK).unwrap(), None);
    // An unknown handle is an error, not a panic.
    assert!(recipe.tick(&host, 99, CLOCK).is_err());
}

#[test]
fn the_listen_set_is_the_zone_the_lights_the_buttons_and_with_a_failsafe_the_motion_sources() {
    let host = home(false);
    let mut recipe = MotionLightRecipe::new();
    let params = json!({"zone": ZONE, "lights": [LIGHT], "buttons": [BUTTON]});
    let update = recipe.start(&host, 1, &params, CLOCK).unwrap();
    assert_eq!(
        update.listen.clone().unwrap(),
        [
            ListenEntry::Zone { zone: ZONE.into() },
            ListenEntry::Equipment {
                equipment: LIGHT.into(),
                alias: "state".into()
            },
            ListenEntry::Equipment {
                equipment: BUTTON.into(),
                alias: "action".into()
            },
        ]
    );
    let params = json!({"zone": ZONE.to_uppercase(), "lights": [LIGHT.to_uppercase()], "maxOnDuration": "5m"});
    let update = recipe.start(&host, 2, &params, CLOCK).unwrap();
    assert_eq!(
        update.listen.clone().unwrap(),
        [
            ListenEntry::Zone { zone: ZONE.into() },
            ListenEntry::Equipment {
                equipment: LIGHT.into(),
                alias: "state".into()
            },
            ListenEntry::Device {
                device: SENSOR_DEVICE.into(),
                key: "occupancy".into()
            },
        ],
        "ids in capitals are listened to in their canonical form"
    );
    update.check().unwrap();
}

#[test]
fn a_zone_with_no_motion_sensor_is_a_warning_at_start() {
    let mut host = home(false);
    host.zone = Some(ZoneData {
        motion: false,
        luminosity: None,
        motion_sensors: 0,
    });
    let mut recipe = MotionLightRecipe::new();
    recipe
        .start(&host, 1, &json!({"zone": ZONE, "lights": [LIGHT]}), CLOCK)
        .unwrap();
    let lines = host.lines.borrow();
    assert_eq!(
        lines[0],
        (
            Level::Warn,
            "Zone has no motion sensors — recipe will never trigger".to_string()
        )
    );
    assert_eq!(lines[1].1, "Recipe activated — no motion, lights off");
}

#[test]
fn prune_takes_out_the_legacy_light_and_the_stale_buttons_and_says_whether_to_restart() {
    let mut host = home(false);
    let recipe = MotionLightRecipe::new();
    let gone = Some(ChangedEquipment {
        id: LIGHT.into(),
        name: "Garage light".into(),
    });
    // The legacy `light` key is the deleted light: emptied, a restart, the engine's line is not
    // there so the recipe says it.
    let params = json!({"zone": ZONE, "light": LIGHT});
    let change = Change {
        equipment: gone.clone(),
        pruned: Vec::new(),
    };
    let pruned = recipe.prune(&host, &params, &change).unwrap();
    assert_eq!(pruned.params["light"], "");
    assert!(pruned.restart);
    assert_eq!(pruned.lines.len(), 1);
    assert_eq!(
        pruned.lines[0].message,
        "Equipment removed from settings: Garage light"
    );
    // The engine took the id out of `lights` already: a restart, and no second line.
    let params = json!({"zone": ZONE, "lights": []});
    let change = Change {
        equipment: gone,
        pruned: vec!["lights".into()],
    };
    let pruned = recipe.prune(&host, &params, &change).unwrap();
    assert!(pruned.restart && pruned.lines.is_empty());
    // Only a button left: nothing for the recipe to do and no restart (`None`).
    let change = Change {
        equipment: Some(ChangedEquipment {
            id: BUTTON.into(),
            name: "Switch".into(),
        }),
        pruned: vec!["buttons".into()],
    };
    assert!(
        recipe
            .prune(&host, &json!({"zone": ZONE, "buttons": []}), &change)
            .is_none()
    );
    // A button with no `action` binding any more, and one that does not exist: named, once each.
    host.equipments[1].data_bindings.clear();
    let ghost = "00000000-0000-4000-8000-0000000000ff";
    let params = json!({"zone": ZONE, "buttons": [BUTTON, ghost, ghost, SENSOR]});
    let pruned = recipe.prune(&host, &params, &Change::default()).unwrap();
    assert_eq!(pruned.params["buttons"], json!([]));
    let messages: Vec<&str> = pruned.lines.iter().map(|l| l.message.as_str()).collect();
    assert_eq!(
        messages,
        [
            "Button removed from settings: Equipment c1",
            &format!("Button removed from settings: {ghost}"),
            "Button removed from settings: Equipment d1",
        ]
    );
    assert!(!pruned.restart);
    // Nothing to prune, or params that are not an object: nothing.
    assert!(
        recipe
            .prune(&home(false), &params_ok(), &Change::default())
            .is_none()
    );
    assert!(
        recipe
            .prune(&host, &json!("text"), &Change::default())
            .is_none()
    );
}

fn params_ok() -> Value {
    json!({"zone": ZONE, "lights": [LIGHT], "buttons": [BUTTON]})
}
