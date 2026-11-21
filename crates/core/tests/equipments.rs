//! Equipments (spec 010, R3, R7.1 to R7.5, R7.8, R7.9, R7.11, R7.13, R7.14): creation from
//! devices with auto-binding by alias, bindings, the routes and their errors, stale readings and
//! the status of an equipment, bounds, validation.

mod support;

use mini_sowel_core::devices::{Devices, ValueType, format_iso};
use mini_sowel_core::equipments::{
    EquipmentType, MAX_BINDINGS, is_stale, parse_list, stale_window_ms,
};
use mini_sowel_core::home::Home;
use serde_json::{Value, json};
use support::*;

fn text(value: &Value) -> &str {
    value.as_str().unwrap()
}

fn aliases(bindings: &Value) -> Vec<&str> {
    bindings
        .as_array()
        .unwrap()
        .iter()
        .map(|b| text(&b["alias"]))
        .collect()
}

/// A rig with a garage zone and the three devices of the garage.
fn garage() -> (Rig, String, String, String, String) {
    let mut rig = Rig::new();
    let th = rig.sensor(1);
    let pir = rig.motion_sensor(2);
    let relay = rig.relay(3);
    let zone = rig.zone("Garage");
    (rig, zone, th, pir, relay)
}

// ---- Creation and auto-binding ----

#[test]
fn a_sensor_from_the_snzb02_binds_its_four_data_with_the_key_as_alias() {
    let (mut rig, zone, th, _, _) = garage();
    let (status, body) = rig.post(
        "/api/v1/equipments",
        json!({"name": "Garage climate", "type": "sensor", "zoneId": zone, "deviceIds": [th]}),
    );
    assert_eq!(status, 201);
    assert_eq!(
        aliases(&body["dataBindings"]),
        ["temperature", "humidity", "battery", "voltage"]
    );
    assert_eq!(body["orderBindings"], json!([]));
    assert_eq!(body["enabled"], true);
    assert_eq!(body["zoneId"], zone.as_str());
    assert_eq!(body["type"], "sensor");
    let first = &body["dataBindings"][0];
    assert_eq!(first["equipmentId"], body["id"]);
    assert_eq!(first["deviceId"], th.as_str());
    assert_eq!(
        first["deviceName"],
        rig.get(&format!("/api/v1/devices/{th}")).1["name"]
    );
    assert_eq!(first["key"], "temperature");
    assert_eq!(first["category"], "temperature");
    assert_eq!(first["unit"], "°C");
    assert_eq!(first["value"], Value::Null);
    assert_eq!(first["lastUpdated"], Value::Null);
    assert_eq!(first["lastChanged"], Value::Null);
    assert_eq!(first["stale"], false);
    assert!(rig.home.changes().1, "written at once");
}

#[test]
fn a_light_from_the_relay_binds_the_state_data_and_the_state_order() {
    let (mut rig, zone, _, _, relay) = garage();
    let (_, body) = rig.post(
        "/api/v1/equipments",
        json!({"name": "Garage light", "type": "light_onoff", "zoneId": zone, "deviceIds": [relay]}),
    );
    assert_eq!(aliases(&body["dataBindings"]), ["state"]);
    assert_eq!(body["dataBindings"][0]["category"], "light_state");
    assert_eq!(body["dataBindings"][0]["type"], "boolean");
    assert_eq!(aliases(&body["orderBindings"]), ["state"]);
    let order = &body["orderBindings"][0];
    assert_eq!(order["key"], "state");
    assert_eq!(order["category"], "light_toggle");
    assert_eq!(order["type"], "boolean");
    assert_eq!(order["deviceId"], relay.as_str());
    assert_eq!(order["equipmentId"], body["id"]);
    assert!(
        order.get("value").is_none(),
        "an order binding has no value"
    );
}

#[test]
fn two_devices_with_the_same_keys_bind_the_first_and_skip_the_aliases_taken() {
    let mut rig = Rig::new();
    let first = rig.sensor(1);
    let second = rig.sensor(4);
    let zone = rig.zone("Room");
    let (status, body) = rig.post(
        "/api/v1/equipments",
        json!({"name": "Two", "type": "sensor", "zoneId": zone, "deviceIds": [first, second]}),
    );
    assert_eq!(status, 201);
    assert_eq!(body["dataBindings"].as_array().unwrap().len(), 4);
    assert!(
        body["dataBindings"]
            .as_array()
            .unwrap()
            .iter()
            .all(|b| b["deviceId"] == first.as_str()),
        "the second device's bindings were skipped"
    );
}

#[test]
fn an_unknown_device_in_device_ids_is_skipped_and_the_equipment_is_created() {
    let (mut rig, zone, th, _, _) = garage();
    let (status, body) = rig.post(
        "/api/v1/equipments",
        json!({
            "name": "Partial", "type": "sensor", "zoneId": zone,
            "deviceIds": ["11111111-1111-4111-8111-111111111111", "not an id", th]
        }),
    );
    assert_eq!(status, 201);
    assert_eq!(body["dataBindings"].as_array().unwrap().len(), 4);
    let (status, body) = rig.post(
        "/api/v1/equipments",
        json!({"name": "None", "type": "sensor", "zoneId": zone, "deviceIds": ["x"]}),
    );
    assert_eq!(
        status, 201,
        "a device list with nothing found still creates it"
    );
    assert_eq!(body["dataBindings"], json!([]));
}

#[test]
fn creating_refuses_a_bad_type_an_unknown_zone_with_sowels_texts() {
    let (mut rig, zone, th, _, _) = garage();
    let unknown = "11111111-1111-4111-8111-111111111111";
    for bad in ["light_color", "awning", "", "Sensor"] {
        assert_eq!(
            rig.refused(
                "POST",
                "/api/v1/equipments",
                &json!({"name": "E", "type": bad, "zoneId": zone}).to_string()
            ),
            (400, format!("Invalid equipment type: {bad}"))
        );
    }
    assert_eq!(
        rig.refused(
            "POST",
            "/api/v1/equipments",
            &json!({"name": "E", "type": "sensor", "zoneId": unknown, "deviceIds": [th]})
                .to_string()
        ),
        (404, format!("Zone not found: {unknown}"))
    );
    assert_eq!(
        rig.refused(
            "POST",
            "/api/v1/equipments",
            r#"{"name":"E","type":"sensor","zoneId":"nope"}"#
        ),
        (404, "Zone not found: nope".to_string())
    );
    assert_eq!(rig.home.equipments().list().len(), 0, "nothing was created");
}

