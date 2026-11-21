//! The device registry: data and orders per cluster, pairing, measures, status, bounds, and
//! when the devices are written.

use std::cell::RefCell;
use std::sync::{Arc, Mutex};
use std::thread;

use mini_sowel_core::devices::{
    DeviceStatus, DeviceStore, DeviceWithDetails, Devices, MAX_DATA, MAX_DEVICES, MAX_ORDERS,
    PowerSource, VALUE_WRITE_INTERVAL_MS, ValueType, WritePolicy, data_for_cluster, lock,
    orders_for_cluster, parse_list,
};
use mini_sowel_core::radio::measures::{Measure, Value, decode};
use mini_sowel_core::radio::{Activity, MeasureEvent, PairingEvent, Sender};

const T0: u64 = 1_795_252_323_120; // 2026-11-21T09:12:03.120Z
const MINUTE: u64 = 60_000;
const HOUR: u64 = 60 * MINUTE;
const EUI: [u8; 8] = [0x00, 0x12, 0x4B, 0x00, 0x12, 0x34, 0x56, 0x78];

fn eui(last: u8) -> [u8; 8] {
    [0x00, 0x12, 0x4B, 0x00, 0x12, 0x34, 0x56, last]
}

/// Random bytes that differ from call to call.
fn counter() -> impl FnMut(&mut [u8]) -> Result<(), String> {
    let mut n = 0u8;
    move |buffer: &mut [u8]| {
        n = n.wrapping_add(1);
        buffer.fill(n);
        Ok(())
    }
}

fn snzb02(devices: &mut Devices, random: &mut impl FnMut(&mut [u8]) -> Result<(), String>) {
    devices
        .identified(EUI, Some("eWeLink"), Some("TH01"), T0, random)
        .unwrap();
    devices
        .reporting_configured(EUI, &[0x0001, 0x0402, 0x0405], T0, random)
        .unwrap();
}

fn temperature(value: f64) -> MeasureEvent {
    MeasureEvent {
        sender: Sender::Eui64(EUI),
        endpoint: 1,
        measures: vec![Measure {
            kind: "temperature",
            value: Value::Number(value),
            unit: "°C",
            device_value: None,
        }],
    }
}

fn keys(device: &DeviceWithDetails) -> Vec<&str> {
    device.data.iter().map(|d| &*d.key).collect()
}

#[test]
fn every_row_of_the_cluster_table_gives_its_data() {
    // (cluster, key, type, category, unit)
    let table = [
        (
            0x0402,
            "temperature",
            ValueType::Number,
            "temperature",
            Some("°C"),
        ),
        (0x0405, "humidity", ValueType::Number, "humidity", Some("%")),
        (
            0x0403,
            "pressure",
            ValueType::Number,
            "pressure",
            Some("hPa"),
        ),
        (
            0x0400,
            "illuminance",
            ValueType::Number,
            "luminosity",
            Some("lx"),
        ),
        (0x0406, "occupancy", ValueType::Boolean, "motion", None),
        (0x0006, "state", ValueType::Boolean, "light_state", None),
        (
            0x0008,
            "brightness",
            ValueType::Number,
            "light_brightness",
            None,
        ),
    ];
    for (cluster, key, value_type, category, unit) in table {
        let data = data_for_cluster(cluster);
        assert_eq!(data.len(), 1, "cluster {cluster:#06X}");
        assert_eq!(
            (
                data[0].key,
                data[0].value_type,
                data[0].category,
                data[0].unit
            ),
            (key, value_type, category, unit),
            "cluster {cluster:#06X}"
        );
    }
    let power = data_for_cluster(0x0001);
    assert_eq!(
        power
            .iter()
            .map(|d| (d.key, d.category, d.unit))
            .collect::<Vec<_>>(),
        vec![
            ("battery", "battery", Some("%")),
            ("voltage", "voltage", Some("mV"))
        ]
    );
    assert!(data_for_cluster(0x0500).is_empty());
    // The orders: on/off and level only.
    assert_eq!(orders_for_cluster(0x0006)[0].category, "light_toggle");
    assert_eq!(orders_for_cluster(0x0008)[0].category, "set_brightness");
    assert!(orders_for_cluster(0x0402).is_empty());
}

