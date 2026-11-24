//! The saves of spec 025: a burst of definition changes written once after the delay, the hold and
//! the flush of the serial console, the write counter. The clock is the `now` given to `tick`; the
//! storage is in memory and counts its writes per file.

mod mem_storage;
mod support;

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Mutex;

use mem_storage::MemStorage;
use serde_json::json;
use solite_core::devices::{VALUE_WRITE_INTERVAL_MS, lock};
use solite_core::engine::{
    DEFINITION_DELAY_MS, DEVICES_FILE, EQUIPMENTS_FILE, RECIPES_FILE, RETRY_MS, SETTINGS_FILE,
    SaveControl, Saver, WriteCounter, ZONES_FILE, flash_line, flushed_line,
};
use solite_core::home::Home;
use support::{Garage, HOUR, MINUTE, Rig, T0, temperature};

/// A start a little after the rig's time, so `T0` is the saver's own start.
const T: u64 = T0 + 1_000;

/// The garage (a zone, a motion sensor, a relay, two equipments), a sensor and a recipe instance,
/// with everything written once: from here a test sees only its own changes.
struct Bench {
    rig: Rig,
    garage: Garage,
    sensor: String,
    home: Mutex<Home>,
    storage: MemStorage,
}

impl Bench {
    fn new() -> Bench {
        let mut rig = Rig::new();
        let garage = rig.garage();
        let sensor = rig.sensor(1);
        rig.measure(&temperature(1, 19.0));
        rig.garage_instance(&garage, json!({}));
        let home = Mutex::new(std::mem::replace(&mut rig.home, Home::new(T0)));
        let storage = MemStorage::default();
        Saver::new(&storage, T0).tick(&home, T0, true).unwrap();
        storage.per_file.borrow_mut().clear();
        storage.attempts.set(0);
        Bench {
            rig,
            garage,
            sensor,
            home,
            storage,
        }
    }

    /// Runs a request of the API on the home in the mutex.
    fn with_rig<R>(&mut self, at: u64, body: impl FnOnce(&mut Rig) -> R) -> R {
        std::mem::swap(&mut self.rig.home, &mut *lock(&self.home));
        self.rig.now = at;
        let result = body(&mut self.rig);
        std::mem::swap(&mut self.rig.home, &mut *lock(&self.home));
        result
    }

    fn rename_light(&mut self, at: u64, name: &str) {
        let path = format!("/api/v1/equipments/{}", self.garage.light);
        let (status, body) = self.with_rig(at, |rig| rig.put(&path, json!({ "name": name })));
        assert_eq!(status, 200, "{body}");
    }

    fn rename_zone(&mut self, at: u64, name: &str) {
        let path = format!("/api/v1/zones/{}", self.garage.zone);
        let (status, body) = self.with_rig(at, |rig| rig.put(&path, json!({ "name": name })));
        assert_eq!(status, 200, "{body}");
    }

    fn rename_device(&mut self, at: u64, name: &str) {
        let path = format!("/api/v1/devices/{}", self.sensor);
        let (status, body) = self.with_rig(at, |rig| rig.put(&path, json!({ "name": name })));
        assert_eq!(status, 200, "{body}");
    }

    fn disable_recipe(&mut self, at: u64) {
        let (_, list) = self.with_rig(at, |rig| rig.get("/api/v1/recipe-instances"));
        let id = list[0]["id"].as_str().unwrap().to_string();
        let path = format!("/api/v1/recipe-instances/{id}/disable");
        let (status, body) = self.with_rig(at, |rig| rig.post(&path, json!({})));
        assert_eq!(status, 200, "{body}");
    }

    /// Changes the four files.
    fn change_everything(&mut self, at: u64) {
        self.rename_zone(at, "Garage 2");
        self.rename_light(at, "Light 2");
        self.rename_device(at, "Sensor 2");
        self.disable_recipe(at);
    }

    fn equipments(&self) -> usize {
        self.storage.writes(EQUIPMENTS_FILE)
    }
}

fn delayed(storage: &MemStorage) -> Saver<&MemStorage> {
    Saver::new(storage, T0).with_definition_delay(DEFINITION_DELAY_MS)
}

fn control() -> &'static SaveControl {
    Box::leak(Box::new(SaveControl::new()))
}

// ---- The delay (R3) ----

