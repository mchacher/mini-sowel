//! Equipments (spec 010, R3, R7.1 to R7.5, R7.8, R7.9, R7.11, R7.13, R7.14): creation from
//! devices with auto-binding by alias, bindings, the routes and their errors, stale readings and
//! the status of an equipment, bounds, validation.

mod support;

use serde_json::{Value, json};
use solite_core::devices::{Devices, ValueType, format_iso};
use solite_core::equipments::{EquipmentType, MAX_BINDINGS, is_stale, parse_list, stale_window_ms};
use solite_core::home::Home;
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
    for bad in ["lamp", "", "Sensor"] {
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
        rig.refused("PUT", &path, r#"{"type":"lamp"}"#),
        (400, "Invalid equipment type: lamp".to_string())
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
        json!({"name": "F", "bogus": true, "timedCommand": null, "meteringParentId": null}),
    );
    assert_eq!(status, 200);
    assert_eq!(body["name"], "F");
    assert!(body.get("bogus").is_none());
    assert!(body.get("timedCommand").is_none() && body.get("meteringParentId").is_none());
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
        // The electrical categories only on metering equipment: none of the types tested here.
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

// ---- Every type of Sowel, the optional fields (spec 027 R2) ----

/// A rig with a zone and one equipment of each of Sowel's 31 types; the zone's id.
fn every_type() -> (Rig, String) {
    let mut rig = Rig::new();
    let zone = rig.zone("Everything");
    for kind in EquipmentType::ALL {
        let name = kind.as_str();
        let (status, body) = rig.post(
            "/api/v1/equipments",
            json!({"name": name, "type": name, "zoneId": zone}),
        );
        assert_eq!(status, 201, "{name}: {body}");
        assert_eq!(body["type"], name, "the type as written");
    }
    (rig, zone)
}

#[test]
fn post_accepts_each_of_the_31_types_of_sowel() {
    assert_eq!(EquipmentType::ALL.len(), 31);
    let (rig, _) = every_type();
    assert_eq!(rig.home.equipments().list().len(), 31);
}

#[test]
fn the_list_is_filtered_by_each_of_the_31_types() {
    let (mut rig, _) = every_type();
    for kind in EquipmentType::ALL {
        let name = kind.as_str();
        let (status, body) = rig.get(&format!("/api/v1/equipments?type={name}"));
        assert_eq!(status, 200);
        let list = body.as_array().unwrap();
        assert_eq!(list.len(), 1, "{name}");
        assert_eq!(list[0]["type"], name);
    }
}

#[test]
fn a_door_equipment_given_as_a_light_is_refused_as_decision_d17_says() {
    let mut rig = Rig::new();
    let g = rig.garage();
    let door = rig.equipment("Door", "door", &g.zone, &[&g.relay_device]);
    let (status, body) = rig.instance(json!({"zone": g.zone, "lights": [door], "timeout": "20s"}));
    assert_eq!(status, 400, "{body}");
    assert_eq!(
        body["error"],
        "Invalid params: Light \"Door\" is type \"door\" but this recipe requires light_onoff"
    );
}

// The log lines of the tests of this file, to see a warning (the equipment's name is unique to
// its test).
struct Capture;
static LINES: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

impl log::Log for Capture {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }
    fn log(&self, record: &log::Record) {
        if let Ok(mut lines) = LINES.lock() {
            lines.push(format!("{}: {}", record.level(), record.args()));
        }
    }
    fn flush(&self) {}
}

fn capture_warnings() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let _ = log::set_logger(&Capture);
        log::set_max_level(log::LevelFilter::Warn);
    });
}

/// The warnings that name this text.
fn warnings_about(text: &str) -> Vec<String> {
    LINES
        .lock()
        .unwrap()
        .iter()
        .filter(|line| line.starts_with("WARN") && line.contains(text))
        .cloned()
        .collect()
}