#[test]
fn every_type_of_decision_d3_is_accepted() {
    let (mut rig, zone, _, _, _) = garage();
    for kind in [
        "light_onoff",
        "light_dimmable",
        "shutter",
        "sensor",
        "switch",
        "thermostat",
        "button",
    ] {
        let (status, body) = rig.post(
            "/api/v1/equipments",
            json!({"name": kind, "type": kind, "zoneId": zone}),
        );
        assert_eq!(status, 201, "{kind}");
        assert_eq!(body["type"], kind);
    }
}

// ---- Reading ----

#[test]
fn get_lists_by_name_filters_by_type_and_an_unknown_type_gives_an_empty_list() {
    let (mut rig, zone, th, pir, relay) = garage();
    rig.equipment("beta", "sensor", &zone, &[&th]);
    rig.equipment("Alpha", "light_onoff", &zone, &[&relay]);
    rig.equipment("alpha 2", "sensor", &zone, &[&pir]);
    let (status, all) = rig.get("/api/v1/equipments");
    assert_eq!(status, 200);
    let names: Vec<&str> = all
        .as_array()
        .unwrap()
        .iter()
        .map(|e| text(&e["name"]))
        .collect();
    assert_eq!(names, ["Alpha", "alpha 2", "beta"], "sorted by name");
    let (_, sensors) = rig.get("/api/v1/equipments?type=sensor");
    let names: Vec<&str> = sensors
        .as_array()
        .unwrap()
        .iter()
        .map(|e| text(&e["name"]))
        .collect();
    assert_eq!(names, ["alpha 2", "beta"]);
    assert!(sensors[0].get("dataBindings").is_some(), "with details");
    assert_eq!(rig.get("/api/v1/equipments?type=unknown"), (200, json!([])));
    assert_eq!(
        rig.get("/api/v1/equipments?type=light_color"),
        (200, json!([]))
    );
    let (_, lights) = rig.get("/api/v1/equipments?x=1&type=light_onoff");
    assert_eq!(lights.as_array().unwrap().len(), 1);
}

#[test]
fn get_one_equipment_has_its_details_and_an_unknown_one_is_404() {
    let (mut rig, zone, th, _, _) = garage();
    let id = rig.equipment("Climate", "sensor", &zone, &[&th]);
    let (status, body) = rig.get(&format!("/api/v1/equipments/{id}"));
    assert_eq!(status, 200);
    assert_eq!(body["id"], id.as_str());
    assert_eq!(body["status"], "online");
    assert!(body.get("statusReason").is_none());
    assert_eq!(
        rig.refused("GET", "/api/v1/equipments/nope", ""),
        (404, "Equipment not found".to_string())
    );
}

// ---- Changing ----

