//! Tests of the simulated radio's private logic: what a device answers and sends, the records,
//! and frames that are not valid.

use super::*;

const EUI: [u8; 8] = [1, 2, 3, 4, 5, 6, 7, 8];
const NWK: u16 = 0x2000;

/// The ZCL frame and cluster of an `incomingMessageHandler` callback (EZSP 13).
fn zcl_of(callback: &[u8]) -> Option<(u16, Vec<u8>)> {
    if callback.get(3..5)? != [0x45, 0x00] {
        return None;
    }
    let params = callback.get(5..)?;
    let cluster = u16_at(params, 3)?;
    let length = usize::from(*params.get(18)?);
    Some((cluster, params.get(19..19 + length)?.to_vec()))
}

/// A radio with one device of this kind in the network, with these attributes reporting.
fn joined(kind: Kind, reporting: &[(u16, u16)]) -> Ncp {
    let mut ncp = Ncp::recording();
    let mut device = SimDevice::of_kind(kind, EUI, None, None);
    device.joined = true;
    device.nwk = NWK;
    device.sleepy_ms = None;
    device.configured_attributes = reporting.to_vec();
    ncp.devices = vec![device];
    ncp
}

/// The host sends a ZCL frame to the device; what the device sends back.
fn ask(ncp: &mut Ncp, cluster: u16, message: &[u8]) -> Vec<(u16, Vec<u8>)> {
    let sent = SentMessage {
        destination: NWK,
        profile: 0x0104,
        cluster,
        source_endpoint: 1,
        destination_endpoint: 1,
        tag: 1,
        message: message.to_vec(),
    };
    let mut reply = Reply::default();
    ncp.device_answers(&mut reply, &sent, &[0; 11], SL_OK);
    reply.after.iter().filter_map(|c| zcl_of(c)).collect()
}

/// What the radio sends at `now` (timed callbacks), as ZCL frames.
fn tick(ncp: &mut Ncp, now: u64) -> Vec<(u16, Vec<u8>)> {
    let bytes = ncp.on_tick(now);
    let mut frames = Vec::new();
    for event in Decoder::new().feed(&bytes) {
        if let DecodeEvent::Frame(AshFrame::Data { payload, .. }) = event
            && let Some(frame) = zcl_of(&payload)
        {
            frames.push(frame);
        }
    }
    frames
}

fn on_off_state(frame: &[u8]) -> bool {
    // Report Attributes: control, tsn, 0x0A, attribute (2), type, value.
    assert_eq!(&frame[2..6], &[0x0A, 0x00, 0x00, 0x10]);
    frame[6] == 1
}

#[test]
fn a_relay_obeys_on_off_and_toggle_with_a_default_response_and_a_report() {
    let mut ncp = joined(Kind::Relay, &[(0x0006, 0x0000)]);
    let mut states = Vec::new();
    for (tsn, command) in [(7u8, 0x01u8), (8, 0x00), (9, 0x02)] {
        let answers = ask(&mut ncp, 0x0006, &[0x01, tsn, command]);
        assert_eq!(answers.len(), 2, "command {command}");
        assert_eq!(answers[0], (0x0006, vec![0x18, tsn, 0x0B, command, 0x00]));
        assert_eq!(answers[1].0, 0x0006);
        states.push(on_off_state(&answers[1].1));
    }
    assert_eq!(states, vec![true, false, true]);
    assert!(ncp.devices[0].relay_on);
}

#[test]
fn a_command_that_disables_the_default_response_gets_none_but_the_relay_still_reports() {
    let mut ncp = joined(Kind::Relay, &[(0x0006, 0x0000)]);
    // Frame control 0x11: cluster-specific, "disable default response".
    let answers = ask(&mut ncp, 0x0006, &[0x11, 3, 0x01]);
    assert_eq!(answers.len(), 1);
    assert!(on_off_state(&answers[0].1));
    // Without a reporting set up, the relay obeys and says nothing more than its Default
    // Response.
    let mut silent = joined(Kind::Relay, &[]);
    let answers = ask(&mut silent, 0x0006, &[0x01, 4, 0x01]);
    assert_eq!(answers, vec![(0x0006, vec![0x18, 4, 0x0B, 0x01, 0x00])]);
    assert!(silent.devices[0].relay_on);
}

#[test]
fn a_motion_sensor_that_is_not_enrolled_sends_nothing() {
    let mut ncp = joined(Kind::Motion, &[]);
    let motion = DeviceReport {
        motion: Some(true),
        ..DeviceReport::default()
    };
    assert!(!ncp.device_report(0, &motion));
    assert!(tick(&mut ncp, 100_000).is_empty());
    // The CIE address alone does not enroll it: the Zone Enroll Response is needed too, and
    // the response without the address is not enough.
    let mut early = joined(Kind::Motion, &[]);
    ask(&mut early, 0x0500, &[0x01, 1, 0x00, 0x00, 0x01]);
    assert!(!early.device_report(0, &motion));
}

