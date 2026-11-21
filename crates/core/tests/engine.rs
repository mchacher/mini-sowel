//! The engine (spec 016): its loop on the simulated radio with the storage in memory, the saver's
//! write policy and retries, the loading of the home.

mod mem_storage;
mod support;

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use mem_storage::MemStorage;
use serde_json::json;
use solite_core::devices::lock;
use solite_core::engine::{
    self, Clocks, DEVICES_FILE, EQUIPMENTS_FILE, RECIPES_FILE, RETRY_MS, Saver, ZONES_FILE,
    load_home,
};
use solite_core::home::Home;
use solite_core::radio::{NetworkSettings, Radio, RadioTransport, TransportError};
use solite_sim::{DeviceReport, FakeClock, Kind, Ncp, SimDevice, SimTransport};
use support::{PIR, RELAY, Rig, T0, eui, motion};

/// The simulated radio, and a way for the test to stop the loop from inside the radio: `done` is
/// asked after every read.
struct Stopper {
    inner: SimTransport<FakeClock>,
    stop: Arc<AtomicBool>,
    done: Rc<RefCell<Box<dyn Fn() -> bool>>>,
}

impl RadioTransport for Stopper {
    fn read(&mut self, buf: &mut [u8], timeout_ms: u64) -> Result<usize, TransportError> {
        let n = self.inner.read(buf, timeout_ms)?;
        if (self.done.borrow())() {
            self.stop.store(true, Ordering::SeqCst);
        }
        Ok(n)
    }

    fn write(&mut self, bytes: &[u8]) -> Result<(), TransportError> {
        self.inner.write(bytes)
    }
}

type Wired = Radio<Stopper, Box<dyn FnMut() -> u64>>;

struct Bench {
    radio: Wired,
    ncp: Rc<RefCell<Ncp>>,
    stop: Arc<AtomicBool>,
    done: Rc<RefCell<Box<dyn Fn() -> bool>>>,
}

fn scripted() -> impl FnMut(&mut [u8]) -> Result<(), String> {
    let mut n = 0u8;
    move |buffer: &mut [u8]| {
        n = n.wrapping_add(1);
        buffer.fill(n);
        buffer[0] = 1;
        Ok(())
    }
}

fn device(kind: Kind, last: u8, nwk: u16) -> SimDevice {
    let mut wire = eui(last);
    wire.reverse();
    let mut device = SimDevice::of_kind(kind, wire, None, None);
    device.nwk = nwk;
    device.auto_join = true;
    device
}

/// A radio on these devices, paired (their reporting is set up) and listening.
fn bench(devices: Vec<SimDevice>) -> Bench {
    let mut ncp = Ncp::recording();
    ncp.devices = devices;
    let ncp = Rc::new(RefCell::new(ncp));
    let clock = FakeClock::default();
    let time = Rc::clone(&clock.time);
    let stop = Arc::new(AtomicBool::new(false));
    let done: Rc<RefCell<Box<dyn Fn() -> bool>>> = Rc::new(RefCell::new(Box::new(|| false)));
    let transport = Stopper {
        inner: SimTransport::new(Rc::clone(&ncp), Vec::new(), 1, clock),
        stop: Arc::clone(&stop),
        done: Rc::clone(&done),
    };
    let mut radio: Wired = Radio::new(transport, Box::new(move || time.get()));
    radio.identify().unwrap();
    radio
        .start_network(
            &NetworkSettings::default(),
            &mut scripted(),
            &mut |_| Ok(()),
        )
        .unwrap();
    radio.permit_join(180).unwrap();
    let mut polls = 0;
    while polls < 2000 {
        radio.poll_pairing().unwrap();
        polls += 1;
        if polls > 50 && !radio.pairing_busy() {
            break;
        }
    }
    radio.close_joining().unwrap();
    Bench {
        radio,
        ncp,
        stop,
        done,
    }
}

fn clocks() -> Clocks {
    let start = Instant::now();
    let monotonic = move || u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
    Clocks {
        now: Arc::new(move || T0 + monotonic()),
        monotonic: Arc::new(monotonic),
    }
}

fn run_engine(
    bench: &mut Bench,
    home: &Mutex<Home>,
    saver: &mut Saver<&MemStorage>,
    verbose: bool,
    lines: &mut Vec<String>,
) -> Result<bool, String> {
    let mut random = scripted();
    engine::run(
        &mut bench.radio,
        home,
        saver,
        &clocks(),
        &bench.stop,
        &mut random,
        verbose,
        &mut |line| lines.push(line.to_string()),
    )
}

fn json_of(home: &mut Home, path: &str) -> serde_json::Value {
    let reply = solite_core::api::handle(
        home,
        &solite_core::api::Request {
            method: "GET",
            path,
            host: Some("127.0.0.1:8080"),
            port: 8080,
            content_type: None,
            body: b"",
        },
        T0,
        &mut scripted(),
    );
    serde_json::from_str(&reply.body.unwrap()).unwrap()
}

