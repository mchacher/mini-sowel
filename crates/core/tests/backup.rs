//! The export and import of the home (spec 026 R3): `GET/POST /backup`, the document's checks
//! (nothing changes on a refusal), what an import replaces, its events and its writes.

mod mem_storage;
mod support;

use std::sync::Mutex;

use mem_storage::MemStorage;
use serde_json::{Value, json};
use solite_core::api::{MAX_BODY_BYTES, Reach, Request, handle};
use solite_core::devices::lock;
use solite_core::engine::{DEFINITION_DELAY_MS, SETTINGS_FILE, Saver, ZONES_FILE};
use solite_core::home::Home;
use solite_core::live::{ClientId, LiveHandle};
use support::{HOST, PORT, ROOT, Rig};

const BACKUP: &str = "/api/v1/backup";

fn uuid(n: u8) -> String {
    format!("{n:02x}{n:02x}{n:02x}{n:02x}-0000-4000-8000-0000000000{n:02x}")
}

fn settings() -> Value {
    json!({
        "home.name": "Annexe",
        "home.latitude": "48.8566",
        "home.longitude": "2.3522",
        "home.timezone": "Europe/Paris",
        "home.sunriseOffset": "20",
        "home.sunsetOffset": "40",
    })
}

/// Home A: the six settings, the root renamed, three zones (one under another).
fn home_a() -> Rig {
    let mut rig = Rig::new();
    assert_eq!(rig.put("/api/v1/settings", settings()).0, 200);
    let (status, _) = rig.put(&format!("/api/v1/zones/{ROOT}"), json!({"name": "Annexe"}));
    assert_eq!(status, 200);
    let garage = rig.zone("Garage");
    let (status, _) = rig.post(
        "/api/v1/zones",
        json!({"name": "Workshop", "parentId": garage, "icon": "wrench", "displayOrder": 2}),
    );
    assert_eq!(status, 201);
    let (status, _) = rig.post(
        "/api/v1/zones",
        json!({"name": "Shed", "description": "At the back"}),
    );
    assert_eq!(status, 201);
    rig
}

fn export(rig: &mut Rig) -> Value {
    let (status, body) = rig.get(BACKUP);
    assert_eq!(status, 200, "{body}");
    body
}

fn import(rig: &mut Rig, document: &Value) -> (u16, Value) {
    rig.post(BACKUP, document.clone())
}

fn refused(rig: &mut Rig, document: &Value) -> String {
    let (status, text) = rig.refused("POST", BACKUP, &document.to_string());
    assert_eq!(status, 400, "{text}");
    text
}

/// The settings and the zones (id, name, parent, icon, description, order), sorted by id.
fn state(rig: &mut Rig) -> (Value, Vec<Value>) {
    let (_, settings) = rig.get("/api/v1/settings");
    let mut zones: Vec<Value> = rig
        .home
        .zones()
        .list()
        .iter()
        .map(|z| {
            json!([
                z.id.to_string(),
                z.name,
                z.parent_id.map(|p| p.to_string()),
                z.icon,
                z.description,
                z.display_order
            ])
        })
        .collect();
    zones.sort_by_key(|z| z[0].as_str().unwrap().to_string());
    (settings, zones)
}

/// A refused document changes nothing.
fn refused_unchanged(rig: &mut Rig, document: &Value) -> String {
    let before = state(rig);
    let text = refused(rig, document);
    assert_eq!(state(rig), before, "{text}");
    text
}

/// A zone of a document.
fn zone(id: &str, name: &str, parent: Option<&str>) -> Value {
    json!({"id": id, "name": name, "parentId": parent, "displayOrder": 0})
}

fn document(settings: Value, zones: Vec<Value>) -> Value {
    json!({"version": 1, "settings": settings, "zones": zones})
}

// ---- Export ----

