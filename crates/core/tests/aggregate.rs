//! Zone aggregation (spec 010, R4, R7.7, R7.8): Sowel's rules of `zone-aggregator.ts`, row by
//! row: means and their rounding, motion and `motionSince`, doors, windows, leaks, smoke, lights,
//! shutters, unavailable equipments, the zone tree, every field present.

mod support;

use serde_json::{Value, json};
use solite_core::devices::{Devices, format_iso};
use solite_core::home::Home;
use support::*;

fn text(value: &Value) -> &str {
    value.as_str().unwrap()
}

/// The aggregation of one zone.
fn aggregate(rig: &mut Rig, zone: &str) -> Value {
    let (status, body) = rig.get("/api/v1/zones/aggregation");
    assert_eq!(status, 200);
    body[zone].clone()
}

/// A device with one temperature binding, battery powered, with a value now.
fn thermometer(n: u8, name: &str, value: Value) -> Value {
    device_json(
        n,
        name,
        true,
        &[("temperature", "temperature", "number", value, Some(T0))],
    )
}

/// One device made from JSON, in an equipment of this type in this zone.
fn place(rig: &mut Rig, zone: &str, kind: &str, name: &str, device: Value) -> String {
    let id = device_json_id(&device);
    rig.add_devices(vec![device]);
    rig.equipment(name, kind, zone, &[&id])
}

#[test]
fn a_zone_with_no_equipment_has_every_field_of_sowels_type_with_its_empty_value() {
    let mut rig = Rig::new();
    let expected = json!({
        "temperature": null, "humidity": null, "luminosity": null,
        "motion": false, "motionSensors": 0, "motionSince": null,
        "openDoors": 0, "openWindows": 0, "waterLeak": false, "smoke": false,
        "lightsOn": 0, "lightsTotal": 0,
        "shuttersOpen": 0, "shuttersTotal": 0, "averageShutterPosition": null,
        "doorsOpen": 0, "doorsTotal": 0, "waterValvesOpen": 0, "waterValvesTotal": 0,
        "waterFlowTotal": null, "powerTotal": null,
        "sunrise": null, "sunset": null, "isDaylight": null,
        "displaysOnline": 0, "displaysTotal": 0,
        "unavailableEquipmentsByCategory": {},
        "unavailableEquipmentNamesByCategory": {}
    });
    assert_eq!(aggregate(&mut rig, ROOT), expected);
    // The same for a zone with equipments that have no value yet.
    let th = rig.sensor(1);
    let relay = rig.relay(3);
    let zone = rig.zone("Garage");
    rig.equipment("Climate", "sensor", &zone, &[&th]);
    let data = aggregate(&mut rig, &zone);
    assert_eq!(data["temperature"], Value::Null);
    assert_eq!(data["humidity"], Value::Null);
    assert_eq!(data["motion"], false);
    assert_eq!(data["lightsTotal"], 0);
    rig.equipment("Light", "light_onoff", &zone, &[&relay]);
    let data = aggregate(&mut rig, &zone);
    assert_eq!(
        (data["lightsTotal"].clone(), data["lightsOn"].clone()),
        (json!(1), json!(0)),
        "a light with no value yet is counted, not on"
    );
}

#[test]
fn the_temperature_is_the_mean_with_one_decimal() {
    let mut rig = Rig::new();
    let zone = rig.zone("Room");
    place(
        &mut rig,
        &zone,
        "sensor",
        "A",
        thermometer(1, "A", json!(19.0)),
    );
    place(
        &mut rig,
        &zone,
        "sensor",
        "B",
        thermometer(2, "B", json!(20.25)),
    );
    assert_eq!(aggregate(&mut rig, &zone)["temperature"], json!(19.6));
    // A whole mean is a whole number.
    let other = rig.zone("Other");
    place(
        &mut rig,
        &other,
        "sensor",
        "C",
        thermometer(3, "C", json!(20)),
    );
    place(
        &mut rig,
        &other,
        "sensor",
        "D",
        thermometer(4, "D", json!(22)),
    );
    let t = aggregate(&mut rig, &other)["temperature"].clone();
    assert_eq!(t.to_string(), "21");
    // Halves round up, as JavaScript's Math.round: 19.05 and 19.1 give 19.1 (19.075 * 10 = 190.75).
    let third = rig.zone("Third");
    place(
        &mut rig,
        &third,
        "sensor",
        "E",
        thermometer(5, "E", json!(19.05)),
    );
    place(
        &mut rig,
        &third,
        "sensor",
        "F",
        thermometer(6, "F", json!(19.1)),
    );
    assert_eq!(aggregate(&mut rig, &third)["temperature"], json!(19.1));
}