/// The twelve fields as a file can hold them: the three objects with their own key order and
/// spacing.
const TWELVE: &str = r#","energyProfile":{"z": 1, "a":2},"requireConfirmation":true,"trackOrderOutcome":false,"meteringParentId":"parent-1","separateSupply":true,"localProduction":false,"family":"climate","plugInHybrid":true,"usualVehicleId":"car-1","solarProfile":{"planes":[ {"tiltDeg":30} ]},"invertDirection":true,"timedCommand":{"value":"ON","alias":"state"}"#;

/// Puts these fields (JSON text of `,"key":value` pairs) into the stored equipment of the rig, as
/// a file written by hand would, and reloads the home from it.
fn reload_with(rig: &mut Rig, fields: &str) {
    rig.home.mark_equipments_changed();
    let json = rig.home.take_equipments_json().unwrap();
    // One equipment: its object ends with `}` then the list's `]`.
    let json = json.trim_end().strip_suffix("}]").unwrap().to_string() + fields + "}]";
    let parsed = parse_list(&json).unwrap();
    rig.home = Home::from_parts(
        Devices::from_list(rig.home.devices.snapshot()),
        rig.home.zones().list().to_vec(),
        parsed,
        T0,
    );
}

/// A rig with one equipment of this type, named `name`, holding these stored fields; its path.
fn with_stored(kind: &str, name: &str, fields: &str) -> (Rig, String) {
    capture_warnings();
    let mut rig = Rig::new();
    let zone = rig.zone("Zone");
    let id = rig.equipment(name, kind, &zone, &[]);
    reload_with(&mut rig, fields);
    (rig, format!("/api/v1/equipments/{id}"))
}

#[test]
fn the_twelve_optional_fields_are_read_served_and_written_back_as_they_were() {
    let (mut rig, path) = with_stored("switch", "Twelve", TWELVE);
    let (status, body) = rig.get(&path);
    assert_eq!(status, 200);
    assert_eq!(body["energyProfile"], json!({"z": 1, "a": 2}));
    assert_eq!(body["requireConfirmation"], true);
    assert_eq!(body["trackOrderOutcome"], false);
    assert_eq!(body["meteringParentId"], "parent-1");
    assert_eq!(body["separateSupply"], true);
    assert_eq!(body["localProduction"], false);
    assert_eq!(body["family"], "climate");
    assert_eq!(body["plugInHybrid"], true);
    assert_eq!(body["usualVehicleId"], "car-1");
    assert_eq!(body["solarProfile"], json!({"planes": [{"tiltDeg": 30}]}));
    assert_eq!(body["invertDirection"], true);
    assert_eq!(
        body["timedCommand"],
        json!({"value": "ON", "alias": "state"})
    );
    // Written back: the same values, the objects with their own bytes.
    rig.home.mark_equipments_changed();
    let written = rig.home.take_equipments_json().unwrap();
    for piece in [
        r#""energyProfile":{"z": 1, "a":2}"#,
        r#""solarProfile":{"planes":[ {"tiltDeg":30} ]}"#,
        r#""timedCommand":{"value":"ON","alias":"state"}"#,
        r#""requireConfirmation":true"#,
        r#""trackOrderOutcome":false"#,
        r#""meteringParentId":"parent-1""#,
        r#""separateSupply":true"#,
        r#""localProduction":false"#,
        r#""family":"climate""#,
        r#""plugInHybrid":true"#,
        r#""usualVehicleId":"car-1""#,
        r#""invertDirection":true"#,
    ] {
        assert!(written.contains(piece), "{piece} in {written}");
    }
    let (_, list) = rig.get("/api/v1/equipments");
    assert_eq!(list[0]["family"], "climate", "the list shows them too");
}