#[test]
fn an_enrolled_motion_sensor_sends_the_alarm_then_clears_it_sixty_seconds_later() {
    let mut ncp = joined(Kind::Motion, &[]);
    // Write Attributes: the CIE address (attribute 0x0010, IEEE address).
    let mut write = vec![0x00, 1, 0x02, 0x10, 0x00, 0xF0];
    write.extend_from_slice(&[9; 8]);
    let answers = ask(&mut ncp, 0x0500, &write);
    assert_eq!(answers, vec![(0x0500, vec![0x18, 1, 0x04, 0x00])]);
    // Zone Enroll Response: success, zone id 0.
    ask(&mut ncp, 0x0500, &[0x01, 2, 0x00, 0x00, 0x00]);
    assert!(ncp.devices[0].enrolled);

    ncp.set_now(1_000);
    let motion = DeviceReport {
        motion: Some(true),
        ..DeviceReport::default()
    };
    assert!(ncp.device_report(0, &motion));
    let set = tick(&mut ncp, 1_000);
    assert_eq!(set.len(), 1);
    assert_eq!(set[0].0, 0x0500);
    // Zone Status Change Notification: cluster-specific, command 0, status 0x0001.
    assert_eq!(&set[0].1[2..5], &[0x00, 0x01, 0x00]);
    assert_eq!(set[0].1[0], 0x19);
    assert!(tick(&mut ncp, 60_999).is_empty());
    let cleared = tick(&mut ncp, 61_000);
    assert_eq!(cleared.len(), 1);
    assert_eq!(&cleared[0].1[2..5], &[0x00, 0x00, 0x00]);
}

#[test]
fn a_report_from_a_device_not_in_the_network_or_for_an_attribute_not_set_up_is_not_sent() {
    let both = DeviceReport {
        temperature: Some(20.0),
        humidity: Some(50.0),
        battery: Some(80),
        ..DeviceReport::default()
    };
    // Not in the network.
    let mut ncp = joined(Kind::TemperatureHumidity, &[(0x0402, 0), (0x0405, 0)]);
    ncp.devices[0].joined = false;
    assert!(!ncp.device_report(0, &both));
    assert!(tick(&mut ncp, 1).is_empty());
    // Only what was set up is sent: humidity and battery were not.
    let mut ncp = joined(Kind::TemperatureHumidity, &[(0x0402, 0)]);
    assert!(ncp.device_report(0, &both));
    let sent = tick(&mut ncp, 1);
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].0, 0x0402);
    // Nothing set up: nothing sent.
    let mut ncp = joined(Kind::TemperatureHumidity, &[]);
    assert!(!ncp.device_report(0, &both));
    // A device that does not exist.
    assert!(!ncp.device_report(5, &both));
}

#[test]
fn a_sleepy_sensor_answers_only_for_ten_seconds_after_joining_or_after_a_report() {
    let read = [0x00, 1, 0x00, 0x04, 0x00, 0x05, 0x00];
    let mut ncp = joined(Kind::TemperatureHumidity, &[(0x0402, 0)]);
    ncp.devices[0].sleepy_ms = Some(AWAKE_MS);
    ncp.devices[0].awake_until = 10_000; // joined at 0
    ncp.set_now(9_000);
    assert_eq!(ask(&mut ncp, 0x0000, &read).len(), 1, "awake after joining");
    ncp.set_now(15_000);
    assert!(ask(&mut ncp, 0x0000, &read).is_empty(), "asleep 15 s later");
    // A report wakes it for ten seconds.
    ncp.set_now(30_000);
    let report = DeviceReport {
        temperature: Some(20.0),
        ..DeviceReport::default()
    };
    assert!(ncp.device_report(0, &report));
    ncp.set_now(35_000);
    assert_eq!(
        ask(&mut ncp, 0x0000, &read).len(),
        1,
        "awake after a report"
    );
    ncp.set_now(45_000);
    assert!(ask(&mut ncp, 0x0000, &read).is_empty());
}