#[test]
fn a_burst_of_33_changes_within_2_s_is_one_write() {
    let mut bench = Bench::new();
    let storage = std::mem::take(&mut bench.storage);
    let mut saver = delayed(&storage);
    for i in 0..33u64 {
        let at = T + i * 50;
        bench.rename_light(at, &format!("Light {i}"));
        saver.tick(&bench.home, at, false).unwrap();
    }
    saver.tick(&bench.home, T + 1_999, false).unwrap();
    assert_eq!(storage.writes(EQUIPMENTS_FILE), 0, "nothing before 2 s");
    saver.tick(&bench.home, T + 2_000, false).unwrap();
    assert_eq!(storage.writes(EQUIPMENTS_FILE), 1);
    assert!(storage.text(EQUIPMENTS_FILE).unwrap().contains("Light 32"));
    assert_eq!(
        storage.attempts.get(),
        1,
        "the other files had nothing to write"
    );
}

#[test]
fn a_change_after_the_write_is_a_new_write_after_the_delay() {
    let mut bench = Bench::new();
    let storage = std::mem::take(&mut bench.storage);
    let mut saver = delayed(&storage);
    bench.rename_light(T, "First");
    saver.tick(&bench.home, T, false).unwrap();
    saver.tick(&bench.home, T + 2_000, false).unwrap();
    assert_eq!(storage.writes(EQUIPMENTS_FILE), 1);
    bench.rename_light(T + 4_000, "Second");
    saver.tick(&bench.home, T + 4_000, false).unwrap();
    saver.tick(&bench.home, T + 5_000, false).unwrap();
    assert_eq!(storage.writes(EQUIPMENTS_FILE), 1, "waits");
    saver.tick(&bench.home, T + 6_000, false).unwrap();
    assert_eq!(storage.writes(EQUIPMENTS_FILE), 2);
    assert!(storage.text(EQUIPMENTS_FILE).unwrap().contains("Second"));
}

#[test]
fn a_stream_of_changes_is_still_written_every_2_s() {
    let mut bench = Bench::new();
    let storage = std::mem::take(&mut bench.storage);
    let mut saver = delayed(&storage);
    for k in 0..=10u64 {
        let at = T + k * 500;
        bench.rename_light(at, &format!("Light {k}"));
        saver.tick(&bench.home, at, false).unwrap();
    }
    let writes = storage.writes(EQUIPMENTS_FILE);
    assert!((2..=3).contains(&writes), "{writes} writes in 5 s");
}

#[test]
fn a_stop_during_the_wait_writes_at_once() {
    let mut bench = Bench::new();
    let storage = std::mem::take(&mut bench.storage);
    let mut saver = delayed(&storage);
    bench.rename_light(T, "Stopped");
    saver.tick(&bench.home, T, false).unwrap();
    saver.tick(&bench.home, T + 500, true).unwrap();
    assert_eq!(storage.writes(EQUIPMENTS_FILE), 1);
    assert!(storage.text(EQUIPMENTS_FILE).unwrap().contains("Stopped"));
}

#[test]
fn a_failed_write_after_the_delay_is_retried_after_retry_ms_not_sooner() {
    let mut bench = Bench::new();
    let storage = std::mem::take(&mut bench.storage);
    let shown = Rc::new(RefCell::new(Vec::new()));
    let sink = Rc::clone(&shown);
    let mut saver =
        delayed(&storage).with_sink(move |message| sink.borrow_mut().push(message.to_string()));
    bench.rename_light(T, "Retried");
    saver.tick(&bench.home, T, false).unwrap();
    storage.fail_writes.set(true);
    saver.tick(&bench.home, T + 2_000, false).unwrap();
    assert!(
        shown.borrow()[0].starts_with("equipments: "),
        "{:?}",
        shown.borrow()
    );
    storage.fail_writes.set(false);
    for at in [T + 4_000, T + 30_000, T + 2_000 + RETRY_MS - 1] {
        saver.tick(&bench.home, at, false).unwrap();
    }
    assert_eq!(
        storage.writes(EQUIPMENTS_FILE),
        1,
        "not tried again before the retry"
    );
    saver
        .tick(&bench.home, T + 2_000 + RETRY_MS, false)
        .unwrap();
    assert_eq!(storage.writes(EQUIPMENTS_FILE), 2);
    assert!(storage.text(EQUIPMENTS_FILE).unwrap().contains("Retried"));
}

