//! The long lists in chunks (spec 010 R7.15): the streamed JSON is the JSON built whole, the
//! filter and the sorting hold, a list changed between two chunks stays valid JSON, an empty list
//! is `[]`; the files written from cursors are the files built whole.

mod support;

use mini_sowel_core::api::{CHUNK_ITEMS, ListCursor, Reply, Request, handle_stream};
use mini_sowel_core::home::Home;
use serde_json::{Value, json};
use support::*;

fn stream(rig: &mut Rig, path: &str) -> ListCursor {
    let reply = handle_stream(
        &mut rig.home,
        &Request {
            method: "GET",
            path,
            host: Some(HOST),
            port: PORT,
            content_type: Some("application/json"),
            body: b"",
        },
        rig.now,
        &mut *rig.random,
    );
    match reply {
        Reply::List(cursor) => cursor,
        other => panic!("not streamed: {other:?}"),
    }
}

fn chunks(cursor: &mut ListCursor, home: &Home) -> Vec<String> {
    std::iter::from_fn(|| cursor.next_chunk(home).unwrap()).collect()
}

/// A home with `n` sensors, each in an equipment named `E<n>` (in a shuffled name order).
fn busy(n: u8) -> Rig {
    let mut rig = Rig::new();
    let zone = rig.zone("Z");
    for i in 0..n {
        let device = rig.sensor(i + 1);
        let kind = if i % 2 == 0 { "sensor" } else { "switch" };
        // Names go down while ids go up: the order served is by name.
        rig.equipment(&format!("E{:02}", n - i), kind, &zone, &[&device]);
    }
    rig
}

#[test]
fn the_chunks_make_the_json_the_whole_response_would_be() {
    let mut rig = busy(20);
    for path in ["/api/v1/devices", "/api/v1/equipments"] {
        let mut cursor = stream(&mut rig, path);
        let parts = chunks(&mut cursor, &rig.home);
        assert_eq!(parts.len(), 20usize.div_ceil(CHUNK_ITEMS), "{path}");
        let joined: Value = serde_json::from_str(&parts.concat()).unwrap();
        // The same as the drained answer of `handle`.
        let whole = rig.get(path).1;
        assert_eq!(joined, whole);
        assert_eq!(joined.as_array().unwrap().len(), 20);
        assert!(parts[0].starts_with('[') && parts[parts.len() - 1].ends_with(']'));
        assert!(parts[1..].iter().all(|p| !p.starts_with('[')));
        assert!(parts[..parts.len() - 1].iter().all(|p| !p.ends_with(']')));
    }
}

#[test]
fn equipments_are_sorted_by_name_and_filtered_by_type_across_chunks() {
    let mut rig = busy(20);
    let list = |rig: &mut Rig, path: &str| -> Vec<String> {
        let mut cursor = stream(rig, path);
        let all: Value = serde_json::from_str(&chunks(&mut cursor, &rig.home).concat()).unwrap();
        all.as_array()
            .unwrap()
            .iter()
            .map(|e| e["name"].as_str().unwrap().to_string())
            .collect()
    };
    let all = list(&mut rig, "/api/v1/equipments");
    let mut sorted = all.clone();
    sorted.sort();
    assert_eq!(all, sorted);
    assert_eq!(all[0], "E01");
    let sensors = list(&mut rig, "/api/v1/equipments?type=sensor");
    assert_eq!(sensors.len(), 10);
    assert!(sensors.windows(2).all(|w| w[0] < w[1]));
    let switches = list(&mut rig, "/api/v1/equipments?type=switch");
    assert_eq!(switches.len(), 10);
    assert!(list(&mut rig, "/api/v1/equipments?type=unknown").is_empty());
}

#[test]
fn an_empty_list_is_one_chunk_with_square_brackets() {
    let mut rig = Rig::new();
    for path in [
        "/api/v1/devices",
        "/api/v1/equipments",
        "/api/v1/equipments?type=nope",
    ] {
        let mut cursor = stream(&mut rig, path);
        assert_eq!(chunks(&mut cursor, &rig.home), ["[]"], "{path}");
        assert_eq!(
            cursor.next_chunk(&rig.home).unwrap(),
            None,
            "finished stays finished"
        );
    }
    let zone = rig.zone("Z");
    rig.equipment("E", "sensor", &zone, &[]);
    let mut cursor = stream(&mut rig, "/api/v1/equipments?type=switch");
    assert_eq!(chunks(&mut cursor, &rig.home), ["[]"]);
    // Exactly one chunk's worth: the closing bracket comes with the last item.
    let mut rig = busy(u8::try_from(CHUNK_ITEMS).unwrap());
    let mut cursor = stream(&mut rig, "/api/v1/devices");
    let parts = chunks(&mut cursor, &rig.home);
    assert_eq!(parts.len(), 1);
    assert_eq!(
        serde_json::from_str::<Value>(&parts[0])
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        CHUNK_ITEMS
    );
}