#[test]
fn put_changes_the_fields_and_answers_an_equipment_without_details() {
    let (mut rig, zone, th, _, _) = garage();
    let other = rig.zone("Other");
    let id = rig.equipment("Climate", "sensor", &zone, &[&th]);
    rig.now += 5000;
    let (status, body) = rig.put(
        &format!("/api/v1/equipments/{id}"),
        json!({"name": " Probe ", "zoneId": other, "enabled": false, "type": "thermostat",
               "icon": "thermo", "description": "d"}),
    );
    assert_eq!(status, 200);
    assert_eq!(body["name"], "Probe");
    assert_eq!(body["zoneId"], other.as_str());
    assert_eq!(body["enabled"], false);
    assert_eq!(body["type"], "thermostat");
    assert_eq!(body["icon"], "thermo");
    assert_eq!(body["updatedAt"], "2026-11-21T09:12:08.120Z");
    assert!(
        body.get("dataBindings").is_none(),
        "PUT answers an Equipment"
    );
    assert!(body.get("status").is_none());
    // The bindings stay; null clears icon and description.
    let (_, body) = rig.put(
        &format!("/api/v1/equipments/{id}"),
        json!({"icon": null, "description": null}),
    );
    assert!(body.get("icon").is_none() && body.get("description").is_none());
    assert_eq!(
        rig.get(&format!("/api/v1/equipments/{id}")).1["dataBindings"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    assert_eq!(
        rig.put(&format!("/api/v1/equipments/{id}"), json!({})).0,
        200
    );
}

#[test]
fn put_refuses_with_sowels_texts_and_changes_nothing() {
    let (mut rig, zone, th, _, _) = garage();
    let id = rig.equipment("Climate", "sensor", &zone, &[&th]);
    let path = format!("/api/v1/equipments/{id}");
    assert_eq!(
        rig.refused("PUT", &path, r#"{"type":"light_color"}"#),
        (400, "Invalid equipment type: light_color".to_string())
    );
    assert_eq!(
        rig.refused("PUT", &path, r#"{"zoneId":"nope"}"#),
        (404, "Zone not found: nope".to_string())
    );
    assert_eq!(
        rig.refused(
            "PUT",
            "/api/v1/equipments/11111111-1111-4111-8111-111111111111",
            r#"{"name":"x"}"#
        ),
        (404, "Equipment not found".to_string())
    );
    // A refused zone changes the name that came with it neither.
    assert_eq!(
        rig.refused("PUT", &path, r#"{"name":"New","zoneId":"nope"}"#)
            .0,
        404
    );
    assert_eq!(rig.get(&path).1["name"], "Climate");
}

#[test]
fn delete_answers_204_and_an_unknown_equipment_is_404() {
    let (mut rig, zone, th, _, _) = garage();
    let id = rig.equipment("Climate", "sensor", &zone, &[&th]);
    assert_eq!(
        rig.delete(&format!("/api/v1/equipments/{id}")),
        (204, Value::Null)
    );
    assert_eq!(rig.get(&format!("/api/v1/equipments/{id}")).0, 404);
    assert_eq!(
        rig.refused("DELETE", &format!("/api/v1/equipments/{id}"), ""),
        (404, "Equipment not found".to_string())
    );
    // The device stays.
    assert_eq!(rig.get(&format!("/api/v1/devices/{th}")).0, 200);
}

// ---- Bindings ----

fn data_id_of(rig: &mut Rig, device: &str, key: &str) -> String {
    let (_, body) = rig.get(&format!("/api/v1/devices/{device}"));
    body["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["key"] == key)
        .map(|d| text(&d["id"]).to_string())
        .unwrap()
}

fn order_id_of(rig: &mut Rig, device: &str) -> String {
    text(&rig.get(&format!("/api/v1/devices/{device}")).1["orders"][0]["id"]).to_string()
}

#[test]
fn bindings_are_added_and_removed_and_answer_sowels_binding() {
    let (mut rig, zone, th, _, relay) = garage();
    let id = rig.equipment("Empty", "sensor", &zone, &[]);
    let data = data_id_of(&mut rig, &th, "temperature");
    let (status, binding) = rig.post(
        &format!("/api/v1/equipments/{id}/data-bindings"),
        json!({"deviceDataId": data, "alias": " temperature "}),
    );
    assert_eq!(status, 201);
    assert_eq!(binding["equipmentId"], id.as_str());
    assert_eq!(binding["deviceDataId"], data.as_str());
    assert_eq!(binding["alias"], "temperature", "trimmed");
    assert_eq!(binding.as_object().unwrap().len(), 4);
    let binding_id = text(&binding["id"]).to_string();

    let order = order_id_of(&mut rig, &relay);
    let (status, order_binding) = rig.post(
        &format!("/api/v1/equipments/{id}/order-bindings"),
        json!({"deviceOrderId": order, "alias": "toggle"}),
    );
    assert_eq!(status, 201);
    assert_eq!(order_binding["deviceOrderId"], order.as_str());
    let order_binding_id = text(&order_binding["id"]).to_string();
    let (_, details) = rig.get(&format!("/api/v1/equipments/{id}"));
    assert_eq!(aliases(&details["dataBindings"]), ["temperature"]);
    assert_eq!(aliases(&details["orderBindings"]), ["toggle"]);

    assert_eq!(
        rig.delete(&format!(
            "/api/v1/equipments/{id}/data-bindings/{binding_id}"
        )),
        (204, Value::Null)
    );
    assert_eq!(
        rig.delete(&format!(
            "/api/v1/equipments/{id}/order-bindings/{order_binding_id}"
        )),
        (204, Value::Null)
    );
    let (_, details) = rig.get(&format!("/api/v1/equipments/{id}"));
    assert_eq!(details["dataBindings"], json!([]));
    assert_eq!(details["orderBindings"], json!([]));
    assert_eq!(details["status"], "offline", "no data binding: offline");
}

#[test]
fn a_taken_alias_is_409_for_data_and_for_orders_with_sowels_texts() {
    let (mut rig, zone, th, _, relay) = garage();
    let id = rig.equipment("Climate", "sensor", &zone, &[&th]);
    let humidity = data_id_of(&mut rig, &th, "humidity");
    assert_eq!(
        rig.refused(
            "POST",
            &format!("/api/v1/equipments/{id}/data-bindings"),
            &json!({"deviceDataId": humidity, "alias": "temperature"}).to_string()
        ),
        (
            409,
            "Alias \"temperature\" already exists on this equipment".to_string()
        )
    );
    let light = rig.equipment("Light", "light_onoff", &zone, &[&relay]);
    let order = order_id_of(&mut rig, &relay);
    assert_eq!(
        rig.refused(
            "POST",
            &format!("/api/v1/equipments/{light}/order-bindings"),
            &json!({"deviceOrderId": order, "alias": "state"}).to_string()
        ),
        (
            409,
            "OrderBinding for alias \"state\" with this device order already exists".to_string()
        )
    );
}

#[test]
fn unknown_data_order_binding_or_equipment_are_404_with_sowels_texts() {
    let (mut rig, zone, th, _, relay) = garage();
    let id = rig.equipment("Climate", "sensor", &zone, &[&th]);
    let other = rig.equipment("Light", "light_onoff", &zone, &[&relay]);
    let unknown = "11111111-1111-4111-8111-111111111111";
    let data_path = format!("/api/v1/equipments/{id}/data-bindings");
    let order_path = format!("/api/v1/equipments/{id}/order-bindings");
    assert_eq!(
        rig.refused(
            "POST",
            &data_path,
            &json!({"deviceDataId": unknown, "alias": "a"}).to_string()
        ),
        (404, format!("DeviceData not found: {unknown}"))
    );
    assert_eq!(
        rig.refused("POST", &data_path, r#"{"deviceDataId":"nope","alias":"a"}"#),
        (404, "DeviceData not found: nope".to_string())
    );
    assert_eq!(
        rig.refused(
            "POST",
            &order_path,
            &json!({"deviceOrderId": unknown, "alias": "a"}).to_string()
        ),
        (404, format!("DeviceOrder not found: {unknown}"))
    );
    assert_eq!(
        rig.refused(
            "POST",
            &format!("/api/v1/equipments/{unknown}/data-bindings"),
            r#"{"deviceDataId":"x","alias":"a"}"#
        ),
        (404, "Equipment not found".to_string()),
        "the equipment is looked up first"
    );
    // A binding that does not exist, and one of another equipment.
    assert_eq!(
        rig.refused("DELETE", &format!("{data_path}/{unknown}"), ""),
        (404, "DataBinding not found".to_string())
    );
    assert_eq!(
        rig.refused("DELETE", &format!("{order_path}/{unknown}"), ""),
        (404, "OrderBinding not found".to_string())
    );
    let (_, light) = rig.get(&format!("/api/v1/equipments/{other}"));
    let foreign_data = text(&light["dataBindings"][0]["id"]).to_string();
    let foreign_order = text(&light["orderBindings"][0]["id"]).to_string();
    assert_eq!(
        rig.refused("DELETE", &format!("{data_path}/{foreign_data}"), ""),
        (404, "DataBinding not found".to_string())
    );
    assert_eq!(
        rig.refused("DELETE", &format!("{order_path}/{foreign_order}"), ""),
        (404, "OrderBinding not found".to_string())
    );
    // Neither was removed from the other equipment.
    let (_, light) = rig.get(&format!("/api/v1/equipments/{other}"));
    assert_eq!(light["dataBindings"].as_array().unwrap().len(), 1);
    assert_eq!(light["orderBindings"].as_array().unwrap().len(), 1);
    assert_eq!(
        rig.refused(
            "DELETE",
            &format!("/api/v1/equipments/{unknown}/data-bindings/{unknown}"),
            ""
        )
        .1,
        "Equipment not found"
    );
}

#[test]
fn binding_bodies_refuse_unknown_fields_and_check_the_alias() {
    let (mut rig, zone, th, _, _) = garage();
    let id = rig.equipment("Empty", "sensor", &zone, &[]);
    let data = data_id_of(&mut rig, &th, "temperature");
    let path = format!("/api/v1/equipments/{id}/data-bindings");
    let post = |rig: &mut Rig, body: Value| rig.refused("POST", &path, &body.to_string());
    assert_eq!(
        post(
            &mut rig,
            json!({"deviceDataId": data, "alias": "a", "historize": 1})
        ),
        (400, "Unknown field: historize".to_string()),
        "unknown fields are refused on bindings"
    );
    for alias in ["", "   ", &"a".repeat(33)] {
        assert_eq!(
            post(&mut rig, json!({"deviceDataId": data, "alias": alias})),
            (400, "Invalid alias".to_string()),
            "{alias:?}"
        );
    }
    assert_eq!(
        post(&mut rig, json!({"deviceDataId": data, "alias": 5})),
        (400, "Invalid field: alias".to_string())
    );
    assert_eq!(
        post(&mut rig, json!({"deviceDataId": 5, "alias": "a"})),
        (400, "Invalid field: deviceDataId".to_string())
    );
    assert_eq!(
        post(&mut rig, json!({"alias": "a"})),
        (400, "Missing field: deviceDataId".to_string())
    );
    assert_eq!(
        post(&mut rig, json!({"deviceDataId": data})),
        (400, "Missing field: alias".to_string())
    );
    assert_eq!(
        rig.refused("POST", &path, "nope"),
        (400, "Invalid JSON".to_string())
    );
    let order_path = format!("/api/v1/equipments/{id}/order-bindings");
    assert_eq!(
        rig.refused(
            "POST",
            &order_path,
            r#"{"deviceOrderId":"x","alias":"a","x":1}"#
        ),
        (400, "Unknown field: x".to_string())
    );
    assert_eq!(
        rig.refused("POST", &order_path, r#"{"alias":"a"}"#),
        (400, "Missing field: deviceOrderId".to_string())
    );
    let ok = rig.post(
        &path,
        json!({"deviceDataId": data, "alias": "a".repeat(32)}),
    );
    assert_eq!(ok.0, 201, "32 characters is the longest alias");
}

#[test]
fn a_deleted_device_loses_its_bindings_and_the_equipment_stays() {
    let (mut rig, zone, th, pir, _) = garage();
    let id = rig.equipment("Both", "sensor", &zone, &[&th, &pir]);
    let before = rig.get(&format!("/api/v1/equipments/{id}")).1;
    assert_eq!(before["dataBindings"].as_array().unwrap().len(), 5);
    assert_eq!(rig.delete(&format!("/api/v1/devices/{th}")).0, 204);
    let (status, after) = rig.get(&format!("/api/v1/equipments/{id}"));
    assert_eq!(status, 200, "the equipment stays");
    assert_eq!(aliases(&after["dataBindings"]), ["occupancy"]);
    assert!(rig.home.changes().1);
    // Removed from the equipment itself, and from what is written, not just hidden.
    assert_eq!(rig.home.equipments().list()[0].data_bindings.len(), 1);
    let stored = rig.home.take_equipments_json().unwrap();
    assert!(
        !stored.contains(&th),
        "no binding to the deleted device in the file"
    );
    assert_eq!(stored.matches("deviceDataId").count(), 1);
    // The last device: the equipment stays with no binding, offline.
    assert_eq!(rig.delete(&format!("/api/v1/devices/{pir}")).0, 204);
    let (_, after) = rig.get(&format!("/api/v1/equipments/{id}"));
    assert_eq!(after["dataBindings"], json!([]));
    assert_eq!(after["status"], "offline");
}

// ---- Bounds ----

#[test]
fn the_65th_equipment_is_refused() {
    let (mut rig, zone, _, _, _) = garage();
    for i in 0..64 {
        let (status, _) = rig.post(
            "/api/v1/equipments",
            json!({"name": format!("E{i}"), "type": "switch", "zoneId": zone}),
        );
        assert_eq!(status, 201, "{i}");
    }
    assert_eq!(
        rig.refused(
            "POST",
            "/api/v1/equipments",
            &json!({"name": "E64", "type": "switch", "zoneId": zone}).to_string()
        ),
        (400, "Too many equipments (64)".to_string())
    );
    assert_eq!(rig.home.equipments().list().len(), 64);
}

fn many_data(device: u8, name: &str, from: usize, count: usize) -> Value {
    let data: Vec<(String, &str, &str, Value, Option<u64>)> = (from..from + count)
        .map(|i| (format!("k{i}"), "generic", "number", json!(i), Some(T0)))
        .collect();
    let borrowed: Vec<(&str, &str, &str, Value, Option<u64>)> = data
        .iter()
        .map(|(k, c, t, v, u)| (k.as_str(), *c, *t, v.clone(), *u))
        .collect();
    device_json(device, name, true, &borrowed)
}

#[test]
fn auto_binding_stops_at_16_bindings_and_skips_the_rest() {
    let mut rig = Rig::new();
    let a = many_data(1, "A", 0, 16);
    let b = many_data(2, "B", 16, 4);
    let (a_id, b_id) = (device_json_id(&a), device_json_id(&b));
    rig.add_devices(vec![a, b]);
    let zone = rig.zone("Z");
    let (status, body) = rig.post(
        "/api/v1/equipments",
        json!({"name": "Big", "type": "sensor", "zoneId": zone, "deviceIds": [a_id, b_id]}),
    );
    assert_eq!(status, 201);
    assert_eq!(body["dataBindings"].as_array().unwrap().len(), MAX_BINDINGS);
    assert!(
        body["dataBindings"]
            .as_array()
            .unwrap()
            .iter()
            .all(|b| b["deviceId"] == a_id.as_str()),
        "the 17th to 20th were skipped, not an error"
    );
}

#[test]
fn an_explicit_17th_binding_is_refused_for_data_and_for_orders() {
    let mut rig = Rig::new();
    let a = many_data(1, "A", 0, 16);
    let b = many_data(2, "B", 16, 2);
    let (a_id, b_data) = (device_json_id(&a), data_id(&b, 0));
    rig.add_devices(vec![a, b]);
    let relay = rig.relay(3);
    let zone = rig.zone("Z");
    let id = rig.equipment("Big", "sensor", &zone, &[&a_id]);
    assert_eq!(
        rig.get(&format!("/api/v1/equipments/{id}")).1["dataBindings"]
            .as_array()
            .unwrap()
            .len(),
        16
    );
    assert_eq!(
        rig.refused(
            "POST",
            &format!("/api/v1/equipments/{id}/data-bindings"),
            &json!({"deviceDataId": b_data, "alias": "extra"}).to_string()
        ),
        (400, "Too many bindings (16)".to_string())
    );
    // Order bindings have their own 16: 16 aliases of the same order, then the 17th.
    let order = order_id_of(&mut rig, &relay);
    for i in 0..16 {
        let (status, _) = rig.post(
            &format!("/api/v1/equipments/{id}/order-bindings"),
            json!({"deviceOrderId": order, "alias": format!("o{i}")}),
        );
        assert_eq!(status, 201, "{i}");
    }
    assert_eq!(
        rig.refused(
            "POST",
            &format!("/api/v1/equipments/{id}/order-bindings"),
            &json!({"deviceOrderId": order, "alias": "o16"}).to_string()
        ),
        (400, "Too many bindings (16)".to_string())
    );
    // A removed binding makes room.
    let (_, details) = rig.get(&format!("/api/v1/equipments/{id}"));
    let first = text(&details["dataBindings"][0]["id"]).to_string();
    rig.delete(&format!("/api/v1/equipments/{id}/data-bindings/{first}"));
    assert_eq!(
        rig.post(
            &format!("/api/v1/equipments/{id}/data-bindings"),
            json!({"deviceDataId": b_data, "alias": "extra"})
        )
        .0,
        201
    );
}

// ---- Validation of the bodies ----

#[test]
fn equipment_bodies_are_checked_json_types_and_missing_fields() {
    let (mut rig, zone, _, _, _) = garage();
    let post = |rig: &mut Rig, body: &str| rig.refused("POST", "/api/v1/equipments", body);
    assert_eq!(post(&mut rig, "nope"), (400, "Invalid JSON".to_string()));
    assert_eq!(post(&mut rig, "[]"), (400, "Invalid JSON".to_string()));
    assert_eq!(
        post(&mut rig, "{}"),
        (400, "Missing field: name".to_string())
    );
    assert_eq!(
        post(
            &mut rig,
            &json!({"type": "sensor", "zoneId": zone}).to_string()
        ),
        (400, "Missing field: name".to_string())
    );
    assert_eq!(
        post(&mut rig, &json!({"name": "E", "zoneId": zone}).to_string()),
        (400, "Missing field: type".to_string())
    );
    assert_eq!(
        post(&mut rig, r#"{"name":"E","type":"sensor"}"#),
        (400, "Missing field: zoneId".to_string())
    );
    for (body, field) in [
        (json!({"name": 5, "type": "sensor", "zoneId": zone}), "name"),
        (json!({"name": "E", "type": 5, "zoneId": zone}), "type"),
        (
            json!({"name": "E", "type": "sensor", "zoneId": 5}),
            "zoneId",
        ),
        (
            json!({"name": "E", "type": "sensor", "zoneId": zone, "icon": 5}),
            "icon",
        ),
        (
            json!({"name": "E", "type": "sensor", "zoneId": zone, "description": []}),
            "description",
        ),
        (
            json!({"name": "E", "type": "sensor", "zoneId": zone, "deviceIds": "x"}),
            "deviceIds",
        ),
        (
            json!({"name": "E", "type": "sensor", "zoneId": zone, "deviceIds": [5]}),
            "deviceIds",
        ),
    ] {
        assert_eq!(
            post(&mut rig, &body.to_string()),
            (400, format!("Invalid field: {field}")),
            "{body}"
        );
    }
    let id = rig.equipment("E", "sensor", &zone, &[]);
    let put =
        |rig: &mut Rig, body: &str| rig.refused("PUT", &format!("/api/v1/equipments/{id}"), body);
    assert_eq!(put(&mut rig, "nope"), (400, "Invalid JSON".to_string()));
    for (body, field) in [
        (r#"{"name":5}"#, "name"),
        (r#"{"type":5}"#, "type"),
        (r#"{"zoneId":5}"#, "zoneId"),
        (r#"{"enabled":"yes"}"#, "enabled"),
        (r#"{"icon":5}"#, "icon"),
    ] {
        assert_eq!(
            put(&mut rig, body),
            (400, format!("Invalid field: {field}")),
            "{body}"
        );
    }
    assert_eq!(rig.home.equipments().list().len(), 1);
}

#[test]
fn unknown_fields_are_ignored_on_post_and_put_of_equipments() {
    let (mut rig, zone, th, _, _) = garage();
    let (status, body) = rig.post(
        "/api/v1/equipments",
        json!({
            "name": "E", "type": "sensor", "zoneId": zone, "deviceIds": [th],
            "energyProfile": {"class": "comfort"}, "requireConfirmation": true, "family": null
        }),
    );
    assert_eq!(status, 201);
    assert!(body.get("energyProfile").is_none() && body.get("requireConfirmation").is_none());
    let id = text(&body["id"]).to_string();
    let (status, body) = rig.put(
        &format!("/api/v1/equipments/{id}"),
        json!({"name": "F", "invertDirection": true, "timedCommand": null, "meteringParentId": null}),
    );
    assert_eq!(status, 200);
    assert_eq!(body["name"], "F");
    assert!(body.get("invertDirection").is_none());
}

#[test]
fn names_icons_and_descriptions_have_their_bounds() {
    let (mut rig, zone, _, _, _) = garage();
    let post = |rig: &mut Rig, extra: Value| {
        let mut body = json!({"name": "E", "type": "sensor", "zoneId": zone});
        for (k, v) in extra.as_object().unwrap() {
            body[k] = v.clone();
        }
        rig.refused("POST", "/api/v1/equipments", &body.to_string())
    };
    assert_eq!(
        post(&mut rig, json!({"name": ""})),
        (400, "Invalid name".to_string())
    );
    assert_eq!(
        post(&mut rig, json!({"name": "  "})),
        (400, "Invalid name".to_string())
    );
    assert_eq!(
        post(&mut rig, json!({"name": "a".repeat(65)})),
        (400, "Invalid name".to_string())
    );
    assert_eq!(
        post(&mut rig, json!({"icon": "i".repeat(33)})),
        (400, "Invalid icon".to_string())
    );
    assert_eq!(
        post(&mut rig, json!({"description": "d".repeat(501)})),
        (400, "Invalid description".to_string())
    );
    let (status, ok) = rig.post(
        "/api/v1/equipments",
        json!({"name": format!(" {} ", "a".repeat(64)), "type": "sensor", "zoneId": zone,
               "icon": "i".repeat(32), "description": "d".repeat(500)}),
    );
    assert_eq!(status, 201);
    assert_eq!(ok["name"], "a".repeat(64));
    let id = text(&ok["id"]).to_string();
    let put = |rig: &mut Rig, body: Value| {
        rig.refused(
            "PUT",
            &format!("/api/v1/equipments/{id}"),
            &body.to_string(),
        )
    };
    assert_eq!(
        put(&mut rig, json!({"name": "a".repeat(65)})),
        (400, "Invalid name".to_string())
    );
    assert_eq!(
        put(&mut rig, json!({"icon": "i".repeat(33)})),
        (400, "Invalid icon".to_string())
    );
    assert_eq!(
        put(&mut rig, json!({"description": "d".repeat(501)})),
        (400, "Invalid description".to_string())
    );
}

// ---- Stale readings (R7.1) ----

#[test]
fn stale_follows_sowels_streaming_windows_strictly() {
    let sensor = EquipmentType::Sensor;
    let day = 24 * HOUR;
    // (category, type, age in ms, stale)
    let table: &[(&str, ValueType, u64, bool)] = &[
        ("temperature", ValueType::Number, 64 * MINUTE, false),
        ("temperature", ValueType::Number, 65 * MINUTE, false),
        ("temperature", ValueType::Number, 65 * MINUTE + 1, true),
        ("temperature", ValueType::Number, 66 * MINUTE, true),
        ("humidity", ValueType::Number, 65 * MINUTE, false),
        ("humidity", ValueType::Number, 65 * MINUTE + 1, true),
        ("pressure", ValueType::Number, 65 * MINUTE, false),
        ("pressure", ValueType::Number, 66 * MINUTE, true),
        ("luminosity", ValueType::Number, 65 * MINUTE, false),
        ("luminosity", ValueType::Number, 66 * MINUTE, true),
        // Other streaming categories have their own window (Sowel's table).
        ("co2", ValueType::Number, 66 * MINUTE, true),
        ("setpoint", ValueType::Number, 60 * MINUTE, false),
        ("setpoint", ValueType::Number, 61 * MINUTE, true),
        ("ups_load", ValueType::Number, 21 * MINUTE, true),
        // Never stale: events, states, battery, generic.
        ("motion", ValueType::Boolean, 2 * day, false),
        ("motion", ValueType::Number, 2 * day, false),
        ("contact_door", ValueType::Boolean, 2 * day, false),
        ("contact_window", ValueType::Boolean, 2 * day, false),
        ("light_state", ValueType::Boolean, 2 * day, false),
        ("water_leak", ValueType::Boolean, 2 * day, false),
        ("smoke", ValueType::Boolean, 2 * day, false),
        ("battery", ValueType::Number, 2 * day, false),
        ("generic", ValueType::Number, 2 * day, false),
        ("shutter_position", ValueType::Number, 2 * day, false),
        // A boolean on a streaming category is a state, not a stream.
        ("temperature", ValueType::Boolean, 2 * day, false),
        ("humidity", ValueType::Boolean, 2 * day, false),
        // The electrical categories only on metering equipment: none of the six kinds.
        ("voltage", ValueType::Number, 2 * day, false),
        ("power", ValueType::Number, 2 * day, false),
        ("energy", ValueType::Number, 2 * day, false),
        ("current", ValueType::Number, 2 * day, false),
    ];
    for (category, value_type, age, expected) in table {
        assert_eq!(
            is_stale(category, *value_type, Some(T0), T0 + age, sensor),
            *expected,
            "{category} {value_type:?} {age} ms"
        );
    }
    for kind in [
        EquipmentType::LightOnoff,
        EquipmentType::LightDimmable,
        EquipmentType::Shutter,
        EquipmentType::Switch,
        EquipmentType::Thermostat,
        EquipmentType::Button,
    ] {
        assert!(!is_stale(
            "voltage",
            ValueType::Number,
            Some(T0),
            T0 + 2 * day,
            kind
        ));
        assert!(is_stale(
            "temperature",
            ValueType::Number,
            Some(T0),
            T0 + 2 * day,
            kind
        ));
    }
    // Never a reading without a date.
    assert!(!is_stale(
        "temperature",
        ValueType::Number,
        None,
        T0 + 2 * day,
        sensor
    ));
    // A clock set back is never stale.
    assert!(!is_stale(
        "temperature",
        ValueType::Number,
        Some(T0),
        T0 - day,
        sensor
    ));
    assert_eq!(stale_window_ms("temperature"), 65 * MINUTE);
    assert_eq!(stale_window_ms("humidity"), 65 * MINUTE);
    assert_eq!(stale_window_ms("pressure"), 65 * MINUTE);
    assert_eq!(stale_window_ms("luminosity"), 65 * MINUTE);
}

#[test]
fn the_stale_flag_of_a_binding_is_computed_at_the_time_of_the_request() {
    let mut rig = Rig::new();
    let probe = device_json(
        1,
        "Probe",
        true,
        &[
            ("temperature", "temperature", "number", json!(20), Some(T0)),
            ("occupancy", "motion", "boolean", json!(true), Some(T0)),
            ("battery", "battery", "number", json!(90), Some(T0)),
        ],
    );
    let id = device_json_id(&probe);
    rig.add_devices(vec![probe]);
    let zone = rig.zone("Z");
    let eq = rig.equipment("E", "sensor", &zone, &[&id]);
    let flags = |rig: &mut Rig| -> Vec<bool> {
        rig.get(&format!("/api/v1/equipments/{eq}")).1["dataBindings"]
            .as_array()
            .unwrap()
            .iter()
            .map(|b| b["stale"].as_bool().unwrap())
            .collect()
    };
    rig.now = T0 + 64 * MINUTE;
    assert_eq!(flags(&mut rig), [false, false, false]);
    rig.now = T0 + 66 * MINUTE;
    assert_eq!(
        flags(&mut rig),
        [true, false, false],
        "only the temperature"
    );
    rig.now = T0 + 2 * 24 * HOUR;
    assert_eq!(
        flags(&mut rig),
        [true, false, false],
        "motion 2 days old: not stale"
    );
}

// ---- Status (R7.2) ----

/// An equipment of this type on these devices, and its details at `now`.
fn status_of(rig: &mut Rig, kind: &str, devices: &[&str]) -> Value {
    let zone = text(&rig.get("/api/v1/zones").1[0]["id"]).to_string();
    let eq = rig.equipment("E", kind, &zone, devices);
    let body = rig.get(&format!("/api/v1/equipments/{eq}")).1;
    rig.delete(&format!("/api/v1/equipments/{eq}"));
    body
}

/// A device with one temperature; its key is `temperature` for the first one, `temperature<n>`
/// for the others (two devices bound to one equipment need distinct aliases).
fn probe(n: u8, name: &str, battery: bool, temperature_at: Option<u64>) -> Value {
    let key = if n == 1 {
        "temperature".to_string()
    } else {
        format!("temperature{n}")
    };
    device_json(
        n,
        name,
        battery,
        &[(&key, "temperature", "number", json!(20), temperature_at)],
    )
}

#[test]
fn status_no_binding_is_offline_with_an_empty_reason() {
    let mut rig = Rig::new();
    let body = status_of(&mut rig, "sensor", &[]);
    assert_eq!(body["status"], "offline");
    assert_eq!(
        body["statusReason"],
        json!({"offlineDevices": [], "staleBindings": [], "offlineSince": null})
    );
}

#[test]
fn status_a_button_is_online_whatever_its_silence() {
    let mut rig = Rig::new();
    let button = probe(1, "Remote", false, Some(T0));
    let id = device_json_id(&button);
    rig.add_devices(vec![button]);
    rig.now = T0 + 3 * 24 * HOUR; // offline and stale
    let body = status_of(&mut rig, "button", &[&id]);
    assert_eq!(body["status"], "online");
    assert!(body.get("statusReason").is_none());
    let same = status_of(&mut rig, "sensor", &[&id]);
    assert_eq!(same["status"], "offline", "any other type is judged");
    // No binding is still offline for a button (the rule comes first).
    assert_eq!(status_of(&mut rig, "button", &[])["status"], "offline");
}

#[test]
fn status_all_devices_offline_is_offline_with_the_reason() {
    let mut rig = Rig::new();
    let (a, b) = (
        probe(1, "Hall", false, Some(T0)),
        probe(2, "Cellar", false, Some(T0 + MINUTE)),
    );
    let (a_id, b_id) = (device_json_id(&a), device_json_id(&b));
    rig.add_devices(vec![a, b]);
    rig.now = T0 + HOUR; // mains devices silent for an hour: offline
    let body = status_of(&mut rig, "sensor", &[&a_id, &b_id]);
    assert_eq!(body["status"], "offline");
    assert_eq!(
        body["statusReason"]["offlineDevices"],
        json!(["Hall", "Cellar"])
    );
    assert_eq!(
        body["statusReason"]["staleBindings"],
        json!([]),
        "the temperature is 60 minutes old: within its window"
    );
    // The earliest lastSeen of the offline devices.
    assert_eq!(
        body["statusReason"]["offlineSince"],
        format_iso(T0).as_str()
    );
}

#[test]
fn status_one_of_two_devices_offline_is_degraded() {
    let mut rig = Rig::new();
    let mains = probe(1, "Mains", false, Some(T0));
    let battery = probe(2, "Battery", true, Some(T0));
    let (m_id, b_id) = (device_json_id(&mains), device_json_id(&battery));
    rig.add_devices(vec![mains, battery]);
    rig.now = T0 + 30 * MINUTE; // the mains device is offline after 10 minutes, the battery one not
    let body = status_of(&mut rig, "sensor", &[&m_id, &b_id]);
    assert_eq!(body["status"], "degraded");
    assert_eq!(body["statusReason"]["offlineDevices"], json!(["Mains"]));
    assert_eq!(
        body["statusReason"]["offlineSince"],
        format_iso(T0).as_str()
    );
}

#[test]
fn status_a_stale_binding_is_degraded_and_names_its_alias() {
    let mut rig = Rig::new();
    let battery = probe(1, "Battery", true, Some(T0));
    let id = device_json_id(&battery);
    rig.add_devices(vec![battery]);
    rig.now = T0 + 66 * MINUTE;
    let body = status_of(&mut rig, "sensor", &[&id]);
    assert_eq!(body["status"], "degraded");
    assert_eq!(
        body["statusReason"],
        json!({
            "offlineDevices": [],
            "staleBindings": ["temperature"],
            "offlineSince": format_iso(T0)
        })
    );
}

#[test]
fn status_a_stale_binding_on_an_offline_device_is_offline_and_lists_both() {
    let mut rig = Rig::new();
    let stale = probe(1, "Hall", false, Some(T0 - 10 * MINUTE));
    let id = device_json_id(&stale);
    rig.add_devices(vec![stale]);
    rig.now = T0 + 66 * MINUTE;
    let body = status_of(&mut rig, "sensor", &[&id]);
    assert_eq!(body["status"], "offline");
    assert_eq!(body["statusReason"]["offlineDevices"], json!(["Hall"]));
    assert_eq!(
        body["statusReason"]["staleBindings"],
        json!(["temperature"])
    );
    // The earliest of lastSeen (T0) and lastUpdated (T0 - 10 min).
    assert_eq!(
        body["statusReason"]["offlineSince"],
        format_iso(T0 - 10 * MINUTE).as_str()
    );
}

#[test]
fn status_a_device_seen_by_two_bindings_is_counted_once() {
    let mut rig = Rig::new();
    let two = device_json(
        1,
        "Two readings",
        false,
        &[
            ("temperature", "temperature", "number", json!(20), Some(T0)),
            ("humidity", "humidity", "number", json!(50), Some(T0)),
        ],
    );
    let id = device_json_id(&two);
    rig.add_devices(vec![two]);
    rig.now = T0 + 30 * MINUTE;
    let body = status_of(&mut rig, "sensor", &[&id]);
    assert_eq!(body["status"], "offline");
    assert_eq!(
        body["statusReason"]["offlineDevices"],
        json!(["Two readings"])
    );
}

#[test]
fn status_an_unknown_device_counts_as_online_and_a_healthy_equipment_is_online() {
    let mut rig = Rig::new();
    let mut never_seen = probe(1, "New", true, None);
    never_seen["lastSeen"] = Value::Null;
    never_seen["status"] = json!("unknown");
    let healthy = probe(2, "Fine", true, Some(T0));
    let (n_id, h_id) = (device_json_id(&never_seen), device_json_id(&healthy));
    rig.add_devices(vec![never_seen, healthy]);
    rig.now = T0 + HOUR;
    let unknown = status_of(&mut rig, "sensor", &[&n_id]);
    assert_eq!(
        rig.get(&format!("/api/v1/devices/{n_id}")).1["status"],
        "unknown"
    );
    assert_eq!(unknown["status"], "online");
    assert!(unknown.get("statusReason").is_none());
    let fine = status_of(&mut rig, "sensor", &[&h_id]);
    assert_eq!(fine["status"], "online");
    assert!(fine.get("statusReason").is_none(), "absent when online");
    // One unknown and one fine: online.
    assert_eq!(
        status_of(&mut rig, "sensor", &[&n_id, &h_id])["status"],
        "online"
    );
}

// ---- Consistency on load (R7.11) ----

#[test]
fn equipments_read_from_storage_lose_dead_bindings_and_move_to_the_root_if_their_zone_is_gone() {
    let (mut rig, zone, th, _, relay) = garage();
    let climate = rig.equipment("Climate", "sensor", &zone, &[&th]);
    rig.equipment("Light", "light_onoff", &zone, &[&relay]);
    let equipments = rig.home.equipments().list().to_vec();
    let zones = rig.home.zones().list().to_vec();
    let devices = rig.home.devices.snapshot();

    // The same files, but the relay is gone from devices.json and the garage from zones.json.
    let without_relay: Vec<_> = devices
        .iter()
        .filter(|d| d.device.id.to_string() != relay)
        .cloned()
        .collect();
    let without_garage: Vec<_> = zones
        .iter()
        .filter(|z| z.name != "Garage")
        .cloned()
        .collect();
    let home = Home::from_parts(
        Devices::from_list(without_relay),
        without_garage,
        equipments.clone(),
        T0,
    );
    let list = home.equipments().list();
    assert_eq!(list.len(), 2);
    assert!(list.iter().all(|e| e.equipment.zone_id.to_string() == ROOT));
    let light = list.iter().find(|e| &*e.equipment.name == "Light").unwrap();
    assert!(light.data_bindings.is_empty() && light.order_bindings.is_empty());
    let sensor = list
        .iter()
        .find(|e| &*e.equipment.name == "Climate")
        .unwrap();
    assert_eq!(sensor.data_bindings.len(), 4);
    assert!(home.changes().1, "the repair is to be written");
    let _ = climate;

    // Untouched files: nothing is repaired.
    let home = Home::from_parts(Devices::from_list(devices), zones, equipments, T0);
    assert_eq!(home.changes(), (false, false));
}

#[test]
fn a_file_with_duplicate_aliases_ids_or_too_many_bindings_is_made_to_fit() {
    let (mut rig, zone, th, _, _) = garage();
    rig.equipment("Climate", "sensor", &zone, &[&th]);
    let mut list = rig.home.equipments().list().to_vec();
    // The same alias twice, the same equipment twice.
    let dup = list[0].data_bindings[0].clone();
    list[0].data_bindings.push(dup);
    list.push(list[0].clone());
    let json = serde_json::to_string(&list).unwrap();
    let parsed = parse_list(&json).unwrap();
    let home = Home::from_parts(
        Devices::from_list(rig.home.devices.snapshot()),
        rig.home.zones().list().to_vec(),
        parsed,
        T0,
    );
    assert_eq!(home.equipments().list().len(), 1);
    assert_eq!(home.equipments().list()[0].data_bindings.len(), 4);
    assert!(home.changes().1);
}

#[test]
fn equipments_round_trip_through_their_json() {
    let (mut rig, zone, th, _, relay) = garage();
    rig.equipment("Climate", "sensor", &zone, &[&th]);
    rig.equipment("Light", "light_onoff", &zone, &[&relay]);
    let json = rig.home.take_equipments_json().unwrap();
    assert_eq!(rig.home.take_equipments_json(), None, "taken once");
    let parsed = parse_list(&json).unwrap();
    assert_eq!(parsed, rig.home.equipments().list());
    assert!(parse_list("{ not json").is_err());
    assert!(parse_list(r#"[{"id":"x"}]"#).is_err());
}

#[test]
fn the_other_methods_and_paths_of_equipments_are_refused() {
    let (mut rig, zone, th, _, _) = garage();
    let id = rig.equipment("Climate", "sensor", &zone, &[&th]);
    for (method, path) in [
        ("PATCH", "/api/v1/equipments".to_string()),
        ("PUT", "/api/v1/equipments".to_string()),
        ("DELETE", "/api/v1/equipments".to_string()),
        ("POST", format!("/api/v1/equipments/{id}")),
        ("GET", format!("/api/v1/equipments/{id}/data-bindings")),
        ("PUT", format!("/api/v1/equipments/{id}/order-bindings")),
        ("GET", format!("/api/v1/equipments/{id}/data-bindings/x")),
        ("POST", format!("/api/v1/equipments/{id}/order-bindings/x")),
    ] {
        assert_eq!(
            rig.refused(method, &path, "{}"),
            (405, "Method not allowed".to_string()),
            "{method} {path}"
        );
    }
    for path in [
        format!("/api/v1/equipments/{id}/orders"),
        format!("/api/v1/equipments/{id}/data-bindings/x/y"),
        "/api/v1/modes".to_string(),
    ] {
        assert_eq!(rig.refused("GET", &path, "").0, 404, "{path}");
    }
    // The foreign host and the big bodies are refused on the new routes as on the old ones.
    let big = "x".repeat(16 * 1024 + 1);
    assert_eq!(
        rig.refused("POST", "/api/v1/equipments", &big),
        (413, "Body too large".to_string())
    );
    assert_eq!(rig.refused("POST", "/api/v1/zones", &big).0, 413);
}
