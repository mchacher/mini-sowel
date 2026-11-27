//! The requests the interface's Topology and Equipments pages send, in their order (spec 018),
//! then the equipment form's sequence of spec 019 (create, then one binding request at a time):
//! the API is unchanged, these tests pin the contract the screens rely on.

mod support;

use serde_json::{Value, json};
use solite_core::api::{Reach, Reply, Request, handle_stream};
use support::*;

const UNKNOWN: &str = "11111111-1111-4111-8111-111111111111";

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

fn zone(rig: &mut Rig, name: &str, parent: &str) -> String {
    let (status, body) = rig.post("/api/v1/zones", json!({"name": name, "parentId": parent}));
    assert_eq!(status, 201, "{body}");
    text(&body["id"]).to_string()
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
fn zones_are_made_moved_renamed_and_refused_as_the_topology_page_does() {
    let mut rig = Rig::new();
    let (status, rdc) = rig.post(
        "/api/v1/zones",
        json!({"name": "Rez-de-chaussée", "parentId": ROOT}),
    );
    assert_eq!(status, 201);
    assert_eq!(rdc["parentId"], ROOT);
    let rdc = text(&rdc["id"]).to_string();
    let (status, garage) = rig.post(
        "/api/v1/zones",
        json!({"name": "Garage", "parentId": rdc, "description": "Box"}),
    );
    assert_eq!(status, 201);
    let garage = text(&garage["id"]).to_string();
    let (_, tree) = rig.get("/api/v1/zones");
    assert_eq!(tree[0]["name"], "Maison");
    assert_eq!(tree[0]["children"][0]["name"], "Rez-de-chaussée");
    assert_eq!(tree[0]["children"][0]["children"][0]["name"], "Garage");

    // A loop is refused: the RDC under its own child.
    assert_eq!(
        rig.refused(
            "PUT",
            &format!("/api/v1/zones/{rdc}"),
            &json!({"name": "RDC", "parentId": garage}).to_string()
        ),
        (
            400,
            "Moving this zone would create a circular reference".to_string()
        )
    );

    // Move and rename.
    let (status, body) = rig.put(
        &format!("/api/v1/zones/{garage}"),
        json!({"name": "Garage 2", "parentId": ROOT, "description": ""}),
    );
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["name"], "Garage 2");
    assert_eq!(body["parentId"], ROOT);
    let (_, tree) = rig.get("/api/v1/zones");
    let children: Vec<&str> = tree[0]["children"]
        .as_array()
        .unwrap()
        .iter()
        .map(|z| text(&z["name"]))
        .collect();
    assert!(children.contains(&"Garage 2") && children.contains(&"Rez-de-chaussée"));
}

#[test]
fn the_root_is_renamed_with_no_parent_id_and_stays_the_root() {
    let mut rig = Rig::new();
    let (status, body) = rig.put(
        &format!("/api/v1/zones/{ROOT}"),
        json!({"name": "Ma maison"}),
    );
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["name"], "Ma maison");
    assert_eq!(body["parentId"], Value::Null);
    assert_eq!(rig.get("/api/v1/zones").1[0]["id"], ROOT);
}

#[test]
fn the_name_and_description_bounds_are_the_forms_maxlength() {
    let mut rig = Rig::new();
    let (status, _) = rig.post("/api/v1/zones", json!({"name": "n".repeat(64)}));
    assert_eq!(status, 201);
    assert_eq!(
        rig.refused(
            "POST",
            "/api/v1/zones",
            &json!({"name": "n".repeat(65)}).to_string()
        ),
        (400, "Invalid name".to_string())
    );
    let (status, _) = rig.post(
        "/api/v1/zones",
        json!({"name": "d1", "description": "d".repeat(500)}),
    );
    assert_eq!(status, 201);
    assert_eq!(
        rig.refused(
            "POST",
            "/api/v1/zones",
            &json!({"name": "d2", "description": "d".repeat(501)}).to_string()
        ),
        (400, "Invalid description".to_string())
    );
}

