//! Live events (spec 013): the topics, the subscribe message, the queues and their bounds, the
//! dedup of a batch, and the events the home builds, with Sowel's types and fields
//! (`src/shared/types.ts` T:1819-1974). The events of orders are in `orders.rs` (they need the
//! radio's runner), the memory in `memory_ceiling.rs`.

mod support;

use mini_sowel_core::devices::{DeviceStatus, Id};
use mini_sowel_core::equipments::EquipmentStatus;
use mini_sowel_core::live::{
    CLOSE_TRY_AGAIN_LATER, ClientId, EngineEvent, LiveHandle, LiveHub, MAX_CLIENTS,
    MAX_QUEUED_EVENTS, Topic, topic_of_type,
};
use mini_sowel_core::orders::OrderSource;
use serde_json::{Value, json};
use support::{Rig, T0, motion, temperature};

/// A client of the hub, subscribed to these topics.
fn client(hub: &LiveHandle, topics: &[&str]) -> ClientId {
    let id = hub.connect().expect("a free slot");
    let message = json!({"type": "subscribe", "topics": topics}).to_string();
    assert!(hub.subscribe(id, &message));
    id
}

/// The events of the client's next batch, as a client reads them: one JSON array.
fn batch(hub: &LiveHandle, id: ClientId) -> Vec<Value> {
    match hub.take_batch(id) {
        None => Vec::new(),
        Some(text) => serde_json::from_str::<Value>(&text)
            .expect("a batch is JSON")
            .as_array()
            .expect("a batch is an array")
            .clone(),
    }
}

fn types(events: &[Value]) -> Vec<&str> {
    events
        .iter()
        .map(|e| e["type"].as_str().expect("a type"))
        .collect()
}

fn of_type<'a>(events: &'a [Value], name: &str) -> Vec<&'a Value> {
    events.iter().filter(|e| e["type"] == name).collect()
}

fn id(n: u8) -> Id {
    Id::from_random([n; 16])
}

fn device_data(device: &str, key: &str, value: i64) -> EngineEvent {
    EngineEvent::DeviceDataUpdated {
        device_id: Id::parse(device).expect("an id"),
        device_name: "Sensor".into(),
        data_id: id(9),
        key: key.into(),
        value: json!(value),
        previous: json!(0),
        timestamp: "2026-11-21T09:12:03.120Z".into(),
    }
}

// ---- The events, as Sowel's types write them ----

