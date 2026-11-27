//! The engine (spec 016): its loop on the simulated radio with the storage in memory, the saver's
//! write policy and retries, the loading of the home.

mod mem_storage;
mod support;

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use mem_storage::MemStorage;
use serde_json::json;
use solite_core::devices::lock;
use solite_core::engine::{
    self, Clocks, DEVICES_FILE, EQUIPMENTS_FILE, RECIPES_FILE, RETRY_MS, Saver, ZONES_FILE,
    load_home,
};
use solite_core::home::{Home, Phase};
use solite_core::radio::{NetworkSettings, Radio, RadioTransport, TransportError};
use solite_sim::{DeviceReport, FakeClock, Kind, Ncp, SimDevice, SimTransport};
use support::{HOUR, MINUTE, PIR, RELAY, Rig, T0, eui, motion};

/// The simulated radio, and a way for the test to stop the loop from inside the radio: `done` is
/// asked after every read.
struct Stopper {
    inner: SimTransport<FakeClock>,
    stop: Arc<AtomicBool>,
    done: Rc<RefCell<Box<dyn Fn() -> bool>>>,
    /// The fake time, copied after every read for the engine's clocks (which must be `Send`).
    time: Rc<Cell<u64>>,
    mirror: Arc<AtomicU64>,
}