#[test]
fn the_clock_going_back_restarts_the_wait_from_the_new_time() {
    let mut bench = Bench::new();
    let storage = std::mem::take(&mut bench.storage);
    let mut saver = delayed(&storage);
    bench.rename_light(T + HOUR, "Back");
    saver.tick(&bench.home, T + HOUR, false).unwrap();
    saver.tick(&bench.home, T, false).unwrap();
    saver.tick(&bench.home, T + 1_000, false).unwrap();
    assert_eq!(bench.equipments(), 0);
    assert_eq!(
        storage.writes(EQUIPMENTS_FILE),
        0,
        "waits 2 s from the new time"
    );
    saver.tick(&bench.home, T + 2_000, false).unwrap();
    assert_eq!(storage.writes(EQUIPMENTS_FILE), 1, "not an hour later");
}

#[test]
fn values_alone_are_written_at_their_interval_without_the_delay() {
    let mut bench = Bench::new();
    let storage = std::mem::take(&mut bench.storage);
    let mut saver = delayed(&storage);
    bench.with_rig(T, |rig| rig.measure(&temperature(1, 21.5)));
    assert_eq!(lock(&bench.home).devices.changes(), (false, true));
    saver.tick(&bench.home, T, false).unwrap();
    assert_eq!(
        storage.writes(DEVICES_FILE),
        0,
        "values wait for their interval"
    );
    saver
        .tick(&bench.home, T0 + VALUE_WRITE_INTERVAL_MS, false)
        .unwrap();
    assert_eq!(storage.writes(DEVICES_FILE), 1, "at the interval, at once");
}

#[test]
fn a_device_renamed_is_written_after_the_delay() {
    let mut bench = Bench::new();
    let storage = std::mem::take(&mut bench.storage);
    let mut saver = delayed(&storage);
    bench.rename_device(T, "Renamed sensor");
    saver.tick(&bench.home, T, false).unwrap();
    saver.tick(&bench.home, T + 1_000, false).unwrap();
    assert_eq!(storage.writes(DEVICES_FILE), 0);
    saver.tick(&bench.home, T + 2_000, false).unwrap();
    assert_eq!(storage.writes(DEVICES_FILE), 1);
    assert!(
        storage
            .text(DEVICES_FILE)
            .unwrap()
            .contains("Renamed sensor")
    );
}

#[test]
fn without_a_delay_or_with_0_a_change_is_written_at_the_next_tick() {
    for delay in [None, Some(0)] {
        let mut bench = Bench::new();
        let storage = std::mem::take(&mut bench.storage);
        let mut saver = Saver::new(&storage, T0);
        if let Some(delay) = delay {
            saver = saver.with_definition_delay(delay);
        }
        bench.rename_light(T, "At once");
        saver.tick(&bench.home, T, false).unwrap();
        assert_eq!(storage.writes(EQUIPMENTS_FILE), 1, "{delay:?}");
    }
}

#[test]
fn the_wait_starts_when_the_clock_gate_opens() {
    let mut bench = Bench::new();
    let storage = std::mem::take(&mut bench.storage);
    let open = Rc::new(Cell::new(false));
    let gate = Rc::clone(&open);
    let mut saver = delayed(&storage).with_gate(move || gate.get());
    bench.rename_light(T, "Gated");
    saver.tick(&bench.home, T, false).unwrap();
    saver.tick(&bench.home, T + 5_000, false).unwrap();
    open.set(true);
    saver.tick(&bench.home, T + 5_000, false).unwrap();
    saver.tick(&bench.home, T + 6_000, false).unwrap();
    assert_eq!(
        storage.writes(EQUIPMENTS_FILE),
        0,
        "2 s from the gate's opening"
    );
    saver.tick(&bench.home, T + 7_000, false).unwrap();
    assert_eq!(storage.writes(EQUIPMENTS_FILE), 1);
}

// ---- The hold and the flush (R1) ----