/// Every event of R4 that does not carry a zone, an equipment or an aggregate (those are
/// checked field by field where the home builds them).
fn simple_events() -> Vec<(EngineEvent, Value)> {
    let source = OrderSource::Recipe {
        instance_id: id(5),
        recipe_name: "Motion Light",
        recipe_id: "motion-light",
    };
    vec![
        (
            EngineEvent::DeviceRemoved {
                device_id: id(1),
                device_name: "Sensor".into(),
            },
            json!({"type": "device.removed", "deviceId": id(1).to_string(), "deviceName": "Sensor"}),
        ),
        (
            EngineEvent::DeviceStatusChanged {
                device_id: id(1),
                device_name: "Sensor".into(),
                status: DeviceStatus::Offline,
            },
            json!({"type": "device.status_changed", "deviceId": id(1).to_string(),
                   "deviceName": "Sensor", "status": "offline"}),
        ),
        (
            EngineEvent::DeviceDataUpdated {
                device_id: id(1),
                device_name: "Sensor".into(),
                data_id: id(2),
                key: "temperature".into(),
                value: json!(21.5),
                previous: json!(20),
                timestamp: "2026-11-21T09:12:03.120Z".into(),
            },
            json!({"type": "device.data.updated", "deviceId": id(1).to_string(),
                   "deviceName": "Sensor", "dataId": id(2).to_string(), "key": "temperature",
                   "value": 21.5, "previous": 20, "timestamp": "2026-11-21T09:12:03.120Z"}),
        ),
        (
            EngineEvent::ZoneRemoved {
                zone_id: id(3),
                zone_name: "Garage".into(),
            },
            json!({"type": "zone.removed", "zoneId": id(3).to_string(), "zoneName": "Garage"}),
        ),
        (
            EngineEvent::EquipmentRemoved {
                equipment_id: id(4),
                equipment_name: "Light".into(),
                zone_id: id(3),
            },
            json!({"type": "equipment.removed", "equipmentId": id(4).to_string(),
                   "equipmentName": "Light", "zoneId": id(3).to_string()}),
        ),
        (
            EngineEvent::EquipmentDataChanged {
                equipment_id: id(4),
                alias: "state".into(),
                value: json!(true),
                previous: json!(false),
            },
            json!({"type": "equipment.data.changed", "equipmentId": id(4).to_string(),
                   "alias": "state", "value": true, "previous": false}),
        ),
        (
            EngineEvent::EquipmentStatusChanged {
                equipment_id: id(4),
                equipment_name: "Light".into(),
                old_status: EquipmentStatus::Online,
                new_status: EquipmentStatus::Degraded,
            },
            json!({"type": "equipment.status.changed", "equipmentId": id(4).to_string(),
                   "equipmentName": "Light", "oldStatus": "online", "newStatus": "degraded"}),
        ),
        (
            EngineEvent::EquipmentOrderExecuted {
                equipment_id: id(4),
                order_alias: "state".into(),
                value: json!(true),
                source: None,
            },
            json!({"type": "equipment.order.executed", "equipmentId": id(4).to_string(),
                   "orderAlias": "state", "value": true}),
        ),
        (
            EngineEvent::EquipmentOrderExecuted {
                equipment_id: id(4),
                order_alias: "state".into(),
                value: json!("ON"),
                source: Some(source.clone()),
            },
            json!({"type": "equipment.order.executed", "equipmentId": id(4).to_string(),
                   "orderAlias": "state", "value": "ON",
                   "source": {"kind": "recipe", "instanceId": id(5).to_string(),
                              "recipeName": "Motion Light", "recipeId": "motion-light"}}),
        ),
        (
            EngineEvent::EquipmentOrderFailed {
                equipment_id: id(4),
                order_alias: "state".into(),
                value: json!(true),
                error: "device did not answer".into(),
                source: None,
            },
            json!({"type": "equipment.order.failed", "equipmentId": id(4).to_string(),
                   "orderAlias": "state", "value": true, "error": "device did not answer"}),
        ),
        (
            EngineEvent::RecipeInstanceCreated {
                instance_id: id(5),
                recipe_id: "motion-light".into(),
            },
            json!({"type": "recipe.instance.created", "instanceId": id(5).to_string(),
                   "recipeId": "motion-light"}),
        ),
        (
            EngineEvent::RecipeInstanceRemoved {
                instance_id: id(5),
                recipe_id: "motion-light".into(),
            },
            json!({"type": "recipe.instance.removed", "instanceId": id(5).to_string(),
                   "recipeId": "motion-light"}),
        ),
        (
            EngineEvent::RecipeInstanceStarted {
                instance_id: id(5),
                recipe_id: "motion-light".into(),
            },
            json!({"type": "recipe.instance.started", "instanceId": id(5).to_string(),
                   "recipeId": "motion-light"}),
        ),
        (
            EngineEvent::RecipeInstanceStopped {
                instance_id: id(5),
                recipe_id: "motion-light".into(),
            },
            json!({"type": "recipe.instance.stopped", "instanceId": id(5).to_string(),
                   "recipeId": "motion-light"}),
        ),
        (
            EngineEvent::RecipeInstanceStateChanged {
                instance_id: id(5),
                recipe_id: "motion-light".into(),
            },
            json!({"type": "recipe.instance.state.changed", "instanceId": id(5).to_string(),
                   "recipeId": "motion-light"}),
        ),
        (
            EngineEvent::RecipeInstanceError {
                instance_id: id(5),
                recipe_id: "motion-light".into(),
                error: "boom".into(),
            },
            json!({"type": "recipe.instance.error", "instanceId": id(5).to_string(),
                   "recipeId": "motion-light", "error": "boom"}),
        ),
    ]
}

#[test]
fn each_event_is_serialized_as_sowels_types_write_it() {
    for (event, expected) in simple_events() {
        let text = serde_json::to_string(&event).unwrap();
        let actual: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(actual, expected, "{text}");
        // `type` is the first field, as Sowel's objects are written.
        assert!(text.starts_with(&format!("{{\"type\":\"{}\"", event.type_name())));
    }
}

#[test]
fn the_topic_of_each_event_type_is_the_one_of_sowels_reference() {
    for (name, topic) in [
        ("device.data.updated", Topic::Devices),
        ("device.status_changed", Topic::Devices),
        ("zone.created", Topic::Zones),
        ("zone.data.changed", Topic::Zones),
        ("equipment.created", Topic::Equipments),
        ("equipment.order.executed", Topic::Equipments),
        ("equipment.order.failed", Topic::Equipments),
        ("equipment.status.changed", Topic::Equipments),
        ("recipe.instance.started", Topic::Recipes),
        ("recipe.instance.state.changed", Topic::Recipes),
        ("mode.activated", Topic::Modes),
        ("calendar.profile.changed", Topic::Calendar),
        ("mqtt-broker.connected", Topic::MqttPublishers),
        ("mqtt-publisher.error", Topic::MqttPublishers),
        ("activity.added", Topic::Activity),
        ("energy.arbiter.status", Topic::Energy),
        ("system.started", Topic::System),
        ("settings.changed", Topic::System),
        ("sunlight.changed", Topic::System),
    ] {
        assert_eq!(topic_of_type(name), topic, "{name}");
    }
    // The event's own topic is the one of its type name.
    for (event, _) in simple_events() {
        assert_eq!(event.topic(), topic_of_type(event.type_name()));
    }
}

// ---- Subscribe ----

