//! Recognition of a paired device (spec 027 R3): `equipments::propose` gives the type and the
//! bindings of a device from its data and orders, with the aliases of Sowel's `resolveAlias`.
//! The garage's devices are built as the other tests build them; the others are synthetic.

mod support;

use serde_json::{Value, json};
use solite_core::devices::{DeviceWithDetails, parse_list};
use solite_core::equipments::{EquipmentType, Proposal, propose};
use support::*;

/// A device made from JSON: data as `(key, category, type)`, orders as `(key, category, type)`.
/// The ids are `n` and the position, so they are the same from run to run.
fn synthetic(
    n: u8,
    data: &[(&str, &str, &str)],
    orders: &[(&str, Option<&str>, &str)],
) -> DeviceWithDetails {
    let id =
        |prefix: u8, j: usize| format!("{prefix:02x}{n:02x}{j:02x}00-0000-4000-8000-000000000000");
    let device_id = id(0xd0, 0);
    let data: Vec<Value> = data
        .iter()
        .enumerate()
        .map(|(j, (key, category, kind))| {
            json!({"id": id(0xda, j), "deviceId": device_id, "key": key, "type": kind,
                   "category": category, "value": null, "lastUpdated": null})
        })
        .collect();
    let orders: Vec<Value> = orders
        .iter()
        .enumerate()
        .map(|(j, (key, category, kind))| {
            let mut order =
                json!({"id": id(0xdb, j), "deviceId": device_id, "key": key, "type": kind});
            if let Some(category) = category {
                order["category"] = json!(category);
            }
            order
        })
        .collect();
    let device = json!({
        "id": device_id, "integrationId": "solite",
        "sourceDeviceId": format!("0x00124b00ee{n:06x}"), "name": format!("Synthetic {n}"),
        "zoneId": null, "source": "solite", "status": "online", "powerSource": "mains",
        "lastSeen": null, "createdAt": solite_core::devices::format_iso(T0),
        "updatedAt": solite_core::devices::format_iso(T0), "data": data, "orders": orders,
    });
    parse_list(&json!([device]).to_string())
        .map(|mut list| list.remove(0))
        .unwrap()
}

/// The data and the order of a relay-like on/off channel, `key` and all.
const ON_OFF_DATA: (&str, &str, &str) = ("state", "light_state", "boolean");
const ON_OFF_ORDER: (&str, Option<&str>, &str) = ("state", Some("light_toggle"), "boolean");

fn aliases(bindings: &[(solite_core::devices::Id, solite_core::equipments::Alias)]) -> Vec<&str> {
    bindings.iter().map(|(_, alias)| &**alias).collect()
}

fn proposal(device: &DeviceWithDetails) -> Proposal {
    propose(device).expect("a proposal")
}

/// A garage device of the home of a rig, as the registry holds it.
fn held(rig: &Rig, id: &str) -> DeviceWithDetails {
    rig.home.devices.get(id).unwrap().clone()
}

#[test]
fn the_garage_relay_is_a_light_with_its_state_data_and_order() {
    let mut rig = Rig::new();
    let relay = rig.relay(3);
    let device = held(&rig, &relay);
    let p = proposal(&device);
    assert_eq!(p.kind, EquipmentType::LightOnoff);
    assert_eq!(aliases(&p.data), ["state"]);
    assert_eq!(aliases(&p.orders), ["state"]);
    assert_eq!(
        p.data[0].0, device.data[0].id,
        "the id of the device's data"
    );
    assert_eq!(p.orders[0].0, device.orders[0].id, "and of its order");
}

#[test]
fn the_garage_snzb02_is_a_sensor_without_its_voltage() {
    let mut rig = Rig::new();
    let th = rig.sensor(1);
    let p = proposal(&held(&rig, &th));
    assert_eq!(p.kind, EquipmentType::Sensor);
    assert_eq!(aliases(&p.data), ["temperature", "humidity", "battery"]);
    assert!(p.orders.is_empty());
}

#[test]
fn the_garage_motion_sensor_is_a_sensor_with_its_occupancy_and_battery() {
    let mut rig = Rig::new();
    let e = eui(2);
    rig.home.devices.announced(e, 0x80, rig.now);
    rig.home
        .devices
        .identified(e, Some("eWeLink"), Some("MS01"), rig.now, &mut *rig.random)
        .unwrap();
    rig.home
        .devices
        .reporting_configured(e, &[0x0406, 0x0001], rig.now, &mut *rig.random)
        .unwrap();
    let pir = rig.device(2);
    let p = proposal(&held(&rig, &pir));
    assert_eq!(p.kind, EquipmentType::Sensor);
    assert_eq!(aliases(&p.data), ["occupancy", "battery"]);
}

#[test]
fn a_dimmer_is_dimmable_with_state_and_brightness_both_ways() {
    let device = synthetic(
        1,
        &[ON_OFF_DATA, ("brightness", "light_brightness", "number")],
        &[
            ON_OFF_ORDER,
            ("brightness", Some("set_brightness"), "number"),
        ],
    );
    let p = proposal(&device);
    assert_eq!(p.kind, EquipmentType::LightDimmable);
    assert_eq!(aliases(&p.data), ["state", "brightness"]);
    assert_eq!(aliases(&p.orders), ["state", "brightness"]);
}