#[test]
fn voltage_is_in_millivolts_and_brightness_keeps_the_device_scale() {
    let mut devices = Devices::new();
    let mut random = counter();
    devices
        .reporting_configured(EUI, &[0x0001, 0x0008], T0, &mut random)
        .unwrap();
    let battery = decode(0x0001, 0x0020, 0x20, &[30]).unwrap(); // 3.0 V
    let level = decode(0x0008, 0x0000, 0x20, &[128]).unwrap(); // 50 % shown
    devices
        .measures(
            &MeasureEvent {
                sender: Sender::Eui64(EUI),
                endpoint: 1,
                measures: vec![battery, level],
            },
            T0,
            &mut counter(),
        )
        .unwrap();
    let item = &devices.list()[0];
    let value = |key: &str| {
        item.data
            .iter()
            .find(|d| d.key == key)
            .unwrap()
            .value
            .clone()
    };
    assert_eq!(value("voltage"), serde_json::json!(3000));
    assert_eq!(value("brightness"), serde_json::json!(128));
}

#[test]
fn pairing_a_snzb02_gives_temperature_humidity_battery_and_voltage() {
    let mut devices = Devices::new();
    snzb02(&mut devices, &mut counter());
    let item = &devices.list()[0];
    assert_eq!(item.device.name, "TH01 5678");
    assert_eq!(
        keys(item),
        ["temperature", "humidity", "battery", "voltage"]
    );
    assert!(item.orders.is_empty());
    assert_eq!(
        item.device.ieee_address.as_deref(),
        Some("0x00124b0012345678")
    );
}

#[test]
fn pairing_the_same_address_again_keeps_the_ids_and_completes_the_data() {
    let mut devices = Devices::new();
    let mut random = counter();
    devices
        .identified(EUI, Some("eWeLink"), Some("TH01"), T0, &mut random)
        .unwrap();
    devices
        .reporting_configured(EUI, &[0x0402], T0, &mut random)
        .unwrap();
    let before = devices.list()[0].clone();
    devices
        .rename(&before.device.id.to_string(), "Garage", T0)
        .unwrap();
    devices
        .identified(EUI, Some("eWeLink"), Some("TH01"), T0 + 5000, &mut random)
        .unwrap();
    devices
        .reporting_configured(EUI, &[0x0402, 0x0405], T0 + 5000, &mut random)
        .unwrap();
    assert_eq!(devices.list().len(), 1);
    let after = &devices.list()[0];
    assert_eq!(after.device.id, before.device.id);
    assert_eq!(after.device.name, "Garage");
    assert_eq!(after.data[0].id, before.data[0].id);
    assert_eq!(keys(after), ["temperature", "humidity"]);
    assert_eq!(after.device.updated_at, T0 + 5000);
}

#[test]
fn a_measure_sets_the_value_and_last_seen_but_not_updated_at() {
    let mut devices = Devices::new();
    snzb02(&mut devices, &mut counter());
    let updated = devices.list()[0].device.updated_at;
    devices
        .measures(&temperature(19.52), T0 + 3000, &mut counter())
        .unwrap();
    let item = &devices.list()[0];
    assert_eq!(item.data[0].value, serde_json::json!(19.52));
    assert_eq!(item.data[0].last_updated, Some(T0 + 3000));
    assert_eq!(item.device.last_seen, Some(T0 + 3000));
    assert_eq!(item.device.updated_at, updated);
}

#[test]
fn a_measure_from_a_device_not_in_the_store_is_ignored() {
    let mut devices = Devices::new();
    devices
        .measures(&temperature(19.52), T0, &mut counter())
        .unwrap();
    assert!(devices.list().is_empty());
    assert_eq!(devices.changes(), (false, false));
}

#[test]
fn a_measure_for_a_data_the_device_lacks_is_ignored() {
    let mut devices = Devices::new();
    devices
        .reporting_configured(EUI, &[0x0405], T0, &mut counter())
        .unwrap();
    devices
        .measures(&temperature(19.52), T0 + 1000, &mut counter())
        .unwrap();
    let item = &devices.list()[0];
    assert_eq!(keys(item), ["humidity"]);
    assert!(item.data[0].value.is_null());
}

