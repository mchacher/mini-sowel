//! The simulated devices joining, played by a scenario against the core's radio.

mod support;
use std::collections::HashSet;

use support::{ONE_SENSOR, identified, joined, measure_lines, sim};

const THREE: &str = r#"{
  "devices": [
    { "id": "th", "kind": "temperature-humidity", "eui64": "00124B0000000001" },
    { "id": "pir", "kind": "motion", "eui64": "00124B0000000002" },
    { "id": "relay", "kind": "relay", "eui64": "00124B0000000003" }
  ],
  "events": [
    { "at": "2s", "device": "th", "do": "join" },
    { "at": "2s", "device": "pir", "do": "join" },
    { "at": "2s", "device": "relay", "do": "join" }
  ]
}"#;

#[test]
fn a_join_before_pairing_opens_waits_and_happens_when_it_opens() {
    let mut s = sim(ONE_SENSOR, None, 1);
    s.start().unwrap();
    // The join is due at 2 s; pairing is closed: nothing joins in 10 s.
    let early = s.poll_for(10_000);
    assert!(joined(&early).is_empty());
    assert!(!s.ncp.borrow().pairing_open());
    // Pairing opens: the device joins at once.
    let events = s.pair(30);
    assert_eq!(joined(&events).len(), 1);
    assert_eq!(joined(&events)[0].0, "00124B0012345678");
    assert_eq!(
        identified(&events),
        vec![(Some("eWeLink".to_string()), Some("TH01".to_string()))]
    );
}

#[test]
fn a_join_after_the_window_closed_waits_for_the_second_window() {
    let json = ONE_SENSOR.replace("\"2s\"", "\"10s\"");
    let mut s = sim(&json, None, 1);
    s.start().unwrap();
    // First window: 3 s, over before the join is due.
    let first = s.pair(3);
    assert!(joined(&first).is_empty());
    let late = s.poll_for(20_000);
    assert!(joined(&late).is_empty(), "the window is closed");
    // Second window: the device joins in it.
    let second = s.pair(30);
    assert_eq!(joined(&second).len(), 1);
}

#[test]
fn three_devices_joining_at_once_get_three_addresses_and_three_interviews() {
    let mut s = sim(THREE, None, 1);
    s.start().unwrap();
    let events = s.pair(30);
    let joins = joined(&events);
    assert_eq!(joins.len(), 3);
    let addresses: HashSet<u16> = joins.iter().map(|(_, nwk)| *nwk).collect();
    assert_eq!(addresses.len(), 3, "distinct network addresses: {joins:?}");
    let mut identities = identified(&events);
    identities.sort();
    let text = |m: &str, n: &str| (Some(m.to_string()), Some(n.to_string()));
    assert_eq!(
        identities,
        vec![
            text("SONOFF", "ZBMINIR2"),
            text("eWeLink", "MS01"),
            text("eWeLink", "TH01"),
        ]
    );
}

#[test]
fn the_default_identity_can_be_overridden_by_the_scenario() {
    let json = ONE_SENSOR.replace(
        r#""kind": "temperature-humidity","#,
        r#""kind": "temperature-humidity", "manufacturer": "Acme", "model": "T-1","#,
    );
    let mut s = sim(&json, None, 1);
    s.start().unwrap();
    let events = s.pair(30);
    assert_eq!(
        identified(&events),
        vec![(Some("Acme".to_string()), Some("T-1".to_string()))]
    );
}

#[test]
fn an_event_at_40_seconds_at_speed_10_is_played_at_4_seconds_of_clock_time() {
    let mut s = sim(ONE_SENSOR, None, 10);
    s.start().unwrap();
    s.radio.permit_join(200).unwrap();
    // The device joins at 0.2 s and is set up at once; the report is due at 4 s, not before.
    let before = s.poll_for(3_500u64.saturating_sub(s.clock.get()));
    assert!(measure_lines(&before).is_empty(), "{before:?}");
    let after = s.poll_for(1_500);
    assert_eq!(
        measure_lines(&after)[..2],
        [
            "0x00124B0012345678 temperature 19.52 °C".to_string(),
            "0x00124B0012345678 humidity 54.1 %".to_string()
        ]
    );
    assert!(s.clock.get() >= 4_000);
}

const PIR: &str = r#"{
  "devices": [ { "id": "pir", "kind": "motion", "eui64": "00124B0000000002" } ],
  "events": [
    { "at": "2s", "device": "pir", "do": "join" },
    { "at": "20s", "device": "pir", "do": "report", "motion": true }
  ]
}"#;

/// The IAS zone messages the host sent, as (command kind, ...): reads, writes, responses.
fn zone_commands(s: &support::Sim) -> Vec<&'static str> {
    s.ncp
        .borrow()
        .sent
        .iter()
        .filter(|m| m.profile == 0x0104 && m.cluster == 0x0500)
        .map(|m| match (m.message[0] & 0x03, m.message[2]) {
            (0, 0x00) => "read",
            (0, 0x02) => "write",
            (1, 0x00) => "response",
            _ => "other",
        })
        .collect()
}

#[test]
fn a_motion_sensor_sends_its_notifications_once_enrolled_and_none_before() {
    // Enrolled by the host's pairing: the motion of the scenario reaches it.
    let mut s = sim(PIR, None, 1);
    s.start().unwrap();
    let events = s.pair(30);
    assert_eq!(
        measure_lines(&events),
        vec![
            "0x00124B0000000002 alarm true".to_string(),
            "0x00124B0000000002 tamper false".to_string()
        ]
    );
    assert_eq!(zone_commands(&s)[..3], ["read", "write", "response"]);

    // A sensor that never reads as enrolled still got the response, but the scenario's
    // motion is not sent by a device that is not enrolled: a device never given the CIE
    // address stays silent.
    let mut silent = sim(PIR, None, 1);
    silent.start().unwrap();
    silent.ncp.borrow_mut().devices[0].zone_stays_unenrolled = true;
    let events = silent.pair(30);
    assert!(
        measure_lines(&events).is_empty(),
        "{:?}",
        measure_lines(&events)
    );
}

#[test]
fn a_second_pairing_finds_the_sensor_enrolled_with_our_address_and_writes_nothing() {
    let dir = support::scratch();
    let file = dir.join("sim-pir.json");
    let mut first = sim(PIR, Some(&file), 1);
    first.start().unwrap();
    first.pair(10);
    assert_eq!(zone_commands(&first)[..3], ["read", "write", "response"]);
    drop(first);

    // A fresh program on the same memory: the sensor announces itself again.
    let mut second = sim(PIR, Some(&file), 1);
    second.start().unwrap();
    let wire = mini_sowel_sim::devices::wire([0x00, 0x12, 0x4B, 0x00, 0x00, 0x00, 0x00, 0x02]);
    let mut announce = vec![0x01, 0x00, 0x20];
    announce.extend_from_slice(&wire);
    announce.push(0x80);
    second
        .ncp
        .borrow_mut()
        .inject(0x2000, 0x0000, 0x0013, 0, &announce);
    let events = second.pair(10);
    assert_eq!(
        zone_commands(&second),
        ["read"],
        "the CIE address read back is ours"
    );
    let outcome = events.iter().find_map(|e| match e {
        mini_sowel_core::radio::PairingEvent::ReportingConfigured { enrollment, .. } => *enrollment,
        _ => None,
    });
    assert_eq!(
        outcome.map(|e| e.outcome),
        Some(mini_sowel_core::radio::EnrollmentOutcome::Enrolled)
    );
    std::fs::remove_dir_all(&dir).unwrap();
}