#[test]
fn only_a_temperature_binding_with_the_alias_temperature_counts() {
    let mut rig = Rig::new();
    let th = rig.sensor(1);
    let zone = rig.zone("Room");
    let eq = rig.equipment("Climate", "sensor", &zone, &[]);
    let (_, device) = rig.get(&format!("/api/v1/devices/{th}"));
    let temperature = text(&device["data"][0]["id"]).to_string();
    rig.post(
        &format!("/api/v1/equipments/{eq}/data-bindings"),
        json!({"deviceDataId": temperature, "alias": "temp"}),
    );
    rig.measure(&support::temperature(1, 18.0));
    assert_eq!(
        aggregate(&mut rig, &zone)["temperature"],
        Value::Null,
        "alias `temp` is not the temperature of the room"
    );
    rig.post(
        &format!("/api/v1/equipments/{eq}/data-bindings"),
        json!({"deviceDataId": temperature, "alias": "temperature"}),
    );
    assert_eq!(aggregate(&mut rig, &zone)["temperature"], json!(18));
}

#[test]
fn humidity_is_the_mean_with_one_decimal_and_luminosity_the_rounded_mean() {
    let mut rig = Rig::new();
    let zone = rig.zone("Room");
    let sensor = |n: u8, humidity: Value, luminosity: Value| {
        device_json(
            n,
            "S",
            true,
            &[
                ("humidity", "humidity", "number", humidity, Some(T0)),
                ("illuminance", "luminosity", "number", luminosity, Some(T0)),
            ],
        )
    };
    place(
        &mut rig,
        &zone,
        "sensor",
        "A",
        sensor(1, json!(50.0), json!(100)),
    );
    place(
        &mut rig,
        &zone,
        "sensor",
        "B",
        sensor(2, json!(55.5), json!(101)),
    );
    let data = aggregate(&mut rig, &zone);
    assert_eq!(data["humidity"], json!(52.8), "52.75 rounds up to 52.8");
    assert_eq!(data["luminosity"], json!(101), "100.5 rounds up to 101");
    // A value that is not a number (no value yet) is not counted.
    place(
        &mut rig,
        &zone,
        "sensor",
        "C",
        sensor(3, Value::Null, Value::Null),
    );
    assert_eq!(aggregate(&mut rig, &zone)["humidity"], json!(52.8));
    // The real devices of the catalog give the same through their measures.
    let mut rig = Rig::new();
    let th = rig.sensor(1);
    let zone = rig.zone("Room");
    rig.equipment("Climate", "sensor", &zone, &[&th]);
    rig.measure(&support::humidity(1, 61.25));
    assert_eq!(aggregate(&mut rig, &zone)["humidity"], json!(61.3));
}

#[test]
fn motion_follows_the_bindings_and_motion_since_changes_with_the_state_only() {
    let mut rig = Rig::new();
    let pir = rig.motion_sensor(2);
    let zone = rig.zone("Garage");
    // Creating the equipment is a change: the first computation.
    rig.now = T0 + 1000;
    let eq = rig.equipment("PIR", "sensor", &zone, &[&pir]);
    // Nobody asks for the aggregation for a while: the change itself brought it up to date.
    rig.now = T0 + 4000;
    let data = aggregate(&mut rig, &zone);
    assert_eq!(data["motionSensors"], 1);
    assert_eq!(data["motion"], false, "no value yet");
    assert_eq!(data["motionSince"], format_iso(T0 + 1000).as_str());

    // Motion: the time of the computation that sees it, kept by later ones.
    rig.now = T0 + 10_000;
    rig.measure(&support::motion(2, true));
    for later in [T0 + 10_000, T0 + 15_000, T0 + 60_000] {
        rig.now = later;
        let data = aggregate(&mut rig, &zone);
        assert_eq!(data["motion"], true);
        assert_eq!(
            data["motionSince"],
            format_iso(T0 + 10_000).as_str(),
            "at {later}"
        );
    }
    // Same state again: unchanged.
    rig.now = T0 + 70_000;
    rig.measure(&support::motion(2, true));
    assert_eq!(
        aggregate(&mut rig, &zone)["motionSince"],
        format_iso(T0 + 10_000).as_str()
    );
    // No motion: a new change.
    rig.now = T0 + 90_000;
    rig.measure(&support::motion(2, false));
    let data = aggregate(&mut rig, &zone);
    assert_eq!(data["motion"], false);
    assert_eq!(data["motionSince"], format_iso(T0 + 90_000).as_str());
    // The root sees it as well (its own time: the same computation).
    assert_eq!(
        aggregate(&mut rig, ROOT)["motionSince"],
        format_iso(T0 + 90_000).as_str()
    );

    // Without any motion sensor: null.
    rig.now = T0 + 100_000;
    rig.delete(&format!("/api/v1/equipments/{eq}"));
    let data = aggregate(&mut rig, &zone);
    assert_eq!(
        (data["motionSensors"].clone(), data["motionSince"].clone()),
        (json!(0), Value::Null)
    );
    // A sensor again: the first computation's time (the previous time was forgotten).
    rig.now = T0 + 110_000;
    rig.equipment("PIR", "sensor", &zone, &[&pir]);
    assert_eq!(
        aggregate(&mut rig, &zone)["motionSince"],
        format_iso(T0 + 110_000).as_str()
    );
}