fn status_after(power: Option<u8>, silence: Option<u64>) -> DeviceStatus {
    let mut devices = Devices::new();
    if let Some(capability) = power {
        devices.announced(EUI, capability, T0);
    }
    devices
        .identified(EUI, None, Some("TH01"), T0, &mut counter())
        .unwrap();
    match silence {
        None => {
            // Never a message: a device read from storage without `lastSeen`.
            let mut list = devices.snapshot();
            list[0].device.last_seen = None;
            let mut devices = Devices::from_list(list);
            devices.refresh_status(T0);
            devices.list()[0].device.status
        }
        Some(ms) => {
            devices.refresh_status(T0 + ms);
            devices.list()[0].device.status
        }
    }
}

#[test]
fn status_follows_the_silence_by_power_source() {
    let mains = Some(0x8C);
    let battery = Some(0x80);
    assert_eq!(status_after(battery, Some(24 * HOUR)), DeviceStatus::Online);
    assert_eq!(
        status_after(battery, Some(26 * HOUR)),
        DeviceStatus::Offline
    );
    assert_eq!(status_after(mains, Some(9 * MINUTE)), DeviceStatus::Online);
    assert_eq!(
        status_after(mains, Some(11 * MINUTE)),
        DeviceStatus::Offline
    );
    assert_eq!(status_after(None, Some(26 * HOUR)), DeviceStatus::Offline);
    assert_eq!(status_after(None, None), DeviceStatus::Unknown);
    // A message brings the device back online, and changes `updatedAt`.
    let mut devices = Devices::new();
    devices.announced(EUI, 0x80, T0);
    devices
        .identified(EUI, None, Some("TH01"), T0, &mut counter())
        .unwrap();
    devices.refresh_status(T0 + 26 * HOUR);
    assert_eq!(devices.list()[0].device.status, DeviceStatus::Offline);
    devices
        .measures(&temperature(20.0), T0 + 26 * HOUR + 1000, &mut counter())
        .unwrap();
    let device = &devices.list()[0].device;
    assert_eq!(device.status, DeviceStatus::Online);
    assert_eq!(device.updated_at, T0 + 26 * HOUR + 1000);
}

#[test]
fn the_65th_device_the_17th_data_and_the_5th_order_are_not_added() {
    let mut devices = Devices::new();
    let mut random = counter();
    for i in 0..MAX_DEVICES {
        let notes = devices
            .identified(eui(u8::try_from(i).unwrap()), None, None, T0, &mut random)
            .unwrap();
        assert!(notes.is_empty());
    }
    let notes = devices
        .identified(eui(0xF0), None, Some("TH01"), T0, &mut random)
        .unwrap();
    assert_eq!(
        notes,
        ["pairing: device store full (64), 0x00124b00123456f0 ignored"]
    );
    assert_eq!(devices.list().len(), MAX_DEVICES);
    // Said once per device.
    let again = devices
        .identified(eui(0xF0), None, Some("TH01"), T0, &mut random)
        .unwrap();
    assert!(again.is_empty());

    // One device holding 16 data and 4 orders: a 17th data and a 5th order are not added.
    let mut one = Devices::new();
    one.reporting_configured(EUI, &[0x0402, 0x0006], T0, &mut random)
        .unwrap();
    let mut list = one.snapshot();
    let (data, order) = (list[0].data[0].clone(), list[0].orders[0].clone());
    // The data is full of other keys (not `temperature`), the orders too.
    list[0].data.clear();
    while list[0].data.len() < MAX_DATA {
        let mut more = data.clone();
        more.key = format!("extra{}", list[0].data.len()).into();
        list[0].data.push(more);
    }
    while list[0].orders.len() < MAX_ORDERS {
        let mut more = order.clone();
        more.key = format!("extra{}", list[0].orders.len()).into();
        list[0].orders.push(more);
    }
    let mut full = Devices::from_list(list);
    full.reporting_configured(EUI, &[0x0402, 0x0006, 0x0008], T0, &mut random)
        .unwrap();
    let item = &full.list()[0];
    assert_eq!(item.data.len(), MAX_DATA);
    assert_eq!(item.orders.len(), MAX_ORDERS);
    assert!(!item.data.iter().any(|d| d.key == "temperature"));
    assert!(!item.orders.iter().any(|o| o.key == "brightness"));
}