#[test]
fn the_export_is_the_version_the_settings_and_the_zones_as_sowels_zone() {
    let mut rig = home_a();
    let body = export(&mut rig);
    assert_eq!(body["version"], 1);
    assert_eq!(body["settings"], settings());
    let zones = body["zones"].as_array().unwrap();
    assert_eq!(zones.len(), 4);
    let workshop = zones.iter().find(|z| z["name"] == "Workshop").unwrap();
    let mut keys: Vec<&String> = workshop.as_object().unwrap().keys().collect();
    keys.sort();
    assert_eq!(
        keys,
        [
            "createdAt",
            "displayOrder",
            "icon",
            "id",
            "name",
            "parentId",
            "updatedAt"
        ]
    );
    assert_eq!(workshop["icon"], "wrench");
    let text = rig.raw("GET", BACKUP, "").body.unwrap();
    assert!(!text.contains('\n') && !text.contains(": "), "compact");
}

#[test]
fn an_export_over_16_kb_is_refused() {
    let mut rig = Rig::new();
    for i in 0..31 {
        let (status, _) = rig.post(
            "/api/v1/zones",
            json!({
                "name": format!("{:z<64}", format!("Zone {i:02}")),
                "icon": "i".repeat(32),
                "description": "d".repeat(500),
            }),
        );
        assert_eq!(status, 201);
    }
    let (status, text) = rig.refused("GET", BACKUP, "");
    assert_eq!(status, 413);
    assert!(
        text.starts_with("Backup too large: ")
            && text.ends_with(" bytes, 16384 at most (shorten the rooms' descriptions)"),
        "{text}"
    );
}

// ---- Import ----

#[test]
fn an_export_imported_into_a_fresh_home_gives_the_same_settings_and_zones() {
    let mut a = home_a();
    let doc = export(&mut a);
    let mut b = Rig::new();
    let (status, body) = import(&mut b, &doc);
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        body,
        json!({"success": true, "settings": 6, "zonesCreated": 3, "zonesUpdated": 1, "zonesUnchanged": 0})
    );
    assert_eq!(state(&mut b), state(&mut a));
}

#[test]
fn a_second_import_of_the_same_document_changes_and_writes_nothing() {
    let mut a = home_a();
    let doc = export(&mut a);
    let mut b = Rig::new();
    import(&mut b, &doc);
    let storage = MemStorage::default();
    let home = Mutex::new(std::mem::replace(&mut b.home, Home::new(0)));
    Saver::new(&storage, b.now)
        .tick(&home, b.now, true)
        .unwrap();
    storage.per_file.borrow_mut().clear();
    storage.attempts.set(0);
    std::mem::swap(&mut b.home, &mut *lock(&home));
    let hub = b.home.live();
    let c = client(&hub);
    let (status, body) = import(&mut b, &doc);
    assert_eq!(status, 200);
    assert_eq!(
        body,
        json!({"success": true, "settings": 6, "zonesCreated": 0, "zonesUpdated": 0, "zonesUnchanged": 4})
    );
    assert_eq!(b.home.zones().list().len(), 4, "no zone duplicated");
    let events = batch(&hub, c);
    assert!(
        !events.iter().any(|e| e["type"] == "settings.changed"
            || e["type"] == "zone.created"
            || e["type"] == "zone.updated"),
        "{events:?}"
    );
    std::mem::swap(&mut b.home, &mut *lock(&home));
    Saver::new(&storage, b.now)
        .tick(&home, b.now, true)
        .unwrap();
    assert_eq!(storage.attempts.get(), 0, "{:?}", storage.per_file.borrow());
}

#[test]
fn the_settings_are_replaced_whole() {
    let mut b = Rig::new();
    b.put(
        "/api/v1/settings",
        json!({"home.name": "B", "home.sunsetOffset": "10"}),
    );
    let (status, body) = import(&mut b, &document(json!({"home.name": "Imported"}), vec![]));
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["settings"], 1);
    assert_eq!(
        b.get("/api/v1/settings").1,
        json!({"home.name": "Imported"})
    );
}

#[test]
fn zones_of_the_home_absent_from_the_document_are_kept() {
    let mut b = Rig::new();
    let kept = b.zone("Kept");
    let doc = document(json!({}), vec![zone(&uuid(1), "New", Some(ROOT))]);
    assert_eq!(import(&mut b, &doc).0, 200);
    assert!(b.home.zones().find(&kept).is_some());
    assert!(b.home.zones().find(&uuid(1)).is_some());
}