#[test]
fn a_list_changed_between_two_chunks_is_still_valid_json() {
    let mut rig = busy(20);
    let mut cursor = stream(&mut rig, "/api/v1/equipments");
    let first = cursor.next_chunk(&rig.home).unwrap().unwrap();
    // Between the chunks: the equipment that would come first of the second chunk is deleted,
    // a new one is added, an item of the third chunk is renamed.
    let all: Vec<Value> = {
        let mut other = busy(20);
        other
            .get("/api/v1/equipments")
            .1
            .as_array()
            .unwrap()
            .clone()
    };
    let _ = all;
    let names: Vec<String> = rig
        .home
        .equipments()
        .list()
        .iter()
        .map(|e| e.equipment.name.to_string())
        .collect();
    let mut by_name = names.clone();
    by_name.sort();
    let eight = rig.get("/api/v1/equipments").1[8]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let seventeen = rig.get("/api/v1/equipments").1[17]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(rig.delete(&format!("/api/v1/equipments/{eight}")).0, 204);
    let zone = rig.zone("Other");
    rig.equipment("Newcomer", "sensor", &zone, &[]);
    rig.put(
        &format!("/api/v1/equipments/{seventeen}"),
        json!({"name": "Renamed"}),
    );
    let mut rest = chunks(&mut cursor, &rig.home);
    rest.insert(0, first);
    let body: Value = serde_json::from_str(&rest.concat()).expect("valid JSON");
    let list = body.as_array().unwrap();
    // Deleted: skipped (19 left of the 20); added: not listed; renamed: its new name.
    assert_eq!(list.len(), 19);
    assert!(list.iter().all(|e| e["id"] != eight.as_str()));
    assert!(list.iter().all(|e| e["name"] != "Newcomer"));
    assert!(list.iter().any(|e| e["name"] == "Renamed"));
    // Every item of the first chunk and the others are whole equipments.
    assert!(
        list.iter()
            .all(|e| e["dataBindings"].is_array() && e["status"].is_string())
    );

    // The same for the devices, the last one deleted before its chunk.
    let mut rig = busy(10);
    let mut cursor = stream(&mut rig, "/api/v1/devices");
    let first = cursor.next_chunk(&rig.home).unwrap().unwrap();
    let last = rig.get("/api/v1/devices").1[9]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let second_to_last = rig.get("/api/v1/devices").1[8]["id"]
        .as_str()
        .unwrap()
        .to_string();
    rig.delete(&format!("/api/v1/devices/{last}"));
    rig.delete(&format!("/api/v1/devices/{second_to_last}"));
    let mut rest = chunks(&mut cursor, &rig.home);
    rest.insert(0, first);
    let body: Value = serde_json::from_str(&rest.concat()).expect("valid JSON");
    assert_eq!(body.as_array().unwrap().len(), 8);
    // Every item deleted: the array closes anyway.
    let mut rig = busy(10);
    let mut cursor = stream(&mut rig, "/api/v1/devices");
    let ids: Vec<String> = rig
        .home
        .devices
        .list()
        .iter()
        .map(|d| d.device.id.to_string())
        .collect();
    for id in ids {
        rig.home.remove_device(&id);
    }
    let parts = chunks(&mut cursor, &rig.home);
    assert_eq!(
        serde_json::from_str::<Value>(&parts.concat()).unwrap(),
        json!([])
    );
}

#[test]
fn the_files_written_from_cursors_are_the_files_built_whole() {
    let mut rig = busy(50);
    rig.measure(&temperature(1, 20.5));
    let built = (
        rig.home.devices.to_json().unwrap(),
        rig.home.zones().to_json().unwrap(),
        rig.home.equipments().to_json().unwrap(),
    );
    let streamed = |mut cursor: ListCursor, home: &Home| chunks(&mut cursor, home).concat();
    assert_eq!(
        streamed(ListCursor::saved_devices(&rig.home), &rig.home),
        built.0
    );
    assert_eq!(
        streamed(ListCursor::saved_zones(&rig.home), &rig.home),
        built.1
    );
    assert_eq!(
        streamed(ListCursor::saved_equipments(&rig.home), &rig.home),
        built.2
    );
    // The flags are cleared when the cursor is taken, and set again by a later change.
    assert!(rig.home.take_zones_cursor().is_some());
    assert!(rig.home.take_zones_cursor().is_none());
    assert!(rig.home.take_equipments_cursor().is_some());
    assert!(rig.home.take_equipments_cursor().is_none());
    rig.zone("More");
    assert!(rig.home.take_zones_cursor().is_some());
    // And they read back.
    let devices = mini_sowel_core::devices::parse_list(&built.0).unwrap();
    assert_eq!(devices, rig.home.devices.snapshot());
}

#[test]
fn equipments_are_sorted_by_exact_name_uppercase_before_lowercase() {
    let mut rig = Rig::new();
    let zone = rig.zone("Z");
    for name in ["beta", "Gamma", "alpha", "Delta"] {
        rig.equipment(name, "sensor", &zone, &[]);
    }
    let mut cursor = stream(&mut rig, "/api/v1/equipments");
    let all: Value = serde_json::from_str(&chunks(&mut cursor, &rig.home).concat()).unwrap();
    let names: Vec<&str> = all
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        ["Delta", "Gamma", "alpha", "beta"],
        "byte order, as SQLite"
    );
}

#[test]
fn an_empty_type_in_the_query_is_no_filter() {
    let mut rig = busy(6);
    for path in ["/api/v1/equipments?type=", "/api/v1/equipments?x=1&type="] {
        let mut cursor = stream(&mut rig, path);
        let all: Value = serde_json::from_str(&chunks(&mut cursor, &rig.home).concat()).unwrap();
        assert_eq!(all.as_array().unwrap().len(), 6, "{path}");
    }
    let (_, whole) = rig.get("/api/v1/equipments?type=");
    assert_eq!(whole.as_array().unwrap().len(), 6);
}
