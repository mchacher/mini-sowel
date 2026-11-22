//! The requests the interface's Topology and Equipments pages send, in their order (spec 018):
//! the API is unchanged, these tests pin the contract the screens rely on.

mod support;

use serde_json::{Value, json};
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
fn bindings_stop_at_16_of_each_kind_and_65_device_ids_are_refused() {
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
    assert_eq!(body["dataBindings"].as_array().unwrap().len(), 16);
    assert_eq!(body["orderBindings"].as_array().unwrap().len(), 16);
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