#[test]
fn equipments_are_made_from_devices_with_the_keys_as_aliases() {
    let mut rig = Rig::new();
    let th = rig.sensor(1);
    let pir = rig.motion_sensor(2);
    let relay = rig.relay(3);
    let garage = zone(&mut rig, "Garage", ROOT);
    let (status, light) = rig.post(
        "/api/v1/equipments",
        json!({"name": "Lumière garage", "type": "light_onoff", "zoneId": garage, "deviceIds": [relay]}),
    );
    assert_eq!(status, 201, "{light}");
    assert_eq!(aliases(&light["dataBindings"]), ["state"]);
    assert_eq!(light["dataBindings"][0]["category"], "light_state");
    assert_eq!(aliases(&light["orderBindings"]), ["state"]);
    let (status, sensor) = rig.post(
        "/api/v1/equipments",
        json!({"name": "Capteurs garage", "type": "sensor", "zoneId": garage, "deviceIds": [th, pir]}),
    );
    assert_eq!(status, 201, "{sensor}");
    let names = aliases(&sensor["dataBindings"]);
    for key in ["temperature", "humidity", "occupancy"] {
        assert!(names.contains(&key), "{key} in {names:?}");
    }
    assert_eq!(names.iter().filter(|a| **a == "battery").count(), 1);
    rig.measure(&temperature(1, 19.6));
    let (_, aggregation) = rig.get("/api/v1/zones/aggregation");
    let garage_agg = &aggregation[garage.as_str()];
    assert_eq!(garage_agg["lightsTotal"], 1);
    assert_eq!(garage_agg["motionSensors"], 1);
    assert_eq!(garage_agg["temperature"], json!(19.6));
}

#[test]
fn an_equipment_with_no_device_or_a_deleted_one_has_no_binding() {
    let mut rig = Rig::new();
    let relay = rig.relay(3);
    let garage = zone(&mut rig, "Garage", ROOT);
    let (status, none) = rig.post(
        "/api/v1/equipments",
        json!({"name": "A", "type": "switch", "zoneId": garage}),
    );
    assert_eq!(status, 201);
    assert_eq!(none["status"], "offline");
    assert_eq!(none["dataBindings"], json!([]));
    let (status, empty) = rig.post(
        "/api/v1/equipments",
        json!({"name": "B", "type": "switch", "zoneId": garage, "deviceIds": []}),
    );
    assert_eq!(status, 201);
    assert_eq!(empty["dataBindings"], json!([]));
    assert_eq!(rig.delete(&format!("/api/v1/devices/{relay}")).0, 204);
    let (status, gone) = rig.post(
        "/api/v1/equipments",
        json!({"name": "C", "type": "switch", "zoneId": garage, "deviceIds": [relay]}),
    );
    assert_eq!(status, 201);
    assert_eq!(gone["dataBindings"], json!([]));
    assert_eq!(gone["orderBindings"], json!([]));
}

#[test]
fn bindings_stop_at_8_of_each_kind_and_65_device_ids_are_refused() {
    let mut rig = Rig::new();
    // A device holds at most 4 orders: five devices give 20 data keys and 20 order keys.
    let mut ids = Vec::new();
    for n in 1..=5u8 {
        let mut device = many_data(n, &format!("D{n}"), usize::from(n) * 4, 4);
        let id = device["id"].clone();
        device["orders"] = Value::Array(
            (0..4)
                .map(|i| {
                    json!({
                        "id": format!("ee{n:02x}{i:02x}00-0000-4000-8000-000000000000"),
                        "deviceId": id,
                        "key": format!("o{}", usize::from(n) * 4 + i),
                        "type": "boolean",
                    })
                })
                .collect(),
        );
        ids.push(device_json_id(&device));
        rig.add_devices(vec![device]);
    }
    let garage = zone(&mut rig, "Z", ROOT);
    let (status, body) = rig.post(
        "/api/v1/equipments",
        json!({"name": "Big", "type": "sensor", "zoneId": garage, "deviceIds": ids}),
    );
    assert_eq!(status, 201, "{body}");
    assert_eq!(body["dataBindings"].as_array().unwrap().len(), 8);
    assert_eq!(body["orderBindings"].as_array().unwrap().len(), 8);
    let many: Vec<String> = (0..65).map(|_| UNKNOWN.to_string()).collect();
    assert_eq!(
        rig.refused(
            "POST",
            "/api/v1/equipments",
            &json!({"name": "X", "type": "sensor", "zoneId": garage, "deviceIds": many})
                .to_string()
        ),
        (400, "Invalid field: deviceIds".to_string())
    );
}

