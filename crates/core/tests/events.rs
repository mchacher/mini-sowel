//! The events recipes listen to (spec 012, R1, R8.1, R8.2): which are queued, the bound, and when
//! the zones' events are produced.

mod support;

use serde_json::json;
use solite_core::devices::Id;
use solite_core::home::Home;
use solite_core::home::events::{Event, EventValue, MAX_EVENTS, MAX_TEXT_BYTES};
use support::*;

fn id(text: &str) -> Id {
    Id::parse(text).unwrap()
}

fn events(rig: &Rig) -> Vec<Event> {
    rig.home.events().iter().cloned().collect()
}

/// Applies a message to the home as the radio loop does, without handing the events over.
fn apply(rig: &mut Rig, event: &solite_core::radio::MeasureEvent) {
    rig.home.measures(event, rig.now, &mut *rig.random).unwrap();
    rig.home.recompute(rig.now);
}

#[test]
fn a_report_of_the_same_value_twice_gives_two_device_events_and_two_equipment_events() {
    let mut rig = Rig::new();
    let g = rig.garage();
    // The failsafe makes the instance listen to the raw reports of its motion sensors.
    rig.garage_instance(&g, json!({"maxOnDuration": "5m"}));
    rig.turn();
    assert!(events(&rig).is_empty());
    for _ in 0..2 {
        apply(&mut rig, &motion(PIR, false));
    }
    let device_events: Vec<_> = events(&rig)
        .into_iter()
        .filter(|e| matches!(e, Event::Device { .. }))
        .collect();
    assert_eq!(device_events.len(), 2, "changed or not");
    assert_eq!(
        device_events[0],
        Event::Device {
            device: id(&g.motion_device),
            key: "occupancy".into(),
            value: EventValue::Bool(false)
        }
    );
    // The light's `state`, bound under the alias `state`.
    for _ in 0..2 {
        apply(&mut rig, &state(RELAY, true));
    }
    let light_events: Vec<_> = events(&rig)
        .into_iter()
        .filter(|e| matches!(e, Event::Equipment { .. }))
        .collect();
    assert_eq!(light_events.len(), 2);
    assert!(light_events.iter().all(|e| matches!(
        e,
        Event::Equipment { equipment, alias, value }
            if *equipment == id(&g.light) && *alias == "state" && *value == EventValue::Bool(true)
    )));
    assert_eq!(events(&rig)[0].name(), "device.data.updated");
}

#[test]
fn a_zone_event_is_queued_only_when_its_aggregate_changed_and_for_each_zone_that_changed() {
    let mut rig = Rig::new();
    let g = rig.garage();
    // A light in the root zone, to have an instance on the root zone, which the garage's motion
    // also changes.
    let relay = rig.relay(0x7B);
    let root_light = rig.equipment("Hall light", "light_onoff", ROOT, &[&relay]);
    rig.garage_instance(&g, json!({}));
    let (status, body) = rig.instance(json!({"zone": ROOT, "lights": [root_light]}));
    assert_eq!(status, 201, "{body}");
    rig.turn();
    assert!(events(&rig).is_empty());
    // Nothing changed: no event.
    rig.home.recompute(rig.now);
    rig.home.recompute(rig.now + 1000);
    assert!(events(&rig).is_empty());
    // Motion changed: the garage and its parent, the root.
    apply(&mut rig, &motion(PIR, true));
    let zones: Vec<Id> = events(&rig)
        .into_iter()
        .filter_map(|e| match e {
            Event::Zone {
                zone, motion: true, ..
            } => Some(zone),
            _ => None,
        })
        .collect();
    assert_eq!(zones.len(), 2, "{:?}", events(&rig));
    assert!(zones.contains(&id(&g.zone)) && zones.contains(&id(ROOT)));
    // Handed over, the queue is empty and a recompute with no change adds none.
    rig.turn();
    assert!(events(&rig).is_empty());
    rig.turn();
    assert!(events(&rig).is_empty());
}

#[test]
fn only_the_events_a_running_instance_listens_to_are_queued() {
    let mut rig = Rig::new();
    let g = rig.garage();
    // Another zone with its own sensor and light, and no instance on it.
    let other = rig.zone("Elsewhere");
    let pir2 = rig.motion_sensor(0x51);
    rig.equipment("Other motion", "sensor", &other, &[&pir2]);
    let relay2 = rig.relay(0x7B);
    rig.equipment("Other light", "light_onoff", &other, &[&relay2]);
    // No instance at all: nothing is queued, whatever happens.
    apply(&mut rig, &motion(PIR, true));
    apply(&mut rig, &state(RELAY, true));
    assert!(events(&rig).is_empty());
    rig.garage_instance(&g, json!({"maxOnDuration": "5m"}));
    rig.turn();
    // The other zone and its devices: no event.
    apply(&mut rig, &motion(0x51, true));
    apply(&mut rig, &state(0x7B, true));
    apply(&mut rig, &temperature(0x60, 1.0));
    assert!(events(&rig).is_empty(), "{:?}", events(&rig));
    // A data of the garage the instance does not read (the luminosity): no device or equipment
    // event, and a zone event (the aggregate changed).
    apply(&mut rig, &luminosity(PIR, 30.0));
    assert!(events(&rig).iter().all(|e| matches!(e, Event::Zone { .. })));
    assert_eq!(events(&rig).len(), 1);
}

