//! The simulated radio's memory file: what a second run finds.

mod support;
use std::fs;
use std::os::unix::fs::PermissionsExt;

use mini_sowel_sim::Ncp;
use support::{ONE_SENSOR, joined, measure_lines, scenario, scratch, sim};

#[test]
fn a_second_run_resumes_the_network_knows_the_device_and_sends_its_reports() {
    let dir = scratch();
    let file = dir.join("sim-test.json");
    let mut first = sim(ONE_SENSOR, Some(&file), 1);
    let network = first.start().unwrap();
    assert!(network.formed);
    let events = first.pair(10);
    assert_eq!(joined(&events).len(), 1);
    let text = fs::read_to_string(&file).unwrap();
    assert!(text.contains("78563412004B1200"), "{text}");
    let mode = fs::metadata(&file).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "the memory file is for its owner only");
    drop(first);

    // A second run: the same file, a fresh program.
    let mut second = sim(ONE_SENSOR, Some(&file), 1);
    let resumed = second.listen().unwrap();
    assert!(!resumed.formed);
    assert_eq!(resumed.pan_id, network.pan_id);
    assert_eq!(resumed.extended_pan_id, network.extended_pan_id);
    // The device is known: its measures are attributed to its address, and its reporting was
    // kept: the report of the scenario goes through.
    let events = second.poll_for(120_000);
    let lines = measure_lines(&events);
    assert_eq!(
        lines[..3],
        [
            "0x00124B0012345678 temperature 19.52 °C".to_string(),
            "0x00124B0012345678 humidity 54.1 %".to_string(),
            "0x00124B0012345678 battery 100 %".to_string(),
        ]
    );
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_formed_network_in_the_scenario_applies_only_when_there_is_no_file() {
    let dir = scratch();
    let file = dir.join("sim-formed.json");
    let json = ONE_SENSOR.replace("\"none\"", "\"formed\"");
    let mut s = sim(&json, Some(&file), 1);
    let network = s.listen().unwrap();
    assert!(!network.formed);
    assert_eq!(network.pan_id, 0x5A5A);
    assert_eq!(network.extended_pan_id, [0x5A; 8]);
    assert_eq!(network.channel, 15);
    assert!(network.keys.is_none(), "a resumed network has no backup");
    assert!(fs::read_to_string(&file).unwrap().contains("23130"));
    drop(s);

    // With a file, the scenario's network is not looked at: a file with no network wins.
    fs::write(&file, r#"{ "network": null, "devices": [] }"#).unwrap();
    let mut s = sim(&json, Some(&file), 1);
    assert!(matches!(
        s.listen(),
        Err(mini_sowel_core::radio::RadioError::NoNetwork)
    ));
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_memory_file_that_does_not_parse_or_cannot_be_written_is_an_error_naming_it() {
    let dir = scratch();
    fs::create_dir_all(&dir).unwrap();
    let scenario = scenario(ONE_SENSOR);

    let file = dir.join("sim-bad.json");
    fs::write(&file, "{ not json").unwrap();
    let error = Ncp::from_scenario(&scenario, Some(&file)).err().unwrap();
    assert!(
        error.starts_with(&format!("sim memory: {}: ", file.display())),
        "{error}"
    );

    // A file with an address the format cannot hold is as broken.
    fs::write(
        &file,
        r#"{ "network": null, "devices": [ { "eui64Wire": "zz", "nwk": 1, "joined": true,
        "bound": false, "configured": [], "configuredAttributes": [], "reportsSent": false,
        "relayOn": false, "cieWritten": false, "enrolled": false } ] }"#,
    )
    .unwrap();
    assert!(Ncp::from_scenario(&scenario, Some(&file)).is_err());

    // The folder of the file is a file: the file cannot be read nor written, and nothing runs.
    let blocker = dir.join("blocker");
    fs::write(&blocker, "").unwrap();
    let file = blocker.join("sim-x.json");
    let error = Ncp::from_scenario(&scenario, Some(&file)).err().unwrap();
    assert!(
        error.starts_with(&format!("sim memory: {}: ", file.display())),
        "{error}"
    );

    // A folder that cannot be written to: the radio does not start.
    let locked = dir.join("locked");
    fs::create_dir_all(&locked).unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o555)).unwrap();
    let file = locked.join("sim-y.json");
    let error = Ncp::from_scenario(&scenario, Some(&file)).err();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
    let error = error.expect("a folder that cannot be written is an error");
    // The same with a file that exists (listening): still an error before running.
    let kept = locked.join("sim-z.json");
    fs::write(&kept, r#"{ "network": null, "devices": [] }"#).unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o555)).unwrap();
    let existing = Ncp::from_scenario(&scenario, Some(&kept)).err();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(
        existing
            .expect("an existing file in a locked folder")
            .contains("cannot write")
    );
    assert!(error.contains("cannot write"), "{error}");
    assert!(error.contains(&file.display().to_string()), "{error}");
    fs::remove_dir_all(&dir).unwrap();
}