impl RadioTransport for Stopper {
    fn read(&mut self, buf: &mut [u8], timeout_ms: u64) -> Result<usize, TransportError> {
        let n = self.inner.read(buf, timeout_ms)?;
        self.mirror.store(self.time.get(), Ordering::SeqCst);
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
    mirror: Arc<AtomicU64>,
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
    bench_on(ncp, false)
}

/// A radio as `--serve` leaves it: the network started, joining closed, devices not followed.
fn serving_bench(devices: Vec<SimDevice>) -> Bench {
    let mut ncp = Ncp::recording();
    ncp.devices = devices;
    bench_on(ncp, true)
}

fn bench_on(ncp: Ncp, serving: bool) -> Bench {
    let ncp = Rc::new(RefCell::new(ncp));
    let clock = FakeClock::default();
    let time = Rc::clone(&clock.time);
    let stop = Arc::new(AtomicBool::new(false));
    let done: Rc<RefCell<Box<dyn Fn() -> bool>>> = Rc::new(RefCell::new(Box::new(|| false)));
    let mirror = Arc::new(AtomicU64::new(0));
    let transport = Stopper {
        inner: SimTransport::new(Rc::clone(&ncp), Vec::new(), 1, clock),
        stop: Arc::clone(&stop),
        done: Rc::clone(&done),
        time: Rc::clone(&time),
        mirror: Arc::clone(&mirror),
    };
    let mut radio: Wired = Radio::new(transport, Box::new(move || time.get()));
    radio.identify().unwrap();
    radio
        .start_network(
            &NetworkSettings::default(),
            None,
            false,
            &mut scripted(),
            &mut |_| Ok(()),
        )
        .unwrap();
    if serving {
        radio.set_following(false);
        return Bench {
            radio,
            ncp,
            stop,
            done,
            mirror,
        };
    }
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
        mirror,
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
    .map_err(String::from)
}

fn json_of(home: &mut Home, path: &str) -> serde_json::Value {
    let reply = solite_core::api::handle(
        home,
        &solite_core::api::Request {
            method: "GET",
            path,
            host: Some("127.0.0.1:8080"),
            port: 8080,
            reach: solite_core::api::Reach::Loopback,
            content_type: None,
            authorization: None,
            content_length: None,
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

// ---- The pairing window in the loop (spec 017) ----

/// The engine's clocks on the radio's fake time.
fn fake_clocks(bench: &Bench) -> Clocks {
    let mirror = Arc::clone(&bench.mirror);
    let again = Arc::clone(&bench.mirror);
    Clocks {
        now: Arc::new(move || T0 + mirror.load(Ordering::SeqCst)),
        monotonic: Arc::new(move || again.load(Ordering::SeqCst)),
    }
}

/// What a test of the window sees: the home, the lines of the sink, the radio's handle.
struct Window {
    bench: Bench,
    home: Arc<Mutex<Home>>,
    lines: Vec<String>,
    /// The window as it was when the run reached its end time, before the stop closes it.
    seen: Rc<RefCell<Option<Seen>>>,
    /// Where the random source goes on from, so that two runs give different ids.
    seed: u8,
}

struct Seen {
    phase: Phase,
    open: bool,
    remaining: u32,
    busy: bool,
}

impl Window {
    fn new(devices: Vec<SimDevice>) -> Window {
        Window::on(serving_bench(devices), Home::new(T0))
    }

    fn on(bench: Bench, home: Home) -> Window {
        Window {
            bench,
            home: Arc::new(Mutex::new(home)),
            lines: Vec::new(),
            seen: Rc::new(RefCell::new(None)),
            seed: 0,
        }
    }

    fn ask(&self, seconds: u8) {
        lock(&self.home).pairing_mut().request(seconds);
    }

    /// Runs the loop until the fake time reaches `ms` (one more request is made at each of the
    /// `asks`, in milliseconds of fake time, once).
    fn run(&mut self, ms: u64, asks: &[(u64, u8)]) -> bool {
        let mirror = Arc::clone(&self.bench.mirror);
        let home = Arc::clone(&self.home);
        let recorded = Rc::clone(&self.seen);
        *recorded.borrow_mut() = None;
        let asks: Vec<(u64, u8, Cell<bool>)> = asks
            .iter()
            .map(|(at, seconds)| (*at, *seconds, Cell::new(false)))
            .collect();
        *self.bench.done.borrow_mut() = Box::new(move || {
            let now = mirror.load(Ordering::SeqCst);
            for (at, seconds, made) in &asks {
                if now >= *at && !made.get() {
                    made.set(true);
                    lock(&home).pairing_mut().request(*seconds);
                }
            }
            if now >= ms && recorded.borrow().is_none() {
                let guard = lock(&home);
                let window = guard.pairing();
                let (open, remaining, _) = window.state(window.now());
                *recorded.borrow_mut() = Some(Seen {
                    phase: window.phase(),
                    open,
                    remaining,
                    busy: window.is_busy(&eui(0x24)),
                });
            }
            now >= ms
        });
        self.bench.stop.store(false, Ordering::SeqCst);
        let storage = MemStorage::default();
        let mut saver = Saver::new(&storage, T0);
        let seed = self.seed;
        self.seed = self.seed.wrapping_add(50);
        let mut n = seed;
        let mut random = move |buffer: &mut [u8]| {
            n = n.wrapping_add(1);
            buffer.fill(n);
            buffer[0] = 1;
            Ok(())
        };
        let lines = &mut self.lines;
        let clocks = fake_clocks(&self.bench);
        engine::run(
            &mut self.bench.radio,
            &self.home,
            &mut saver,
            &clocks,
            &self.bench.stop,
            &mut random,
            false,
            &mut |line| lines.push(line.to_string()),
        )
        .unwrap()
    }

    fn count(&self, prefix: &str) -> usize {
        self.lines.iter().filter(|l| l.starts_with(prefix)).count()
    }

    fn joining_closed(&self) -> bool {
        // permitJoining (0x22) with 0 seconds
        self.bench
            .ncp
            .borrow()
            .commands
            .iter()
            .any(|c| c.0 == 0x22 && c.1 == [0])
    }

    fn devices(&self) -> usize {
        lock(&self.home).devices.list().len()
    }
}

fn three_devices() -> Vec<SimDevice> {
    vec![
        device(Kind::TemperatureHumidity, 0x20, 0x2000),
        device(Kind::Motion, 0x21, 0x2001),
        device(Kind::Relay, 0x22, 0x2002),
    ]
}

#[test]
fn without_a_request_no_device_joins_and_the_measures_of_known_devices_go_on() {
    let mut window = Window::new(three_devices());
    assert!(window.run(20_000, &[]));
    assert_eq!(window.devices(), 0);
    assert!(window.lines.is_empty(), "{:?}", window.lines);
}

#[test]
fn a_request_recorded_before_the_first_turn_opens_a_window_and_the_devices_join() {
    let mut window = Window::new(three_devices());
    window.ask(30);
    assert!(window.run(60_000, &[]));
    assert_eq!(window.devices(), 3);
    assert_eq!(window.lines[0], "pairing: open for 30 s");
    assert_eq!(window.count("joined:"), 3, "{:?}", window.lines);
    assert_eq!(window.count("device:"), 3);
    assert_eq!(window.count("reporting:"), 3);
    // The measures of the sensors came through as outside a window: the data has values.
    let mut home = lock(&window.home);
    let data = home.devices.list()[0].data.clone();
    assert!(!data.is_empty());
    let _ = &mut home;
}

#[test]
fn the_end_of_the_time_closes_joining_and_a_device_joining_later_does_not_join() {
    let mut late = device(Kind::Relay, 0x23, 0x2003);
    late.join_delay_ms = 40_000;
    let mut window = Window::new(vec![device(Kind::Relay, 0x22, 0x2002), late]);
    window.ask(30);
    window.run(50_000, &[]);
    assert!(window.joining_closed());
    assert_eq!(window.count("pairing: closed"), 1, "{:?}", window.lines);
    assert_eq!(window.devices(), 1, "the late device never joined");
    assert_eq!(lock(&window.home).pairing().phase(), Phase::Closed);
}

#[test]
fn a_close_request_while_open_closes_at_the_next_turn() {
    let mut window = Window::new(three_devices());
    window.ask(120);
    window.run(10_000, &[(4_000, 0)]);
    assert!(window.joining_closed());
    assert_eq!(window.count("pairing: closed"), 1);
    assert_eq!(lock(&window.home).pairing().phase(), Phase::Closed);
}

#[test]
fn a_new_request_while_open_restarts_the_window_with_the_new_time() {
    let mut window = Window::new(Vec::new());
    window.ask(30);
    window.run(12_000, &[(10_000, 60)]);
    assert_eq!(window.count("pairing: open for"), 2, "{:?}", window.lines);
    assert_eq!(window.lines[1], "pairing: open for 60 s");
    let seen = window.seen.borrow();
    let seen = seen.as_ref().expect("seen");
    assert!(
        seen.open && (57..=60).contains(&seen.remaining),
        "{}",
        seen.remaining
    );
}

#[test]
fn a_setup_still_running_at_the_cap_is_cut_short_and_the_coordinator_listens_again() {
    let mut stuck = device(Kind::Relay, 0x24, 0x2004);
    stuck.ignore_configures = u32::MAX;
    let mut window = Window::new(vec![stuck]);
    lock(&window.home).pairing_mut().set_grace_ms(3_000);
    window.ask(10);
    window.run(14_000, &[]);
    assert!(
        window
            .lines
            .iter()
            .any(|l| l.starts_with("reporting: 0x") && l.ends_with("cut short")),
        "{:?}",
        window.lines
    );
    assert_eq!(lock(&window.home).pairing().phase(), Phase::Closed);
    assert!(
        window.bench.radio.setups_in_progress().is_empty(),
        "the line is only printed for a setup really abandoned"
    );
}

#[test]
fn a_device_still_being_set_up_is_busy_in_the_window_and_kept_by_a_new_request_while_finishing() {
    let mut stuck = device(Kind::Relay, 0x24, 0x2004);
    stuck.ignore_configures = u32::MAX;
    let mut window = Window::new(vec![stuck]);
    window.ask(5);
    // The first window is over at 5 s, its setup is still running; a request at 8 s opens a new
    // window and the device is kept.
    window.run(12_000, &[(8_000, 30)]);
    assert_eq!(window.count("pairing: open for"), 2, "{:?}", window.lines);
    {
        let seen = window.seen.borrow();
        let seen = seen.as_ref().expect("seen");
        assert!(matches!(seen.phase, Phase::Open { .. }));
        assert!(seen.busy, "the device being set up is busy in the window");
    }
    // And the API refuses to delete it while the window says so.
    lock(&window.home).pairing_mut().set_busy(&[eui(0x24)]);
    let id = lock(&window.home).devices.list()[0].device.id.to_string();
    let (status, _) = {
        let mut home = lock(&window.home);
        let response = solite_core::api::handle(
            &mut home,
            &solite_core::api::Request {
                method: "DELETE",
                path: &format!("/api/v1/devices/{id}"),
                host: Some("127.0.0.1:8080"),
                port: 8080,
                reach: solite_core::api::Reach::Loopback,
                content_type: None,
                authorization: None,
                content_length: None,
                body: b"",
            },
            T0,
            &mut scripted(),
        );
        (response.status, response.body)
    };
    assert_eq!(status, 409);
}

#[test]
fn the_radio_refusing_the_opening_closes_the_window_sets_the_error_and_the_loop_goes_on() {
    let mut ncp = Ncp::recording();
    ncp.devices = three_devices();
    ncp.refuse_permit_joining = true;
    let mut window = Window::on(bench_on(ncp, true), Home::new(T0));
    window.ask(30);
    assert!(window.run(5_000, &[]));
    assert!(
        window
            .lines
            .iter()
            .any(|l| l.starts_with("pairing: failed: the radio refused permitJoining")),
        "{:?}",
        window.lines
    );
    let mut home = lock(&window.home);
    let now = home.pairing().now();
    let (open, _, error) = home.pairing().state(now);
    assert!(!open);
    assert!(error.is_some_and(|e| e.contains("permitJoining")));
    // The next request clears it.
    home.pairing_mut().request(30);
    assert!(home.pairing().state(now).2.is_none());
}

#[test]
fn stopping_while_open_closes_joining_and_the_loop_returns_true() {
    let mut window = Window::new(Vec::new());
    window.ask(120);
    assert!(window.run(3_000, &[]));
    assert!(window.joining_closed());
    assert_eq!(window.count("pairing: closed"), 1);
}

#[test]
fn a_device_deleted_from_the_home_comes_back_as_new_when_it_announces_itself_in_a_window() {
    let mut window = Window::new(vec![device(Kind::Relay, 0x22, 0x2002)]);
    window.ask(30);
    window.run(40_000, &[]);
    let first = lock(&window.home).devices.list()[0].device.id.to_string();
    assert!(lock(&window.home).remove_device(&first, 0));
    // The device is still in the simulated network: it announces itself again.
    let mut ncp = window.bench.ncp.borrow_mut();
    let mut again = device(Kind::Relay, 0x22, 0x2002);
    again.announce = true;
    ncp.devices = vec![again];
    drop(ncp);
    let hub = lock(&window.home).live();
    let client = hub.connect().unwrap();
    assert!(hub.subscribe(client, r#"{"type":"subscribe","topics":["devices"]}"#));
    window.ask(30);
    window.run(80_000, &[]);
    let list = lock(&window.home).devices.list().len();
    assert_eq!(list, 1);
    let second = lock(&window.home).devices.list()[0].device.id.to_string();
    assert_ne!(first, second, "a new id");
    let batch = hub.take_batch(client).unwrap_or_default();
    assert!(batch.contains("device.discovered"), "{batch}");
}

#[test]
fn a_full_device_store_gives_a_note_and_no_device() {
    let mut home = Home::new(T0);
    let mut random = scripted();
    for n in 0..64u8 {
        let mut eui = [0u8; 8];
        eui[0] = 0xAA;
        eui[7] = n;
        home.devices
            .identified(eui, Some("X"), Some("Y"), T0, &mut random)
            .unwrap();
    }
    let mut window = Window::on(serving_bench(vec![device(Kind::Relay, 0x22, 0x2002)]), home);
    window.ask(30);
    window.run(40_000, &[]);
    assert_eq!(window.devices(), 64);
    assert_eq!(
        window.count("pairing: device store full (64)"),
        1,
        "{:?}",
        window.lines
    );
}

/// A transport that fails on every read.
struct Broken;

impl RadioTransport for Broken {
    fn read(&mut self, _buf: &mut [u8], _timeout_ms: u64) -> Result<usize, TransportError> {
        Err(TransportError::Io("uart read failed".into()))
    }

    fn write(&mut self, _bytes: &[u8]) -> Result<(), TransportError> {
        Ok(())
    }
}

#[test]
fn a_radio_error_in_a_poll_is_a_radio_run_error_with_the_text_of_before() {
    let mut radio = Radio::new(Broken, || 0u64);
    let home = home_with_a_zone();
    let storage = MemStorage::default();
    let mut saver = Saver::new(&storage, T0);
    let stop = AtomicBool::new(false);
    let error = engine::run(
        &mut radio,
        &home,
        &mut saver,
        &clocks(),
        &stop,
        &mut scripted(),
        false,
        &mut |_| {},
    )
    .unwrap_err();
    assert!(matches!(error, engine::RunError::Radio(_)), "{error:?}");
    // The text of today: `radio: ` and the radio's error (here, a radio never identified).
    assert_eq!(error.to_string(), "radio: the radio was not identified");
    assert_eq!(String::from(error), "radio: the radio was not identified");
}

#[test]
fn a_storage_error_is_an_other_run_error_that_no_radio_restart_mends() {
    // A storage error reaches the loop as the saver's text: it is `Other`, with its text as it
    // is, never `Radio`.
    let error =
        engine::RunError::from("zones: memory://zones.json: cannot write: full".to_string());
    assert!(matches!(error, engine::RunError::Other(_)));
    assert_eq!(
        error.to_string(),
        "zones: memory://zones.json: cannot write: full"
    );
}

/// The box's clock is set by SNTP after a boot in 1970 (spec 022 R6.2): the wall clock jumps
/// forward by decades while the loop runs.
#[test]
fn a_wall_clock_that_jumps_forward_by_56_years_does_not_stop_the_engine() {
    const JUMP_MS: u64 = 56 * 365 * 24 * 3600 * 1000;
    let mut rig = Rig::new();
    let zone = rig.zone("Cellar");
    let sensor = rig.sensor(0x20);
    rig.equipment("Cellar climate", "sensor", &zone, &[&sensor]);
    let mut bench = bench(vec![device(Kind::TemperatureHumidity, 0x20, 0x2000)]);
    let home = Mutex::new(rig.home);
    let jumped = Arc::new(AtomicBool::new(false));
    let start = Instant::now();
    let monotonic = move || u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
    let wall = Arc::clone(&jumped);
    let clocks = Clocks {
        now: Arc::new(move || {
            let jump = if wall.load(Ordering::SeqCst) {
                JUMP_MS
            } else {
                0
            };
            T0 + jump + monotonic()
        }),
        monotonic: Arc::new(monotonic),
    };
    bench.ncp.borrow_mut().device_report(
        0,
        &DeviceReport {
            temperature: Some(21.5),
            ..DeviceReport::default()
        },
    );
    let (ncp, flag, seen) = (
        Rc::clone(&bench.ncp),
        Arc::clone(&jumped),
        Rc::new(Cell::new(0)),
    );
    *bench.done.borrow_mut() = Box::new(move || {
        seen.set(seen.get() + 1);
        match seen.get() {
            10 => flag.store(true, Ordering::SeqCst),
            12 => {
                ncp.borrow_mut().device_report(
                    0,
                    &DeviceReport {
                        temperature: Some(30.0),
                        ..DeviceReport::default()
                    },
                );
            }
            _ => {}
        }
        seen.get() > 40
    });
    let storage = MemStorage::default();
    let mut saver = Saver::new(&storage, T0);
    let stopped = engine::run(
        &mut bench.radio,
        &home,
        &mut saver,
        &clocks,
        &bench.stop,
        &mut scripted(),
        false,
        &mut |_| {},
    )
    .unwrap();
    assert!(stopped, "the loop ended by its stop flag, not by an error");
    assert!(jumped.load(Ordering::SeqCst));
    let before = storage.attempts.get();
    // The next save after the jump is written (a tick that is not a stop), with the value
    // reported after it.
    saver
        .tick(&home, T0 + JUMP_MS + 60 * 60 * 1000, false)
        .unwrap();
    assert!(
        storage.attempts.get() > before,
        "a save is due after the jump and is attempted"
    );
    let mut home = lock(&home);
    let devices = json_of(&mut home, "/api/v1/devices");
    assert_eq!(devices[0]["status"], "online");
    let data = devices[0]["data"].as_array().unwrap();
    let temperature = data.iter().find(|d| d["key"] == "temperature").unwrap();
    assert_eq!(temperature["value"], json!(30));
    let saved: serde_json::Value =
        serde_json::from_str(&storage.text(DEVICES_FILE).unwrap()).unwrap();
    let saved_data = saved[0]["data"].as_array().unwrap();
    let saved_temperature = saved_data
        .iter()
        .find(|d| d["key"] == "temperature")
        .expect("the temperature is saved");
    assert_eq!(saved_temperature["value"], json!(30));
}

/// A home with a zone and a device changed, a storage, a saver behind a gate the test opens, and
/// what the saver showed (spec 023 R3.2).
type Gated = (
    Mutex<Home>,
    MemStorage,
    Rc<Cell<bool>>,
    Rc<RefCell<Vec<String>>>,
);

fn gated() -> Gated {
    let mut rig = Rig::new();
    rig.zone("Attic");
    rig.sensor(1);
    let home = Mutex::new(rig.home);
    assert!(lock(&home).changes().0 && lock(&home).devices.changes().0);
    (
        home,
        MemStorage::default(),
        Rc::new(Cell::new(false)),
        Rc::new(RefCell::new(Vec::new())),
    )
}

#[test]
fn a_closed_gate_writes_nothing_and_keeps_every_file_due() {
    let (home, storage, open, shown) = gated();
    let (gate, sink) = (Rc::clone(&open), Rc::clone(&shown));
    let mut saver = Saver::new(&storage, 0)
        .with_sink(move |message| sink.borrow_mut().push(message.to_string()))
        .with_gate(move || gate.get());
    for now in [1_000, 61_000, 121_000] {
        saver.tick(&home, now, false).unwrap();
    }
    assert_eq!(storage.attempts.get(), 0, "nothing written");
    assert!(shown.borrow().is_empty(), "{:?}", shown.borrow());
    assert!(lock(&home).changes().0, "the zones still due");
    assert!(lock(&home).devices.changes().0, "the devices still due");
}

#[test]
fn the_tick_after_the_gate_opens_writes_the_files_due_at_once() {
    let (home, storage, open, shown) = gated();
    let (gate, sink) = (Rc::clone(&open), Rc::clone(&shown));
    let mut saver = Saver::new(&storage, 0)
        .with_sink(move |message| sink.borrow_mut().push(message.to_string()))
        .with_gate(move || gate.get());
    saver.tick(&home, 1_000, false).unwrap();
    open.set(true);
    saver.tick(&home, 31_000, false).unwrap();
    assert!(storage.text(ZONES_FILE).unwrap().contains("Attic"));
    assert!(storage.text(DEVICES_FILE).is_some());
    assert!(
        shown.borrow().is_empty(),
        "no failure: {:?}",
        shown.borrow()
    );
}

#[test]
fn a_stop_writes_the_files_due_whatever_the_gate() {
    let (home, storage, _open, _shown) = gated();
    let mut saver = Saver::new(&storage, 0).with_gate(|| false);
    saver.tick(&home, 1_000, true).unwrap();
    assert!(storage.text(ZONES_FILE).unwrap().contains("Attic"));
    assert!(storage.text(DEVICES_FILE).is_some());
}

// ---- Spec 028, R1: the dates of a device heard before the clock is set ----

/// A SNZB-02 (EUI ending 0x20) paired at `at` on the box's clock.
fn paired_at(at: u64) -> solite_core::devices::Devices {
    let mut devices = solite_core::devices::Devices::new();
    let mut random = scripted();
    devices.announced(eui(0x20), 0x80, at);
    devices
        .identified(eui(0x20), Some("eWeLink"), Some("TH01"), at, &mut random)
        .unwrap();
    devices
        .reporting_configured(eui(0x20), &[0x0001, 0x0402, 0x0405], at, &mut random)
        .unwrap();
    devices
}

fn status_events(
    hub: &solite_core::live::LiveHandle,
    id: solite_core::live::ClientId,
) -> Vec<String> {
    let mut statuses = Vec::new();
    while let Some(text) = hub.take_batch(id) {
        for event in serde_json::from_str::<serde_json::Value>(&text)
            .unwrap()
            .as_array()
            .unwrap()
        {
            if event["type"] == "device.status_changed" {
                statuses.push(event["status"].as_str().unwrap().to_string());
            }
        }
    }
    statuses
}

fn listen_to_devices(
    home: &Mutex<Home>,
) -> (solite_core::live::LiveHandle, solite_core::live::ClientId) {
    let hub = lock(home).live();
    let id = hub.connect().expect("a free slot");
    let message = json!({"type": "subscribe", "topics": ["devices"]}).to_string();
    assert!(hub.subscribe(id, &message));
    (hub, id)
}

/// A wall clock that reads `base + elapsed`; the test moves `base` (the box's SNTP).
fn settable_clocks(base: &Arc<AtomicU64>) -> Clocks {
    let start = Instant::now();
    let monotonic = move || u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
    let base = Arc::clone(base);
    Clocks {
        now: Arc::new(move || base.load(Ordering::SeqCst) + monotonic()),
        monotonic: Arc::new(monotonic),
    }
}

fn report_temperature(ncp: &Rc<RefCell<Ncp>>, value: f64) {
    ncp.borrow_mut().device_report(
        0,
        &DeviceReport {
            temperature: Some(value),
            ..DeviceReport::default()
        },
    );
}

#[test]
fn a_device_heard_before_the_clock_is_set_gets_the_real_date_and_stays_online() {
    let home = Mutex::new(Home::from_devices(paired_at(0), 0));
    let (hub, client) = listen_to_devices(&home);
    let mut bench = bench(vec![device(Kind::TemperatureHumidity, 0x20, 0x2000)]);
    let base = Arc::new(AtomicU64::new(0));
    let clocks = settable_clocks(&base);
    let (ncp, flag, seen) = (
        Rc::clone(&bench.ncp),
        Arc::clone(&base),
        Rc::new(Cell::new(0)),
    );
    // The events are taken after every read: a status event waiting is replaced by the next one.
    let statuses = Rc::new(RefCell::new(Vec::new()));
    let collected = Rc::clone(&statuses);
    *bench.done.borrow_mut() = Box::new(move || {
        seen.set(seen.get() + 1);
        collected.borrow_mut().extend(status_events(&hub, client));
        match seen.get() {
            5 => report_temperature(&ncp, 21.5),
            10 => flag.store(T0, Ordering::SeqCst),
            _ => {}
        }
        seen.get() > 30
    });
    let storage = MemStorage::default();
    let mut saver = Saver::new(&storage, 0);
    let stopped = engine::run(
        &mut bench.radio,
        &home,
        &mut saver,
        &clocks,
        &bench.stop,
        &mut scripted(),
        false,
        &mut |_| {},
    )
    .unwrap();
    assert!(stopped);
    let end = (clocks.now)();
    let mut guard = lock(&home);
    let seen_at = guard.devices.list()[0].device.last_seen.expect("heard");
    assert!(
        (T0..=end).contains(&seen_at),
        "lastSeen {seen_at} is the real date (clock {T0}..{end})"
    );
    let devices = json_of(&mut guard, "/api/v1/devices");
    assert_eq!(devices[0]["status"], "online");
    assert!(
        devices[0]["lastSeen"]
            .as_str()
            .unwrap()
            .starts_with("2026-")
    );
    drop(guard);
    assert!(
        !statuses.borrow().contains(&"offline".to_string()),
        "the jump does not turn the device offline"
    );
}

#[test]
fn the_clock_set_between_two_runs_of_the_loop_shifts_the_dates_and_keeps_the_device_online() {
    let home = Mutex::new(Home::from_devices(paired_at(0), 0));
    let (hub, client) = listen_to_devices(&home);
    let mut bench = bench(vec![device(Kind::TemperatureHumidity, 0x20, 0x2000)]);
    let base = Arc::new(AtomicU64::new(0));
    let clocks = settable_clocks(&base);
    let (ncp, seen) = (Rc::clone(&bench.ncp), Rc::new(Cell::new(0)));
    let statuses = Rc::new(RefCell::new(Vec::new()));
    let collected = Rc::clone(&statuses);
    let watching = hub.clone();
    *bench.done.borrow_mut() = Box::new(move || {
        seen.set(seen.get() + 1);
        collected
            .borrow_mut()
            .extend(status_events(&watching, client));
        if seen.get() == 5 {
            report_temperature(&ncp, 21.5);
        }
        seen.get() > 12
    });
    let storage = MemStorage::default();
    let mut saver = Saver::new(&storage, 0);
    let run = |bench: &mut Bench, saver: &mut Saver<&MemStorage>| {
        engine::run(
            &mut bench.radio,
            &home,
            saver,
            &clocks,
            &bench.stop,
            &mut scripted(),
            false,
            &mut |_| {},
        )
        .unwrap()
    };
    assert!(run(&mut bench, &mut saver));
    assert!(lock(&home).devices.list()[0].device.last_seen.unwrap() < 1_000);
    // The clock is set while no run is going: the saver's tick sees it first.
    base.store(T0, Ordering::SeqCst);
    saver.tick(&home, (clocks.now)(), false).unwrap();
    let seen_at = lock(&home).devices.list()[0].device.last_seen.unwrap();
    assert!(seen_at >= T0, "lastSeen {seen_at} moved by the jump");
    // The radio restarts: the loop runs again on the real clock.
    bench.stop.store(false, Ordering::SeqCst);
    let again = Rc::new(Cell::new(0));
    let collected = Rc::clone(&statuses);
    let watching = hub.clone();
    *bench.done.borrow_mut() = Box::new(move || {
        again.set(again.get() + 1);
        collected
            .borrow_mut()
            .extend(status_events(&watching, client));
        again.get() > 5
    });
    assert!(run(&mut bench, &mut saver));
    let mut guard = lock(&home);
    assert_eq!(
        json_of(&mut guard, "/api/v1/devices")[0]["status"],
        "online"
    );
    drop(guard);
    assert!(!statuses.borrow().contains(&"offline".to_string()));
}

#[test]
fn a_last_seen_stored_in_1970_loads_as_null_and_unknown_and_a_crossing_keeps_it_so() {
    let devices = paired_at(4_600);
    let storage = MemStorage::with(&[(DEVICES_FILE, &devices.to_json().unwrap())]);
    let mut home = load_home(&&storage, T0).unwrap();
    assert_eq!(home.devices.list()[0].device.last_seen, None);
    // The first turn of the loop computes the statuses.
    home.devices.refresh_status(T0);
    home.observe_wall(T0);
    let listed = json_of(&mut home, "/api/v1/devices");
    assert_eq!(listed[0]["lastSeen"], serde_json::Value::Null);
    assert_eq!(listed[0]["status"], "unknown");
    // A clock that crosses later does not invent a date for it.
    home.observe_wall(5_000);
    home.observe_wall(T0 + 1_000);
    assert_eq!(home.devices.list()[0].device.last_seen, None);
}

// ---- The engine's watchdog (spec 036 R8) ----

#[test]
fn the_saver_feeds_the_watchdog_once_before_each_file_it_writes() {
    let mut rig = Rig::new();
    let garage = rig.garage();
    rig.garage_instance(&garage, json!({}));
    let home = Mutex::new(rig.home);
    let storage = MemStorage::default();
    let feeds = Rc::new(Cell::new(0usize));
    let counter = Rc::clone(&feeds);
    let mut saver = Saver::new(&storage, 0).with_feed(move || counter.set(counter.get() + 1));
    saver.tick(&home, 1, false).unwrap();
    assert!(storage.attempts.get() >= 4, "every file is dirty");
    assert_eq!(feeds.get(), storage.attempts.get(), "one feed per file");
    // Nothing due: nothing written, nothing fed.
    saver.tick(&home, 2, false).unwrap();
    assert_eq!(feeds.get(), storage.attempts.get());
}

#[test]
fn a_saver_without_a_feed_saves_as_before_and_feeds_nothing() {
    let home = home_with_a_zone();
    let storage = MemStorage::default();
    let mut saver = Saver::new(&storage, 0);
    saver.feed();
    saver.tick(&home, 1, false).unwrap();
    assert!(storage.text(ZONES_FILE).is_some());
}

#[test]
fn the_loop_feeds_the_watchdog_at_each_turn() {
    let mut bench = bench(Vec::new());
    let reads = Cell::new(0);
    *bench.done.borrow_mut() = Box::new(move || {
        reads.set(reads.get() + 1);
        reads.get() > 60
    });
    let home = Mutex::new(Home::new(T0));
    let storage = MemStorage::default();
    let feeds = Rc::new(Cell::new(0usize));
    let turns = Rc::new(Cell::new(0usize));
    let (f, t) = (Rc::clone(&feeds), Rc::clone(&turns));
    // The gate is asked once at the end of each turn.
    let mut saver = Saver::new(&storage, T0)
        .with_feed(move || f.set(f.get() + 1))
        .with_gate(move || {
            t.set(t.get() + 1);
            true
        });
    run_engine(&mut bench, &home, &mut saver, false, &mut Vec::new()).unwrap();
    assert!(turns.get() >= 5, "the loop ran {} turns", turns.get());
    assert!(
        feeds.get() >= turns.get(),
        "{} feeds for {} turns",
        feeds.get(),
        turns.get()
    );
}

// ---- A quiet mains device is pinged before it is called offline (fix 2026-11-27) ----

/// The ZBMINIR2 relay of the box session: mains powered, last heard at `seen_at`.
fn relay_home(seen_at: u64) -> Home {
    let mut devices = solite_core::devices::Devices::new();
    let mut random = scripted();
    // Capability 0x8E: a router, mains powered, receiving when idle.
    devices.announced(eui(RELAY), 0x8E, seen_at);
    devices
        .identified(
            eui(RELAY),
            Some("SONOFF"),
            Some("ZBMINIR2"),
            seen_at,
            &mut random,
        )
        .unwrap();
    devices
        .reporting_configured(eui(RELAY), &[0x0006], seen_at, &mut random)
        .unwrap();
    Home::from_devices(devices, seen_at)
}

/// The relay already in the network (as after a restart of the box), quiet on its own.
fn joined_relay() -> SimDevice {
    let mut relay = device(Kind::Relay, RELAY, 0x7A00).in_network();
    relay.reports_on_resume = false;
    relay
}

fn relay_status(window: &Window) -> solite_core::devices::DeviceStatus {
    lock(&window.home).devices.list()[0].device.status
}

#[test]
fn a_quiet_mains_device_that_answers_the_ping_stays_online() {
    // The relay keeps its state for an hour: it sends nothing on its own (its On/Off is reported
    // at most every 3600 s), as the spots' relay did on the box on 2026-11-27.
    let mut window = Window::on(serving_bench(vec![joined_relay()]), relay_home(T0));
    let (hub, client) = listen_to_devices(&window.home);
    window.run(25 * MINUTE, &[]);
    assert_eq!(
        relay_status(&window),
        solite_core::devices::DeviceStatus::Online
    );
    assert!(
        !status_events(&hub, client).contains(&"offline".to_string()),
        "never seen offline"
    );
}

#[test]
fn a_quiet_mains_device_that_does_not_answer_goes_offline_after_ten_minutes() {
    let mut relay = joined_relay();
    relay.silent = true;
    relay.ignore_reads = u32::MAX;
    let mut window = Window::on(serving_bench(vec![relay]), relay_home(T0));
    window.run(9 * MINUTE + 50_000, &[]);
    assert_eq!(
        relay_status(&window),
        solite_core::devices::DeviceStatus::Online
    );
    window.run(10 * MINUTE + 10_000, &[]);
    assert_eq!(
        relay_status(&window),
        solite_core::devices::DeviceStatus::Offline
    );
}

#[test]
fn an_offline_mains_device_that_answers_the_ping_comes_back_online() {
    // Last heard two hours before the boot: offline at the first turn, back once it answers.
    let mut window = Window::on(
        serving_bench(vec![joined_relay()]),
        relay_home(T0 - 2 * HOUR),
    );
    window.run(MINUTE, &[]);
    assert_eq!(
        relay_status(&window),
        solite_core::devices::DeviceStatus::Online
    );
}
