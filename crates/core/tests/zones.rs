//! Zones (spec 010, R1, R2, R7.5, R7.9 to R7.11, R7.13, R7.14): the tree under the root zone,
//! its rules and the texts of Sowel's errors, devices in zones, the consistency of what is read
//! from storage.

mod support;

use serde_json::{Value, json};
use solite_core::devices::Devices;
use solite_core::home::Home;
use solite_core::zones::{Zone, parse_list};
use support::*;

fn text(value: &Value) -> &str {
    value.as_str().unwrap()
}

fn names(children: &Value) -> Vec<&str> {
    children
        .as_array()
        .unwrap()
        .iter()
        .map(|z| text(&z["name"]))
        .collect()
}

#[test]
fn the_first_start_has_the_root_zone_maison_with_sowels_id_and_icon() {
    let mut rig = Rig::new();
    let (status, body) = rig.get("/api/v1/zones");
    assert_eq!(status, 200);
    assert_eq!(
        body,
        json!([{
            "id": ROOT,
            "name": "Maison",
            "parentId": null,
            "icon": "home",
            "displayOrder": 0,
            "createdAt": "2026-11-21T09:12:03.120Z",
            "updatedAt": "2026-11-21T09:12:03.120Z",
            "children": []
        }])
    );
    // It is to be written at the first start.
    assert_eq!(rig.home.changes(), (true, false));
}

#[test]
fn a_zone_is_created_under_the_root_without_a_parent_and_under_a_parent_with_one() {
    let mut rig = Rig::new();
    let (status, garage) = rig.post("/api/v1/zones", json!({"name": "  Garage  "}));
    assert_eq!(status, 201);
    assert_eq!(garage["name"], "Garage", "trimmed");
    assert_eq!(garage["parentId"], ROOT);
    assert_eq!(garage["displayOrder"], 0);
    assert!(garage.get("icon").is_none() && garage.get("description").is_none());
    let garage_id = text(&garage["id"]).to_string();
    let (_, shelf) = rig.post(
        "/api/v1/zones",
        json!({"name": "Shelf", "parentId": garage_id, "icon": "box", "description": "d", "displayOrder": 3}),
    );
    assert_eq!(shelf["parentId"], garage_id.as_str());
    assert_eq!(shelf["icon"], "box");
    assert_eq!(shelf["description"], "d");
    assert_eq!(shelf["displayOrder"], 3);
    // `parentId: null` means the root.
    let (status, attic) = rig.post("/api/v1/zones", json!({"name": "Attic", "parentId": null}));
    assert_eq!(status, 201);
    assert_eq!(attic["parentId"], ROOT);
}

#[test]
fn an_unknown_parent_is_404_with_its_id_in_the_text() {
    let mut rig = Rig::new();
    let unknown = "11111111-1111-4111-8111-111111111111";
    assert_eq!(
        rig.refused(
            "POST",
            "/api/v1/zones",
            &json!({"name": "A", "parentId": unknown}).to_string()
        ),
        (404, format!("Parent zone not found: {unknown}"))
    );
    assert_eq!(
        rig.refused(
            "POST",
            "/api/v1/zones",
            &json!({"name": "A", "parentId": "nope"}).to_string()
        ),
        (404, "Parent zone not found: nope".to_string())
    );
    assert_eq!(rig.home.zones().list().len(), 1, "nothing was created");
}

#[test]
fn the_tree_sorts_children_by_display_order_then_name_at_every_level() {
    let mut rig = Rig::new();
    for (name, order) in [("b", 1), ("a", 1), ("z", 0), ("c", 2)] {
        rig.post(
            "/api/v1/zones",
            json!({"name": name, "displayOrder": order}),
        );
    }
    let (_, tree) = rig.get("/api/v1/zones");
    assert_eq!(names(&tree[0]["children"]), ["z", "a", "b", "c"]);
    // A child of "c": its own children are sorted too.
    let c = text(&tree[0]["children"][3]["id"]).to_string();
    for (name, order) in [("y", 5), ("x", 5), ("w", 4)] {
        rig.post(
            "/api/v1/zones",
            json!({"name": name, "parentId": c, "displayOrder": order}),
        );
    }
    let (_, tree) = rig.get("/api/v1/zones");
    assert_eq!(names(&tree[0]["children"][3]["children"]), ["w", "x", "y"]);
}