#[test]
fn the_radio_loop_recomputes_after_the_measures_so_motion_since_is_the_time_of_the_change() {
    let mut rig = Rig::new();
    let pir = rig.motion_sensor(2);
    let zone = rig.zone("Garage");
    rig.equipment("PIR", "sensor", &zone, &[&pir]);
    // `measure` is what the loop does: the measures, then the recompute at the same time.
    rig.now = T0 + 5000;
    rig.measure(&support::motion(2, true));
    // Nobody asked for the aggregation until much later: the time is still the change's.
    rig.now = T0 + 3_600_000;
    assert_eq!(
        aggregate(&mut rig, &zone)["motionSince"],
        format_iso(T0 + 5000).as_str()
    );
}

#[test]
fn after_a_restart_the_first_computation_gives_the_time_of_motion_since() {
    let mut rig = Rig::new();
    let pir = rig.motion_sensor(2);
    let zone = rig.zone("Garage");
    rig.equipment("PIR", "sensor", &zone, &[&pir]);
    rig.now = T0 + 5000;
    rig.measure(&support::motion(2, true));
    assert_eq!(
        aggregate(&mut rig, &zone)["motionSince"],
        format_iso(T0 + 5000).as_str()
    );
    // The files are read again by a new process, much later.
    let home = Home::from_parts(
        Devices::from_list(rig.home.devices.snapshot()),
        rig.home.zones().list().to_vec(),
        rig.home.equipments().list().to_vec(),
        T0 + 100_000,
    );
    let mut restarted = Rig {
        home,
        now: T0 + 100_000,
        random: counter(),
    };
    let data = aggregate(&mut restarted, &zone);
    assert_eq!(data["motion"], true);
    assert_eq!(data["motionSince"], format_iso(T0 + 100_000).as_str());
}

#[test]
fn motion_sensors_counts_the_motion_bindings_of_every_equipment() {
    let mut rig = Rig::new();
    let zone = rig.zone("Hall");
    let two = device_json(
        1,
        "Two",
        true,
        &[
            ("occupancy", "motion", "boolean", json!(false), Some(T0)),
            ("occupancy2", "motion", "boolean", json!(true), Some(T0)),
        ],
    );
    place(&mut rig, &zone, "sensor", "Two motions", two);
    let one = device_json(
        2,
        "One",
        true,
        &[("occupancy", "motion", "boolean", json!(false), Some(T0))],
    );
    place(&mut rig, &zone, "sensor", "One motion", one);
    let data = aggregate(&mut rig, &zone);
    assert_eq!(data["motionSensors"], 3, "bindings, not equipments");
    assert_eq!(data["motion"], true, "one is enough");
}