#[test]
fn a_field_of_the_wrong_shape_or_past_1024_bytes_is_dropped_with_a_warning_and_the_rest_kept() {
    let huge = format!(r#","timedCommand":{{"alias":"{}"}}"#, "x".repeat(1100));
    let fields = format!(
        r#","requireConfirmation":"yes","family":"nope","meteringParentId":"","usualVehicleId":"{}","energyProfile":[1],"plugInHybrid":true,"separateSupply":null{huge}"#,
        "u".repeat(65)
    );
    let (mut rig, path) = with_stored("switch", "Shape check", &fields);
    let (_, body) = rig.get(&path);
    for gone in [
        "requireConfirmation",
        "family",
        "meteringParentId",
        "usualVehicleId",
        "energyProfile",
        "separateSupply",
        "timedCommand",
    ] {
        assert!(body.get(gone).is_none(), "{gone} is dropped: {body}");
    }
    assert_eq!(body["plugInHybrid"], true, "the rest is kept");
    assert_eq!(body["name"], "Shape check");
    let warnings = warnings_about("Shape check");
    for field in [
        "requireConfirmation",
        "family",
        "meteringParentId",
        "usualVehicleId",
        "energyProfile",
        "timedCommand",
    ] {
        assert!(
            warnings.iter().any(|w| w.contains(field)),
            "a warning names {field}: {warnings:?}"
        );
    }
}

#[test]
fn an_equipment_file_with_an_unknown_key_is_read_without_it() {
    let (mut rig, path) = with_stored(
        "sensor",
        "Unknown key",
        r#","somethingNew":{"a":1},"invertDirection":true"#,
    );
    let (status, body) = rig.get(&path);
    assert_eq!(status, 200);
    assert_eq!(body["invertDirection"], true);
    assert!(body.get("somethingNew").is_none());
}

/// Extras that are one timed command whose alias has `alias_chars` characters: that many bytes
/// plus 29 of JSON around them.
fn timed_command(alias_chars: usize) -> Box<solite_core::equipments::Extras> {
    use solite_core::equipments::{Extras, Field, Piece, RawObject};
    let mut extras = Extras::default();
    let object = RawObject::new(&format!(r#"{{"alias":"{}"}}"#, "t".repeat(alias_chars))).unwrap();
    extras.set(Field::TimedCommand, Some(Piece::Object(object)));
    assert_eq!(extras.json_len(), alias_chars + 29);
    Box::new(extras)
}

/// Reloads the home with these fields (or none) on its equipments, in the file's order, as a file
/// would give them.
fn reload_extras(rig: &mut Rig, extras: Vec<Option<Box<solite_core::equipments::Extras>>>) {
    rig.home.mark_equipments_changed();
    let json = rig.home.take_equipments_json().unwrap();
    let mut parsed = parse_list(&json).unwrap();
    assert_eq!(parsed.len(), extras.len());
    for (item, extras) in parsed.iter_mut().zip(extras) {
        item.equipment.extras = extras;
    }
    rig.home = Home::from_parts(
        Devices::from_list(rig.home.devices.snapshot()),
        rig.home.zones().list().to_vec(),
        parsed,
        T0,
    );
}

#[test]
fn equipments_whose_fields_total_past_8192_bytes_lose_the_fields_past_the_total() {
    capture_warnings();
    let mut rig = Rig::new();
    let zone = rig.zone("Zone");
    for i in 0..12 {
        rig.equipment(&format!("Total {i:02}"), "switch", &zone, &[]);
    }
    // 909 bytes each: nine fit under 8192 (8181), the tenth would make 9090.
    reload_extras(
        &mut rig,
        (0..12).map(|_| Some(timed_command(880))).collect(),
    );
    let kept: Vec<bool> = rig
        .home
        .equipments()
        .list()
        .iter()
        .map(|e| e.equipment.extras.is_some())
        .collect();
    assert_eq!(
        kept[..9],
        [true; 9],
        "the first nine, in the file's order: {kept:?}"
    );
    assert_eq!(kept[9..], [false; 3], "the rest lose the field");
    assert_eq!(rig.home.equipments().extras_bytes(None), 9 * 909);
    for name in ["Total 09", "Total 10", "Total 11"] {
        assert!(!warnings_about(name).is_empty(), "a warning names {name}");
    }
    assert!(warnings_about("Total 08").is_empty());
}

#[test]
fn a_put_sets_the_booleans_and_clears_track_order_outcome_with_null() {
    let (mut rig, path) = with_stored("shutter", "Booleans", r#","trackOrderOutcome":true"#);
    let (status, body) = rig.put(
        &path,
        json!({"requireConfirmation": true, "invertDirection": true, "plugInHybrid": false, "trackOrderOutcome": null}),
    );
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["requireConfirmation"], true);
    assert_eq!(body["invertDirection"], true);
    assert_eq!(body["plugInHybrid"], false);
    assert!(
        body.get("trackOrderOutcome").is_none(),
        "null is not stored"
    );
    let (_, again) = rig.get(&path);
    assert_eq!(again["invertDirection"], true);
    assert!(rig.home.changes().1, "written");
}

#[test]
fn a_put_with_a_field_of_the_wrong_shape_is_refused_and_changes_nothing() {
    let (mut rig, path) = with_stored("sensor", "Wrong shape", "");
    rig.home.take_equipments_json();
    for (body, field) in [
        (r#"{"requireConfirmation":"yes"}"#, "requireConfirmation"),
        (r#"{"invertDirection":null}"#, "invertDirection"),
        (r#"{"plugInHybrid":1}"#, "plugInHybrid"),
        (r#"{"trackOrderOutcome":"no"}"#, "trackOrderOutcome"),
        (r#"{"family":"kitchen"}"#, "family"),
        (r#"{"energyProfile":5}"#, "energyProfile"),
        (r#"{"meteringParentId":""}"#, "meteringParentId"),
    ] {
        assert_eq!(
            rig.refused(
                "PUT",
                &path,
                &format!(r#"{{"name":"Changed",{}"#, &body[1..])
            ),
            (400, format!("Invalid {field}")),
            "{body}"
        );
    }
    let (_, now) = rig.get(&path);
    assert_eq!(now["name"], "Wrong shape");
    assert_eq!(rig.home.changes(), (false, false));
}

#[test]
fn a_family_is_kept_on_a_switch_and_goes_with_null() {
    let (mut rig, path) = with_stored("switch", "Plug", "");
    let (status, body) = rig.put(&path, json!({"family": "climate"}));
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["family"], "climate");
    let (status, body) = rig.put(&path, json!({"family": null}));
    assert_eq!(status, 200);
    assert!(body.get("family").is_none());
}

#[test]
fn a_family_on_another_type_is_refused_with_sowels_code_and_text() {
    let (mut rig, path) = with_stored("light_onoff", "Lamp", "");
    rig.home.take_equipments_json();
    let (status, body) = rig.put(&path, json!({"family": "climate", "name": "Other"}));
    assert_eq!(status, 400);
    assert_eq!(
        body,
        json!({
            "error": "FamilyNotApplicable",
            "message": "Lamp is a light_onoff — only a switch can be given a family"
        })
    );
    let (_, now) = rig.get(&path);
    assert_eq!(now["name"], "Lamp");
    assert!(now.get("family").is_none());
    assert_eq!(rig.home.changes(), (false, false));
    // The family of the type it becomes counts: a switch made a light in the same request.
    let (mut rig, path) = with_stored("switch", "Plug", "");
    let (status, body) = rig.put(&path, json!({"family": "climate", "type": "heater"}));
    assert_eq!(status, 400);
    assert_eq!(body["error"], "FamilyNotApplicable");
    assert_eq!(
        body["message"],
        "Plug is a heater — only a switch can be given a family"
    );
}

#[test]
fn a_switch_made_something_else_loses_its_family() {
    let (mut rig, path) = with_stored("switch", "Plug", r#","family":"climate""#);
    let (status, body) = rig.put(&path, json!({"type": "light_onoff"}));
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["type"], "light_onoff");
    assert!(body.get("family").is_none());
    let (_, stored) = rig.get(&path);
    assert!(stored.get("family").is_none());
}

#[test]
fn a_type_change_that_leaves_separate_supply_outside_a_submeter_is_refused() {
    let (mut rig, path) = with_stored("energy_meter", "Meter", r#","separateSupply":true"#);
    let (status, body) = rig.put(&path, json!({"type": "solar_panel"}));
    assert_eq!(status, 400);
    assert_eq!(body["error"], "SeparateSupplyNotApplicable");
    assert_eq!(
        body["message"],
        "Meter is the house total or a production meter — the reconciliation reference cannot be outside itself"
    );
    let (_, now) = rig.get(&path);
    assert_eq!(now["type"], "energy_meter", "nothing changed");
    // Given back as stored it is no change, and the type that is not a total passes.
    let (status, _) = rig.put(&path, json!({"separateSupply": true, "type": "appliance"}));
    assert_eq!(status, 200);
}

#[test]
fn a_type_change_that_leaves_local_production_on_a_total_or_a_separate_supply_is_refused() {
    let (mut rig, path) = with_stored("energy_meter", "Clamp", r#","localProduction":true"#);
    let (status, body) = rig.put(&path, json!({"type": "main_energy_meter"}));
    assert_eq!(status, 400);
    assert_eq!(body["error"], "LocalProductionNotApplicable");
    assert_eq!(
        body["message"],
        "Clamp is the house total or a production meter — it cannot also carry local production"
    );
    let (mut rig2, path2) = with_stored(
        "energy_meter",
        "Remote",
        r#","localProduction":true,"separateSupply":true"#,
    );
    let (status, body) = rig2.put(&path2, json!({"separateSupply": true}));
    assert_eq!(status, 400);
    assert_eq!(body["error"], "LocalProductionNotApplicable");
    assert_eq!(
        body["message"],
        "Remote is fed by a separate supply — its production never reaches the house"
    );
    // The same request on the equipment without the other flag passes.
    let (status, _) = rig.put(&path, json!({"type": "appliance"}));
    assert_eq!(status, 200);
}

#[test]
fn the_seven_fields_solite_cannot_check_are_refused_unless_they_are_given_as_stored() {
    let (mut rig, path) = with_stored("energy_meter", "Seven", TWELVE);
    rig.home.take_equipments_json();
    // A new value, `null` with one stored, and a new flag.
    for body in [
        r#"{"energyProfile":null}"#,
        r#"{"energyProfile":{"a":2,"z":1}}"#,
        r#"{"solarProfile":{"planes":[]}}"#,
        r#"{"timedCommand":null}"#,
        r#"{"meteringParentId":"parent-2"}"#,
        r#"{"usualVehicleId":null}"#,
        r#"{"separateSupply":false}"#,
        r#"{"localProduction":true}"#,
    ] {
        let name = body
            .trim_start_matches("{\"")
            .split('"')
            .next()
            .unwrap()
            .to_string();
        assert_eq!(
            rig.refused(
                "PUT",
                &path,
                &format!(r#"{{"name":"Changed",{}"#, &body[1..])
            ),
            (400, format!("Not supported: {name}")),
            "{body}"
        );
    }
    let (_, now) = rig.get(&path);
    assert_eq!(now["name"], "Seven", "nothing changed");
    assert_eq!(rig.home.changes(), (false, false));
}

#[test]
fn an_object_field_given_back_with_its_stored_bytes_and_a_null_with_nothing_stored_change_nothing()
{
    let (mut rig, path) = with_stored(
        "energy_meter",
        "Same",
        r#","energyProfile":{"z": 1, "a":2}"#,
    );
    let (status, body) = rig.call(
        "PUT",
        &path,
        r#"{"energyProfile":{"z": 1, "a":2},"solarProfile":null,"separateSupply":false,"name":"Same 2"}"#,
    );
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["name"], "Same 2");
    assert_eq!(body["energyProfile"], json!({"z": 1, "a": 2}));
    assert!(body.get("solarProfile").is_none() && body.get("separateSupply").is_none());
    rig.home.mark_equipments_changed();
    let written = rig.home.take_equipments_json().unwrap();
    assert!(
        written.contains(r#""energyProfile":{"z": 1, "a":2}"#),
        "{written}"
    );
}

#[test]
fn a_put_that_makes_the_fields_pass_a_bound_is_refused() {
    const TOO_LARGE: &str = "Equipment fields too large (1024 bytes each, 8192 in all)";
    // One equipment: 1016 bytes of fields, and a family (19 more) would pass 1024.
    let big = format!(r#","energyProfile":{{"a":"{}"}}"#, "e".repeat(990));
    let (mut rig, path) = with_stored("switch", "Large", &big);
    rig.home.take_equipments_json();
    assert_eq!(
        rig.refused("PUT", &path, r#"{"family":"climate"}"#),
        (400, TOO_LARGE.to_string())
    );
    let (_, now) = rig.get(&path);
    assert!(now.get("family").is_none(), "nothing changed");
    assert_eq!(rig.home.changes(), (false, false));

    // All equipments: nine of 909 bytes leave 11 of the 8192, and a family needs 20.
    let (mut rig, path) = with_stored("switch", "Last", "");
    let zone = rig.zone("Other");
    for i in 0..9 {
        rig.equipment(&format!("Filler {i}"), "switch", &zone, &[]);
    }
    let mut extras: Vec<_> = vec![None];
    extras.extend((0..9).map(|_| Some(timed_command(880))));
    reload_extras(&mut rig, extras);
    assert_eq!(rig.home.equipments().extras_bytes(None), 9 * 909);
    assert_eq!(
        rig.refused("PUT", &path, r#"{"family":"climate"}"#),
        (400, TOO_LARGE.to_string())
    );
}

#[test]
fn the_per_equipment_bound_is_1024_bytes_of_json_with_the_keys() {
    use solite_core::equipments::{EquipmentFamily, Extras, Field, Piece};
    let mut extras = Extras::default();
    assert_eq!(extras.json_len(), 0, "none costs nothing");
    extras.set(Field::Family, Some(Piece::Family(EquipmentFamily::Climate)));
    assert_eq!(
        extras.json_len(),
        serde_json::to_string(&extras).unwrap().len(),
        "the bytes are the JSON's, keys, braces and commas included"
    );
    assert_eq!(extras.json_len(), r#"{"family":"climate"}"#.len());
}

#[test]
fn post_ignores_the_optional_fields() {
    let (mut rig, zone, _, _, _) = garage();
    let (status, body) = rig.post(
        "/api/v1/equipments",
        json!({"name": "Plain", "type": "switch", "zoneId": zone, "requireConfirmation": true,
               "family": "climate", "energyProfile": {"class": "comfort"}, "invertDirection": true}),
    );
    assert_eq!(status, 201, "{body}");
    for field in [
        "requireConfirmation",
        "family",
        "energyProfile",
        "invertDirection",
    ] {
        assert!(body.get(field).is_none(), "{field}");
    }
}

#[test]
fn the_events_carry_the_fields_as_stored() {
    let (mut rig, path) = with_stored("switch", "Evented", r#","requireConfirmation":true"#);
    let hub = rig.home.live();
    let c = hub.connect().expect("a free slot");
    assert!(hub.subscribe(
        c,
        &json!({"type": "subscribe", "topics": ["equipments"]}).to_string()
    ));
    let (status, _) = rig.put(&path, json!({"family": "water"}));
    assert_eq!(status, 200);
    let events: Value = serde_json::from_str(&hub.take_batch(c).unwrap()).unwrap();
    let updated = &events[0];
    assert_eq!(updated["type"], "equipment.updated");
    assert_eq!(updated["equipment"]["requireConfirmation"], true);
    assert_eq!(updated["equipment"]["family"], "water");
    // The same for an equipment created and removed: created has none (create takes none).
    let zone = rig.zone("Z2");
    let created = rig.equipment("Evented 2", "switch", &zone, &[]);
    let (status, _) = rig.delete(&format!("/api/v1/equipments/{created}"));
    assert_eq!(status, 204);
    let events: Value = serde_json::from_str(&hub.take_batch(c).unwrap()).unwrap();
    let events = events.as_array().unwrap();
    let created_event = events
        .iter()
        .find(|e| e["type"] == "equipment.created")
        .unwrap();
    assert!(
        created_event["equipment"]
            .get("requireConfirmation")
            .is_none()
    );
    let removed = events
        .iter()
        .find(|e| e["type"] == "equipment.removed")
        .unwrap();
    assert_eq!(removed["equipmentName"], "Evented 2");
}

#[test]
fn a_light_color_counts_as_a_light_and_a_door_with_a_light_state_does_not() {
    let mut rig = Rig::new();
    let zone = rig.zone("Hall");
    let state = |n: u8, name: &str| {
        device_json(
            n,
            name,
            false,
            &[("state", "light_state", "boolean", json!(true), Some(T0))],
        )
    };
    let bulb = state(1, "Bulb");
    let relay = state(2, "Relay");
    let (bulb_id, relay_id) = (device_json_id(&bulb), device_json_id(&relay));
    rig.add_devices(vec![bulb, relay]);
    rig.equipment("Colour", "light_color", &zone, &[&bulb_id]);
    let (_, all) = rig.get("/api/v1/zones/aggregation");
    assert_eq!(all[zone.as_str()]["lightsOn"], 1);
    assert_eq!(all[zone.as_str()]["lightsTotal"], 1);
    rig.equipment("Gate", "door", &zone, &[&relay_id]);
    let (_, all) = rig.get("/api/v1/zones/aggregation");
    assert_eq!(all[zone.as_str()]["lightsTotal"], 1, "a door is no light");
    assert_eq!(all[zone.as_str()]["lightsOn"], 1);
}

// ---- The lists of values the contract check reads (spec 027 R1.3) ----

/// Each value of `all`, as JSON, is the text `as_str` gives, and the texts are distinct.
fn assert_listed<T: serde::Serialize + Copy>(all: &[T], as_str: impl Fn(T) -> &'static str) {
    let mut seen = Vec::new();
    for value in all {
        let wire = serde_json::to_value(value).unwrap();
        assert_eq!(wire, as_str(*value), "serde and `as_str` agree");
        assert!(!seen.contains(&wire), "{wire} twice");
        seen.push(wire);
    }
}

#[test]
fn the_all_lists_hold_every_value_with_its_json_string() {
    use solite_core::devices::{DeviceStatus, PowerSource};
    use solite_core::equipments::EquipmentFamily;
    assert_eq!(EquipmentType::ALL.len(), 31);
    assert_listed(&EquipmentType::ALL, EquipmentType::as_str);
    for kind in EquipmentType::ALL {
        assert_eq!(EquipmentType::parse(kind.as_str()), Some(kind));
    }
    assert_eq!(EquipmentFamily::ALL.len(), 6);
    assert_listed(&EquipmentFamily::ALL, EquipmentFamily::as_str);
    assert_listed(&DeviceStatus::ALL, DeviceStatus::as_str);
    assert_listed(&PowerSource::ALL, PowerSource::as_str);
    assert_listed(&ValueType::ALL, ValueType::as_str);
}

#[test]
fn the_catalog_lists_every_data_and_order_spec() {
    use solite_core::devices::{all_data, all_orders, data_for_cluster, orders_for_cluster};
    let data: Vec<&str> = all_data().map(|d| d.key).collect();
    for cluster in [
        0x0001, 0x0006, 0x0008, 0x0400, 0x0402, 0x0403, 0x0405, 0x0406,
    ] {
        for spec in data_for_cluster(cluster) {
            assert!(data.contains(&spec.key), "{}", spec.key);
        }
    }
    // The zone types' and the zone flags' too.
    for key in ["contact", "water_leak", "smoke", "tamper", "battery_low"] {
        assert!(data.contains(&key), "{key}");
    }
    let orders: Vec<&str> = all_orders().map(|o| o.key).collect();
    assert_eq!(
        orders.len(),
        orders_for_cluster(0x0006).len() + orders_for_cluster(0x0008).len()
    );
    assert!(orders.contains(&"state") && orders.contains(&"brightness"));
}