#[test]
fn get_one_zone_has_two_levels_the_grandchildren_with_empty_children() {
    let mut rig = Rig::new();
    let a = rig.zone("A");
    let (_, b) = rig.post("/api/v1/zones", json!({"name": "B", "parentId": a}));
    let b = text(&b["id"]).to_string();
    let (_, c) = rig.post("/api/v1/zones", json!({"name": "C", "parentId": b}));
    let c = text(&c["id"]).to_string();
    rig.post("/api/v1/zones", json!({"name": "D", "parentId": c}));
    let (status, zone) = rig.get(&format!("/api/v1/zones/{a}"));
    assert_eq!(status, 200);
    assert_eq!(zone["name"], "A");
    assert_eq!(zone["children"][0]["name"], "B");
    assert_eq!(zone["children"][0]["children"][0]["name"], "C");
    assert_eq!(
        zone["children"][0]["children"][0]["children"],
        json!([]),
        "the grandchildren carry no children"
    );
    // The full tree goes all the way down.
    let (_, tree) = rig.get("/api/v1/zones");
    assert_eq!(
        tree[0]["children"][0]["children"][0]["children"][0]["children"][0]["name"],
        "D"
    );
    assert_eq!(
        rig.refused("GET", "/api/v1/zones/nope", ""),
        (404, "Zone not found".to_string())
    );
}

#[test]
fn put_changes_the_fields_null_clears_icon_and_description_and_an_empty_body_is_ok() {
    let mut rig = Rig::new();
    let (_, zone) = rig.post(
        "/api/v1/zones",
        json!({"name": "Garage", "icon": "car", "description": "d"}),
    );
    let id = text(&zone["id"]).to_string();
    rig.now += 1000;
    let (status, updated) = rig.put(
        &format!("/api/v1/zones/{id}"),
        json!({"name": " Cellar ", "displayOrder": 7, "icon": null, "description": null}),
    );
    assert_eq!(status, 200);
    assert_eq!(updated["name"], "Cellar");
    assert_eq!(updated["displayOrder"], 7);
    assert!(updated.get("icon").is_none() && updated.get("description").is_none());
    assert_eq!(updated["createdAt"], "2026-11-21T09:12:03.120Z");
    assert_eq!(updated["updatedAt"], "2026-11-21T09:12:04.120Z");
    let (status, same) = rig.put(&format!("/api/v1/zones/{id}"), json!({}));
    assert_eq!(status, 200);
    assert_eq!(same["name"], "Cellar");
    // The change is the one answered.
    assert_eq!(rig.get(&format!("/api/v1/zones/{id}")).1["name"], "Cellar");
}

#[test]
fn a_zone_cannot_be_its_own_parent_nor_move_under_a_descendant() {
    let mut rig = Rig::new();
    let a = rig.zone("A");
    let (_, b) = rig.post("/api/v1/zones", json!({"name": "B", "parentId": a}));
    let b = text(&b["id"]).to_string();
    let (_, c) = rig.post("/api/v1/zones", json!({"name": "C", "parentId": b}));
    let c = text(&c["id"]).to_string();
    let put = |rig: &mut Rig, id: &str, parent: &str| {
        rig.refused(
            "PUT",
            &format!("/api/v1/zones/{id}"),
            &json!({ "parentId": parent }).to_string(),
        )
    };
    assert_eq!(
        put(&mut rig, &a, &a),
        (400, "A zone cannot be its own parent".to_string())
    );
    assert_eq!(
        put(&mut rig, &a, &b),
        (
            400,
            "Moving this zone would create a circular reference".to_string()
        )
    );
    assert_eq!(
        put(&mut rig, &a, &c),
        (
            400,
            "Moving this zone would create a circular reference".to_string()
        )
    );
    // The root under one of its descendants is a loop too.
    assert_eq!(put(&mut rig, ROOT, &a).0, 400);
    // Nothing moved.
    assert_eq!(rig.get(&format!("/api/v1/zones/{a}")).1["parentId"], ROOT);
    // A legal move: C under A.
    let (status, moved) = rig.put(&format!("/api/v1/zones/{c}"), json!({"parentId": a}));
    assert_eq!(status, 200);
    assert_eq!(moved["parentId"], a.as_str());
}