#[test]
fn while_held_nothing_is_written_and_every_file_stays_due() {
    let mut bench = Bench::new();
    let storage = std::mem::take(&mut bench.storage);
    let control = control();
    let mut saver = delayed(&storage).with_control(control);
    control.hold();
    bench.change_everything(T);
    for minute in 0..=20 {
        bench.with_rig(T + minute * MINUTE, |rig| {
            rig.measure(&temperature(1, 20.0 + minute as f64 / 10.0));
        });
        saver.tick(&bench.home, T + minute * MINUTE, false).unwrap();
    }
    assert_eq!(storage.attempts.get(), 0, "nothing written while held");
    let home = lock(&bench.home);
    assert_eq!(home.changes(), (true, true));
    assert!(home.recipes_changed());
    assert!(home.devices.changes().0);
}

#[test]
fn a_flush_writes_each_changed_file_once_and_ends_the_hold() {
    let mut bench = Bench::new();
    let storage = std::mem::take(&mut bench.storage);
    let control = control();
    let mut saver = delayed(&storage).with_control(control);
    control.hold();
    bench.change_everything(T);
    saver.tick(&bench.home, T, false).unwrap();
    control.ask_flush();
    saver.tick(&bench.home, T + 100, false).unwrap();
    for name in [DEVICES_FILE, ZONES_FILE, EQUIPMENTS_FILE, RECIPES_FILE] {
        assert_eq!(storage.writes(name), 1, "{name}");
    }
    assert!(!control.is_held(), "the hold ended");
    assert_eq!(flushed_line(4), "saves: flushed, 4 files written");
    // Then the normal rule: a new change waits for the delay.
    bench.rename_light(T + 1_000, "After");
    saver.tick(&bench.home, T + 1_000, false).unwrap();
    assert_eq!(storage.writes(EQUIPMENTS_FILE), 1);
    saver.tick(&bench.home, T + 3_000, false).unwrap();
    assert_eq!(storage.writes(EQUIPMENTS_FILE), 2);
}

#[test]
fn a_flush_writes_the_settings_among_the_changed_files() {
    let mut bench = Bench::new();
    let storage = std::mem::take(&mut bench.storage);
    let control = control();
    let mut saver = delayed(&storage).with_control(control);
    control.hold();
    let (status, body) = bench.with_rig(T, |rig| {
        rig.put("/api/v1/settings", json!({"home.name": "Annexe"}))
    });
    assert_eq!(status, 200, "{body}");
    bench.rename_zone(T, "Garage 2");
    saver.tick(&bench.home, T + 5_000, false).unwrap();
    assert_eq!(storage.attempts.get(), 0, "held");
    control.ask_flush();
    saver.tick(&bench.home, T + 5_100, false).unwrap();
    assert_eq!(storage.writes(SETTINGS_FILE), 1);
    assert_eq!(storage.writes(ZONES_FILE), 1);
    assert_eq!(storage.attempts.get(), 2, "the two files the flush counts");
    assert_eq!(
        storage.text(SETTINGS_FILE).unwrap(),
        r#"{"home.name":"Annexe"}"#
    );
    assert!(!control.is_held());
}

#[test]
fn a_flush_with_nothing_unsaved_writes_nothing_and_ends_the_hold() {
    let bench = Bench::new();
    let control = control();
    let mut saver = delayed(&bench.storage).with_control(control);
    control.hold();
    control.ask_flush();
    saver.tick(&bench.home, T, false).unwrap();
    assert_eq!(bench.storage.attempts.get(), 0);
    assert!(!control.is_held());
}

#[test]
fn a_flush_asked_while_the_clock_gate_is_closed_waits_for_it() {
    let mut bench = Bench::new();
    let storage = std::mem::take(&mut bench.storage);
    let control = control();
    let open = Rc::new(Cell::new(false));
    let gate = Rc::clone(&open);
    let mut saver = delayed(&storage)
        .with_gate(move || gate.get())
        .with_control(control);
    control.hold();
    bench.rename_zone(T, "Gated zone");
    control.ask_flush();
    saver.tick(&bench.home, T, false).unwrap();
    assert_eq!(storage.attempts.get(), 0, "no 1970 date: the flush waits");
    assert!(control.is_held());
    open.set(true);
    saver.tick(&bench.home, T + 100, false).unwrap();
    assert_eq!(storage.writes(ZONES_FILE), 1);
    assert!(!control.is_held());
}

#[test]
fn a_stop_while_held_writes_the_files() {
    let mut bench = Bench::new();
    let storage = std::mem::take(&mut bench.storage);
    let control = control();
    let mut saver = delayed(&storage).with_control(control);
    control.hold();
    bench.change_everything(T);
    saver.tick(&bench.home, T + 100, true).unwrap();
    assert_eq!(storage.attempts.get(), 4);
}