#[test]
fn an_equipment_is_renamed_and_moved_with_its_bindings_unchanged_then_deleted() {
    let mut rig = Rig::new();
    let relay = rig.relay(3);
    let rdc = zone(&mut rig, "RDC", ROOT);
    let garage = zone(&mut rig, "Garage", &rdc);
    let light = rig.equipment("Lumière garage", "light_onoff", &garage, &[&relay]);
    let path = format!("/api/v1/equipments/{light}");
    let before = rig.get(&path).1["dataBindings"].clone();
    let (status, body) = rig.put(&path, json!({"name": "Plafonnier", "zoneId": ROOT}));
    assert_eq!(status, 200, "{body}");
    let (_, after) = rig.get(&path);
    assert_eq!(after["name"], "Plafonnier");
    assert_eq!(after["zoneId"], ROOT);
    assert_eq!(after["dataBindings"], before);
    let (status, _) = rig.put(&path, json!({"zoneId": garage}));
    assert_eq!(status, 200);

    // Refusals of a zone delete.
    assert_eq!(
        rig.refused("DELETE", &format!("/api/v1/zones/{rdc}"), ""),
        (
            400,
            "Cannot delete zone with 1 child zone. Remove or move them first.".to_string()
        )
    );
    assert_eq!(
        rig.refused("DELETE", &format!("/api/v1/zones/{garage}"), ""),
        (
            400,
            "Cannot delete zone with 1 equipment. Remove or move them first.".to_string()
        )
    );
    // Delete the equipment, then the zone; a device placed there goes back to no room.
    assert_eq!(
        rig.put(
            &format!("/api/v1/devices/{relay}"),
            json!({"zoneId": garage})
        )
        .0,
        200
    );
    assert_eq!(rig.delete(&path).0, 204);
    assert_eq!(rig.delete(&format!("/api/v1/zones/{garage}")).0, 204);
    assert_eq!(
        rig.get(&format!("/api/v1/devices/{relay}")).1["zoneId"],
        Value::Null
    );
    // Gone: 404 on a second delete and on a put.
    assert_eq!(
        rig.refused("DELETE", &path, ""),
        (404, "Equipment not found".to_string())
    );
    assert_eq!(
        rig.refused("PUT", &path, r#"{"name":"x"}"#),
        (404, "Equipment not found".to_string())
    );
}

#[test]
fn the_devices_list_carries_the_data_the_selector_filters_on() {
    let mut rig = Rig::new();
    let th = rig.sensor(1);
    rig.relay(3);
    let (status, list) = rig.get("/api/v1/devices");
    assert_eq!(status, 200);
    let list = list.as_array().unwrap();
    assert_eq!(list.len(), 2);
    for device in list {
        for data in device["data"].as_array().unwrap() {
            assert!(data["key"].is_string() && data["category"].is_string());
        }
    }
    let sensor = list.iter().find(|d| d["id"] == th.as_str()).unwrap();
    let categories: Vec<&str> = sensor["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| text(&d["category"]))
        .collect();
    assert!(categories.contains(&"temperature") && categories.contains(&"humidity"));
}

// ---- The equipment form of spec 019: create, then one binding request at a time ----

/// The id of data (or order) number `i` of a device, read as the form does (`GET /devices/:id`).
fn part_id(rig: &mut Rig, device: &str, list: &str, i: usize) -> String {
    let (status, body) = rig.get(&format!("/api/v1/devices/{device}"));
    assert_eq!(status, 200, "{body}");
    text(&body[list][i]["id"]).to_string()
}

fn data_of(rig: &mut Rig, device: &str, key: &str) -> String {
    let (_, body) = rig.get(&format!("/api/v1/devices/{device}"));
    let found = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["key"] == key)
        .unwrap_or_else(|| panic!("no data {key}"));
    text(&found["id"]).to_string()
}

fn create(rig: &mut Rig, name: &str, kind: &str, zone: &str) -> String {
    let (status, body) = rig.post(
        "/api/v1/equipments",
        json!({"name": name, "type": kind, "zoneId": zone}),
    );
    assert_eq!(status, 201, "{body}");
    assert_eq!(body["dataBindings"], json!([]));
    text(&body["id"]).to_string()
}

fn bind_data(rig: &mut Rig, equipment: &str, data: &str, alias: &str) -> (u16, Value) {
    rig.post(
        &format!("/api/v1/equipments/{equipment}/data-bindings"),
        json!({"deviceDataId": data, "alias": alias}),
    )
}

fn bind_order(rig: &mut Rig, equipment: &str, order: &str, alias: &str) -> (u16, Value) {
    rig.post(
        &format!("/api/v1/equipments/{equipment}/order-bindings"),
        json!({"deviceOrderId": order, "alias": alias}),
    )
}

#[test]
fn a_light_is_made_by_the_forms_sequence_and_switches_the_relay() {
    let mut rig = Rig::new();
    let relay = rig.relay(3);
    let garage = zone(&mut rig, "Garage", ROOT);
    let light = create(&mut rig, "Lumière garage", "light_onoff", &garage);
    let data = data_of(&mut rig, &relay, "state");
    let order = part_id(&mut rig, &relay, "orders", 0);
    assert_eq!(bind_data(&mut rig, &light, &data, "state").0, 201);
    assert_eq!(bind_order(&mut rig, &light, &order, "state").0, 201);
    let (_, equipment) = rig.get(&format!("/api/v1/equipments/{light}"));
    assert_eq!(aliases(&equipment["dataBindings"]), ["state"]);
    assert_eq!(aliases(&equipment["orderBindings"]), ["state"]);
    assert_eq!(equipment["dataBindings"][0]["deviceId"], relay.as_str());
    // The status follows the relay.
    assert_eq!(equipment["status"], "online");
    rig.measure(&state(3, true));
    let (_, aggregation) = rig.get("/api/v1/zones/aggregation");
    assert_eq!(aggregation[garage.as_str()]["lightsOn"], 1);
    // The order is accepted and queued for the relay.
    let reply = handle_stream(
        &mut rig.home,
        &Request {
            method: "POST",
            path: &format!("/api/v1/equipments/{light}/orders/state"),
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
    );
    assert!(matches!(reply, Reply::Order(_)), "{reply:?}");
    assert!(rig.home.orders().take(rig.now).is_some());
    // Nothing in the sequence wrote the device's room.
    assert_eq!(
        rig.get(&format!("/api/v1/devices/{relay}")).1["zoneId"],
        Value::Null
    );
}

#[test]
fn a_sensor_is_made_from_two_devices_by_the_forms_sequence() {
    let mut rig = Rig::new();
    let th = rig.sensor(1);
    let pir = rig.motion_sensor(2);
    let garage = zone(&mut rig, "Garage", ROOT);
    let sensor = create(&mut rig, "Capteurs garage", "sensor", &garage);
    let plan = [
        (&th, "temperature", "temperature"),
        (&th, "humidity", "humidity"),
        (&th, "battery", "battery"),
        (&pir, "occupancy", "occupancy"),
        // The simulated MS01 has no battery data: any data takes the suffixed alias.
        (&th, "voltage", "battery_2"),
    ];
    for (device, key, alias) in plan {
        let data = data_of(&mut rig, device, key);
        assert_eq!(bind_data(&mut rig, &sensor, &data, alias).0, 201, "{alias}");
    }
    let (_, equipment) = rig.get(&format!("/api/v1/equipments/{sensor}"));
    assert_eq!(
        aliases(&equipment["dataBindings"]),
        [
            "temperature",
            "humidity",
            "battery",
            "occupancy",
            "battery_2"
        ]
    );
    assert_eq!(equipment["orderBindings"], json!([]));
    rig.measure(&temperature(1, 19.6));
    let (_, aggregation) = rig.get("/api/v1/zones/aggregation");
    assert_eq!(aggregation[garage.as_str()]["temperature"], json!(19.6));
    assert_eq!(aggregation[garage.as_str()]["motionSensors"], 1);
    // Every binding carries the deviceId of its device (the Equipment column's source).
    let (_, list) = rig.get("/api/v1/equipments");
    let owners: Vec<&str> = list[0]["dataBindings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| text(&b["deviceId"]))
        .collect();
    assert_eq!(owners, [&th, &th, &th, &pir, &th]);
    // No room written on the devices.
    for device in [&th, &pir] {
        assert_eq!(
            rig.get(&format!("/api/v1/devices/{device}")).1["zoneId"],
            Value::Null
        );
    }
}

#[test]
fn a_binding_to_a_deleted_data_is_refused_and_the_rollback_leaves_the_list_as_before() {
    let mut rig = Rig::new();
    let relay = rig.relay(3);
    let garage = zone(&mut rig, "Garage", ROOT);
    let before = rig.get("/api/v1/equipments").1;
    let light = create(&mut rig, "Lumière", "light_onoff", &garage);
    let data = data_of(&mut rig, &relay, "state");
    assert_eq!(rig.delete(&format!("/api/v1/devices/{relay}")).0, 204);
    assert_eq!(
        rig.refused(
            "POST",
            &format!("/api/v1/equipments/{light}/data-bindings"),
            &json!({"deviceDataId": data, "alias": "state"}).to_string()
        ),
        (404, format!("DeviceData not found: {data}"))
    );
    assert_eq!(rig.delete(&format!("/api/v1/equipments/{light}")).0, 204);
    assert_eq!(rig.get("/api/v1/equipments").1, before);
}

/// Five devices of four data and four orders each: 20 of each kind (a device holds at most 4
/// orders). The ids of the data, then of the orders, in device order.
fn twenty_and_twenty(rig: &mut Rig) -> (Vec<String>, Vec<String>) {
    let (mut data, mut orders) = (Vec::new(), Vec::new());
    for n in 1..=5u8 {
        let mut device = many_data(n, &format!("D{n}"), usize::from(n) * 4, 4);
        let id = device["id"].clone();
        device["orders"] = Value::Array(
            (0..4)
                .map(|i| {
                    json!({
                        "id": format!("ee{n:02x}{i:02x}00-0000-4000-8000-000000000000"),
                        "deviceId": id,
                        "key": format!("o{}", usize::from(n) * 4 + i),
                        "type": "boolean",
                    })
                })
                .collect(),
        );
        for i in 0..4 {
            data.push(data_id(&device, i));
            orders.push(text(&device["orders"][i]["id"]).to_string());
        }
        rig.add_devices(vec![device]);
    }
    (data, orders)
}

#[test]
fn the_largest_creation_is_17_requests_one_after_another_all_answered_201() {
    let mut rig = Rig::new();
    let (data, orders) = twenty_and_twenty(&mut rig);
    let room = zone(&mut rig, "Z", ROOT);
    let thermostat = create(&mut rig, "Thermostat", "thermostat", &room);
    let mut answers = 1;
    for (i, id) in data.iter().take(8).enumerate() {
        let (status, body) = bind_data(&mut rig, &thermostat, id, &format!("d{i}"));
        assert_eq!(status, 201, "data {i}: {body}");
        answers += 1;
    }
    for (i, id) in orders.iter().take(8).enumerate() {
        let (status, body) = bind_order(&mut rig, &thermostat, id, &format!("o{i}"));
        assert_eq!(status, 201, "order {i}: {body}");
        answers += 1;
    }
    assert_eq!(answers, 17);
    let (_, equipment) = rig.get(&format!("/api/v1/equipments/{thermostat}"));
    assert_eq!(equipment["dataBindings"].as_array().unwrap().len(), 8);
    assert_eq!(equipment["orderBindings"].as_array().unwrap().len(), 8);
    // A 9th of either kind is refused: the plan never sends one.
    assert_eq!(
        rig.refused(
            "POST",
            &format!("/api/v1/equipments/{thermostat}/data-bindings"),
            &json!({"deviceDataId": data[8], "alias": "d8"}).to_string()
        ),
        (400, "Too many bindings (8)".to_string())
    );
    assert_eq!(
        rig.refused(
            "POST",
            &format!("/api/v1/equipments/{thermostat}/order-bindings"),
            &json!({"deviceOrderId": orders[8], "alias": "o8"}).to_string()
        ),
        (400, "Too many bindings (8)".to_string())
    );
}

#[test]
fn an_alias_of_33_characters_is_refused() {
    let mut rig = Rig::new();
    let relay = rig.relay(3);
    let room = zone(&mut rig, "Z", ROOT);
    let light = create(&mut rig, "L", "light_onoff", &room);
    let data = data_of(&mut rig, &relay, "state");
    let long = "k".repeat(33);
    assert_eq!(
        rig.refused(
            "POST",
            &format!("/api/v1/equipments/{light}/data-bindings"),
            &json!({"deviceDataId": data, "alias": long}).to_string()
        ),
        (400, "Invalid alias".to_string())
    );
    let ok = "k".repeat(32);
    assert_eq!(bind_data(&mut rig, &light, &data, &ok).0, 201);
}