#[test]
fn the_records_are_off_by_default_and_capped_at_ten_thousand_entries_when_on() {
    // Off by default: nothing is kept.
    let mut quiet = Ncp::new();
    quiet.on_bytes(&[0x42; 100]);
    for _ in 0..20 {
        quiet.respond(&[0, 0x00, 0x18]);
    }
    assert!(quiet.received.is_empty());
    assert!(quiet.commands.is_empty());
    assert!(quiet.sent.is_empty());
    assert!(!Ncp::new().record);

    // On: kept, up to the cap, then dropped.
    let mut ncp = Ncp::recording();
    ncp.on_bytes(&vec![0x42; MAX_RECORDS + 50]);
    assert_eq!(ncp.received.len(), MAX_RECORDS);
    for _ in 0..MAX_RECORDS + 50 {
        ncp.respond(&[0, 0x00, 0x18]);
    }
    assert_eq!(ncp.commands.len(), MAX_RECORDS);
    let mut args = vec![0u8; 15];
    args[14] = 1; // tag
    args.push(0); // length 0
    let mut command = vec![0, 0x00, 0x01, 0x34, 0x00];
    command.extend_from_slice(&args);
    for _ in 0..MAX_RECORDS + 50 {
        ncp.respond(&command);
    }
    assert_eq!(ncp.sent.len(), MAX_RECORDS);
}

/// Bytes from a generator that is the same on every run.
fn noise(seed: u32, len: usize) -> Vec<u8> {
    let mut state = seed;
    (0..len)
        .map(|_| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (state >> 24) as u8
        })
        .collect()
}

#[test]
fn frames_that_are_not_valid_never_bring_the_radio_down() {
    // Random bytes at the ASH layer.
    let mut ncp = Ncp::recording();
    for seed in 0..200 {
        ncp.on_bytes(&noise(seed, 1 + (seed as usize % 300)));
    }
    // Truncated or garbled EZSP in valid ASH frames, on both stack versions.
    for protocol in [13u8, 14] {
        let mut ncp = Ncp::recording();
        ncp.protocol = protocol;
        ncp.devices = vec![SimDevice::sensor()];
        ncp.devices[0].joined = true;
        let mut frm = 0u8;
        for seed in 0..300u32 {
            let mut ezsp = noise(seed, (seed as usize) % 40);
            if seed % 3 == 0 && ezsp.len() >= 5 {
                ezsp[2] = 0x01; // extended format
            }
            if seed % 5 == 0 && ezsp.len() >= 5 {
                // A command the radio knows, with arguments cut short.
                let id = [0x34u16, 0x61, 0x22, 0x68, 0x1E][seed as usize % 5];
                ezsp[2] = 0x01;
                ezsp[3..5].copy_from_slice(&id.to_le_bytes());
            }
            let bytes = encode(&AshFrame::Data {
                frm: frm % 8,
                ack: 0,
                retransmit: false,
                payload: ezsp,
            });
            frm = frm.wrapping_add(1);
            ncp.on_bytes(&bytes);
        }
    }
    // Every command with every argument length, and messages to every kind of device that are
    // empty or cut short.
    let mut ncp = Ncp::recording();
    ncp.devices = vec![SimDevice::sensor()];
    ncp.devices[0].joined = true;
    for id in 0..0x200u16 {
        for len in 0..60usize {
            let mut command = vec![1, 0x00, 0x01];
            command.extend_from_slice(&id.to_le_bytes());
            command.extend(std::iter::repeat_n(0xFF, len));
            ncp.respond(&command);
            let mut command = vec![1, 0x00, 0x01];
            command.extend_from_slice(&id.to_le_bytes());
            command.extend(std::iter::repeat_n(0x00, len));
            ncp.respond(&command);
        }
    }
    for kind in [
        Kind::TemperatureHumidity,
        Kind::Motion,
        Kind::Relay,
        Kind::Dimmer,
    ] {
        let mut ncp = joined(kind, &[(0x0006, 0), (0x0008, 0), (0x0402, 0)]);
        for cluster in [
            0x0000, 0x0004, 0x0005, 0x0006, 0x0008, 0x0021, 0x0402, 0x0500,
        ] {
            for len in 0..14usize {
                for fill in [0x00u8, 0x01, 0x02, 0x06, 0x11, 0xFF] {
                    for profile in [0x0000u16, 0x0104] {
                        let sent = SentMessage {
                            destination: NWK,
                            profile,
                            cluster,
                            source_endpoint: 1,
                            destination_endpoint: 1,
                            tag: 0,
                            message: vec![fill; len],
                        };
                        let mut reply = Reply::default();
                        ncp.device_answers(&mut reply, &sent, &[0; 11], SL_OK);
                    }
                }
            }
        }
    }
}