#[test]
fn the_queue_holds_128_events_and_the_oldest_is_dropped() {
    let mut rig = Rig::new();
    let g = rig.garage();
    rig.garage_instance(&g, json!({}));
    rig.turn();
    // One event with a value no other has, then 128 more.
    apply(&mut rig, &state(RELAY, true));
    for _ in 0..MAX_EVENTS {
        rig.home
            .measures(&state(RELAY, false), rig.now, &mut *rig.random)
            .unwrap();
    }
    assert_eq!(rig.home.events().len(), MAX_EVENTS);
    assert!(
        events(&rig).iter().all(|e| matches!(
            e,
            Event::Equipment {
                value: EventValue::Bool(false),
                ..
            }
        )),
        "the oldest event (the true one) went"
    );
}

#[test]
fn a_text_value_is_cut_at_32_bytes_on_a_character_boundary() {
    let long = "é".repeat(40);
    match EventValue::from_json(&json!(long)) {
        EventValue::Text(text) => {
            assert_eq!(text.len(), 32);
            assert!(text.chars().all(|c| c == 'é'));
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(MAX_TEXT_BYTES, 32);
    assert_eq!(EventValue::from_json(&json!([1])), EventValue::Null);
    assert_eq!(EventValue::from_json(&json!(2.5)), EventValue::Number(2.5));
}

#[test]
fn an_edit_over_the_api_queues_its_zone_event_which_goes_to_the_instance_at_the_next_loop_turn() {
    let mut rig = Rig::new();
    let g = rig.garage();
    rig.garage_instance(&g, json!({}));
    rig.turn();
    assert!(events(&rig).is_empty());
    // A second motion sensor, already seeing a motion, is put in the garage over the API: the
    // aggregate of the zone changes on the HTTP thread, and the event waits.
    let pir2 = rig.motion_sensor(0x51);
    rig.measure(&motion(0x51, true));
    let (status, _) = rig.post(
        "/api/v1/equipments",
        json!({"name": "Second", "type": "sensor", "zoneId": g.zone, "deviceIds": [pir2]}),
    );
    assert_eq!(status, 201);
    let queued = events(&rig);
    assert_eq!(queued.len(), 1, "{queued:?}");
    assert!(matches!(
        queued[0],
        Event::Zone { zone, motion: true, .. } if zone == id(&g.zone)
    ));
    rig.sent();
    // The next loop turn hands it over: the recipe acts on the motion.
    rig.turn();
    assert!(events(&rig).is_empty());
    assert_eq!(rig.sent(), [solite_core::radio::zcl::Command::On]);
}

#[test]
fn the_first_recompute_after_a_start_gives_one_zone_event_per_zone_listened_to() {
    let mut rig = Rig::new();
    let g = rig.garage();
    let relay = rig.relay(0x7B);
    let root_light = rig.equipment("Hall light", "light_onoff", ROOT, &[&relay]);
    // A home as it is right after a start: it has computed no zone yet.
    rig.add_devices(Vec::new());
    rig.load_motion_light();
    assert!(events(&rig).is_empty());
    let one = rig
        .home
        .create_recipe_instance(
            "motion-light",
            json!({"zone": g.zone, "lights": [g.light]}),
            rig.now,
            &mut *rig.random,
        )
        .unwrap();
    let two = rig
        .home
        .create_recipe_instance(
            "motion-light",
            json!({"zone": ROOT, "lights": [root_light]}),
            rig.now,
            &mut *rig.random,
        )
        .unwrap();
    assert_ne!(one.id, two.id);
    // No previous aggregate: every zone listened to has changed.
    rig.home.recompute(rig.now);
    let zones: Vec<Id> = events(&rig)
        .into_iter()
        .filter_map(|e| match e {
            Event::Zone { zone, .. } => Some(zone),
            _ => None,
        })
        .collect();
    assert_eq!(zones.len(), 2);
    assert!(zones.contains(&id(&g.zone)) && zones.contains(&id(ROOT)));
    // And only that time.
    rig.turn();
    rig.home.recompute(rig.now);
    assert!(events(&rig).is_empty());
}

#[test]
fn a_home_with_no_instance_queues_nothing_and_the_recipes_loop_turn_costs_nothing() {
    let mut home = Home::new(T0);
    home.recompute(T0);
    home.run_recipes(T0, T0);
    assert!(home.events().is_empty());
}