#[test]
fn put_checks_the_unknown_zone_the_unknown_parent_and_the_null_parent() {
    let mut rig = Rig::new();
    let a = rig.zone("A");
    let unknown = "11111111-1111-4111-8111-111111111111";
    assert_eq!(
        rig.refused(
            "PUT",
            &format!("/api/v1/zones/{unknown}"),
            r#"{"name":"X"}"#
        ),
        (404, "Zone not found".to_string())
    );
    assert_eq!(
        rig.refused(
            "PUT",
            &format!("/api/v1/zones/{a}"),
            &json!({ "parentId": unknown }).to_string()
        ),
        (404, format!("Parent zone not found: {unknown}"))
    );
    assert_eq!(
        rig.refused("PUT", &format!("/api/v1/zones/{a}"), r#"{"parentId":null}"#),
        (400, "Only the root zone has no parent".to_string())
    );
    // The root keeps no parent.
    let (status, root) = rig.put(&format!("/api/v1/zones/{ROOT}"), json!({"parentId": null}));
    assert_eq!(status, 200);
    assert_eq!(root["parentId"], Value::Null);
}

#[test]
fn deleting_follows_sowels_rules_and_texts_singular_and_plural() {
    let mut rig = Rig::new();
    let th = rig.sensor(1);
    let relay = rig.relay(3);
    assert_eq!(
        rig.refused("DELETE", &format!("/api/v1/zones/{ROOT}"), ""),
        (400, "Cannot delete the root zone".to_string())
    );
    assert_eq!(
        rig.refused("DELETE", "/api/v1/zones/nope", ""),
        (404, "Zone not found".to_string())
    );
    // Children: one, then two.
    let parent = rig.zone("Parent");
    let (_, child) = rig.post("/api/v1/zones", json!({"name": "C1", "parentId": parent}));
    let c1 = text(&child["id"]).to_string();
    assert_eq!(
        rig.refused("DELETE", &format!("/api/v1/zones/{parent}"), ""),
        (
            400,
            "Cannot delete zone with 1 child zone. Remove or move them first.".to_string()
        )
    );
    let (_, child) = rig.post("/api/v1/zones", json!({"name": "C2", "parentId": parent}));
    let c2 = text(&child["id"]).to_string();
    assert_eq!(
        rig.refused("DELETE", &format!("/api/v1/zones/{parent}"), ""),
        (
            400,
            "Cannot delete zone with 2 child zones. Remove or move them first.".to_string()
        )
    );
    // Equipments: one, then two.
    rig.equipment("E1", "sensor", &c1, &[&th]);
    assert_eq!(
        rig.refused("DELETE", &format!("/api/v1/zones/{c1}"), ""),
        (
            400,
            "Cannot delete zone with 1 equipment. Remove or move them first.".to_string()
        )
    );
    rig.equipment("E2", "switch", &c1, &[&relay]);
    assert_eq!(
        rig.refused("DELETE", &format!("/api/v1/zones/{c1}"), ""),
        (
            400,
            "Cannot delete zone with 2 equipments. Remove or move them first.".to_string()
        )
    );
    // An empty leaf is deleted, then its parent.
    assert_eq!(
        rig.delete(&format!("/api/v1/zones/{c2}")),
        (204, Value::Null)
    );
    assert_eq!(rig.get(&format!("/api/v1/zones/{c2}")).0, 404);
    assert!(rig.home.changes().0);
}

#[test]
fn deleting_a_zone_that_holds_devices_returns_them_to_none() {
    let mut rig = Rig::new();
    let th = rig.sensor(1);
    let relay = rig.relay(3);
    let garage = rig.zone("Garage");
    let other = rig.zone("Other");
    for (device, zone) in [(&th, &garage), (&relay, &other)] {
        let (status, body) = rig.put(
            &format!("/api/v1/devices/{device}"),
            json!({ "zoneId": zone }),
        );
        assert_eq!(status, 200);
        assert_eq!(body["zoneId"], zone.as_str());
    }
    assert_eq!(rig.delete(&format!("/api/v1/zones/{garage}")).0, 204);
    assert_eq!(
        rig.get(&format!("/api/v1/devices/{th}")).1["zoneId"],
        Value::Null
    );
    assert_eq!(
        rig.get(&format!("/api/v1/devices/{relay}")).1["zoneId"],
        other.as_str(),
        "a device in another zone stays"
    );
}

#[test]
fn a_device_goes_in_an_existing_zone_or_in_none_and_an_unknown_zone_is_404() {
    let mut rig = Rig::new();
    let th = rig.sensor(1);
    let garage = rig.zone("Garage");
    let path = format!("/api/v1/devices/{th}");
    let (status, body) = rig.put(&path, json!({ "zoneId": garage }));
    assert_eq!(status, 200);
    assert_eq!(body["zoneId"], garage.as_str());
    assert!(
        body.get("data").is_none(),
        "the answer is a Device, not its details"
    );
    assert!(rig.home.devices.changes().0, "written at once");
    assert_eq!(
        rig.refused(
            "PUT",
            &path,
            r#"{"zoneId":"11111111-1111-4111-8111-111111111111"}"#
        ),
        (404, "Zone not found".to_string())
    );
    assert_eq!(
        rig.get(&path).1["zoneId"],
        garage.as_str(),
        "refused: unchanged"
    );
    let (status, body) = rig.put(&path, json!({ "zoneId": null }));
    assert_eq!(status, 200);
    assert_eq!(body["zoneId"], Value::Null);
    // Name and zone together: one body, both applied.
    let (_, body) = rig.put(&path, json!({"name": "Probe", "zoneId": garage}));
    assert_eq!(
        (body["name"].as_str(), body["zoneId"].as_str()),
        (Some("Probe"), Some(garage.as_str()))
    );
    // A refused zone changes the name neither.
    let refused = rig.put(&path, json!({"name": "Other", "zoneId": "x"})).0;
    assert_eq!(refused, 404);
    assert_eq!(rig.get(&path).1["name"], "Probe");
}

#[test]
fn names_are_one_to_64_characters_after_trimming() {
    let mut rig = Rig::new();
    for name in ["", "   ", &"a".repeat(65)] {
        assert_eq!(
            rig.refused(
                "POST",
                "/api/v1/zones",
                &json!({ "name": name }).to_string()
            ),
            (400, "Invalid name".to_string()),
            "{name:?}"
        );
    }
    let padded = format!("  {}  ", "é".repeat(64));
    let (status, zone) = rig.post("/api/v1/zones", json!({ "name": padded }));
    assert_eq!(status, 201, "64 characters, counted as characters");
    assert_eq!(zone["name"], "é".repeat(64));
    let id = text(&zone["id"]).to_string();
    assert_eq!(
        rig.refused(
            "PUT",
            &format!("/api/v1/zones/{id}"),
            &json!({ "name": "a".repeat(65) }).to_string()
        ),
        (400, "Invalid name".to_string())
    );
}

#[test]
fn icon_and_description_have_their_bounds() {
    let mut rig = Rig::new();
    let post = |rig: &mut Rig, body: Value| rig.refused("POST", "/api/v1/zones", &body.to_string());
    assert_eq!(
        post(&mut rig, json!({"name": "A", "icon": "i".repeat(33)})),
        (400, "Invalid icon".to_string())
    );
    assert_eq!(
        post(
            &mut rig,
            json!({"name": "A", "description": "d".repeat(501)})
        ),
        (400, "Invalid description".to_string())
    );
    let (status, ok) = rig.post(
        "/api/v1/zones",
        json!({"name": "A", "icon": "i".repeat(32), "description": "d".repeat(500)}),
    );
    assert_eq!(status, 201);
    assert_eq!(ok["icon"].as_str().unwrap().len(), 32);
    let id = text(&ok["id"]).to_string();
    assert_eq!(
        rig.refused(
            "PUT",
            &format!("/api/v1/zones/{id}"),
            &json!({"icon": "i".repeat(33)}).to_string()
        ),
        (400, "Invalid icon".to_string())
    );
}

#[test]
fn the_33rd_zone_the_root_counting_is_refused() {
    let mut rig = Rig::new();
    for i in 1..32 {
        rig.zone(&format!("Zone {i}"));
    }
    assert_eq!(rig.home.zones().list().len(), 32);
    assert_eq!(
        rig.refused("POST", "/api/v1/zones", r#"{"name":"One more"}"#),
        (400, "Too many zones (32)".to_string())
    );
    assert_eq!(rig.home.zones().list().len(), 32);
    // Deleting one makes room.
    let (_, tree) = rig.get("/api/v1/zones");
    let last = text(&tree[0]["children"][30]["id"]).to_string();
    assert_eq!(rig.delete(&format!("/api/v1/zones/{last}")).0, 204);
    assert_eq!(rig.post("/api/v1/zones", json!({"name": "Fits"})).0, 201);
}

#[test]
fn zone_bodies_are_checked_json_unknown_fields_types_and_missing_fields() {
    let mut rig = Rig::new();
    let a = rig.zone("A");
    let put = format!("/api/v1/zones/{a}");
    let refused =
        |rig: &mut Rig, method: &str, path: &str, body: &str| rig.refused(method, path, body);
    for method_path in [("POST", "/api/v1/zones"), ("PUT", put.as_str())] {
        let (method, path) = method_path;
        assert_eq!(
            refused(&mut rig, method, path, "not json"),
            (400, "Invalid JSON".to_string())
        );
        assert_eq!(
            refused(&mut rig, method, path, "[1]"),
            (400, "Invalid JSON".to_string()),
            "a body that is not an object"
        );
        assert_eq!(
            refused(&mut rig, method, path, r#"{"name":"A","color":"red"}"#),
            (400, "Unknown field: color".to_string()),
            "unknown fields are refused on zone routes"
        );
        for (body, field) in [
            (r#"{"name":5}"#, "name"),
            (r#"{"name":"A","parentId":5}"#, "parentId"),
            (r#"{"name":"A","icon":5}"#, "icon"),
            (r#"{"name":"A","description":true}"#, "description"),
            (r#"{"name":"A","displayOrder":"1"}"#, "displayOrder"),
            (r#"{"name":"A","displayOrder":1.5}"#, "displayOrder"),
        ] {
            assert_eq!(
                refused(&mut rig, method, path, body),
                (400, format!("Invalid field: {field}")),
                "{body}"
            );
        }
    }
    assert_eq!(
        refused(&mut rig, "POST", "/api/v1/zones", "{}"),
        (400, "Missing field: name".to_string())
    );
    assert_eq!(
        refused(&mut rig, "POST", "/api/v1/zones", r#"{"icon":"x"}"#),
        (400, "Missing field: name".to_string())
    );
    // Nothing was created or changed by any of it.
    assert_eq!(rig.home.zones().list().len(), 2);
    assert_eq!(rig.get(&put).1["name"], "A");
}

#[test]
fn zones_aggregation_is_matched_before_zones_id() {
    let mut rig = Rig::new();
    let a = rig.zone("A");
    let (status, body) = rig.get("/api/v1/zones/aggregation");
    assert_eq!(status, 200);
    // The aggregation: an object keyed by zone id, not a zone named "aggregation".
    let keys: Vec<&str> = body
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(keys.len(), 2);
    assert!(keys.contains(&ROOT) && keys.contains(&a.as_str()));
    assert!(body.get("error").is_none() && body.get("name").is_none());
    // The other methods on that path are not a zone either.
    for method in ["PUT", "DELETE", "POST"] {
        assert_eq!(
            rig.refused(method, "/api/v1/zones/aggregation", "{}"),
            (405, "Method not allowed".to_string()),
            "{method}"
        );
    }
    // And the other paths: wrong methods, unknown sub-paths.
    assert_eq!(rig.refused("PATCH", "/api/v1/zones", "").0, 405);
    assert_eq!(
        rig.refused("PATCH", &format!("/api/v1/zones/{a}"), "").0,
        405
    );
    assert_eq!(
        rig.refused("GET", &format!("/api/v1/zones/{a}/x"), "").0,
        404
    );
}

// ---- Consistency of what is read from storage (R7.11) ----

fn zone_json(id: &str, name: &str, parent: Option<&str>) -> Value {
    json!({
        "id": id, "name": name, "parentId": parent, "displayOrder": 0,
        "createdAt": "2026-11-21T09:12:03.120Z", "updatedAt": "2026-11-21T09:12:03.120Z"
    })
}

fn load(list: Vec<Value>) -> Vec<Zone> {
    parse_list(&Value::Array(list).to_string()).unwrap()
}

const A: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
const B: &str = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
const C: &str = "cccccccc-cccc-4ccc-8ccc-cccccccccccc";

#[test]
fn a_zones_file_without_the_root_gets_it_back_and_the_orphans_move_under_it() {
    let zones = load(vec![
        zone_json(A, "Garage", None),
        zone_json(B, "Shelf", Some(A)),
        zone_json(C, "Lost", Some("dddddddd-dddd-4ddd-8ddd-dddddddddddd")),
    ]);
    let home = Home::from_parts(Devices::new(), zones, Vec::new(), T0);
    let by_name = |name: &str| {
        home.zones()
            .list()
            .iter()
            .find(|z| z.name == name)
            .unwrap()
            .clone()
    };
    let root = by_name("Maison");
    assert_eq!(root.id.to_string(), ROOT);
    assert_eq!(root.parent_id, None);
    assert_eq!(by_name("Garage").parent_id, Some(root.id));
    assert_eq!(by_name("Lost").parent_id, Some(root.id), "unknown parent");
    assert_eq!(
        by_name("Shelf").parent_id,
        Some(by_name("Garage").id),
        "a good parent is kept"
    );
    assert!(home.changes().0, "the repair is to be written");
}

#[test]
fn a_zone_loop_in_the_file_is_broken_under_the_root() {
    let zones = load(vec![
        zone_json(ROOT, "Maison", None),
        zone_json(A, "A", Some(B)),
        zone_json(B, "B", Some(A)),
        zone_json(C, "C", Some(C)),
    ]);
    let home = Home::from_parts(Devices::new(), zones, Vec::new(), T0);
    // Every zone leads to the root now: the tree holds all of them.
    let mut rig = Rig::new();
    rig.home = home;
    let (_, tree) = rig.get("/api/v1/zones");
    let mut seen = 0;
    let mut stack = vec![&tree[0]];
    while let Some(zone) = stack.pop() {
        seen += 1;
        stack.extend(zone["children"].as_array().unwrap());
    }
    assert_eq!(seen, 4);
}

#[test]
fn a_clean_file_is_not_marked_changed_and_the_root_keeps_no_parent() {
    let clean = Home::from_parts(
        Devices::new(),
        load(vec![
            zone_json(ROOT, "Maison", None),
            zone_json(A, "A", Some(ROOT)),
        ]),
        Vec::new(),
        T0,
    );
    assert_eq!(clean.changes(), (false, false));
    let odd = Home::from_parts(
        Devices::new(),
        load(vec![
            zone_json(ROOT, "Maison", Some(A)),
            zone_json(A, "A", Some(ROOT)),
        ]),
        Vec::new(),
        T0,
    );
    assert_eq!(odd.zones().find(ROOT).unwrap().parent_id, None);
    assert!(odd.changes().0);
}

#[test]
fn a_file_of_more_than_32_zones_keeps_32_with_the_root() {
    let mut list = vec![zone_json(ROOT, "Maison", None)];
    for i in 0..40u8 {
        let id = format!("{i:02x}000000-0000-4000-8000-000000000000");
        list.push(zone_json(&id, &format!("Z{i}"), Some(ROOT)));
    }
    let home = Home::from_parts(Devices::new(), load(list), Vec::new(), T0);
    assert_eq!(home.zones().list().len(), 32);
    assert!(home.zones().find(ROOT).is_some());
}

#[test]
fn a_device_in_an_unknown_zone_in_the_file_gets_none() {
    let mut rig = Rig::new();
    let th = rig.sensor(1);
    let garage = rig.zone("Garage");
    rig.put(
        &format!("/api/v1/devices/{th}"),
        json!({ "zoneId": garage }),
    );
    // Read back with a zones file that does not know the garage.
    let devices = Devices::from_list(rig.home.devices.snapshot());
    let home = Home::from_parts(devices, Vec::new(), Vec::new(), T0);
    assert_eq!(home.devices.list()[0].device.zone_id, None);
    assert!(home.devices.changes().0, "to be written");
    // With the garage known, the zone is kept.
    let devices = Devices::from_list(rig.home.devices.snapshot());
    let zones = rig.home.zones().list().to_vec();
    let home = Home::from_parts(devices, zones, Vec::new(), T0);
    assert_eq!(
        home.devices.list()[0].device.zone_id.map(|z| z.to_string()),
        Some(garage.clone())
    );
}