#[test]
fn configuring_a_cluster_twice_keeps_one_entry() {
    let mut ncp = joined(Kind::TemperatureHumidity, &[]);
    // Configure Reporting of the temperature: one record.
    let request = [
        0x00, 1, 0x06, 0x00, 0x00, 0x00, 0x29, 30, 0, 0x10, 0x0E, 20, 0,
    ];
    for _ in 0..3 {
        assert_eq!(ask(&mut ncp, 0x0402, &request).len(), 1);
    }
    assert_eq!(ncp.devices[0].configured, vec![0x0402]);
    assert_eq!(ncp.devices[0].configured_attributes, vec![(0x0402, 0x0000)]);
}

#[test]
fn a_dimmer_obeys_move_to_level_with_a_default_response_and_reports_level_and_state() {
    let mut ncp = joined(Kind::Dimmer, &[(0x0006, 0x0000), (0x0008, 0x0000)]);
    // Move to Level with On/Off: level 200, transition 0, options 0.
    let answers = ask(&mut ncp, 0x0008, &[0x01, 5, 0x04, 200, 0, 0, 0, 0]);
    assert_eq!(answers.len(), 3);
    assert_eq!(answers[0], (0x0008, vec![0x18, 5, 0x0B, 0x04, 0x00]));
    assert!(on_off_state(&answers[1].1));
    assert_eq!(answers[2].0, 0x0008);
    // Report Attributes: control, tsn, 0x0A, attribute (2), type uint8, value.
    assert_eq!(&answers[2].1[2..6], &[0x0A, 0x00, 0x00, 0x20]);
    assert_eq!(answers[2].1[6], 200);
    assert!(ncp.devices[0].relay_on);
    assert_eq!(ncp.devices[0].level, 200);
    // Level 0 turns it off.
    let answers = ask(&mut ncp, 0x0008, &[0x01, 6, 0x04, 0, 0, 0, 0, 0]);
    assert!(!on_off_state(&answers[1].1));
    assert!(!ncp.devices[0].relay_on);
    // A relay has no level control: it answers nothing.
    let mut relay = joined(Kind::Relay, &[]);
    assert!(ask(&mut relay, 0x0008, &[0x01, 7, 0x04, 9, 0, 0, 0, 0]).is_empty());
}

#[test]
fn a_silent_device_answers_no_command_and_no_read_and_a_refusing_one_changes_nothing() {
    let mut ncp = joined(Kind::Relay, &[(0x0006, 0x0000)]);
    ncp.devices[0].silent = true;
    assert!(ask(&mut ncp, 0x0006, &[0x01, 1, 0x01]).is_empty());
    assert!(ask(&mut ncp, 0x0006, &[0x10, 2, 0x00, 0x00, 0x00]).is_empty());
    assert!(!ncp.devices[0].relay_on);
    ncp.devices[0].silent = false;
    ncp.devices[0].refuse_commands = Some(0x81);
    let answers = ask(&mut ncp, 0x0006, &[0x01, 3, 0x01]);
    assert_eq!(answers, vec![(0x0006, vec![0x18, 3, 0x0B, 0x01, 0x81])]);
    assert!(!ncp.devices[0].relay_on);
}

#[test]
fn reading_the_on_off_state_and_the_level_gives_a_read_attributes_response() {
    let mut ncp = joined(Kind::Dimmer, &[]);
    ask(&mut ncp, 0x0008, &[0x01, 1, 0x04, 90, 0, 0, 0, 0]);
    let on_off = ask(&mut ncp, 0x0006, &[0x10, 2, 0x00, 0x00, 0x00]);
    assert_eq!(
        on_off,
        vec![(0x0006, vec![0x18, 2, 0x01, 0x00, 0x00, 0x00, 0x10, 0x01])]
    );
    let level = ask(&mut ncp, 0x0008, &[0x10, 3, 0x00, 0x00, 0x00]);
    assert_eq!(
        level,
        vec![(0x0008, vec![0x18, 3, 0x01, 0x00, 0x00, 0x00, 0x20, 90])]
    );
}

#[test]
fn lookup_node_id_by_eui64_gives_the_node_address_of_a_joined_device_or_0xffff() {
    let mut ncp = joined(Kind::Relay, &[]);
    let mut command = vec![1, 0x00, 0x01, 0x60, 0x00];
    command.extend_from_slice(&EUI);
    let reply = ncp.respond(&command);
    // sequence, frame control, extended marker, id (2), then the node address alone.
    assert_eq!(
        reply.response,
        Some(vec![1, 0x80, 0x01, 0x60, 0x00, 0x00, 0x20])
    );
    let mut other = vec![1, 0x00, 0x01, 0x60, 0x00];
    other.extend_from_slice(&[9; 8]);
    let reply = ncp.respond(&other);
    assert_eq!(
        reply.response,
        Some(vec![1, 0x80, 0x01, 0x60, 0x00, 0xFF, 0xFF])
    );
}