#[test]
fn a_turn_with_measures_updates_the_device_recomputes_the_zone_and_writes_the_files_due() {
    let mut rig = Rig::new();
    let zone = rig.zone("Cellar");
    let sensor = rig.sensor(0x20);
    rig.equipment("Cellar climate", "sensor", &zone, &[&sensor]);
    let mut bench = bench(vec![device(Kind::TemperatureHumidity, 0x20, 0x2000)]);
    let home = Mutex::new(rig.home);
    // The device reports once the loop runs; the loop stops when the temperature is in.
    bench.ncp.borrow_mut().device_report(
        0,
        &DeviceReport {
            temperature: Some(21.5),
            humidity: Some(48.0),
            ..DeviceReport::default()
        },
    );
    let seen = Rc::new(Cell::new(0));
    let counter = Rc::clone(&seen);
    *bench.done.borrow_mut() = Box::new(move || {
        counter.set(counter.get() + 1);
        counter.get() > 20
    });
    let storage = MemStorage::default();
    let mut saver = Saver::new(&storage, T0);
    let mut lines = Vec::new();
    let stopped = run_engine(&mut bench, &home, &mut saver, true, &mut lines).unwrap();
    assert!(stopped);
    assert!(
        lines.contains(&"0x00124B0012345620 temperature 21.5 °C".to_string()),
        "the sink gets the lines of the measures: {lines:?}"
    );
    let mut home = lock(&home);
    let devices = json_of(&mut home, "/api/v1/devices");
    let data = devices[0]["data"].as_array().unwrap();
    let value = |key: &str| data.iter().find(|d| d["key"] == key).unwrap()["value"].clone();
    assert_eq!(value("temperature"), json!(21.5));
    assert_eq!(value("humidity"), json!(48));
    let aggregation = json_of(&mut home, "/api/v1/zones/aggregation");
    assert!(
        aggregation.to_string().contains("21.5"),
        "the zone is recomputed: {aggregation}"
    );
    // The definitions changed at the start: written by the first turn, without any stop.
    assert!(storage.text(DEVICES_FILE).is_some());
    assert!(storage.text(ZONES_FILE).is_some());
    assert!(storage.text(EQUIPMENTS_FILE).is_some());
}

#[test]
fn motion_through_the_loop_sends_the_lights_order_through_the_radio() {
    let mut rig = Rig::new();
    let garage = rig.garage();
    rig.garage_instance(&garage, json!({}));
    rig.home.start_recipes(0, T0);
    // The sensor sees someone before the loop starts: the recipe orders the light on.
    rig.report(&motion(PIR, true));
    let mut bench = bench(vec![device(Kind::Relay, RELAY, 0x2001)]);
    let ncp = Rc::clone(&bench.ncp);
    // Stops at the command, or after 2000 reads: a loop that never sends fails the test below.
    let reads = Cell::new(0);
    *bench.done.borrow_mut() = Box::new(move || {
        reads.set(reads.get() + 1);
        reads.get() > 2000
            || ncp.borrow().sent.iter().any(|m| {
                m.profile == 0x0104
                    && m.cluster == 0x0006
                    && m.message[0] & 0x03 == 0x01
                    && m.message[2] == 0x01
            })
    });
    let home = Mutex::new(rig.home);
    let storage = MemStorage::default();
    let mut saver = Saver::new(&storage, T0);
    run_engine(&mut bench, &home, &mut saver, false, &mut Vec::new()).unwrap();
    let sent = bench.ncp.borrow().sent.clone();
    let command = sent
        .iter()
        .find(|m| m.cluster == 0x0006 && m.message[0] & 0x03 == 0x01 && m.message[2] == 0x01)
        .expect("a command to the relay");
    assert_eq!(
        command.message[2], 0x01,
        "the On command of the On/Off cluster"
    );
    assert_eq!(command.destination, 0x2001);
}

#[test]
fn the_stop_flag_ends_the_loop_and_the_saver_writes_what_is_due_on_stop() {
    let mut rig = Rig::new();
    rig.zone("Attic");
    let mut bench = bench(Vec::new());
    bench.stop.store(true, Ordering::SeqCst);
    let home = Mutex::new(rig.home);
    let storage = MemStorage::default();
    let mut saver = Saver::new(&storage, T0);
    assert!(run_engine(&mut bench, &home, &mut saver, false, &mut Vec::new()).unwrap());
    assert_eq!(storage.attempts.get(), 0, "no turn, nothing written yet");
    saver.tick(&home, T0 + 1, true).unwrap();
    assert!(storage.text(ZONES_FILE).unwrap().contains("Attic"));
}

fn home_with_a_zone() -> Mutex<Home> {
    let mut rig = Rig::new();
    rig.zone("Attic");
    Mutex::new(rig.home)
}