#[test]
fn a_child_before_its_parent_in_the_list_is_accepted() {
    let mut b = Rig::new();
    let doc = document(
        json!({}),
        vec![
            zone(&uuid(2), "Child", Some(&uuid(1))),
            zone(&uuid(1), "Parent", Some(ROOT)),
        ],
    );
    let (status, body) = import(&mut b, &doc);
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["zonesCreated"], 2);
    let child = b.home.zones().find(&uuid(2)).unwrap();
    assert_eq!(child.parent_id.unwrap().to_string(), uuid(1));
}

#[test]
fn zones_empty_replaces_the_settings_and_leaves_the_zones() {
    let mut b = Rig::new();
    let kept = b.zone("Kept");
    let (status, body) = import(&mut b, &document(settings(), vec![]));
    assert_eq!(status, 200, "{body}");
    assert_eq!(b.get("/api/v1/settings").1, settings());
    assert_eq!(b.home.zones().list().len(), 2);
    assert!(b.home.zones().find(&kept).is_some());
}

#[test]
fn a_document_without_the_root_zone_keeps_the_root() {
    let mut b = Rig::new();
    let doc = document(json!({}), vec![zone(&uuid(1), "Alone", Some(ROOT))]);
    assert_eq!(import(&mut b, &doc).0, 200);
    assert!(b.home.zones().find(ROOT).is_some());
}

// ---- Events and writes ----