#[test]
fn doors_windows_leak_and_smoke_follow_their_categories() {
    let mut rig = Rig::new();
    let zone = rig.zone("Hall");
    let contact = |n: u8, category: &str, value: Value| {
        device_json(
            n,
            "C",
            true,
            &[("contact", category, "boolean", value, Some(T0))],
        )
    };
    // A door contact is open when its value is false.
    place(
        &mut rig,
        &zone,
        "sensor",
        "Door A",
        contact(1, "contact_door", json!(false)),
    );
    place(
        &mut rig,
        &zone,
        "sensor",
        "Door B",
        contact(2, "contact_door", json!(true)),
    );
    place(
        &mut rig,
        &zone,
        "sensor",
        "Door C",
        contact(3, "contact_door", json!(false)),
    );
    place(
        &mut rig,
        &zone,
        "sensor",
        "Window A",
        contact(4, "contact_window", json!(false)),
    );
    let data = aggregate(&mut rig, &zone);
    assert_eq!(
        (data["openDoors"].clone(), data["openWindows"].clone()),
        (json!(2), json!(1))
    );
    assert_eq!(
        (data["waterLeak"].clone(), data["smoke"].clone()),
        (json!(false), json!(false))
    );
    let alarm = |n: u8, category: &str, value: Value| {
        device_json(
            n,
            "A",
            true,
            &[(category, category, "boolean", value, Some(T0))],
        )
    };
    place(
        &mut rig,
        &zone,
        "sensor",
        "Leak off",
        alarm(5, "water_leak", json!(false)),
    );
    place(
        &mut rig,
        &zone,
        "sensor",
        "Smoke off",
        alarm(6, "smoke", json!(false)),
    );
    let data = aggregate(&mut rig, &zone);
    assert_eq!(
        (data["waterLeak"].clone(), data["smoke"].clone()),
        (json!(false), json!(false))
    );
    place(
        &mut rig,
        &zone,
        "sensor",
        "Leak on",
        alarm(7, "water_leak", json!(true)),
    );
    let data = aggregate(&mut rig, &zone);
    assert_eq!(
        (data["waterLeak"].clone(), data["smoke"].clone()),
        (json!(true), json!(false))
    );
    place(
        &mut rig,
        &zone,
        "sensor",
        "Smoke on",
        alarm(8, "smoke", json!(true)),
    );
    assert_eq!(aggregate(&mut rig, &zone)["smoke"], true);
    // "ON" and "OFF" strings count as Sowel reads them.
    let other = rig.zone("Other");
    let strings = device_json(
        9,
        "S",
        true,
        &[
            ("c", "contact_door", "boolean", json!("OFF"), Some(T0)),
            ("m", "motion", "boolean", json!("ON"), Some(T0)),
        ],
    );
    place(&mut rig, &other, "sensor", "Strings", strings);
    let data = aggregate(&mut rig, &other);
    assert_eq!(
        (data["openDoors"].clone(), data["motion"].clone()),
        (json!(1), json!(true))
    );
}

#[test]
fn the_real_contact_device_is_a_door_so_open_windows_stays_zero() {
    let mut rig = Rig::new();
    let zone = rig.zone("Hall");
    let contact = device_json(
        1,
        "Contact",
        true,
        &[("contact", "contact_door", "boolean", json!(false), Some(T0))],
    );
    place(&mut rig, &zone, "sensor", "Contact", contact);
    let data = aggregate(&mut rig, &zone);
    assert_eq!(
        (data["openDoors"].clone(), data["openWindows"].clone()),
        (json!(1), json!(0))
    );
}

#[test]
fn lights_are_counted_once_per_light_equipment_on_when_a_channel_is_on() {
    let mut rig = Rig::new();
    let relay = rig.relay(3);
    let zone = rig.zone("Garage");
    rig.equipment("Light", "light_onoff", &zone, &[&relay]);
    let data = aggregate(&mut rig, &zone);
    assert_eq!(
        (data["lightsOn"].clone(), data["lightsTotal"].clone()),
        (json!(0), json!(1))
    );
    rig.measure(&support::state(3, true));
    let data = aggregate(&mut rig, &zone);
    assert_eq!(
        (data["lightsOn"].clone(), data["lightsTotal"].clone()),
        (json!(1), json!(1))
    );
    rig.measure(&support::state(3, false));
    assert_eq!(aggregate(&mut rig, &zone)["lightsOn"], 0);
    // A two-channel light is one light, on when any channel is.
    let two = device_json(
        1,
        "Two channels",
        false,
        &[
            ("state_l1", "light_state", "boolean", json!(false), Some(T0)),
            ("state_l2", "light_state", "boolean", json!(true), Some(T0)),
        ],
    );
    place(&mut rig, &zone, "light_dimmable", "Two", two);
    let data = aggregate(&mut rig, &zone);
    assert_eq!(
        (data["lightsOn"].clone(), data["lightsTotal"].clone()),
        (json!(1), json!(2))
    );
    // A light_state on a switch, a sensor or a shutter is not a light.
    let plug = device_json(
        2,
        "Plug",
        false,
        &[("state", "light_state", "boolean", json!(true), Some(T0))],
    );
    place(&mut rig, &zone, "switch", "Plug", plug);
    let data = aggregate(&mut rig, &zone);
    assert_eq!(
        (data["lightsOn"].clone(), data["lightsTotal"].clone()),
        (json!(1), json!(2))
    );
}

