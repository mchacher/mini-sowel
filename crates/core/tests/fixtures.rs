//! The realistic home of decision D20 as files in `firmware/fixtures/`: the four files of the
//! engine, a scenario that makes its devices report, and the simulated radio's memory that has
//! them joined. The firmware embeds them (spec 016 R4); this test builds the same home as
//! `memory_ceiling.rs` and checks the files are its output. `UPDATE_FIXTURES=1 cargo test -p
//! solite-core --features script-recipes --test fixtures` writes them.

mod mem_storage;
mod realistic;

use std::path::PathBuf;
use std::sync::Mutex;

use mem_storage::MemStorage;
use serde_json::json;
use solite_core::engine::{
    DEVICES_FILE, EQUIPMENTS_FILE, RECIPES_FILE, Saver, ZONES_FILE, load_home,
};
use solite_sim::Scenario;

/// The scenario's file name; the simulated radio's memory is `sim-<name>.json`.
const SCENARIO_FILE: &str = "realistic.json";
const MEMORY_FILE: &str = "sim-realistic.json";

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../firmware/fixtures")
}

/// The device number `i` of the home: its address, most significant byte first.
fn eui(i: u8) -> String {
    format!("00124B00123456{i:02X}")
}

/// Its address as the radio sends it.
fn wire(i: u8) -> String {
    let mut bytes: Vec<String> = (0..8)
        .map(|n| eui(i)[2 * n..2 * n + 2].to_string())
        .collect();
    bytes.reverse();
    bytes.concat()
}

/// The 34 first devices are sensors, the 16 last are the lights of the first zone.
fn is_light(i: u8) -> bool {
    i >= 34
}

/// 120 s of scenario time at most: every sensor reports its measures (a motion sensor its
/// alarm, which makes `motion-light` switch the lights), then its battery, and every light is
/// switched on, the first four off again.
fn scenario() -> String {
    let devices: Vec<_> = (0..50u8)
        .map(|i| {
            if is_light(i) {
                json!({"id": format!("light{i}"), "kind": "relay", "eui64": eui(i)})
            } else if realistic::is_motion(i) {
                json!({"id": format!("motion{i}"), "kind": "motion", "eui64": eui(i)})
            } else {
                json!({"id": format!("sensor{i}"), "kind": "temperature-humidity", "eui64": eui(i)})
            }
        })
        .collect();
    let mut events = Vec::new();
    for i in 0..34u8 {
        let motion = realistic::is_motion(i);
        let id = format!("{}{i}", if motion { "motion" } else { "sensor" });
        let at = format!("{}s", 5 + u32::from(i));
        events.push(if motion {
            // After the start's orders have gone out (the queue of orders is full at the start),
            // before the lights' own reports: the order the recipe sends is the first `on`.
            let late = format!("{}s", 60 + u32::from(i) / 10);
            json!({"at": late, "device": id, "do": "report", "motion": true})
        } else {
            json!({"at": at, "device": id, "do": "report",
            "temperature": 18.0 + f64::from(i) / 4.0, "humidity": 40.0 + f64::from(i)})
        });
        events.push(
            json!({"at": format!("{}s", 50 + u32::from(i)), "device": id, "do": "report",
            "battery": 100 - i}),
        );
    }
    for i in 34..50u8 {
        events.push(
            json!({"at": format!("{}s", 90 + u32::from(i - 34)), "device": format!("light{i}"),
            "do": "report", "light_state": "on"}),
        );
    }
    for i in 34..38u8 {
        events.push(
            json!({"at": format!("{}s", 110 + u32::from(i - 34)), "device": format!("light{i}"),
            "do": "report", "light_state": "off"}),
        );
    }
    let scenario = json!({
        "radio": {"ezsp": 13, "stack": "7.4.4.0", "eui64": "8C6FB9FFFE123456", "network": "formed"},
        "devices": devices,
        "events": events,
    });
    serde_json::to_string_pretty(&scenario).unwrap() + "\n"
}