fn client(hub: &LiveHandle) -> ClientId {
    let id = hub.connect().unwrap();
    assert!(hub.subscribe(id, r#"{"type":"subscribe","topics":["zones"]}"#));
    id
}

fn batch(hub: &LiveHandle, id: ClientId) -> Vec<Value> {
    hub.take_batch(id)
        .map(|text| serde_json::from_str::<Value>(&text).unwrap())
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default()
}

#[test]
fn an_import_emits_settings_changed_with_the_changed_keys_and_one_event_per_zone() {
    let mut a = home_a();
    let doc = export(&mut a);
    let mut b = Rig::new();
    b.put(
        "/api/v1/settings",
        json!({"home.name": "Annexe", "home.latitude": "1"}),
    );
    let hub = b.home.live();
    let c = client(&hub);
    assert_eq!(import(&mut b, &doc).0, 200);
    let events = batch(&hub, c);
    let changed: Vec<&Value> = events
        .iter()
        .filter(|e| e["type"] == "settings.changed")
        .collect();
    assert_eq!(changed.len(), 1);
    assert_eq!(
        changed[0]["keys"],
        json!([
            "home.latitude",
            "home.longitude",
            "home.timezone",
            "home.sunriseOffset",
            "home.sunsetOffset"
        ])
    );
    let count = |name: &str| events.iter().filter(|e| e["type"] == name).count();
    assert_eq!(count("zone.created"), 3);
    assert_eq!(count("zone.updated"), 1);
}

#[test]
fn an_import_is_saved_after_the_delay_once_per_file() {
    let mut a = home_a();
    let doc = export(&mut a);
    let storage = MemStorage::default();
    let home = Mutex::new(Home::new(support::T0));
    let t = support::T0;
    Saver::new(&storage, t).tick(&home, t, true).unwrap();
    storage.per_file.borrow_mut().clear();
    storage.attempts.set(0);
    let mut saver = Saver::new(&storage, t).with_definition_delay(DEFINITION_DELAY_MS);
    let text = doc.to_string();
    let mut random = |b: &mut [u8]| {
        b.fill(3);
        Ok(())
    };
    let response = handle(
        &mut lock(&home),
        &Request {
            method: "POST",
            path: BACKUP,
            host: Some(HOST),
            port: PORT,
            reach: Reach::Loopback,
            content_type: Some("application/json"),
            body: text.as_bytes(),
        },
        t,
        &mut random,
    );
    assert_eq!(response.status, 200, "{response:?}");
    saver.tick(&home, t, false).unwrap();
    saver.tick(&home, t + 1_999, false).unwrap();
    assert_eq!(storage.attempts.get(), 0);
    saver.tick(&home, t + 2_000, false).unwrap();
    assert_eq!(storage.writes(SETTINGS_FILE), 1);
    assert_eq!(storage.writes(ZONES_FILE), 1);
}

// ---- Refusals: nothing changes ----

#[test]
fn a_bad_version_a_missing_field_and_an_extra_field_are_refused() {
    let mut b = home_a();
    let mut doc = document(json!({}), vec![]);
    doc["version"] = json!(2);
    assert_eq!(
        refused_unchanged(&mut b, &doc),
        "Invalid backup: unsupported version 2"
    );
    let doc = json!({"version": 1, "settings": {}});
    assert_eq!(
        refused_unchanged(&mut b, &doc),
        "Invalid backup: missing field zones"
    );
    let mut doc = document(json!({}), vec![]);
    doc["devices"] = json!([]);
    assert_eq!(
        refused_unchanged(&mut b, &doc),
        "Invalid backup: unknown field devices"
    );
    assert_eq!(
        refused_unchanged(&mut b, &json!([])),
        "Invalid backup: not an object"
    );
}

#[test]
fn a_bad_setting_is_refused_and_the_zones_are_untouched() {
    let mut b = home_a();
    let doc = document(
        json!({"home.latitude": "91"}),
        vec![zone(&uuid(1), "New", Some(ROOT))],
    );
    assert_eq!(
        refused_unchanged(&mut b, &doc),
        "Invalid backup: Invalid value for home.latitude"
    );
    let doc = document(json!({"home.foo": "x"}), vec![]);
    assert_eq!(
        refused_unchanged(&mut b, &doc),
        "Invalid backup: Unknown setting: home.foo"
    );
}

#[test]
fn valid_settings_with_an_invalid_zone_leave_the_settings_unchanged() {
    let mut b = home_a();
    let doc = document(
        json!({"home.name": "Changed"}),
        vec![zone("nope", "X", Some(ROOT))],
    );
    assert_eq!(
        refused_unchanged(&mut b, &doc),
        "Invalid backup: invalid zone id"
    );
}

#[test]
fn the_rules_of_a_zone_are_those_of_post_zones() {
    let mut b = home_a();
    let id = uuid(1);
    let cases = [
        (
            vec![zone(&id, "A", Some(ROOT)), zone(&id, "B", Some(ROOT))],
            format!("Invalid backup: duplicate zone {id}"),
        ),
        (
            vec![zone("not-a-uuid", "A", Some(ROOT))],
            "Invalid backup: invalid zone id".to_string(),
        ),
        (
            vec![zone(&id, "  ", Some(ROOT))],
            format!("Invalid backup: Invalid name in zone {id}"),
        ),
        (
            vec![zone(&id, &"n".repeat(65), Some(ROOT))],
            format!("Invalid backup: Invalid name in zone {id}"),
        ),
        (
            vec![json!({"id": id, "name": "A", "parentId": ROOT, "children": []})],
            "Invalid backup: unknown field children in a zone".to_string(),
        ),
        (
            vec![json!({"id": id, "name": "A", "parentId": ROOT, "displayOrder": "1"})],
            format!("Invalid backup: Invalid field: displayOrder in zone {id}"),
        ),
        (
            vec![json!({"id": id, "name": "A", "parentId": ROOT, "createdAt": 5})],
            format!("Invalid backup: Invalid field: createdAt in zone {id}"),
        ),
    ];
    for (zones, expected) in cases {
        assert_eq!(
            refused_unchanged(&mut b, &document(json!({}), zones)),
            expected
        );
    }
}

#[test]
fn a_parent_in_neither_the_document_nor_the_home_is_refused() {
    let mut b = home_a();
    let doc = document(json!({}), vec![zone(&uuid(1), "Lost", Some(&uuid(9)))]);
    assert_eq!(
        refused_unchanged(&mut b, &doc),
        format!("Invalid backup: parent not found for zone {}", uuid(1))
    );
}

#[test]
fn two_new_zones_each_others_parent_are_refused() {
    let mut b = home_a();
    let doc = document(
        json!({}),
        vec![
            zone(&uuid(1), "A", Some(&uuid(2))),
            zone(&uuid(2), "B", Some(&uuid(1))),
        ],
    );
    assert_eq!(
        refused_unchanged(&mut b, &doc),
        format!("Invalid backup: parent not found for zone {}", uuid(1))
    );
}

#[test]
fn a_cycle_of_existing_zones_is_refused() {
    let mut b = Rig::new();
    let a = b.zone("A");
    let c = b.zone("B");
    let doc = document(
        json!({}),
        vec![zone(&a, "A", Some(&c)), zone(&c, "B", Some(&a))],
    );
    let text = refused_unchanged(&mut b, &doc);
    assert_eq!(
        text,
        format!("Invalid backup: Moving this zone would create a circular reference in zone {c}")
    );
}

#[test]
fn an_existing_zone_moved_under_its_own_child_is_refused() {
    let mut b = Rig::new();
    let parent = b.zone("Parent");
    let (_, child) = b.post(
        "/api/v1/zones",
        json!({"name": "Child", "parentId": parent}),
    );
    let child = child["id"].as_str().unwrap().to_string();
    let doc = document(json!({}), vec![zone(&parent, "Parent", Some(&child))]);
    let text = refused_unchanged(&mut b, &doc);
    assert!(text.ends_with(&format!("in zone {parent}")), "{text}");
}

#[test]
fn only_the_root_zone_has_no_parent() {
    let mut b = home_a();
    let doc = document(json!({}), vec![zone(ROOT, "Root", Some(&uuid(1)))]);
    assert_eq!(
        refused_unchanged(&mut b, &doc),
        "Invalid backup: only the root zone has no parent"
    );
    let doc = document(json!({}), vec![zone(&uuid(1), "Orphan", None)]);
    assert_eq!(
        refused_unchanged(&mut b, &doc),
        "Invalid backup: only the root zone has no parent"
    );
    let doc = document(
        json!({}),
        vec![zone(&uuid(1), "One", None), zone(&uuid(2), "Two", None)],
    );
    assert_eq!(
        refused_unchanged(&mut b, &doc),
        "Invalid backup: only the root zone has no parent"
    );
}

#[test]
fn thirty_three_zones_in_the_document_or_past_32_in_the_home_are_refused() {
    let mut b = Rig::new();
    let zones: Vec<Value> = (1..=33).map(|n| zone(&uuid(n), "Z", Some(ROOT))).collect();
    assert_eq!(
        refused_unchanged(&mut b, &document(json!({}), zones)),
        "Invalid backup: too many zones"
    );
    for i in 0..19 {
        b.zone(&format!("Home {i}"));
    }
    assert_eq!(b.home.zones().list().len(), 20);
    let zones: Vec<Value> = (100..113)
        .map(|n| zone(&uuid(n), "New", Some(ROOT)))
        .collect();
    assert_eq!(
        refused_unchanged(&mut b, &document(json!({}), zones)),
        "Invalid backup: too many zones"
    );
    // 12 new zones reach the bound exactly.
    let zones: Vec<Value> = (100..112)
        .map(|n| zone(&uuid(n), "New", Some(ROOT)))
        .collect();
    assert_eq!(import(&mut b, &document(json!({}), zones)).0, 200);
    assert_eq!(b.home.zones().list().len(), 32);
}

#[test]
fn a_body_over_16_kb_is_413_and_one_of_exactly_16_kb_is_read() {
    let mut b = Rig::new();
    let mut text = document(json!({"home.name": "Exact"}), vec![]).to_string();
    let pad = MAX_BODY_BYTES - text.len();
    text.insert_str(1, &" ".repeat(pad));
    assert_eq!(text.len(), MAX_BODY_BYTES);
    let (status, body) = b.call("POST", BACKUP, &text);
    assert_eq!(status, 200, "{body}");
    text.insert(1, ' ');
    assert_eq!(b.refused("POST", BACKUP, &text).0, 413);
}

#[test]
fn a_body_that_is_not_declared_json_is_415() {
    let mut b = Rig::new();
    let text = document(json!({}), vec![]).to_string();
    let response = handle(
        &mut b.home,
        &Request {
            method: "POST",
            path: BACKUP,
            host: Some(HOST),
            port: PORT,
            reach: Reach::Loopback,
            content_type: Some("text/plain"),
            body: text.as_bytes(),
        },
        b.now,
        &mut *b.random,
    );
    assert_eq!(response.status, 415);
}