#[test]
fn shutters_count_the_shutter_type_only_with_their_position() {
    let mut rig = Rig::new();
    let zone = rig.zone("Living");
    let shutter = |n: u8, position: Value| {
        device_json(
            n,
            "S",
            false,
            &[("position", "shutter_position", "number", position, Some(T0))],
        )
    };
    place(&mut rig, &zone, "shutter", "S1", shutter(1, json!(0)));
    place(&mut rig, &zone, "shutter", "S2", shutter(2, json!(40)));
    place(&mut rig, &zone, "shutter", "S3", shutter(3, json!(60)));
    let data = aggregate(&mut rig, &zone);
    assert_eq!(data["shuttersTotal"], 3);
    assert_eq!(data["shuttersOpen"], 2, "position > 0");
    assert_eq!(data["averageShutterPosition"], json!(33), "100 / 3 rounded");
    // Another type with a position is not a shutter.
    place(
        &mut rig,
        &zone,
        "switch",
        "Not a shutter",
        shutter(4, json!(100)),
    );
    assert_eq!(aggregate(&mut rig, &zone)["shuttersTotal"], 3);
    // A shutter with no position yet is counted, not open, and has no average.
    let other = rig.zone("Other");
    place(&mut rig, &other, "shutter", "S5", shutter(5, Value::Null));
    let data = aggregate(&mut rig, &other);
    assert_eq!(data["shuttersTotal"], 1);
    assert_eq!(data["shuttersOpen"], 0);
    assert_eq!(data["averageShutterPosition"], Value::Null);
    // No device gives a position yet: with the real devices 0 and null.
    let mut rig = Rig::new();
    let relay = rig.relay(3);
    let zone = rig.zone("Z");
    rig.equipment("Shutter", "shutter", &zone, &[&relay]);
    let data = aggregate(&mut rig, &zone);
    assert_eq!(data["shuttersTotal"], 0);
    assert_eq!(data["averageShutterPosition"], Value::Null);
}

#[test]
fn an_offline_equipment_is_skipped_and_listed_once_per_category() {
    let mut rig = Rig::new();
    let zone = rig.zone("Cellar");
    let probe = device_json(
        1,
        "Probe",
        false,
        &[
            ("temperature", "temperature", "number", json!(30), Some(T0)),
            ("humidity", "humidity", "number", json!(80), Some(T0)),
            ("humidity2", "humidity", "number", json!(90), Some(T0)),
            ("occupancy", "motion", "boolean", json!(true), Some(T0)),
        ],
    );
    place(&mut rig, &zone, "sensor", "Probe", probe);
    let data = aggregate(&mut rig, &zone);
    assert_eq!(data["temperature"], json!(30));
    assert_eq!(data["unavailableEquipmentsByCategory"], json!({}));
    // Mains device silent for 11 minutes: offline.
    rig.now = T0 + 11 * MINUTE;
    let data = aggregate(&mut rig, &zone);
    assert_eq!(data["temperature"], Value::Null, "skipped");
    assert_eq!(data["humidity"], Value::Null);
    assert_eq!(data["motion"], false);
    assert_eq!(data["motionSensors"], 0);
    assert_eq!(
        data["unavailableEquipmentsByCategory"],
        json!({"temperature": 1, "humidity": 1, "motion": 1}),
        "once per equipment, not per binding (two humidity bindings)"
    );
    assert_eq!(
        data["unavailableEquipmentNamesByCategory"],
        json!({"temperature": ["Probe"], "humidity": ["Probe"], "motion": ["Probe"]})
    );
    // The root lists it as well.
    let root = aggregate(&mut rig, ROOT);
    assert_eq!(root["unavailableEquipmentsByCategory"]["temperature"], 1);
}