#[test]
fn a_flush_whose_zones_write_fails_writes_the_others_and_retries_the_zones_later() {
    let mut bench = Bench::new();
    let storage = std::mem::take(&mut bench.storage);
    let control = control();
    let shown = Rc::new(RefCell::new(Vec::new()));
    let sink = Rc::clone(&shown);
    let mut saver = delayed(&storage)
        .with_control(control)
        .with_sink(move |message| sink.borrow_mut().push(message.to_string()));
    control.hold();
    bench.change_everything(T);
    *storage.fail_file.borrow_mut() = Some(ZONES_FILE.to_string());
    control.ask_flush();
    saver.tick(&bench.home, T, false).unwrap();
    assert!(
        shown.borrow()[0].starts_with("zones: "),
        "{:?}",
        shown.borrow()
    );
    let written: Vec<_> = [DEVICES_FILE, EQUIPMENTS_FILE, RECIPES_FILE]
        .into_iter()
        .filter(|name| storage.text(name).is_some())
        .collect();
    assert_eq!(written.len(), 3, "the three others written: {written:?}");
    assert!(!control.is_held());
    *storage.fail_file.borrow_mut() = None;
    saver.tick(&bench.home, T + 10_000, false).unwrap();
    assert_eq!(storage.writes(ZONES_FILE), 1, "not before the retry");
    saver.tick(&bench.home, T + RETRY_MS, false).unwrap();
    assert_eq!(storage.writes(ZONES_FILE), 2);
    assert!(storage.text(ZONES_FILE).unwrap().contains("Garage 2"));
}

#[test]
fn holding_twice_is_one_hold_and_a_second_flush_writes_only_what_changed_since() {
    let mut bench = Bench::new();
    let storage = std::mem::take(&mut bench.storage);
    let control = control();
    let mut saver = delayed(&storage).with_control(control);
    control.hold();
    control.hold();
    bench.change_everything(T);
    control.ask_flush();
    saver.tick(&bench.home, T, false).unwrap();
    assert_eq!(storage.attempts.get(), 4);
    control.hold();
    bench.rename_zone(T + 1_000, "Garage 3");
    control.ask_flush();
    saver.tick(&bench.home, T + 1_000, false).unwrap();
    assert_eq!(storage.attempts.get(), 5, "the zones only");
    assert_eq!(storage.writes(ZONES_FILE), 2);
}

// ---- The write counter (R2) ----

#[test]
fn the_write_counter_counts_and_stops_at_its_maximum() {
    let counter = WriteCounter::new();
    assert_eq!(
        (counter.record(), counter.record(), counter.record()),
        (1, 2, 3)
    );
    assert_eq!(counter.count(), 3);
    assert_eq!(flash_line(3), "flash: 3 writes since boot");
    let full = WriteCounter::starting_at(u32::MAX - 1);
    assert_eq!(full.record(), u32::MAX);
    assert_eq!(full.record(), u32::MAX, "never wraps");
    assert_eq!(full.count(), u32::MAX);
}

#[test]
fn the_values_interval_starts_again_when_the_clock_gate_opens() {
    // The box's saver starts in 1970: when the clock is set, the values are not due at once.
    let mut bench = Bench::new();
    let storage = std::mem::take(&mut bench.storage);
    let open = Rc::new(Cell::new(false));
    let gate = Rc::clone(&open);
    let mut saver = Saver::new(&storage, 0).with_gate(move || gate.get());
    saver.tick(&bench.home, 1_000, false).unwrap();
    open.set(true);
    bench.with_rig(T, |rig| rig.measure(&temperature(1, 22.5)));
    assert_eq!(lock(&bench.home).devices.changes(), (false, true));
    saver.tick(&bench.home, T, false).unwrap();
    saver.tick(&bench.home, T + MINUTE, false).unwrap();
    assert_eq!(storage.writes(DEVICES_FILE), 0, "not at the clock's jump");
    saver
        .tick(&bench.home, T + VALUE_WRITE_INTERVAL_MS, false)
        .unwrap();
    assert_eq!(
        storage.writes(DEVICES_FILE),
        1,
        "10 minutes after the gate opened"
    );
}