#[test]
fn relay_announce_gives_capability_0x8c_and_a_mains_power_source() {
    use mini_sowel_sim::{Kind, Ncp, SimDevice, harness};
    let mut device = SimDevice::of_kind(
        Kind::Relay,
        [0x7A, 0x56, 0x34, 0x12, 0x00, 0x4B, 0x12, 0x00],
        None,
        None,
    );
    device.auto_join = true;
    device.nwk = 0x1111;
    let mut ncp = Ncp::recording();
    ncp.devices = vec![device];
    let mut h = harness(ncp);
    h.radio.identify().unwrap();
    h.radio
        .start_network(
            &mini_sowel_core::radio::NetworkSettings::default(),
            &mut |b: &mut [u8]| {
                b.fill(7);
                Ok(())
            },
            &mut |_| Ok(()),
        )
        .unwrap();
    h.radio.permit_join(60).unwrap();
    let mut events = Vec::new();
    let mut activity = Vec::new();
    let end = h.clock.get() + 60_000;
    loop {
        events.extend(h.radio.poll_pairing().unwrap());
        activity.extend(h.radio.take_activity());
        if !h.radio.pairing_busy() || h.clock.get() >= end {
            break;
        }
    }
    let wire = [0x00, 0x12, 0x4B, 0x00, 0x12, 0x34, 0x56, 0x7A];
    assert!(activity.contains(&Activity::Announced {
        eui64: wire,
        capability: 0x8C
    }));
    let mut devices = Devices::new();
    let mut random = counter();
    for a in &activity {
        devices.apply_activity(a, T0);
    }
    for event in &events {
        devices.apply(event, T0, &mut random).unwrap();
    }
    let item = &devices.list()[0];
    assert_eq!(item.device.power_source, PowerSource::Mains);
    assert_eq!(keys(item), ["state"]);
    assert_eq!(item.orders[0].key, "state");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, PairingEvent::Identified { .. }))
    );
}

/// A store that keeps what it is given and counts the writes, with the thread that wrote.
#[derive(Default)]
struct MemoryStore {
    saved: RefCell<Vec<Vec<DeviceWithDetails>>>,
}

impl DeviceStore for MemoryStore {
    fn load(&self) -> Result<Vec<DeviceWithDetails>, String> {
        Ok(self.saved.borrow().last().cloned().unwrap_or_default())
    }
    fn save(&self, json: &str) -> Result<(), String> {
        self.saved.borrow_mut().push(parse_list(json)?);
        Ok(())
    }
}

/// One turn of the writer: the snapshot if one is due, written.
fn turn(
    policy: &mut WritePolicy,
    devices: &mut Devices,
    store: &MemoryStore,
    now: u64,
    stopping: bool,
) {
    if let Some(snapshot) = policy.due(devices, now, stopping) {
        store.save(&snapshot).unwrap();
    }
}

#[test]
fn a_store_gives_back_what_it_was_given() {
    let mut devices = Devices::new();
    snzb02(&mut devices, &mut counter());
    let store = MemoryStore::default();
    store.save(&devices.to_json().unwrap()).unwrap();
    assert_eq!(
        Devices::from_list(store.load().unwrap()).list(),
        devices.list()
    );
}

#[test]
fn values_changing_for_9_minutes_are_not_written_and_at_10_minutes_one_write_is() {
    let mut devices = Devices::new();
    snzb02(&mut devices, &mut counter());
    let store = MemoryStore::default();
    let mut policy = WritePolicy::new(T0);
    turn(&mut policy, &mut devices, &store, T0, false); // the pairing: definitions
    assert_eq!(store.saved.borrow().len(), 1);
    for second in 1..=540 {
        devices
            .measures(
                &temperature(20.0 + (second as f64) / 100.0),
                T0 + second * 1000,
                &mut counter(),
            )
            .unwrap();
        turn(&mut policy, &mut devices, &store, T0 + second * 1000, false);
    }
    assert_eq!(
        store.saved.borrow().len(),
        1,
        "no value write before 10 minutes"
    );
    devices
        .measures(
            &temperature(30.0),
            T0 + VALUE_WRITE_INTERVAL_MS,
            &mut counter(),
        )
        .unwrap();
    turn(
        &mut policy,
        &mut devices,
        &store,
        T0 + VALUE_WRITE_INTERVAL_MS,
        false,
    );
    assert_eq!(store.saved.borrow().len(), 2);
    assert_eq!(
        store.saved.borrow()[1][0].data[0].value,
        serde_json::json!(30)
    );
    // Nothing changed since: nothing written.
    turn(
        &mut policy,
        &mut devices,
        &store,
        T0 + 2 * VALUE_WRITE_INTERVAL_MS,
        false,
    );
    assert_eq!(store.saved.borrow().len(), 2);
}