#[test]
fn unavailable_categories_follow_the_online_rules_and_names_are_sorted() {
    let mut rig = Rig::new();
    let zone = rig.zone("Z");
    let mains = |n: u8, name: &str, data: &[(&str, &str, &str, Value, Option<u64>)]| {
        device_json(n, name, false, data)
    };
    // A light is an unavailable light; a switch with a light_state is not.
    let state = [("state", "light_state", "boolean", json!(true), Some(T0))];
    place(
        &mut rig,
        &zone,
        "light_onoff",
        "b light",
        mains(1, "L", &state),
    );
    place(
        &mut rig,
        &zone,
        "light_onoff",
        "A light",
        mains(2, "L", &state),
    );
    place(
        &mut rig,
        &zone,
        "light_dimmable",
        "c light",
        mains(3, "L", &state),
    );
    place(&mut rig, &zone, "switch", "Plug", mains(4, "P", &state));
    // A temperature under another alias than `temperature` is not listed.
    let setpoint = [("setpoint", "temperature", "number", json!(19), Some(T0))];
    place(
        &mut rig,
        &zone,
        "thermostat",
        "Thermostat",
        mains(5, "T", &setpoint),
    );
    // A shutter position is listed for a shutter only.
    let position = [(
        "position",
        "shutter_position",
        "number",
        json!(50),
        Some(T0),
    )];
    place(
        &mut rig,
        &zone,
        "shutter",
        "Shutter",
        mains(6, "S", &position),
    );
    place(
        &mut rig,
        &zone,
        "switch",
        "Plug 2",
        mains(7, "P2", &position),
    );
    rig.now = T0 + 11 * MINUTE;
    let data = aggregate(&mut rig, &zone);
    assert_eq!(
        data["unavailableEquipmentNamesByCategory"],
        json!({
            "light_state": ["A light", "b light", "c light"],
            "shutter_position": ["Shutter"]
        })
    );
    assert_eq!(
        data["unavailableEquipmentsByCategory"],
        json!({"light_state": 3, "shutter_position": 1})
    );
    assert_eq!(
        data["lightsTotal"], 0,
        "offline lights are not counted as lights"
    );
    assert_eq!(data["shuttersTotal"], 0);
}

#[test]
fn an_equipment_with_no_data_binding_is_offline_and_listed_nowhere() {
    let mut rig = Rig::new();
    let relay = rig.relay(3);
    let zone = rig.zone("Z");
    rig.equipment("Empty", "sensor", &zone, &[]);
    rig.equipment("Order only", "light_onoff", &zone, &[]);
    let order =
        text(&rig.get(&format!("/api/v1/devices/{relay}")).1["orders"][0]["id"]).to_string();
    let id = text(&rig.get("/api/v1/equipments?type=light_onoff").1[0]["id"]).to_string();
    rig.post(
        &format!("/api/v1/equipments/{id}/order-bindings"),
        json!({"deviceOrderId": order, "alias": "state"}),
    );
    let data = aggregate(&mut rig, &zone);
    assert_eq!(data["unavailableEquipmentsByCategory"], json!({}));
    assert_eq!(data["lightsTotal"], 0);
}

#[test]
fn a_degraded_equipment_still_counts_and_a_disabled_one_too() {
    let mut rig = Rig::new();
    let zone = rig.zone("Room");
    // A battery thermometer whose reading is 70 minutes old: degraded (stale), still counted.
    place(
        &mut rig,
        &zone,
        "sensor",
        "Old",
        thermometer(1, "Old", json!(21)),
    );
    rig.now = T0 + 70 * MINUTE;
    let (_, equipments) = rig.get("/api/v1/equipments");
    assert_eq!(equipments[0]["status"], "degraded");
    assert_eq!(aggregate(&mut rig, &zone)["temperature"], json!(21));
    // `enabled: false` does not remove it from the aggregation (as in Sowel).
    let id = text(&equipments[0]["id"]).to_string();
    let (status, _) = rig.put(
        &format!("/api/v1/equipments/{id}"),
        json!({"enabled": false}),
    );
    assert_eq!(status, 200);
    let data = aggregate(&mut rig, &zone);
    assert_eq!(data["temperature"], json!(21));
    assert_eq!(data["unavailableEquipmentsByCategory"], json!({}));
}