#[test]
fn a_failed_write_is_shown_then_written_at_the_retry() {
    let home = home_with_a_zone();
    let storage = MemStorage::default();
    let shown = Rc::new(RefCell::new(Vec::new()));
    let sink = Rc::clone(&shown);
    let mut saver = Saver::new(&storage, 0)
        .with_sink(move |message| sink.borrow_mut().push(message.to_string()));
    storage.fail_writes.set(true);
    saver.tick(&home, 1000, false).unwrap();
    assert!(
        shown.borrow()[0].starts_with("zones: "),
        "{:?}",
        shown.borrow()
    );
    assert!(lock(&home).changes().0, "due again");
    let attempts = storage.attempts.get();
    // Not before the retry delay, and then not in a loop.
    storage.fail_writes.set(false);
    saver.tick(&home, 2000, false).unwrap();
    assert_eq!(storage.attempts.get(), attempts);
    assert!(storage.text(ZONES_FILE).is_none());
    saver.tick(&home, 1000 + RETRY_MS, false).unwrap();
    assert!(storage.text(ZONES_FILE).unwrap().contains("Attic"));
}

#[test]
fn a_failed_write_on_stop_is_the_error() {
    let home = home_with_a_zone();
    let storage = MemStorage::default();
    storage.fail_writes.set(true);
    let mut saver = Saver::new(&storage, 0);
    let error = saver.tick(&home, 5, true).unwrap_err();
    assert!(error.starts_with("zones: "), "{error}");
}

#[test]
fn the_recipes_and_equipments_are_written_at_once_when_they_changed() {
    let mut rig = Rig::new();
    let garage = rig.garage();
    rig.garage_instance(&garage, json!({}));
    let home = Mutex::new(rig.home);
    let storage = MemStorage::default();
    let mut saver = Saver::new(&storage, 0);
    saver.tick(&home, 1, false).unwrap();
    for name in [DEVICES_FILE, ZONES_FILE, EQUIPMENTS_FILE, RECIPES_FILE] {
        assert!(storage.text(name).is_some(), "{name}");
    }
    // Nothing changed: nothing is written again.
    let attempts = storage.attempts.get();
    saver.tick(&home, 2, false).unwrap();
    assert_eq!(storage.attempts.get(), attempts);
}

#[test]
fn an_empty_storage_gives_an_empty_home() {
    let home = load_home(&&MemStorage::default(), T0).unwrap();
    assert!(home.devices.list().is_empty());
    assert_eq!(home.zones().list().len(), 1, "the root zone");
    assert!(home.equipments().list().is_empty());
    assert!(home.recipes().is_empty());
}

#[test]
fn a_file_that_cannot_be_read_or_parsed_stops_the_loading_and_names_the_file() {
    let storage = MemStorage::default();
    *storage.unreadable.borrow_mut() = Some(RECIPES_FILE.to_string());
    let error = load_home(&&storage, T0).err().unwrap();
    assert!(error.contains("recipes.json"), "{error}");

    let storage = MemStorage::with(&[(ZONES_FILE, "[ not json")]);
    let error = load_home(&&storage, T0).err().unwrap();
    assert!(
        error.starts_with("zones: memory://zones.json: cannot read: "),
        "{error}"
    );
    assert_eq!(
        storage.text(ZONES_FILE).unwrap(),
        "[ not json",
        "left as it is"
    );
}

mod interrupted_save {
    use std::cell::RefCell;
    use std::collections::BTreeSet;

    use solite_core::engine::{complete_name, recover_interrupted_save, temporary_name};

    fn recover(files: &RefCell<BTreeSet<String>>) -> bool {
        let complete = complete_name("devices.json");
        let temporary = temporary_name("devices.json");
        recover_interrupted_save(
            "devices.json",
            complete.as_str(),
            temporary.as_str(),
            |name| files.borrow().contains(name),
            |from, to| {
                let mut files = files.borrow_mut();
                files.remove(from);
                files.insert(to.to_string());
                Ok(())
            },
            |name| {
                files.borrow_mut().remove(name);
                Ok(())
            },
        )
        .unwrap()
    }

    fn files(names: &[&str]) -> RefCell<BTreeSet<String>> {
        RefCell::new(names.iter().map(|n| n.to_string()).collect())
    }

    #[test]
    fn only_the_complete_file_is_renamed_to_the_missing_one() {
        let set = files(&["devices.json.new"]);
        assert!(recover(&set));
        assert_eq!(*set.borrow(), BTreeSet::from(["devices.json".to_string()]));
    }

    #[test]
    fn a_truncated_temporary_file_with_no_file_is_deleted_not_promoted() {
        let set = files(&["devices.json.tmp"]);
        assert!(!recover(&set));
        assert!(set.borrow().is_empty());
    }

    #[test]
    fn a_temporary_file_beside_a_good_file_is_ignored_and_removed() {
        let set = files(&["devices.json", "devices.json.tmp"]);
        assert!(!recover(&set));
        assert_eq!(*set.borrow(), BTreeSet::from(["devices.json".to_string()]));
    }

    #[test]
    fn a_complete_file_beside_an_existing_one_changes_nothing_but_is_removed() {
        let set = files(&["devices.json", "devices.json.new"]);
        assert!(!recover(&set));
        assert_eq!(*set.borrow(), BTreeSet::from(["devices.json".to_string()]));
        let none = files(&[]);
        assert!(!recover(&none));
        assert!(none.borrow().is_empty());
    }
}