#[test]
fn a_rename_is_written_at_once() {
    let mut devices = Devices::new();
    snzb02(&mut devices, &mut counter());
    let store = MemoryStore::default();
    let mut policy = WritePolicy::new(T0);
    turn(&mut policy, &mut devices, &store, T0, false);
    let id = devices.list()[0].device.id.to_string();
    devices.rename(&id, "Garage", T0 + 1000).unwrap();
    turn(&mut policy, &mut devices, &store, T0 + 1000, false);
    assert_eq!(store.saved.borrow().len(), 2);
    assert_eq!(store.saved.borrow()[1][0].device.name, "Garage");
}

#[test]
fn stopping_writes_the_changed_values() {
    let mut devices = Devices::new();
    snzb02(&mut devices, &mut counter());
    let store = MemoryStore::default();
    let mut policy = WritePolicy::new(T0);
    turn(&mut policy, &mut devices, &store, T0, false);
    devices
        .measures(&temperature(21.5), T0 + 2000, &mut counter())
        .unwrap();
    turn(&mut policy, &mut devices, &store, T0 + 2000, false);
    assert_eq!(store.saved.borrow().len(), 1);
    turn(&mut policy, &mut devices, &store, T0 + 3000, true);
    assert_eq!(store.saved.borrow().len(), 2);
    assert_eq!(
        store.saved.borrow()[1][0].data[0].value,
        serde_json::json!(21.5)
    );
}

#[test]
fn a_failed_write_is_tried_again_after_the_interval_not_in_a_loop() {
    let mut devices = Devices::new();
    snzb02(&mut devices, &mut counter());
    let mut policy = WritePolicy::new(T0);
    assert!(policy.due(&mut devices, T0, false).is_some());
    policy.failed(&mut devices, T0);
    assert!(policy.due(&mut devices, T0 + 1000, false).is_none());
    assert!(
        policy
            .due(&mut devices, T0 + VALUE_WRITE_INTERVAL_MS, false)
            .is_some()
    );
}

/// A store that remembers the threads that wrote.
struct ThreadStore {
    writers: Mutex<Vec<thread::ThreadId>>,
    last: Mutex<Vec<DeviceWithDetails>>,
}

impl DeviceStore for ThreadStore {
    fn load(&self) -> Result<Vec<DeviceWithDetails>, String> {
        Ok(Vec::new())
    }
    fn save(&self, json: &str) -> Result<(), String> {
        self.writers.lock().unwrap().push(thread::current().id());
        *self.last.lock().unwrap() = parse_list(json)?;
        Ok(())
    }
}