/// The simulated radio's memory, with every device joined and its reporting set up, as a run of
/// `--pair` leaves it.
fn sim_memory() -> String {
    let devices: Vec<_> = (0..50u8)
        .map(|i| {
            let motion = realistic::is_motion(i);
            let (configured, attributes) = if is_light(i) {
                (json!([6]), json!([[6, 0]]))
            } else if realistic::is_motion(i) {
                (json!([1]), json!([[1, 0x21], [1, 0x20]]))
            } else {
                (
                    json!([0x0402, 0x0405, 0x0001]),
                    json!([[0x0402, 0], [0x0405, 0], [1, 0x21], [1, 0x20]]),
                )
            };
            json!({"eui64Wire": wire(i), "nwk": 0x2000 + u32::from(i), "joined": true,
                "bound": true, "configured": configured, "configuredAttributes": attributes,
                "reportsSent": true, "relayOn": false, "cieWritten": motion, "enrolled": motion,
                "cie": if motion { "563412FEFFB96F8C" } else { "0000000000000000" }, "level": 0, "silent": false, "lookupUnknown": false})
        })
        .collect();
    let memory = json!({
        "network": {"extendedPanWire": "5A5A5A5A5A5A5A5A", "panId": 0x5A5A, "channel": 15,
            "nodeType": 1, "networkKey": "0102030405060708090A0B0C0D0E0F10",
            "trustCenterKey": "1112131415161718191A1B1C1D1E1F20"},
        "devices": devices,
    });
    serde_json::to_string_pretty(&memory).unwrap() + "\n"
}

/// What the home writes, as the engine's saver does it.
fn home_files() -> Vec<(&'static str, String)> {
    let mut random = realistic::random();
    let home = Mutex::new(realistic::build(&mut random).home);
    let storage = MemStorage::default();
    Saver::new(&storage, realistic::T0)
        .tick(&home, realistic::T0, true)
        .unwrap();
    [DEVICES_FILE, ZONES_FILE, EQUIPMENTS_FILE, RECIPES_FILE]
        .into_iter()
        .map(|name| (name, storage.text(name).expect("the saver wrote the file")))
        .collect()
}

/// Every fixture: its name and what it holds.
fn expected() -> Vec<(&'static str, String)> {
    let mut files = home_files();
    files.push((SCENARIO_FILE, scenario()));
    files.push((MEMORY_FILE, sim_memory()));
    files
}

#[test]
fn the_fixture_files_are_the_realistic_homes_output() {
    let update = std::env::var_os("UPDATE_FIXTURES").is_some_and(|v| v == "1");
    for (name, text) in expected() {
        let path = dir().join(name);
        if update {
            std::fs::create_dir_all(dir()).unwrap();
            std::fs::write(&path, &text).unwrap();
        }
        let on_disk = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{name}: {e} (UPDATE_FIXTURES=1 writes the fixtures)"));
        assert!(
            on_disk == text,
            "{name} is not the output of the realistic home (UPDATE_FIXTURES=1 rewrites it)"
        );
    }
}

#[test]
fn the_fixture_home_loads_and_its_scenario_plays_on_its_devices() {
    let files: Vec<(String, String)> = expected()
        .into_iter()
        .map(|(name, text)| (name.to_string(), text))
        .collect();
    let storage = MemStorage::default();
    for (name, text) in &files {
        storage
            .files
            .borrow_mut()
            .insert(name.clone(), text.clone());
    }
    let home = load_home(&&storage, realistic::T0).unwrap();
    assert_eq!(home.devices.list().len(), 50);
    assert_eq!(home.zones().list().len(), 11, "the root and 10 zones");
    assert_eq!(home.equipments().list().len(), 50);
    assert_eq!(home.recipes().len(), 3);

    let scenario = Scenario::parse(&scenario(), SCENARIO_FILE).unwrap();
    assert_eq!(scenario.devices.len(), 50);
    assert!(scenario.events.last().unwrap().at_ms <= 120_000);
    // Every device of the home is a device of the scenario, and every one reports at least once.
    for (i, item) in home.devices.list().iter().enumerate() {
        let spec = &scenario.devices[i];
        assert_eq!(
            &*item.device.source_device_id,
            format!("0x{}", eui(u8::try_from(i).unwrap()).to_lowercase())
        );
        assert!(
            scenario.events.iter().any(|e| e.device == i),
            "device {} ({}) never reports",
            i,
            spec.id
        );
    }
}