#[test]
fn a_colour_bulb_is_light_color_with_sowels_aliases() {
    let device = synthetic(
        2,
        &[
            ON_OFF_DATA,
            ("brightness", "light_brightness", "number"),
            ("color_temp", "light_color_temp", "number"),
        ],
        &[
            ON_OFF_ORDER,
            ("brightness", Some("set_brightness"), "number"),
            ("color_temp", Some("set_color_temp"), "number"),
        ],
    );
    let p = proposal(&device);
    assert_eq!(p.kind, EquipmentType::LightColor);
    assert_eq!(aliases(&p.data), ["state", "brightness", "color_temp"]);
    assert_eq!(aliases(&p.orders), ["state", "brightness", "color_temp"]);
    // A colour order alone, with no colour data, is enough; its key is renamed by its category.
    let device = synthetic(
        3,
        &[ON_OFF_DATA],
        &[ON_OFF_ORDER, ("color", Some("set_color"), "number")],
    );
    let p = proposal(&device);
    assert_eq!(p.kind, EquipmentType::LightColor);
    assert_eq!(aliases(&p.orders), ["state", "color"]);
}

#[test]
fn a_thermostat_binds_its_data_and_every_order() {
    let device = synthetic(
        4,
        &[
            ("local_temperature", "temperature", "number"),
            ("occupied_heating_setpoint", "setpoint", "number"),
            ("running", "power", "boolean"),
        ],
        &[
            ("occupied_heating_setpoint", Some("set_setpoint"), "number"),
            ("child_lock", None, "boolean"),
        ],
    );
    let p = proposal(&device);
    assert_eq!(p.kind, EquipmentType::Thermostat);
    assert_eq!(aliases(&p.data), ["temperature", "setpoint", "state"]);
    assert_eq!(aliases(&p.orders), ["setpoint", "child_lock"]);
}

#[test]
fn a_shutter_binds_its_position() {
    let device = synthetic(
        5,
        &[("position", "shutter_position", "number")],
        &[
            ("position", Some("set_shutter_position"), "number"),
            ("state", Some("shutter_move"), "boolean"),
        ],
    );
    let p = proposal(&device);
    assert_eq!(p.kind, EquipmentType::Shutter);
    assert_eq!(aliases(&p.data), ["position"]);
    assert_eq!(aliases(&p.orders), ["position", "state"]);
}

#[test]
fn a_button_binds_its_action_and_battery() {
    let device = synthetic(
        6,
        &[
            ("action", "action", "number"),
            ("battery", "battery", "number"),
        ],
        &[],
    );
    let p = proposal(&device);
    assert_eq!(p.kind, EquipmentType::Button);
    assert_eq!(aliases(&p.data), ["action", "battery"]);
}

#[test]
fn a_device_with_only_a_battery_or_nothing_read_gets_no_proposal() {
    let battery = synthetic(7, &[("battery", "battery", "number")], &[]);
    assert_eq!(propose(&battery), None);
    let nothing = synthetic(8, &[], &[]);
    assert_eq!(propose(&nothing), None);
}

#[test]
fn two_state_data_get_state_and_state_2() {
    let device = synthetic(
        9,
        &[
            ("state_l1", "light_state", "boolean"),
            ("state_l2", "light_state", "boolean"),
        ],
        &[ON_OFF_ORDER],
    );
    let p = proposal(&device);
    assert_eq!(p.kind, EquipmentType::LightOnoff);
    assert_eq!(aliases(&p.data), ["state", "state_2"]);
}

#[test]
fn twenty_sensor_data_give_sixteen_bindings() {
    // The key is the alias (a temperature has no alias of its own): twenty of the same key.
    let data = [("temperature", "temperature", "number"); 20];
    let p = proposal(&synthetic(10, &data, &[]));
    assert_eq!(p.kind, EquipmentType::Sensor);
    assert_eq!(p.data.len(), 16);
    assert_eq!(aliases(&p.data)[..2], ["temperature", "temperature_2"]);
    assert_eq!(aliases(&p.data)[15], "temperature_16");
}

#[test]
fn an_alias_past_32_characters_is_left_out() {
    let long = "x".repeat(33);
    let p = proposal(&synthetic(
        11,
        &[
            (long.as_str(), "humidity", "number"),
            ("t", "temperature", "number"),
        ],
        &[],
    ));
    assert_eq!(aliases(&p.data), ["t"], "the long alias is left out");
}

#[test]
fn a_paired_colour_bulb_is_a_light_color_with_its_four_data_and_four_orders() {
    // The registry's own device for the clusters 0x0006, 0x0008 and 0x0300 (spec 031).
    let mut rig = Rig::new();
    let e = eui(0x7D);
    rig.home.devices.announced(e, 0x8C, rig.now);
    rig.home
        .devices
        .identified(e, Some("TEST"), Some("CLR01"), rig.now, &mut *rig.random)
        .unwrap();
    rig.home
        .devices
        .reporting_configured(e, &[0x0006, 0x0008, 0x0300], rig.now, &mut *rig.random)
        .unwrap();
    let device = held(&rig, &rig.device(0x7D));
    let p = proposal(&device);
    assert_eq!(p.kind, EquipmentType::LightColor);
    assert_eq!(
        aliases(&p.data),
        ["state", "brightness", "color_temp", "color"]
    );
    assert_eq!(
        aliases(&p.orders),
        ["state", "brightness", "color_temp", "color"]
    );
}