#[test]
fn a_rename_from_another_thread_is_written_by_the_one_writer_and_not_lost() {
    let mut devices = Devices::new();
    snzb02(&mut devices, &mut counter());
    let id = devices.list()[0].device.id.to_string();
    let shared = Arc::new(Mutex::new(devices));
    let store = Arc::new(ThreadStore {
        writers: Mutex::new(Vec::new()),
        last: Mutex::new(Vec::new()),
    });
    let writer = {
        let (shared, store) = (Arc::clone(&shared), Arc::clone(&store));
        thread::spawn(move || {
            let mut policy = WritePolicy::new(T0);
            let mut now = T0;
            let mut renamed = false;
            while !renamed {
                now += 1;
                let snapshot = policy.due(&mut lock(&shared), now, false);
                if let Some(snapshot) = snapshot {
                    store.save(&snapshot).unwrap();
                    renamed = snapshot.contains(r#""name":"Garage""#);
                }
                // A measure now and then, as the radio loop does.
                lock(&shared)
                    .measures(&temperature(20.0), now, &mut counter())
                    .unwrap();
                thread::yield_now();
            }
            thread::current().id()
        })
    };
    // The HTTP thread: renames under the lock, never writes.
    lock(&shared).rename(&id, "Garage", T0 + 5).unwrap();
    let writer_id = writer.join().unwrap();
    let writers = store.writers.lock().unwrap();
    assert!(!writers.is_empty());
    assert!(writers.iter().all(|w| *w == writer_id));
    assert_eq!(store.last.lock().unwrap()[0].device.name, "Garage");
}

// ---- Spec 009: the data of IAS zone devices ----

use mini_sowel_core::radio::measures::{self, from_zone_status};
use mini_sowel_core::radio::zcl::parse_attribute_report;
use mini_sowel_core::radio::{Enrollment, EnrollmentOutcome};
use serde_json::Value as Json;

type Random<'a> = &'a mut dyn FnMut(&mut [u8]) -> Result<(), String>;

/// A sensor with an IAS zone of this type, enrolled, paired through the radio's events.
fn zone_sensor(devices: &mut Devices, zone_type: Option<u16>, random: Random) {
    for event in [
        PairingEvent::Identified {
            eui64: EUI,
            manufacturer: Some("eWeLink".into()),
            model: Some("MS01".into()),
        },
        PairingEvent::ReportingConfigured {
            eui64: EUI,
            ok: vec![0x0001],
            failed: vec![],
            no_descriptor: false,
            enrollment: Some(Enrollment {
                zone_type,
                outcome: EnrollmentOutcome::Enrolled,
            }),
        },
    ] {
        devices.apply(&event, T0, random).unwrap();
    }
}

fn zone_event(status: u16) -> PairingEvent {
    PairingEvent::Measures(MeasureEvent {
        sender: Sender::Eui64(EUI),
        endpoint: 1,
        measures: from_zone_status(status),
    })
}

/// The value of one data of the only device.
fn value_of(devices: &Devices, key: &str) -> Option<Json> {
    devices.list()[0]
        .data
        .iter()
        .find(|d| d.key == key)
        .map(|d| d.value.clone())
}

#[test]
fn every_zone_type_gives_its_data_and_an_unknown_one_gives_none() {
    let table = [
        (0x000D, Some(("occupancy", "motion"))),
        (0x0015, Some(("contact", "contact_door"))),
        (0x002A, Some(("water_leak", "water_leak"))),
        (0x0028, Some(("smoke", "smoke"))),
        (0x002D, None),
    ];
    for (zone_type, expected) in table {
        let mut random = counter();
        let mut devices = Devices::new();
        zone_sensor(&mut devices, Some(zone_type), &mut random);
        let device = &devices.list()[0];
        match expected {
            Some((key, category)) => {
                assert_eq!(
                    keys(device),
                    vec![key, "battery", "voltage"],
                    "{zone_type:#06X}"
                );
                let data = &device.data[0];
                assert_eq!(data.value_type, ValueType::Boolean);
                assert_eq!(&*data.category, category);
            }
            None => assert_eq!(keys(device), vec!["battery", "voltage"], "{zone_type:#06X}"),
        }
        // The tamper data come with the first notification that sets the bit, not with the zone.
        assert!(!keys(device).contains(&"tamper"));
    }
}

#[test]
fn an_alarm_then_its_clearing_on_a_motion_zone_is_occupancy_true_then_false() {
    let mut random = counter();
    let mut devices = Devices::new();
    zone_sensor(&mut devices, Some(0x000D), &mut random);
    assert_eq!(value_of(&devices, "occupancy"), Some(Json::Null));
    devices
        .apply(&zone_event(0x0001), T0 + 1, &mut random)
        .unwrap();
    assert_eq!(value_of(&devices, "occupancy"), Some(Json::Bool(true)));
    devices
        .apply(&zone_event(0x0000), T0 + 2, &mut random)
        .unwrap();
    assert_eq!(value_of(&devices, "occupancy"), Some(Json::Bool(false)));
}

#[test]
fn an_alarm_on_a_contact_zone_is_contact_false_open_and_closed_again_is_true() {
    let mut random = counter();
    let mut devices = Devices::new();
    zone_sensor(&mut devices, Some(0x0015), &mut random);
    devices
        .apply(&zone_event(0x0001), T0 + 1, &mut random)
        .unwrap();
    assert_eq!(value_of(&devices, "contact"), Some(Json::Bool(false)));
    devices
        .apply(&zone_event(0x0000), T0 + 2, &mut random)
        .unwrap();
    assert_eq!(value_of(&devices, "contact"), Some(Json::Bool(true)));
}

#[test]
fn the_tamper_and_battery_low_bits_add_their_data_the_first_time_they_are_set() {
    let mut random = counter();
    let mut devices = Devices::new();
    zone_sensor(&mut devices, Some(0x000D), &mut random);
    // Bits clear: nothing added.
    devices
        .apply(&zone_event(0x0000), T0 + 1, &mut random)
        .unwrap();
    assert_eq!(value_of(&devices, "tamper"), None);
    assert_eq!(value_of(&devices, "battery_low"), None);
    // Tamper set: added, true, boolean, generic; then cleared: false.
    devices
        .apply(&zone_event(0x0004), T0 + 2, &mut random)
        .unwrap();
    let tamper = devices.list()[0]
        .data
        .iter()
        .find(|d| d.key == "tamper")
        .unwrap();
    assert_eq!(tamper.value, Json::Bool(true));
    assert_eq!(tamper.value_type, ValueType::Boolean);
    assert_eq!(&*tamper.category, "generic");
    devices
        .apply(&zone_event(0x0000), T0 + 3, &mut random)
        .unwrap();
    assert_eq!(value_of(&devices, "tamper"), Some(Json::Bool(false)));
    // Battery low set: added, true.
    devices
        .apply(&zone_event(0x0008), T0 + 4, &mut random)
        .unwrap();
    assert_eq!(value_of(&devices, "battery_low"), Some(Json::Bool(true)));
    // Each is added once.
    assert_eq!(
        keys(&devices.list()[0])
            .iter()
            .filter(|k| **k == "tamper")
            .count(),
        1
    );
}

#[test]
fn a_motion_sensor_that_never_sends_the_tamper_bit_has_no_tamper_data() {
    let mut random = counter();
    let mut devices = Devices::new();
    zone_sensor(&mut devices, Some(0x000D), &mut random);
    for (i, status) in [0x0001u16, 0x0000, 0x0001, 0x0000].into_iter().enumerate() {
        devices
            .apply(&zone_event(status), T0 + i as u64, &mut random)
            .unwrap();
    }
    assert_eq!(
        keys(&devices.list()[0]),
        vec!["occupancy", "battery", "voltage"]
    );
}

#[test]
fn the_zone_status_attribute_in_a_report_is_read_like_a_notification() {
    let mut random = counter();
    let mut devices = Devices::new();
    zone_sensor(&mut devices, Some(0x000D), &mut random);
    // Report Attributes of 0x0002 (bitmap16): alarm and tamper bits set.
    let report = parse_attribute_report(&[0x18, 0x01, 0x0A, 0x02, 0x00, 0x19, 0x05, 0x00]).unwrap();
    let event = PairingEvent::Measures(MeasureEvent {
        sender: Sender::Eui64(EUI),
        endpoint: 1,
        measures: measures::from_records(0x0500, &report.records),
    });
    devices.apply(&event, T0 + 1, &mut random).unwrap();
    assert_eq!(value_of(&devices, "occupancy"), Some(Json::Bool(true)));
    assert_eq!(value_of(&devices, "tamper"), Some(Json::Bool(true)));
}

#[test]
fn the_zone_data_are_kept_in_devices_json_and_known_by_a_later_process() {
    let mut random = counter();
    let mut devices = Devices::new();
    zone_sensor(&mut devices, Some(0x0015), &mut random);
    devices
        .apply(&zone_event(0x0005), T0 + 1, &mut random)
        .unwrap();
    let json = devices.to_json().unwrap();

    // Another process: it reads the file, and the next notification finds the data.
    let mut later = Devices::from_list(parse_list(&json).unwrap());
    assert_eq!(
        keys(&later.list()[0]),
        vec!["contact", "battery", "voltage", "tamper"]
    );
    assert_eq!(value_of(&later, "contact"), Some(Json::Bool(false)));
    later
        .apply(&zone_event(0x0000), T0 + 2, &mut random)
        .unwrap();
    assert_eq!(value_of(&later, "contact"), Some(Json::Bool(true)));
    assert_eq!(value_of(&later, "tamper"), Some(Json::Bool(false)));
}