#[test]
fn a_parent_zone_and_the_root_include_the_zones_below_them() {
    let mut rig = Rig::new();
    let house = rig.zone("House");
    let (_, garage) = rig.post(
        "/api/v1/zones",
        json!({"name": "Garage", "parentId": house}),
    );
    let garage = text(&garage["id"]).to_string();
    let (_, shelf) = rig.post(
        "/api/v1/zones",
        json!({"name": "Shelf", "parentId": garage}),
    );
    let shelf = text(&shelf["id"]).to_string();
    place(
        &mut rig,
        &house,
        "sensor",
        "House",
        thermometer(1, "H", json!(20)),
    );
    place(
        &mut rig,
        &garage,
        "sensor",
        "Garage",
        thermometer(2, "G", json!(10)),
    );
    place(
        &mut rig,
        &shelf,
        "sensor",
        "Shelf",
        thermometer(3, "S", json!(12)),
    );
    let relay = rig.relay(9);
    rig.equipment("Light", "light_onoff", &shelf, &[&relay]);
    let all = rig.get("/api/v1/zones/aggregation").1;
    assert_eq!(all[&shelf]["temperature"], json!(12));
    assert_eq!(all[&garage]["temperature"], json!(11), "garage and shelf");
    assert_eq!(
        all[&house]["temperature"],
        json!(14),
        "house, garage and shelf"
    );
    assert_eq!(all[ROOT]["temperature"], json!(14));
    assert_eq!(all[&shelf]["lightsTotal"], 1);
    assert_eq!(all[&garage]["lightsTotal"], 1);
    assert_eq!(all[ROOT]["lightsTotal"], 1);
    assert_eq!(all.as_object().unwrap().len(), 4, "every zone");
    // A sibling zone is not included.
    let other = rig.zone("Other");
    assert_eq!(
        rig.get("/api/v1/zones/aggregation").1[&other]["temperature"],
        Value::Null
    );
}

#[test]
fn moving_a_zone_moves_its_values_with_it() {
    let mut rig = Rig::new();
    let a = rig.zone("A");
    let b = rig.zone("B");
    place(
        &mut rig,
        &b,
        "sensor",
        "In B",
        thermometer(1, "B", json!(15)),
    );
    assert_eq!(aggregate(&mut rig, &a)["temperature"], Value::Null);
    rig.put(&format!("/api/v1/zones/{b}"), json!({"parentId": a}));
    assert_eq!(aggregate(&mut rig, &a)["temperature"], json!(15));
}

#[test]
fn unavailable_names_are_sorted_as_javascripts_locale_compare() {
    let mut rig = Rig::new();
    let zone = rig.zone("Z");
    let state = [("state", "light_state", "boolean", json!(true), Some(T0))];
    for (i, name) in ["b", "A", "a", "B"].iter().enumerate() {
        let device = device_json(u8::try_from(i).unwrap() + 1, "L", false, &state);
        place(&mut rig, &zone, "light_onoff", name, device);
    }
    rig.now = T0 + 11 * MINUTE;
    let data = aggregate(&mut rig, &zone);
    // Letters regardless of case; the same letter, lowercase first.
    assert_eq!(
        data["unavailableEquipmentNamesByCategory"]["light_state"],
        json!(["a", "A", "b", "B"])
    );
}

#[test]
fn a_device_turned_offline_by_silence_brings_the_aggregation_up_to_date_at_that_time() {
    let mut rig = Rig::new();
    let zone = rig.zone("Hall");
    // A mains sensor with motion, and a battery one without.
    let busy = device_json(
        1,
        "A",
        false,
        &[("occupancy", "motion", "boolean", json!(true), Some(T0))],
    );
    let calm = device_json(
        2,
        "B",
        true,
        &[("occupancy", "motion", "boolean", json!(false), Some(T0))],
    );
    place(&mut rig, &zone, "sensor", "A", busy);
    place(&mut rig, &zone, "sensor", "B", calm);
    assert_eq!(aggregate(&mut rig, &zone)["motion"], true);
    // 11 minutes of silence: a request that is not about the aggregation sees the mains device go
    // offline, and the motion state with it.
    rig.now = T0 + 11 * MINUTE;
    assert_eq!(rig.get("/api/v1/devices").0, 200);
    // Much later, the aggregation says since when: the time the silence was seen.
    rig.now = T0 + 30 * MINUTE;
    let data = aggregate(&mut rig, &zone);
    assert_eq!(data["motion"], false);
    assert_eq!(data["motionSensors"], 1);
    assert_eq!(data["motionSince"], format_iso(T0 + 11 * MINUTE).as_str());
}