#[test]
fn a_subscribe_replaces_the_topics_and_system_is_always_there() {
    let hub = LiveHub::handle();
    let c = client(&hub, &["devices", "zones"]);
    assert_eq!(
        hub.topics(c).unwrap().names(),
        ["devices", "zones", "system"]
    );
    assert!(hub.subscribe(c, r#"{"type":"subscribe","topics":["zones"]}"#));
    assert_eq!(hub.topics(c).unwrap().names(), ["zones", "system"]);
}

#[test]
fn unknown_names_an_empty_list_text_that_is_not_json_and_another_type_are_handled_as_sowel() {
    let hub = LiveHub::handle();
    let c = hub.connect().unwrap();
    // Unknown names and entries that are not strings are ignored.
    assert!(hub.subscribe(
        c,
        r#"{"type":"subscribe","topics":["nope","zones",1,null,"devices"]}"#
    ));
    assert_eq!(
        hub.topics(c).unwrap().names(),
        ["devices", "zones", "system"]
    );
    // An empty list: system only.
    assert!(hub.subscribe(c, r#"{"type":"subscribe","topics":[]}"#));
    assert_eq!(hub.topics(c).unwrap().names(), ["system"]);
    // The eleven names of Sowel are all accepted.
    assert!(hub.subscribe(
        c,
        r#"{"type":"subscribe","topics":["devices","equipments","zones","modes","recipes","calendar","mqtt-publishers","system","logs","activity","energy"]}"#
    ));
    assert_eq!(hub.topics(c).unwrap().names().len(), 11);
    // Everything else is ignored: the topics stay as they were.
    assert!(hub.subscribe(c, r#"{"type":"subscribe","topics":["zones"]}"#));
    for text in [
        "not json",
        "",
        r#"{"type":"other","topics":["devices"]}"#,
        r#"{"topics":["devices"]}"#,
        r#"{"type":"subscribe","topics":"devices"}"#,
        r#"{"type":"subscribe"}"#,
        r#"["subscribe"]"#,
        "42",
    ] {
        assert!(!hub.subscribe(c, text), "{text:?}");
        assert_eq!(
            hub.topics(c).unwrap().names(),
            ["zones", "system"],
            "{text:?}"
        );
    }
}

#[test]
fn before_any_subscribe_only_system_is_listened_to_and_a_device_event_is_not_queued() {
    let hub = LiveHub::handle();
    let c = hub.connect().unwrap();
    assert_eq!(hub.topics(c).unwrap().names(), ["system"]);
    assert!(!hub.wants(Topic::Devices));
    hub.emit(Topic::Devices, || device_data(&id(1).to_string(), "t", 1));
    assert_eq!(hub.queued(c), (0, 0));
    assert_eq!(hub.events_built(), 0);
    assert_eq!(hub.take_batch(c), None);
}

// ---- Batches and dedup ----

#[test]
fn ten_reports_of_one_key_then_one_of_another_make_two_events_the_first_at_its_last_place() {
    let hub = LiveHub::handle();
    let c = client(&hub, &["devices"]);
    let device = id(1).to_string();
    for n in 0..10 {
        hub.emit(Topic::Devices, || device_data(&device, "temperature", n));
    }
    hub.emit(Topic::Devices, || device_data(&device, "humidity", 50));
    let events = batch(&hub, c);
    assert_eq!(events.len(), 2);
    assert_eq!(events[0]["key"], "temperature");
    assert_eq!(events[0]["value"], 9, "the last one");
    assert_eq!(events[1]["key"], "humidity");
    // A key reported again moves to the last place.
    hub.emit(Topic::Devices, || device_data(&device, "temperature", 1));
    hub.emit(Topic::Devices, || device_data(&device, "humidity", 2));
    hub.emit(Topic::Devices, || device_data(&device, "temperature", 3));
    let events = batch(&hub, c);
    assert_eq!(
        events
            .iter()
            .map(|e| e["key"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["humidity", "temperature"]
    );
    assert_eq!(events[1]["value"], 3);
    // Another device with the same key is another key.
    let other = id(2).to_string();
    hub.emit(Topic::Devices, || device_data(&device, "temperature", 1));
    hub.emit(Topic::Devices, || device_data(&other, "temperature", 1));
    assert_eq!(batch(&hub, c).len(), 2);
}

#[test]
fn events_that_are_not_deduped_are_all_kept() {
    let hub = LiveHub::handle();
    let c = client(&hub, &["equipments", "recipes"]);
    for _ in 0..2 {
        hub.emit(Topic::Equipments, || EngineEvent::EquipmentRemoved {
            equipment_id: id(1),
            equipment_name: "A".into(),
            zone_id: id(2),
        });
        hub.emit(Topic::Recipes, || EngineEvent::RecipeInstanceStateChanged {
            instance_id: id(3),
            recipe_id: "motion-light".into(),
        });
    }
    let events = batch(&hub, c);
    assert_eq!(events.len(), 4);
}

#[test]
fn equipment_created_twice_is_two_events() {
    let mut rig = Rig::new();
    let zone = rig.zone("Garage");
    let sensor = rig.sensor(0x10);
    rig.equipment("Climate", "sensor", &zone, &[&sensor]);
    let equipment = rig.home.equipments().list()[0].equipment.clone();
    let hub = LiveHub::handle();
    let c = client(&hub, &["equipments"]);
    for _ in 0..2 {
        let equipment = equipment.clone();
        hub.emit(Topic::Equipments, || EngineEvent::EquipmentCreated {
            equipment,
        });
    }
    let events = batch(&hub, c);
    assert_eq!(types(&events), ["equipment.created", "equipment.created"]);
    assert_eq!(events[0], events[1]);
    assert_eq!(events[0]["equipment"]["name"], "Climate");
}

#[test]
fn no_event_waiting_gives_no_batch_and_the_batch_is_a_flat_array_in_emission_order() {
    let hub = LiveHub::handle();
    let c = client(&hub, &["devices", "zones"]);
    assert_eq!(hub.take_batch(c), None);
    let device = id(1).to_string();
    hub.emit(Topic::Devices, || device_data(&device, "a", 1));
    hub.emit(Topic::Zones, || EngineEvent::ZoneRemoved {
        zone_id: id(2),
        zone_name: "Z".into(),
    });
    hub.emit(Topic::Devices, || device_data(&device, "b", 1));
    let text = hub.take_batch(c).unwrap();
    assert!(text.starts_with("[{") && text.ends_with("}]"), "{text}");
    let events: Vec<Value> = serde_json::from_str(&text).unwrap();
    assert_eq!(
        types(&events),
        ["device.data.updated", "zone.removed", "device.data.updated"]
    );
    // Taken once: nothing left.
    assert_eq!(hub.take_batch(c), None);
    assert_eq!(hub.queued(c), (0, 0));
}

#[test]
fn an_event_goes_only_to_the_clients_that_listen_to_its_topic() {
    let hub = LiveHub::handle();
    let devices = client(&hub, &["devices"]);
    let zones = client(&hub, &["zones"]);
    let device = id(1).to_string();
    hub.emit(Topic::Devices, || device_data(&device, "a", 1));
    assert_eq!(batch(&hub, devices).len(), 1);
    assert_eq!(batch(&hub, zones).len(), 0);
}

#[test]
fn with_no_client_nothing_is_built_and_with_one_on_zones_no_device_event_is_built() {
    let mut rig = Rig::new();
    let zone = rig.zone("Garage");
    let sensor = rig.sensor(0x10);
    rig.equipment("Climate", "sensor", &zone, &[&sensor]);
    let hub = rig.home.live();
    rig.report(&temperature(0x10, 20.0));
    rig.report(&temperature(0x10, 21.0));
    assert_eq!(hub.events_built(), 0, "no client: no event built");
    // A client of zones only: the zone's aggregate is built, no device or equipment event.
    let c = client(&hub, &["zones"]);
    rig.report(&temperature(0x10, 25.0));
    let events = batch(&hub, c);
    // The garage's aggregate and the root's, which holds the garage's.
    assert_eq!(types(&events), ["zone.data.changed", "zone.data.changed"]);
    assert_eq!(hub.events_built(), 2);
}

// ---- Bounds ----

#[test]
fn the_257th_event_closes_the_client_with_1013_and_another_client_is_unaffected() {
    let hub = LiveHub::handle();
    let slow = client(&hub, &["devices"]);
    let other = client(&hub, &["zones"]);
    let device = id(1).to_string();
    // Distinct keys, so none is deduped; short names keep the 8 KB away.
    for n in 0..MAX_QUEUED_EVENTS {
        hub.emit(Topic::Devices, || device_data(&device, &format!("k{n}"), 1));
    }
    // The cost of an event is its JSON and overhead: 256 of them take well over 8 KB.
    assert_eq!(hub.close_request(slow), Some(CLOSE_TRY_AGAIN_LATER));
    assert_eq!(hub.queued(slow), (0, 0), "the queue is freed");
    assert_eq!(hub.close_request(other), None);
    hub.emit(Topic::Zones, || EngineEvent::ZoneRemoved {
        zone_id: id(2),
        zone_name: "Z".into(),
    });
    assert_eq!(batch(&hub, other).len(), 1);
    // A client being closed gets nothing more, and is not counted as a listener.
    assert!(!hub.wants(Topic::Devices));
}

#[test]
fn the_event_count_bound_alone_closes_a_client_at_257_small_events() {
    // Tiny events: 256 of them fit 8 KB only if each costs 32 bytes; with the overhead they do
    // not, so the byte bound is the one met first. The count bound is checked on its own by
    // a client whose events are as small as the type allows.
    let hub = LiveHub::handle();
    let c = client(&hub, &["recipes"]);
    let mut pushed = 0usize;
    while hub.close_request(c).is_none() {
        hub.emit(Topic::Recipes, || EngineEvent::RecipeInstanceStarted {
            instance_id: id(1),
            recipe_id: "m".into(),
        });
        pushed += 1;
        assert!(pushed <= MAX_QUEUED_EVENTS + 1);
    }
    // Never past either bound.
    assert!(pushed <= MAX_QUEUED_EVENTS);
}

#[test]
fn the_8_kb_bound_counts_the_overhead_of_every_entry() {
    let hub = LiveHub::handle();
    let c = client(&hub, &["devices"]);
    let device = id(1).to_string();
    let mut last = (0, 0);
    let mut n = 0;
    while hub.close_request(c).is_none() {
        hub.emit(Topic::Devices, || {
            device_data(&device, &format!("key{n}"), 1)
        });
        let now = hub.queued(c);
        if hub.close_request(c).is_none() {
            last = now;
        }
        n += 1;
    }
    // The queue was under 8 KB (overhead included) at its fullest, and the next event did not fit.
    assert!(last.1 <= 8 * 1024, "{last:?}");
    assert!(last.1 > 8 * 1024 - 600, "{last:?}");
    // The overhead is counted: more than the text alone.
    let json_only: usize = serde_json::to_string(&device_data(&device, "key10", 1))
        .unwrap()
        .len()
        * last.0;
    assert!(last.1 > json_only, "{last:?} against {json_only}");
}

#[test]
fn a_fifth_client_is_refused_and_a_removed_client_frees_its_slot() {
    let hub = LiveHub::handle();
    let ids: Vec<ClientId> = (0..MAX_CLIENTS).map(|_| hub.connect().unwrap()).collect();
    assert_eq!(hub.client_count(), 4);
    assert_eq!(hub.connect(), None);
    hub.disconnect(ids[1]);
    assert_eq!(hub.client_count(), 3);
    let again = hub.connect().expect("the slot is free again");
    assert!(!ids.contains(&again), "numbers are not reused");
    assert_eq!(hub.connect(), None);
}

#[test]
fn events_that_arrive_while_a_batch_is_written_wait_for_the_next_one() {
    let hub = LiveHub::handle();
    let c = client(&hub, &["devices"]);
    let device = id(1).to_string();
    hub.emit(Topic::Devices, || device_data(&device, "a", 1));
    hub.emit(Topic::Devices, || device_data(&device, "b", 1));
    assert_eq!(hub.begin_batch(c), 2);
    let first = hub.pop(c).unwrap();
    // The same key as the one being sent: a new event, not a replacement.
    hub.emit(Topic::Devices, || device_data(&device, "b", 2));
    let second = hub.pop(c).unwrap();
    assert!(first.contains("\"key\":\"a\"") && second.contains("\"key\":\"b\""));
    assert!(second.contains("\"value\":1"));
    assert_eq!(hub.pop(c), None, "the batch ends");
    let events = batch(&hub, c);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["value"], 2);
}

// ---- The events the home builds ----

/// A home with a client of every topic the home feeds.
fn listening(rig: &Rig) -> (LiveHandle, ClientId) {
    let hub = rig.home.live();
    let c = client(&hub, &["devices", "equipments", "zones", "recipes"]);
    (hub, c)
}

#[test]
fn deleting_a_device_is_a_device_removed_event() {
    let mut rig = Rig::new();
    let sensor = rig.sensor(0x10);
    let (hub, c) = listening(&rig);
    let (status, _) = rig.delete(&format!("/api/v1/devices/{sensor}"));
    assert_eq!(status, 204);
    let events = batch(&hub, c);
    assert_eq!(of_type(&events, "device.removed").len(), 1);
    let events: Vec<Value> = events
        .into_iter()
        .filter(|e| e["type"] == "device.removed")
        .collect();
    assert_eq!(events[0]["deviceId"], sensor.as_str());
    assert!(events[0]["deviceName"].is_string());
}

#[test]
fn a_report_gives_device_data_updated_and_an_equipment_data_changed_per_binding() {
    let mut rig = Rig::new();
    let zone = rig.zone("Garage");
    let sensor = rig.sensor(0x10);
    // Two equipments bind the same device: each of its bindings gets an event.
    let first = rig.equipment("First", "sensor", &zone, &[&sensor]);
    let second = rig.equipment("Second", "sensor", &zone, &[&sensor]);
    rig.report(&temperature(0x10, 20.0));
    let (hub, c) = listening(&rig);
    rig.report(&temperature(0x10, 21.5));
    let events = batch(&hub, c);
    let updated = of_type(&events, "device.data.updated");
    assert_eq!(updated.len(), 1);
    let update = updated[0];
    assert_eq!(update["deviceId"], sensor.as_str());
    assert_eq!(update["key"], "temperature");
    assert_eq!(update["value"], 21.5);
    assert_eq!(update["previous"], 20);
    assert!(update["dataId"].is_string() && update["deviceName"].is_string());
    let timestamp = update["timestamp"].as_str().unwrap();
    assert!(timestamp.starts_with("2026-11-21T09:12:03"), "{timestamp}");
    let changed = of_type(&events, "equipment.data.changed");
    let mut ids: Vec<&str> = changed
        .iter()
        .map(|e| e["equipmentId"].as_str().unwrap())
        .collect();
    ids.sort_unstable();
    let mut wanted = vec![first.as_str(), second.as_str()];
    wanted.sort_unstable();
    assert_eq!(ids, wanted);
    for event in &changed {
        assert_eq!(event["alias"], "temperature");
        assert_eq!(event["value"], 21.5);
        assert_eq!(event["previous"], 20);
    }
    // The same value again is still an update (changed or not), as the recipes' events are.
    rig.report(&temperature(0x10, 21.5));
    let events = batch(&hub, c);
    assert_eq!(of_type(&events, "device.data.updated").len(), 1);
    assert_eq!(of_type(&events, "device.data.updated")[0]["previous"], 21.5);
}

#[test]
fn a_device_status_change_and_the_status_of_its_equipment_are_two_events() {
    let mut rig = Rig::new();
    let zone = rig.zone("Garage");
    let sensor = rig.sensor(0x10);
    let equipment = rig.equipment("Climate", "sensor", &zone, &[&sensor]);
    let (hub, c) = listening(&rig);
    // The device is heard: the equipment's status is seen for the first time (it is only noted:
    // no event).
    rig.report(&temperature(0x10, 20.0));
    let events = batch(&hub, c);
    assert!(of_type(&events, "equipment.status.changed").is_empty());
    // A battery device silent for more than 25 hours is offline.
    rig.now = T0 + 26 * support::HOUR;
    assert!(rig.home.devices.refresh_status(rig.now));
    rig.home.recompute(rig.now);
    let events = batch(&hub, c);
    let device_status = of_type(&events, "device.status_changed");
    assert_eq!(device_status.len(), 1);
    assert_eq!(device_status[0]["deviceId"], sensor.as_str());
    assert_eq!(device_status[0]["status"], "offline");
    let status = of_type(&events, "equipment.status.changed");
    assert_eq!(status.len(), 1);
    assert_eq!(status[0]["equipmentId"], equipment.as_str());
    assert_eq!(status[0]["equipmentName"], "Climate");
    assert_eq!(status[0]["oldStatus"], "online");
    assert_eq!(status[0]["newStatus"], "offline");
    // Nothing changed since: nothing more.
    rig.home.recompute(rig.now + 1);
    assert!(batch(&hub, c).is_empty());
}

#[test]
fn an_equipment_status_seen_for_the_first_time_is_silent_and_a_reading_going_stale_is_told_by_the_minute_check()
 {
    let mut rig = Rig::new();
    let zone = rig.zone("Garage");
    let sensor = rig.sensor(0x10);
    rig.report(&temperature(0x10, 20.0));
    let (hub, c) = listening(&rig);
    // A new equipment: its status is noted at the next recompute, with no status event.
    let equipment = rig.equipment("Climate", "sensor", &zone, &[&sensor]);
    let events = batch(&hub, c);
    assert_eq!(of_type(&events, "equipment.created").len(), 1, "{events:?}");
    assert!(of_type(&events, "equipment.status.changed").is_empty());
    rig.home.recompute(rig.now);
    assert!(of_type(&batch(&hub, c), "equipment.status.changed").is_empty());
    // 66 minutes later the temperature (windows of 65 minutes) is stale while the battery
    // device is still online: nobody sent anything, the check by the minute sees it.
    rig.home.tick(rig.now + 59 * 1000);
    assert!(
        batch(&hub, c).is_empty(),
        "less than a minute since the last recompute"
    );
    rig.now += 66 * support::MINUTE;
    rig.home.tick(rig.now);
    let events = batch(&hub, c);
    let status = of_type(&events, "equipment.status.changed");
    assert_eq!(status.len(), 1, "{events:?}");
    assert_eq!(status[0]["equipmentId"], equipment.as_str());
    assert_eq!(status[0]["oldStatus"], "online");
    assert_eq!(status[0]["newStatus"], "degraded");
    // The status tracker forgets a deleted equipment.
    let (code, _) = rig.delete(&format!("/api/v1/equipments/{equipment}"));
    assert_eq!(code, 204);
    assert_eq!(of_type(&batch(&hub, c), "equipment.removed").len(), 1);
}

#[test]
fn zones_are_created_updated_and_deleted_with_their_events_and_an_aggregate_change_is_told() {
    let mut rig = Rig::new();
    let sensor = rig.sensor(0x10);
    let (hub, c) = listening(&rig);
    let zone = rig.zone("Garage");
    let events = batch(&hub, c);
    assert_eq!(of_type(&events, "zone.created").len(), 1);
    // The new zone's first aggregate is told too.
    assert_eq!(of_type(&events, "zone.data.changed").len(), 2);
    let events: Vec<Value> = events
        .into_iter()
        .filter(|e| e["type"] == "zone.created")
        .collect();
    assert_eq!(events[0]["zone"]["id"], zone.as_str());
    assert_eq!(events[0]["zone"]["name"], "Garage");
    assert!(events[0]["zone"]["createdAt"].is_string());
    let (status, _) = rig.put(
        &format!("/api/v1/zones/{zone}"),
        json!({"name": "Workshop"}),
    );
    assert_eq!(status, 200);
    let events = batch(&hub, c);
    assert_eq!(types(&events), ["zone.updated"]);
    assert_eq!(events[0]["zone"]["name"], "Workshop");
    // An equipment in the zone, then a temperature: the aggregate of the zone changes.
    rig.equipment("Climate", "sensor", &zone, &[&sensor]);
    batch(&hub, c);
    rig.report(&temperature(0x10, 22.0));
    let events = batch(&hub, c);
    let aggregates: Vec<&Value> = events
        .iter()
        .filter(|e| e["type"] == "zone.data.changed" && e["zoneId"] == zone.as_str())
        .collect();
    assert_eq!(aggregates.len(), 1, "{events:?}");
    assert_eq!(aggregates[0]["aggregatedData"]["temperature"], 22);
    // The same value again changes nothing: no zone event.
    rig.report(&temperature(0x10, 22.0));
    assert!(of_type(&batch(&hub, c), "zone.data.changed").is_empty());
    // Delete: the equipment first (a zone with one is refused), then the zone.
    let equipment = rig.home.equipments().list()[0].equipment.id.to_string();
    rig.delete(&format!("/api/v1/equipments/{equipment}"));
    batch(&hub, c);
    let (status, _) = rig.delete(&format!("/api/v1/zones/{zone}"));
    assert_eq!(status, 204);
    let events = batch(&hub, c);
    assert_eq!(types(&events), ["zone.removed"]);
    assert_eq!(events[0]["zoneId"], zone.as_str());
    assert_eq!(events[0]["zoneName"], "Workshop");
}

#[test]
fn equipments_are_created_updated_deleted_and_a_binding_added_is_an_update() {
    let mut rig = Rig::new();
    let zone = rig.zone("Garage");
    let sensor = rig.sensor(0x10);
    let relay = rig.relay(0x7A);
    let (hub, c) = listening(&rig);
    let equipment = rig.equipment("Climate", "sensor", &zone, &[&sensor]);
    let events = batch(&hub, c);
    assert_eq!(types(&events), ["equipment.created"]);
    assert_eq!(events[0]["equipment"]["id"], equipment.as_str());
    assert_eq!(events[0]["equipment"]["name"], "Climate");
    assert_eq!(events[0]["equipment"]["zoneId"], zone.as_str());
    let (status, _) = rig.put(
        &format!("/api/v1/equipments/{equipment}"),
        json!({"name": "Climate 2"}),
    );
    assert_eq!(status, 200);
    let events = batch(&hub, c);
    assert_eq!(types(&events), ["equipment.updated"]);
    assert_eq!(events[0]["equipment"]["name"], "Climate 2");
    // A data binding added.
    let (_, device) = rig.get(&format!("/api/v1/devices/{relay}"));
    let data = device["data"][0]["id"].as_str().unwrap().to_string();
    let (status, body) = rig.post(
        &format!("/api/v1/equipments/{equipment}/data-bindings"),
        json!({"deviceDataId": data, "alias": "relay"}),
    );
    assert_eq!(status, 201, "{body}");
    let events = batch(&hub, c);
    assert_eq!(types(&events), ["equipment.updated"]);
    // And removed.
    let binding = body["id"].as_str().unwrap();
    let (status, _) = rig.delete(&format!(
        "/api/v1/equipments/{equipment}/data-bindings/{binding}"
    ));
    assert_eq!(status, 204);
    assert_eq!(types(&batch(&hub, c)), ["equipment.updated"]);
    let (status, _) = rig.delete(&format!("/api/v1/equipments/{equipment}"));
    assert_eq!(status, 204);
    let events = batch(&hub, c);
    assert_eq!(types(&events), ["equipment.removed"]);
    assert_eq!(events[0]["equipmentId"], equipment.as_str());
    assert_eq!(events[0]["equipmentName"], "Climate 2");
    assert_eq!(events[0]["zoneId"], zone.as_str());
}

// ---- Recipes ----

#[test]
fn recipe_instances_tell_their_life_as_sowels_recipe_manager_does() {
    let mut rig = Rig::new();
    let garage = rig.garage();
    let (hub, c) = listening(&rig);
    // Created and started.
    let instance = rig.garage_instance(&garage, json!({}));
    let events = batch(&hub, c);
    let recipes: Vec<&Value> = events
        .iter()
        .filter(|e| e["type"].as_str().unwrap().starts_with("recipe."))
        .collect();
    assert_eq!(
        recipes
            .iter()
            .map(|e| e["type"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["recipe.instance.created", "recipe.instance.started"]
    );
    for event in &recipes {
        assert_eq!(event["instanceId"], instance.as_str());
        assert_eq!(event["recipeId"], "motion-light");
    }
    // A PUT: stopped, then started.
    let params = json!({"zone": garage.zone, "lights": [garage.light], "timeout": "30s"});
    let (status, _) = rig.put(
        &format!("/api/v1/recipe-instances/{instance}"),
        json!({"params": params}),
    );
    assert_eq!(status, 200);
    assert_eq!(
        types(&batch(&hub, c)),
        ["recipe.instance.stopped", "recipe.instance.started"]
    );
    // A state key written: the end of the motion arms the off timer's key.
    rig.report(&motion(support::PIR, true));
    rig.report(&support::state(support::RELAY, true));
    rig.advance(1000);
    batch(&hub, c);
    rig.report(&motion(support::PIR, false));
    let events = batch(&hub, c);
    assert!(
        types(&events).contains(&"recipe.instance.state.changed"),
        "{events:?}"
    );
    // Disabling a running instance: stopped (and nothing else); enabling: started.
    let (status, _) = rig.post(
        &format!("/api/v1/recipe-instances/{instance}/disable"),
        json!({}),
    );
    assert_eq!(status, 200);
    assert_eq!(types(&batch(&hub, c)), ["recipe.instance.stopped"]);
    let (status, _) = rig.post(
        &format!("/api/v1/recipe-instances/{instance}/enable"),
        json!({}),
    );
    assert_eq!(status, 200);
    assert_eq!(types(&batch(&hub, c)), ["recipe.instance.started"]);
    // Disabled again, the light gone: enabling cannot start it, which is told as a state
    // change; disabling an instance that is enabled but not running is one too.
    rig.post(
        &format!("/api/v1/recipe-instances/{instance}/disable"),
        json!({}),
    );
    batch(&hub, c);
    let (status, _) = rig.delete(&format!("/api/v1/equipments/{}", garage.light));
    assert_eq!(status, 204);
    batch(&hub, c);
    rig.post(
        &format!("/api/v1/recipe-instances/{instance}/enable"),
        json!({}),
    );
    assert_eq!(types(&batch(&hub, c)), ["recipe.instance.state.changed"]);
    rig.post(
        &format!("/api/v1/recipe-instances/{instance}/disable"),
        json!({}),
    );
    assert_eq!(types(&batch(&hub, c)), ["recipe.instance.state.changed"]);
    // Disabled already: nothing at all.
    rig.post(
        &format!("/api/v1/recipe-instances/{instance}/disable"),
        json!({}),
    );
    assert!(batch(&hub, c).is_empty());
    // Removed.
    let (status, _) = rig.delete(&format!("/api/v1/recipe-instances/{instance}"));
    assert_eq!(status, 204);
    assert_eq!(types(&batch(&hub, c)), ["recipe.instance.removed"]);
}

#[test]
fn deleting_a_running_instance_is_stopped_then_removed() {
    let mut rig = Rig::new();
    let garage = rig.garage();
    let instance = rig.garage_instance(&garage, json!({}));
    let (hub, c) = listening(&rig);
    let (status, _) = rig.delete(&format!("/api/v1/recipe-instances/{instance}"));
    assert_eq!(status, 204);
    assert_eq!(
        types(&batch(&hub, c)),
        ["recipe.instance.stopped", "recipe.instance.removed"]
    );
}

#[test]
fn no_recipe_event_is_built_for_a_client_that_does_not_listen_to_recipes() {
    let mut rig = Rig::new();
    let garage = rig.garage();
    let hub = rig.home.live();
    let c = client(&hub, &["devices"]);
    rig.garage_instance(&garage, json!({}));
    assert!(batch(&hub, c).is_empty());
    assert_eq!(hub.events_built(), 0);
}

#[test]
fn an_instance_that_cannot_be_restored_is_logged_and_is_never_an_error_event() {
    use mini_sowel_core::recipes::RecipeInstance;
    let mut rig = Rig::new();
    let (hub, c) = listening(&rig);
    let broken = RecipeInstance {
        id: id(7),
        recipe_id: "motion-light".into(),
        params: r#"{"zone":"nope","lights":[],"timeout":"20s"}"#.into(),
        enabled: true,
        created_at: T0,
    };
    rig.home.load_recipes(vec![broken]);
    rig.home.start_recipes(1, T0);
    let events = batch(&hub, c);
    assert!(
        events
            .iter()
            .all(|e| !e["type"].as_str().unwrap().starts_with("recipe.")),
        "{events:?}"
    );
    let log = rig.messages(&id(7).to_string());
    assert_eq!(log.len(), 1);
    assert!(log[0].starts_with("Failed to restore: "), "{log:?}");
}

#[test]
fn the_five_dedup_keys_each_keep_only_the_last_event_of_a_key() {
    let hub = LiveHub::handle();
    let c = client(&hub, &["devices", "equipments", "zones"]);
    let device = id(1);
    let status = |s| EngineEvent::DeviceStatusChanged {
        device_id: device,
        device_name: "S".into(),
        status: s,
    };
    let equipment_data = |v: i64| EngineEvent::EquipmentDataChanged {
        equipment_id: id(2),
        alias: "state".into(),
        value: json!(v),
        previous: json!(0),
    };
    let equipment_status = |s| EngineEvent::EquipmentStatusChanged {
        equipment_id: id(2),
        equipment_name: "E".into(),
        old_status: EquipmentStatus::Online,
        new_status: s,
    };
    // d: `device.data.updated`
    for n in 0..3 {
        hub.emit(Topic::Devices, || device_data(&device.to_string(), "t", n));
    }
    // ds
    hub.emit(Topic::Devices, || status(DeviceStatus::Offline));
    hub.emit(Topic::Devices, || status(DeviceStatus::Online));
    // e
    hub.emit(Topic::Equipments, || equipment_data(1));
    hub.emit(Topic::Equipments, || equipment_data(2));
    // es
    hub.emit(Topic::Equipments, || {
        equipment_status(EquipmentStatus::Degraded)
    });
    hub.emit(Topic::Equipments, || {
        equipment_status(EquipmentStatus::Offline)
    });
    // z: two aggregates of one zone
    let aggregate = Rig::new().home.aggregate(T0)[0].1.clone();
    for _ in 0..2 {
        let data = aggregate.clone();
        hub.emit(Topic::Zones, || EngineEvent::ZoneDataChanged {
            zone_id: id(3),
            aggregated_data: Box::new(data),
        });
    }
    let events = batch(&hub, c);
    assert_eq!(
        types(&events),
        [
            "device.data.updated",
            "device.status_changed",
            "equipment.data.changed",
            "equipment.status.changed",
            "zone.data.changed"
        ]
    );
    assert_eq!(events[0]["value"], 2);
    assert_eq!(events[1]["status"], "online");
    assert_eq!(events[2]["value"], 2);
    assert_eq!(events[3]["newStatus"], "offline");
}
