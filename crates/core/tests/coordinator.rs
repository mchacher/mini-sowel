//! The coordinator against the scripted radio, with fake time: start or resume the network,
//! pairing, identification of the devices that join.

use solite_core::radio::{Network, NetworkSettings, PairingEvent, RadioError};
use solite_sim::{Attr, Harness, Ncp, SentMessage, SimDevice, StoredNetwork, harness};

const DEVICE_EUI: [u8; 8] = [0x00, 0x12, 0x4B, 0x00, 0x12, 0x34, 0x56, 0x78];
const DEVICE_NWK: u16 = 0x5C3D;

/// A random source that gives the scripted values in order, and then fails.
fn scripted(values: Vec<Vec<u8>>) -> impl FnMut(&mut [u8]) -> Result<(), String> {
    let mut values = values.into_iter();
    move |buffer: &mut [u8]| {
        let value = values.next().ok_or("script exhausted")?;
        assert_eq!(value.len(), buffer.len(), "the core asked for another size");
        buffer.copy_from_slice(&value);
        Ok(())
    }
}

/// Network key, trust-centre key, extended PAN (wire order), PAN (little-endian).
fn draws() -> Vec<Vec<u8>> {
    vec![
        vec![0x11; 16],
        vec![0x22; 16],
        vec![1, 2, 3, 4, 5, 6, 7, 8],
        vec![0x2B, 0x1A],
    ]
}

fn ncp_at(protocol: u8) -> Ncp {
    let mut ncp = Ncp::recording();
    ncp.protocol = protocol;
    ncp
}

fn with_device(device: SimDevice) -> Ncp {
    let mut ncp = Ncp::recording();
    ncp.devices = vec![device];
    ncp
}

/// Identifies the radio and starts its network.
fn start(h: &mut Harness) -> Result<Network, RadioError> {
    h.radio.identify()?;
    h.radio.start_network(
        &NetworkSettings::default(),
        None,
        false,
        &mut scripted(draws()),
        &mut |_| Ok(()),
    )
}

fn started(ncp: Ncp) -> Harness {
    let mut h = harness(ncp);
    start(&mut h).expect("the network starts");
    h
}

fn ids(h: &Harness) -> Vec<u16> {
    h.ncp.borrow().commands.iter().map(|c| c.0).collect()
}

/// Polls for `ms` milliseconds of fake time and returns the events.
fn poll_for(h: &mut Harness, ms: u64) -> Vec<PairingEvent> {
    let end = h.clock.get() + ms;
    let mut events = Vec::new();
    while h.clock.get() < end {
        events.extend(h.radio.poll_pairing().unwrap());
    }
    events
}

/// Polls until no identification is in progress (at most 60 s of fake time).
fn poll_until_idle(h: &mut Harness) -> Vec<PairingEvent> {
    let end = h.clock.get() + 60_000;
    let mut events = Vec::new();
    loop {
        events.extend(h.radio.poll_pairing().unwrap());
        if !h.radio.pairing_busy() || h.clock.get() >= end {
            return events;
        }
    }
}

/// What the host sent for the identification (endpoints and Basic read), not for the reporting.
fn identification_sends(h: &Harness) -> Vec<SentMessage> {
    h.ncp
        .borrow()
        .sent
        .iter()
        .filter(|m| {
            (m.profile == 0x0000 && m.cluster == 0x0005) || (m.profile == 0x0104 && m.cluster == 0)
        })
        .cloned()
        .collect()
}

fn joined_count(events: &[PairingEvent]) -> usize {
    events
        .iter()
        .filter(|e| matches!(e, PairingEvent::Joined { .. }))
        .count()
}

fn identified(events: &[PairingEvent]) -> Vec<(Option<String>, Option<String>)> {
    events
        .iter()
        .filter_map(|e| match e {
            PairingEvent::Identified {
                manufacturer,
                model,
                ..
            } => Some((manufacturer.clone(), model.clone())),
            _ => None,
        })
        .collect()
}

fn some(text: &str) -> Option<String> {
    Some(text.to_string())
}

#[test]
fn a_radio_with_no_network_ezsp_13_forms_one_with_the_given_random_values_in_the_documented_order()
{
    let mut h = harness(Ncp::recording());
    let network = start(&mut h).unwrap();
    assert_eq!(network.pan_id, 0x1A2B);
    assert_eq!(network.extended_pan_id, [8, 7, 6, 5, 4, 3, 2, 1]);
    assert_eq!(network.channel, 15);
    assert!(network.formed);
    let keys = network.keys.clone().expect("a formed network has keys");
    assert_eq!(keys.network_key, [0x11; 16]);
    assert_eq!(keys.trust_center_key, [0x22; 16]);

    // The commands between networkInit and getNetworkParameters, with their parameters.
    let commands = h.ncp.borrow().commands.clone();
    let init = commands.iter().position(|c| c.0 == 0x17).unwrap();
    let params = commands.iter().position(|c| c.0 == 0x28).unwrap();
    let between: Vec<(u16, Vec<u8>)> = commands[init + 1..params].to_vec();
    let ids: Vec<u16> = between.iter().map(|c| c.0).collect();
    assert_eq!(ids, vec![0x68, 0xAB, 0xB1, 0x1E, 0xAB]);
    assert_eq!(between[0].1.len(), 43);
    assert_eq!(&between[0].1[..2], &[0x84, 0x0B]);
    assert_eq!(&between[0].1[2..18], &[0x22; 16]); // the trust-centre key
    assert_eq!(&between[0].1[18..34], &[0x11; 16]); // the network key
    assert_eq!(between[1].1, vec![0x0A, 2, 0x10, 0x01]); // extended security bitmask 0x0110
    let form = &between[3].1;
    assert_eq!(&form[..8], &[1, 2, 3, 4, 5, 6, 7, 8]);
    assert_eq!(&form[8..10], &[0x2B, 0x1A]);
    assert_eq!(form[11], 15); // channel
    assert_eq!(between[4].1, vec![0x07, 1, 1]); // stack token writing on
}

#[test]
fn the_keys_of_a_formed_network_are_never_in_its_debug_text() {
    let mut h = harness(Ncp::recording());
    let network = start(&mut h).unwrap();
    let text = format!("{network:?}");
    assert!(
        !text.contains("17, 17") && !text.contains("34, 34"),
        "{text}"
    );
    assert!(text.contains("hidden"), "{text}");
}

#[test]
fn start_up_sets_the_values_a_sleepy_end_device_needs() {
    let h = started(Ncp::recording());
    let commands = h.ncp.borrow().commands.clone();
    let config = |id: u8, value: u16| {
        let v = value.to_le_bytes();
        commands
            .iter()
            .any(|c| c.0 == 0x53 && c.1 == vec![id, v[0], v[1]])
    };
    assert!(config(0x12, 7680), "INDIRECT_TRANSMISSION_TIMEOUT");
    assert!(config(0x11, 32), "MAX_END_DEVICE_CHILDREN");
    assert!(config(0x13, 8), "END_DEVICE_POLL_TIMEOUT");
    // The host's endpoint 1, Home Automation profile, is registered before the network.
    let add = commands.iter().position(|c| c.0 == 0x02).unwrap();
    let init = commands.iter().position(|c| c.0 == 0x17).unwrap();
    assert!(add < init);
    assert_eq!(&commands[add].1[..3], &[1, 0x04, 0x01]);
    // The concentrator is set up after the network is up.
    assert!(ids(&h).contains(&0x10) && ids(&h).contains(&0x5A));
}

#[test]
fn network_up_arriving_before_the_form_network_response_is_kept_and_the_start_succeeds() {
    let mut ncp = Ncp::recording();
    ncp.network_up_before_response = true;
    let mut h = harness(ncp);
    assert!(start(&mut h).unwrap().formed);
}

#[test]
fn twenty_unused_callbacks_then_a_trust_center_join_do_not_lose_the_join() {
    let mut device = SimDevice::sensor();
    device.announce = false;
    let mut ncp = with_device(device);
    ncp.unused_burst = 20;
    let mut h = started(ncp);
    h.radio.permit_join(30).unwrap();
    let events = poll_for(&mut h, 500);
    assert_eq!(
        events,
        vec![PairingEvent::Joined {
            eui64: DEVICE_EUI,
            nwk: DEVICE_NWK
        }]
    );
}

#[test]
fn a_response_with_the_overflow_flag_is_used_and_is_not_an_error() {
    let mut ncp = Ncp::recording();
    ncp.overflow_flag = true;
    let mut h = harness(ncp);
    assert!(start(&mut h).unwrap().formed);
}

#[test]
fn a_random_pan_id_of_ffff_or_0000_is_drawn_again() {
    let mut values = draws();
    values.pop();
    values.push(vec![0xFF, 0xFF]);
    values.push(vec![0x00, 0x00]);
    values.push(vec![0x34, 0x12]);
    let mut h = harness(Ncp::recording());
    h.radio.identify().unwrap();
    let network = h
        .radio
        .start_network(
            &NetworkSettings::default(),
            None,
            false,
            &mut scripted(values),
            &mut |_| Ok(()),
        )
        .unwrap();
    assert_eq!(network.pan_id, 0x1234);
}

#[test]
fn an_extended_pan_id_of_all_zero_or_all_ff_is_drawn_again() {
    let mut values = draws();
    values.insert(2, vec![0x00; 8]);
    values.insert(3, vec![0xFF; 8]);
    let mut h = harness(Ncp::recording());
    h.radio.identify().unwrap();
    let network = h
        .radio
        .start_network(
            &NetworkSettings::default(),
            None,
            false,
            &mut scripted(values),
            &mut |_| Ok(()),
        )
        .unwrap();
    assert_eq!(network.extended_pan_id, [8, 7, 6, 5, 4, 3, 2, 1]);
}

#[test]
fn a_random_source_that_fails_stops_the_start_before_the_network_is_formed() {
    let mut h = harness(Ncp::recording());
    h.radio.identify().unwrap();
    let error = h
        .radio
        .start_network(
            &NetworkSettings::default(),
            None,
            false,
            &mut scripted(vec![]),
            &mut |_| Ok(()),
        )
        .unwrap_err();
    assert_eq!(error.to_string(), "no random source: script exhausted");
    assert!(!ids(&h).contains(&0x1E));
}

#[test]
fn network_init_not_joined_is_0x93_in_ezsp_13_and_0x17_in_ezsp_14_and_later_and_both_form() {
    for protocol in [13, 14, 19] {
        let mut h = harness(ncp_at(protocol));
        let network = start(&mut h).unwrap();
        assert!(network.formed, "protocol {protocol}");
        assert!(ids(&h).contains(&0x1E), "protocol {protocol}");
    }
}

#[test]
fn a_radio_with_no_network_ezsp_14_and_later_forms_one() {
    let mut h = harness(ncp_at(19));
    let network = start(&mut h).unwrap();
    assert!(network.formed);
    assert_eq!(network.pan_id, 0x1A2B);
    // The statuses are read in four bytes: the later commands worked.
    h.radio.permit_join(10).unwrap();
}

#[test]
fn a_radio_with_a_network_as_coordinator_resumes_it_without_a_form_command_or_new_keys() {
    let mut ncp = Ncp::recording();
    ncp.network = Some(StoredNetwork {
        extended_pan_wire: [9, 9, 9, 9, 9, 9, 9, 0xAA],
        pan_id: 0x4321,
        channel: 20,
        node_type: 1,
    });
    let mut h = harness(ncp);
    let network = start(&mut h).unwrap();
    assert!(!network.formed);
    assert!(network.keys.is_none());
    assert_eq!(network.pan_id, 0x4321);
    assert_eq!(network.channel, 20);
    assert_eq!(network.extended_pan_id, [0xAA, 9, 9, 9, 9, 9, 9, 9]);
    let ids = ids(&h);
    for formed_only in [0x68, 0x1E, 0xB1] {
        assert!(!ids.contains(&formed_only), "command 0x{formed_only:X}");
    }
}

#[test]
fn a_radio_joined_as_a_router_is_an_error_and_is_not_made_to_leave() {
    let mut ncp = Ncp::recording();
    ncp.network = Some(StoredNetwork {
        extended_pan_wire: [1; 8],
        pan_id: 0x4321,
        channel: 20,
        node_type: 2,
    });
    let mut h = harness(ncp);
    let error = start(&mut h).unwrap_err();
    assert_eq!(error, RadioError::NotCoordinator);
    assert_eq!(error.to_string(), "the radio is not a coordinator");
    let ids = ids(&h);
    assert!(!ids.contains(&0x20), "leaveNetwork was sent");
    assert!(!ids.contains(&0x1E), "formNetwork was sent");
}

#[test]
fn a_network_that_never_comes_up_gives_network_did_not_start_after_10_seconds() {
    let mut ncp = Ncp::recording();
    ncp.no_network_up = true;
    let mut h = harness(ncp);
    let error = start(&mut h).unwrap_err();
    assert_eq!(error, RadioError::NetworkDidNotStart);
    assert_eq!(error.to_string(), "network did not start");
    let elapsed = h.clock.get();
    assert!((10_000..11_000).contains(&elapsed), "elapsed {elapsed} ms");
}

#[test]
fn a_start_without_identifying_the_radio_first_is_an_error() {
    let mut h = harness(Ncp::recording());
    let error = h
        .radio
        .start_network(
            &NetworkSettings::default(),
            None,
            false,
            &mut scripted(draws()),
            &mut |_| Ok(()),
        )
        .unwrap_err();
    assert_eq!(error, RadioError::NotIdentified);
}

#[test]
fn permit_join_180_imports_the_well_known_key_allows_joins_then_opens_in_that_order() {
    for (protocol, key_params) in [(13u8, 25usize), (19, 24)] {
        let mut h = started(ncp_at(protocol));
        let before = h.ncp.borrow().commands.len();
        h.radio.permit_join(180).unwrap();
        let commands = h.ncp.borrow().commands[before..].to_vec();
        let ids: Vec<u16> = commands.iter().map(|c| c.0).collect();
        assert_eq!(ids, vec![0x111, 0x55, 0x22], "protocol {protocol}");
        let key = &commands[0].1;
        assert_eq!(key.len(), key_params, "protocol {protocol}");
        assert_eq!(&key[..8], &[0xFF; 8]);
        assert_eq!(&key[8..24], b"ZigBeeAlliance09");
        assert_eq!(commands[1].1, vec![0, 0x03]);
        assert_eq!(commands[2].1, vec![180]);
    }
}

#[test]
fn permit_join_0_and_255_are_refused_and_nothing_is_sent() {
    let mut h = started(Ncp::recording());
    let before = h.ncp.borrow().commands.len();
    for seconds in [0u8, 255] {
        assert!(matches!(
            h.radio.permit_join(seconds),
            Err(RadioError::InvalidArgument(_))
        ));
    }
    assert_eq!(h.ncp.borrow().commands.len(), before);
}

#[test]
fn closing_sends_permit_joining_0_clears_the_transient_keys_and_goes_back_to_rejoins_only() {
    let mut h = started(Ncp::recording());
    h.radio.permit_join(30).unwrap();
    let before = h.ncp.borrow().commands.len();
    h.radio.close_joining().unwrap();
    let commands = h.ncp.borrow().commands[before..].to_vec();
    assert_eq!(
        commands,
        vec![(0x22, vec![0]), (0x6B, vec![]), (0x55, vec![0, 0x02]),]
    );
}

#[test]
fn a_join_then_the_announce_of_the_same_device_gives_one_joined_event() {
    let mut device = SimDevice::sensor();
    device.announce_twice = true;
    let mut h = started(with_device(device));
    h.radio.permit_join(30).unwrap();
    let events = poll_until_idle(&mut h);
    assert_eq!(joined_count(&events), 1, "{events:?}");
}

#[test]
fn a_device_that_announces_and_answers_the_endpoint_request_and_the_basic_read_is_identified() {
    for protocol in [13, 19] {
        let mut h = started(with_device_at(protocol, SimDevice::sensor()));
        h.radio.permit_join(30).unwrap();
        let events = poll_until_idle(&mut h);
        let without_measures: Vec<PairingEvent> = events
            .iter()
            .filter(|e| !matches!(e, PairingEvent::Measures(_)))
            .cloned()
            .collect();
        assert_eq!(
            without_measures,
            vec![
                PairingEvent::Joined {
                    eui64: DEVICE_EUI,
                    nwk: DEVICE_NWK
                },
                PairingEvent::Identified {
                    eui64: DEVICE_EUI,
                    manufacturer: some("eWeLink"),
                    model: some("TH01"),
                },
                PairingEvent::ReportingConfigured {
                    eui64: DEVICE_EUI,
                    ok: vec![0x0402, 0x0405, 0x0001],
                    failed: vec![],
                    no_descriptor: false,
                    enrollment: None,
                },
            ],
            "protocol {protocol}"
        );
        // Endpoint request, then the read on the endpoint the device listed.
        let sent = identification_sends(&h);
        assert_eq!(sent.len(), 2);
        assert_eq!((sent[0].profile, sent[0].cluster), (0x0000, 0x0005));
        assert_eq!((sent[1].profile, sent[1].cluster), (0x0104, 0x0000));
        assert_eq!(sent[1].source_endpoint, 1);
        assert_eq!(&sent[1].message[2..], &[0x00, 0x04, 0x00, 0x05, 0x00]);
    }
}

fn with_device_at(protocol: u8, device: SimDevice) -> Ncp {
    let mut ncp = ncp_at(protocol);
    ncp.devices = vec![device];
    ncp
}

#[test]
fn a_trust_center_join_without_announce_starts_the_identification_5_seconds_later() {
    let mut device = SimDevice::sensor();
    device.announce = false;
    let mut h = started(with_device(device));
    h.radio.permit_join(30).unwrap();
    let opened = h.clock.get();
    let mut events = Vec::new();
    while h.ncp.borrow().sent.is_empty() {
        events.extend(h.radio.poll_pairing().unwrap());
        assert!(h.clock.get() < opened + 10_000, "nothing was sent");
    }
    let waited = h.clock.get() - opened;
    assert!((5_000..5_300).contains(&waited), "waited {waited} ms");
    events.extend(poll_until_idle(&mut h));
    assert_eq!(identified(&events), vec![(some("eWeLink"), some("TH01"))]);
}

#[test]
fn an_answer_from_another_device_or_with_another_tsn_or_a_default_response_is_ignored() {
    let mut device = SimDevice::sensor();
    device.decoys = true;
    device.endpoints = Some(vec![2]);
    let mut h = started(with_device(device));
    h.radio.permit_join(30).unwrap();
    let events = poll_until_idle(&mut h);
    assert_eq!(identified(&events), vec![(some("eWeLink"), some("TH01"))]);
    // The read went to endpoint 2, from the right device's list, not endpoint 9.
    let sent = identification_sends(&h);
    assert_eq!(sent.len(), 2);
    assert_eq!(sent[1].destination_endpoint, 2);
}

#[test]
fn strings_with_trailing_zero_bytes_are_trimmed_and_non_utf8_is_read_lossily() {
    let mut device = SimDevice::sensor();
    device.manufacturer = Attr::Text(b"eWeLink\0\0\0".to_vec());
    device.model = Attr::Text(vec![b'T', 0xFF, b'1', 0x00]);
    let mut h = started(with_device(device));
    h.radio.permit_join(30).unwrap();
    let events = poll_until_idle(&mut h);
    assert_eq!(
        identified(&events),
        vec![(some("eWeLink"), some("T\u{FFFD}1"))]
    );
}

#[test]
fn an_unsupported_attribute_is_unknown_and_the_other_is_read() {
    let mut device = SimDevice::sensor();
    device.manufacturer = Attr::Unsupported;
    let mut h = started(with_device(device));
    h.radio.permit_join(30).unwrap();
    let events = poll_until_idle(&mut h);
    assert_eq!(identified(&events), vec![(None, some("TH01"))]);
}

#[test]
fn a_device_joining_one_second_before_the_window_ends_is_identified_after_the_window_closed() {
    let mut device = SimDevice::sensor();
    device.join_delay_ms = 4_000;
    let mut h = started(with_device(device));
    h.radio.permit_join(5).unwrap();
    let opened = h.clock.get();
    let mut events = Vec::new();
    while h.clock.get() < opened + 5_000 {
        events.extend(h.radio.poll_pairing().unwrap());
    }
    assert_eq!(joined_count(&events), 1, "the device joined in the window");
    h.radio.close_joining().unwrap();
    events.extend(poll_until_idle(&mut h));
    assert_eq!(identified(&events), vec![(some("eWeLink"), some("TH01"))]);
    assert!(!h.radio.pairing_busy());
}

#[test]
fn a_device_silent_on_the_endpoint_request_is_read_on_endpoint_1_after_15_seconds() {
    let mut device = SimDevice::sensor();
    device.endpoints = None;
    let mut h = started(with_device(device));
    h.radio.permit_join(30).unwrap();
    let opened = h.clock.get();
    let events = poll_until_idle(&mut h);
    assert_eq!(identified(&events), vec![(some("eWeLink"), some("TH01"))]);
    let sent = identification_sends(&h);
    assert_eq!(sent.len(), 2);
    assert_eq!(sent[1].cluster, 0x0000);
    assert_eq!(sent[1].destination_endpoint, 1);
    let waited = h.clock.get() - opened;
    assert!((15_000..16_000).contains(&waited), "waited {waited} ms");
}

#[test]
fn a_device_that_gives_a_failure_status_to_the_endpoint_request_is_read_on_endpoint_1() {
    let mut device = SimDevice::sensor();
    device.endpoint_status = 0x80;
    let mut h = started(with_device(device));
    h.radio.permit_join(30).unwrap();
    let events = poll_until_idle(&mut h);
    assert_eq!(identified(&events), vec![(some("eWeLink"), some("TH01"))]);
    assert_eq!(h.ncp.borrow().sent[1].destination_endpoint, 1);
}

#[test]
fn a_device_silent_on_the_basic_read_is_asked_twice_then_identified_with_both_unknown() {
    let mut device = SimDevice::sensor();
    device.ignore_reads = u32::MAX;
    let mut h = started(with_device(device));
    h.radio.permit_join(30).unwrap();
    let events = poll_until_idle(&mut h);
    assert_eq!(identified(&events), vec![(None, None)]);
    let reads = h
        .ncp
        .borrow()
        .sent
        .iter()
        .filter(|m| m.cluster == 0x0000)
        .count();
    assert_eq!(reads, 2, "one read and one retry");
}

#[test]
fn a_device_that_answers_the_retry_is_identified() {
    let mut device = SimDevice::sensor();
    device.ignore_reads = 1;
    let mut h = started(with_device(device));
    h.radio.permit_join(30).unwrap();
    let events = poll_until_idle(&mut h);
    assert_eq!(identified(&events), vec![(some("eWeLink"), some("TH01"))]);
}

#[test]
fn a_trust_center_join_denied_or_a_device_that_left_gives_no_event() {
    for (status, decision) in [(1u8, 2u8), (2, 0)] {
        let mut device = SimDevice::sensor();
        device.announce = false;
        device.join_status = status;
        device.join_decision = decision;
        let mut h = started(with_device(device));
        h.radio.permit_join(30).unwrap();
        let events = poll_for(&mut h, 6_000);
        assert!(events.is_empty(), "status {status}: {events:?}");
        assert!(h.ncp.borrow().sent.is_empty());
    }
}

#[test]
fn a_secure_rejoin_is_not_a_new_join_and_an_unsecured_rejoin_is() {
    let mut device = SimDevice::sensor();
    device.announce = false;
    device.join_status = 0; // secured rejoin
    let mut h = started(with_device(device.clone()));
    h.radio.permit_join(30).unwrap();
    assert!(poll_for(&mut h, 500).is_empty());

    device.join_status = 3; // unsecured rejoin
    let mut h = started(with_device(device));
    h.radio.permit_join(30).unwrap();
    assert_eq!(joined_count(&poll_for(&mut h, 500)), 1);
}

#[test]
fn send_unicast_busy_twice_then_ok_identifies_and_busy_three_times_gives_not_identified() {
    // Busy twice: the third try goes through, 500 ms apart.
    let mut ncp = with_device(SimDevice::sensor());
    ncp.busy_sends = 2;
    let mut h = started(ncp);
    h.radio.permit_join(30).unwrap();
    let opened = h.clock.get();
    let events = poll_until_idle(&mut h);
    assert_eq!(identified(&events), vec![(some("eWeLink"), some("TH01"))]);
    assert_eq!(
        identification_sends(&h).len(),
        4,
        "3 endpoint requests and 1 read"
    );
    assert!(h.clock.get() - opened >= 1_000);

    // Busy three times: the identification fails and the session goes on.
    let mut ncp = with_device(SimDevice::sensor());
    ncp.busy_sends = 3;
    let mut h = started(ncp);
    h.radio.permit_join(30).unwrap();
    let events = poll_until_idle(&mut h);
    assert!(
        events.contains(&PairingEvent::NotIdentified { eui64: DEVICE_EUI }),
        "{events:?}"
    );
    assert!(poll_for(&mut h, 1_000).is_empty(), "the session goes on");
}

#[test]
fn the_33rd_device_is_reported_and_ignored() {
    let mut device = SimDevice::sensor();
    device.announce = false;
    let mut ncp = with_device(device);
    ncp.crowd = 32; // with the main device: 33 joins
    let mut h = started(ncp);
    h.radio.permit_join(30).unwrap();
    let events = poll_for(&mut h, 3_000);
    assert_eq!(joined_count(&events), 32);
    let too_many: Vec<_> = events
        .iter()
        .filter(|e| matches!(e, PairingEvent::TooManyDevices { .. }))
        .collect();
    assert_eq!(too_many.len(), 1, "{events:?}");
    assert!(
        too_many[0]
            .line()
            .starts_with("pairing: too many devices, 0x")
    );
}

#[test]
fn a_callback_between_a_command_and_its_response_is_not_lost() {
    let mut ncp = with_device(SimDevice::sensor());
    ncp.join_before_response = true;
    let mut h = started(ncp);
    // The join and the announce arrive before the response of permitJoining.
    h.radio.permit_join(30).unwrap();
    let events = poll_until_idle(&mut h);
    assert_eq!(joined_count(&events), 1);
    assert_eq!(identified(&events).len(), 1);
}

#[test]
fn the_queue_keeps_the_16_newest_callbacks_and_drops_the_oldest_without_failing() {
    let mut device = SimDevice::sensor();
    device.announce = false;
    let mut ncp = with_device(device);
    ncp.crowd = 20; // 21 joins arrive before the response
    ncp.crowd_spread_ms = 0;
    ncp.join_before_response = true;
    let mut h = started(ncp);
    h.radio.permit_join(30).unwrap();
    let events = poll_for(&mut h, 500);
    assert_eq!(joined_count(&events), 16, "{events:?}");
    // The main device's join was the oldest: it is the one dropped.
    assert!(!events.contains(&PairingEvent::Joined {
        eui64: DEVICE_EUI,
        nwk: DEVICE_NWK
    }));
}

#[test]
fn the_network_line_and_the_event_lines_are_those_of_the_spec() {
    use solite_core::radio::network_line;
    let network = Network {
        pan_id: 0x1A2B,
        extended_pan_id: [0xDD; 8],
        channel: 15,
        formed: true,
        from_backup: false,
        keys: None,
    };
    assert_eq!(
        network_line(&network, false),
        "network: formed, channel 15, PAN 0x1A2B, extended PAN 0xDDDDDDDDDDDDDDDD"
    );
    let restored_network = Network {
        from_backup: true,
        ..network.clone()
    };
    assert!(
        network_line(&restored_network, false)
            .starts_with("network: formed (restored from the backup), channel 15")
    );
    let resumed = Network {
        formed: false,
        ..network
    };
    assert!(network_line(&resumed, false).starts_with("network: resumed, channel 15"));
    assert!(
        network_line(&resumed, true).starts_with("network: resumed (made by another program), ")
    );
    assert_eq!(
        PairingEvent::Joined {
            eui64: DEVICE_EUI,
            nwk: DEVICE_NWK
        }
        .line(),
        "joined: 0x00124B0012345678 (0x5C3D)"
    );
    assert_eq!(
        PairingEvent::Identified {
            eui64: DEVICE_EUI,
            manufacturer: some("eWeLink"),
            model: None
        }
        .line(),
        "device: 0x00124B0012345678 eWeLink unknown"
    );
    assert_eq!(
        PairingEvent::NotIdentified { eui64: DEVICE_EUI }.line(),
        "device: 0x00124B0012345678 not identified (radio busy)"
    );
}

#[test]
fn on_formed_is_called_with_the_network_and_keys_right_after_the_form_before_later_steps() {
    let mut h = harness(Ncp::recording());
    h.radio.identify().unwrap();
    let mut seen = Vec::new();
    let ncp = std::rc::Rc::clone(&h.ncp);
    let network = h
        .radio
        .start_network(
            &NetworkSettings::default(),
            None,
            false,
            &mut scripted(draws()),
            &mut |n| {
                // At that moment the last command the radio got is formNetwork.
                seen.push((n.clone(), ncp.borrow().commands.last().unwrap().0));
                Ok(())
            },
        )
        .unwrap();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].1, 0x1E);
    assert_eq!(seen[0].0, network);
    assert_eq!(seen[0].0.keys.as_ref().unwrap().network_key, [0x11; 16]);
}

#[test]
fn a_step_after_the_form_that_fails_still_leaves_the_keys_to_the_caller() {
    let mut ncp = Ncp::recording();
    ncp.refuse_concentrator = true;
    let mut h = harness(ncp);
    h.radio.identify().unwrap();
    let mut keys = None;
    let error = h
        .radio
        .start_network(
            &NetworkSettings::default(),
            None,
            false,
            &mut scripted(draws()),
            &mut |n| {
                keys = n.keys.clone();
                Ok(())
            },
        )
        .unwrap_err();
    assert!(matches!(
        error,
        RadioError::Refused {
            command: "setConcentrator",
            ..
        }
    ));
    assert_eq!(keys.unwrap().trust_center_key, [0x22; 16]);
}

#[test]
fn an_on_formed_that_fails_ends_the_start_with_its_message_before_other_steps() {
    let mut h = harness(Ncp::recording());
    h.radio.identify().unwrap();
    let error = h
        .radio
        .start_network(
            &NetworkSettings::default(),
            None,
            false,
            &mut scripted(draws()),
            &mut |_| Err("cannot write /x".to_string()),
        )
        .unwrap_err();
    assert_eq!(error.to_string(), "cannot write /x");
    assert!(!ids(&h).contains(&0x28));
}

#[test]
fn a_late_response_with_an_older_sequence_number_is_ignored_not_an_error() {
    let mut ncp = Ncp::recording();
    ncp.stale_response_first = true;
    let mut h = harness(ncp);
    assert!(start(&mut h).unwrap().formed);
}

// Reporting setup and measures (spec 006).

use solite_core::radio::measures::{Measure, Value};
use solite_core::radio::{MeasureEvent, Sender};
use solite_sim::{ConfigureAnswer, report_frame, temperature_report};

const TEMPERATURE: u16 = 0x0402;
const HUMIDITY: u16 = 0x0405;
const POWER: u16 = 0x0001;

/// The `ReportingConfigured` events, as (ok, failed, no_descriptor).
fn reporting(events: &[PairingEvent]) -> Vec<(Vec<u16>, Vec<u16>, bool)> {
    events
        .iter()
        .filter_map(|e| match e {
            PairingEvent::ReportingConfigured {
                ok,
                failed,
                no_descriptor,
                ..
            } => Some((ok.clone(), failed.clone(), *no_descriptor)),
            _ => None,
        })
        .collect()
}

fn measure_events(events: &[PairingEvent]) -> Vec<MeasureEvent> {
    events
        .iter()
        .filter_map(|e| match e {
            PairingEvent::Measures(m) => Some(m.clone()),
            _ => None,
        })
        .collect()
}

/// The ZDO or ZCL requests of one kind the host sent: (profile, cluster) and the messages.
fn sends_on(h: &Harness, profile: u16, cluster: u16) -> Vec<SentMessage> {
    h.ncp
        .borrow()
        .sent
        .iter()
        .filter(|m| m.profile == profile && m.cluster == cluster)
        .cloned()
        .collect()
}

/// The clusters the host bound, in order.
fn bound_clusters(h: &Harness) -> Vec<u16> {
    sends_on(h, 0x0000, 0x0021)
        .iter()
        .map(|m| u16::from_le_bytes([m.message[10], m.message[11]]))
        .collect()
}

/// Pairs the device on a radio and polls until the identification and setup are over.
fn paired(device: SimDevice) -> (Harness, Vec<PairingEvent>) {
    let mut h = started(with_device(device));
    h.radio.permit_join(180).unwrap();
    let events = poll_until_idle(&mut h);
    (h, events)
}

fn temperature(value: f64) -> Measure {
    Measure {
        kind: "temperature",
        value: Value::Number(value),
        unit: "°C",
        device_value: None,
    }
}

#[test]
fn a_paired_fake_sensor_has_bind_and_configure_reporting_done_for_the_three_clusters() {
    let (h, events) = paired(SimDevice::sensor());
    assert_eq!(
        reporting(&events),
        vec![(vec![TEMPERATURE, HUMIDITY, POWER], vec![], false)]
    );
    assert_eq!(bound_clusters(&h), vec![TEMPERATURE, HUMIDITY, POWER]);
    for cluster in [POWER, TEMPERATURE, HUMIDITY] {
        let configures = sends_on(&h, 0x0104, cluster);
        assert_eq!(configures.len(), 1, "cluster {cluster:#06X}");
        assert_eq!(configures[0].message[2], 0x06);
        assert_eq!(configures[0].destination_endpoint, 1);
    }
    // The bind names the device, its endpoint, the cluster and the coordinator's own address.
    let bind = &sends_on(&h, 0x0000, 0x0021)[0].message;
    assert_eq!(
        &bind[1..9],
        &[0x78, 0x56, 0x34, 0x12, 0x00, 0x4B, 0x12, 0x00]
    );
    assert_eq!(bind[9], 1);
    assert_eq!(
        &bind[13..21],
        &[0x56, 0x34, 0x12, 0xFE, 0xFF, 0xB9, 0x6F, 0x8C]
    );
    assert_eq!(bind[21], 1);
    assert!(!h.radio.pairing_busy());
}

#[test]
fn a_descriptor_unanswered_twice_gives_failed_no_descriptor_after_two_requests() {
    let mut device = SimDevice::sensor();
    device.ignore_descriptors = u32::MAX;
    let (h, events) = paired(device);
    assert_eq!(reporting(&events), vec![(vec![], vec![], true)]);
    assert_eq!(sends_on(&h, 0x0000, 0x0004).len(), 2);
    assert!(bound_clusters(&h).is_empty());
    assert_eq!(
        events.last().unwrap().line(),
        "reporting: 0x00124B0012345678 failed: no descriptor"
    );
}

#[test]
fn a_bind_refused_for_humidity_fails_that_cluster_and_the_others_go_on() {
    let mut device = SimDevice::sensor();
    device.bind_refused = vec![HUMIDITY];
    let (h, events) = paired(device);
    assert_eq!(
        reporting(&events),
        vec![(vec![TEMPERATURE, POWER], vec![HUMIDITY], false)]
    );
    // No Configure Reporting for the refused cluster.
    assert!(sends_on(&h, 0x0104, HUMIDITY).is_empty());
    assert_eq!(sends_on(&h, 0x0104, TEMPERATURE).len(), 1);
    assert_eq!(sends_on(&h, 0x0104, POWER).len(), 1);
}

#[test]
fn a_configure_reporting_unanswered_after_its_retry_stops_the_setup_and_fails_the_rest() {
    let mut device = SimDevice::sensor();
    device.configure = ConfigureAnswer::Silent;
    let (h, events) = paired(device);
    assert_eq!(
        reporting(&events),
        vec![(vec![], vec![TEMPERATURE, HUMIDITY, POWER], false)]
    );
    // Two Configure Reporting for the first cluster, and nothing for the others.
    assert_eq!(sends_on(&h, 0x0104, TEMPERATURE).len(), 2);
    assert_eq!(bound_clusters(&h), vec![TEMPERATURE]);
    assert!(!h.radio.pairing_busy());
}

#[test]
fn failed_clusters_are_tried_again_once_when_the_device_next_sends_a_message() {
    let mut device = SimDevice::sensor();
    device.ignore_configures = 2; // the first request and its retry are not answered
    let (mut h, events) = paired(device);
    assert_eq!(
        reporting(&events),
        vec![(vec![], vec![TEMPERATURE, HUMIDITY, POWER], false)]
    );
    let before = bound_clusters(&h).len();

    // Any message from the device: it is awake.
    h.ncp.borrow_mut().inject(
        DEVICE_NWK,
        0x0104,
        TEMPERATURE,
        1,
        &temperature_report(0x50, 2000),
    );
    let events = poll_until_idle(&mut h);
    assert_eq!(
        reporting(&events),
        vec![(vec![TEMPERATURE, HUMIDITY, POWER], vec![], false)]
    );
    assert_eq!(bound_clusters(&h).len(), before + 3);

    // Once only: a later message starts nothing.
    let sent = h.ncp.borrow().sent.len();
    h.ncp.borrow_mut().inject(
        DEVICE_NWK,
        0x0104,
        TEMPERATURE,
        1,
        &temperature_report(0x51, 2000),
    );
    let events = poll_for(&mut h, 500);
    assert!(reporting(&events).is_empty());
    assert_eq!(h.ncp.borrow().sent.len(), sent);
}

#[test]
fn a_report_between_the_bind_and_the_configure_response_is_shown_and_the_response_still_matched() {
    let mut device = SimDevice::sensor();
    device.reports_during_setup = vec![(TEMPERATURE, temperature_report(0x60, 2100))];
    device.reports = Vec::new();
    let (_h, events) = paired(device);
    assert_eq!(
        reporting(&events),
        vec![(vec![TEMPERATURE, HUMIDITY, POWER], vec![], false)]
    );
    let shown = measure_events(&events);
    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0].measures, vec![temperature(21.0)]);
    // It arrived while the Configure Reporting was awaited, not after the setup.
    let at = |f: fn(&PairingEvent) -> bool| events.iter().position(f).unwrap();
    assert!(
        at(|e| matches!(e, PairingEvent::Measures(_)))
            < at(|e| matches!(e, PairingEvent::ReportingConfigured { .. }))
    );
}

#[test]
fn an_eui64_of_all_zeros_in_the_packet_info_that_the_radio_does_not_know_shows_the_nwk() {
    let mut h = started(ncp_at(19));
    h.ncp
        .borrow_mut()
        .inject(0x1234, 0x0104, TEMPERATURE, 1, &temperature_report(1, 1952));
    let events = poll_for(&mut h, 500);
    let shown = measure_events(&events);
    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0].sender, Sender::Nwk(0x1234));
    assert_eq!(shown[0].lines(), vec!["0x1234 temperature 19.52 °C"]);
}

#[test]
fn a_frame_giving_more_than_16_measures_keeps_16() {
    let mut h = started(Ncp::recording());
    let records: Vec<(u16, u8, Vec<u8>)> = (0..20)
        .map(|i| (0x0000, 0x29, (1000i16 + i).to_le_bytes().to_vec()))
        .collect();
    h.ncp
        .borrow_mut()
        .inject(0x1234, 0x0104, TEMPERATURE, 1, &report_frame(1, &records));
    let shown = measure_events(&poll_for(&mut h, 500));
    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0].measures.len(), 16);
    assert_eq!(shown[0].measures[0], temperature(10.0));
}

#[test]
fn reports_from_the_paired_device_give_measure_events_with_its_eui64() {
    let (_h, events) = paired(SimDevice::sensor());
    let shown = measure_events(&events);
    let lines: Vec<String> = shown.iter().flat_map(MeasureEvent::lines).collect();
    assert_eq!(
        lines,
        vec![
            "0x00124B0012345678 temperature 19.52 °C",
            "0x00124B0012345678 humidity 54.1 %",
            "0x00124B0012345678 battery 100 %",
        ]
    );
    assert!(
        shown
            .iter()
            .all(|m| m.sender == Sender::Eui64(DEVICE_EUI) && m.endpoint == 1)
    );
}

#[test]
fn a_report_from_an_unknown_nwk_shows_the_eui64_the_radio_knows_or_the_nwk_when_it_does_not() {
    for protocol in [13, 19] {
        let mut ncp = ncp_at(protocol);
        ncp.known_addresses = vec![(0x1111, [0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08])];
        let mut h = started(ncp);
        for nwk in [0x1111u16, 0x2222] {
            h.ncp
                .borrow_mut()
                .inject(nwk, 0x0104, TEMPERATURE, 1, &temperature_report(1, 1952));
        }
        let shown = measure_events(&poll_for(&mut h, 500));
        assert_eq!(shown.len(), 2, "protocol {protocol}");
        assert_eq!(
            shown[0].sender,
            Sender::Eui64([0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01]),
            "protocol {protocol}"
        );
        assert_eq!(shown[1].sender, Sender::Nwk(0x2222), "protocol {protocol}");
    }
    // The sender's address in the packet is used without asking the radio.
    let mut ncp = ncp_at(19);
    ncp.packet_eui_wire = Some([0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F, 0x10, 0x11]);
    let mut h = started(ncp);
    h.ncp
        .borrow_mut()
        .inject(0x3333, 0x0104, TEMPERATURE, 1, &temperature_report(1, 1952));
    let shown = measure_events(&poll_for(&mut h, 500));
    assert_eq!(
        shown[0].sender,
        Sender::Eui64([0x11, 0x10, 0x0F, 0x0E, 0x0D, 0x0C, 0x0B, 0x0A])
    );
    assert!(!ids(&h).contains(&0x61));
}

#[test]
fn the_address_cache_holds_64_entries_and_drops_the_oldest() {
    let mut ncp = Ncp::recording();
    ncp.known_addresses = (0..65u16)
        .map(|i| (0x4000 + i, [i as u8 + 1, 0, 0, 0, 0, 0, 0, 0]))
        .collect();
    let mut h = started(ncp);
    let lookups = |h: &Harness| ids(h).iter().filter(|id| **id == 0x61).count();
    let report = |h: &mut Harness, nwk: u16| {
        h.ncp
            .borrow_mut()
            .inject(nwk, 0x0104, TEMPERATURE, 1, &temperature_report(1, 1952));
        poll_for(h, 100);
    };
    for i in 0..65u16 {
        report(&mut h, 0x4000 + i);
    }
    assert_eq!(lookups(&h), 65);
    // The newest is remembered: no new lookup. The oldest was dropped: one more.
    report(&mut h, 0x4000 + 64);
    assert_eq!(lookups(&h), 65);
    report(&mut h, 0x4000);
    assert_eq!(lookups(&h), 66);
    // The second oldest was dropped by that one.
    report(&mut h, 0x4001);
    assert_eq!(lookups(&h), 67);
}

#[test]
fn listen_on_a_radio_with_no_network_is_the_error_no_network_and_forms_nothing() {
    let mut h = harness(Ncp::recording());
    h.radio.identify().unwrap();
    let error = h.radio.listen().unwrap_err();
    assert_eq!(error, RadioError::NoNetwork);
    assert_eq!(error.to_string(), "no network, pair a device first");
    assert!(!ids(&h).contains(&0x1E));
}

#[test]
fn listen_resumes_the_network_and_never_forms_one() {
    let mut ncp = Ncp::recording();
    ncp.network = Some(StoredNetwork {
        extended_pan_wire: [1, 2, 3, 4, 5, 6, 7, 8],
        pan_id: 0x4321,
        channel: 20,
        node_type: 1,
    });
    let mut h = harness(ncp);
    h.radio.identify().unwrap();
    let network = h.radio.listen().unwrap();
    assert!(!network.formed);
    assert_eq!((network.pan_id, network.channel), (0x4321, 20));
    // No form, no security state, no permit joining.
    let ids = ids(&h);
    assert!(!ids.contains(&0x1E) && !ids.contains(&0x68) && !ids.contains(&0x22));
    assert!(ids.contains(&0x17));
    // And it reads measures from then on.
    h.ncp
        .borrow_mut()
        .inject(0x1234, 0x0104, TEMPERATURE, 1, &temperature_report(1, 1952));
    let end = h.clock.get() + 500;
    let mut shown = Vec::new();
    while h.clock.get() < end {
        shown.extend(h.radio.poll().unwrap());
    }
    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0].measures, vec![temperature(19.52)]);
}

#[test]
fn a_configure_reporting_response_with_another_tsn_is_not_the_awaited_answer() {
    let mut device = SimDevice::sensor();
    device.configure = ConfigureAnswer::Silent;
    let mut h = started(with_device(device));
    h.radio.permit_join(180).unwrap();
    while sends_on(&h, 0x0104, TEMPERATURE).is_empty() {
        h.radio.poll_pairing().unwrap();
    }
    let tsn = sends_on(&h, 0x0104, TEMPERATURE)[0].message[1];
    h.ncp.borrow_mut().inject(
        DEVICE_NWK,
        0x0104,
        TEMPERATURE,
        1,
        &[0x18, tsn ^ 0x55, 0x07, 0x00],
    );
    let events = poll_for(&mut h, 500);
    assert!(reporting(&events).is_empty());
    assert_eq!(bound_clusters(&h), vec![TEMPERATURE], "the setup went on");
}

/// Pairs the sensor with the given answer to Configure Reporting for one cluster and returns
/// the first result of the setup. The device's first reports wake the retry of what failed: it
/// is tried once more (and fails again, the device answers the same), exactly when a cluster
/// failed.
fn configure_answered(cluster: u16, answer: ConfigureAnswer) -> (Vec<u16>, Vec<u16>, bool) {
    let mut device = SimDevice::sensor();
    device.configure_overrides = vec![(cluster, answer)];
    let (_h, events) = paired(device);
    let results = reporting(&events);
    let (ok, failed, no_descriptor) = results[0].clone();
    if failed.is_empty() {
        assert_eq!(results.len(), 1, "no retry when nothing failed");
    } else {
        assert_eq!(results.len(), 2, "one retry: {results:?}");
        assert_eq!(results[1], (vec![], failed.clone(), false));
    }
    (ok, failed, no_descriptor)
}

#[test]
fn a_configure_reporting_answered_by_a_default_response_86_fails_the_cluster_and_arms_the_retry() {
    let (ok, failed, _) = configure_answered(HUMIDITY, ConfigureAnswer::Default(0x86));
    assert_eq!((ok, failed), (vec![TEMPERATURE, POWER], vec![HUMIDITY]));
}

#[test]
fn a_refusal_record_for_the_only_attribute_fails_the_cluster() {
    let (ok, failed, _) = configure_answered(TEMPERATURE, ConfigureAnswer::Refuse(0x86));
    assert_eq!((ok, failed), (vec![HUMIDITY, POWER], vec![TEMPERATURE]));
}

#[test]
fn power_with_the_battery_accepted_and_the_voltage_refused_is_set_up() {
    let answer = ConfigureAnswer::RefuseAttribute(0x0020, 0x86);
    let (ok, failed, _) = configure_answered(POWER, answer);
    assert_eq!((ok, failed), (vec![TEMPERATURE, HUMIDITY, POWER], vec![]));
}

#[test]
fn power_answered_by_a_single_failure_byte_86_fails() {
    let (ok, failed, _) = configure_answered(POWER, ConfigureAnswer::SingleStatus(0x86));
    assert_eq!((ok, failed), (vec![TEMPERATURE, HUMIDITY], vec![POWER]));
}

#[test]
fn a_manufacturer_specific_report_on_the_on_off_cluster_gives_no_measure() {
    let mut h = started(Ncp::recording());
    // Global report from the server, manufacturer code 0x1234, on/off attribute 0 = on.
    let frame = [0x1C, 0x34, 0x12, 0x01, 0x0A, 0x00, 0x00, 0x10, 0x01];
    h.ncp.borrow_mut().inject(0x1234, 0x0104, 0x0006, 1, &frame);
    // The same without the code gives a measure: it is the code that matters.
    let plain = [0x18, 0x02, 0x0A, 0x00, 0x00, 0x10, 0x01];
    h.ncp.borrow_mut().inject(0x1234, 0x0104, 0x0006, 1, &plain);
    let shown = measure_events(&poll_for(&mut h, 500));
    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0].lines(), vec!["0x1234 light_state on"]);
}

#[test]
fn a_default_response_for_another_command_is_not_the_answer_to_the_configure_reporting() {
    let mut device = SimDevice::sensor();
    device.configure = ConfigureAnswer::Silent;
    let mut h = started(with_device(device));
    h.radio.permit_join(180).unwrap();
    while sends_on(&h, 0x0104, TEMPERATURE).is_empty() {
        h.radio.poll_pairing().unwrap();
    }
    let tsn = sends_on(&h, 0x0104, TEMPERATURE)[0].message[1];
    // The right transaction number, but it answers command 0x00 (Read Attributes), not 0x06.
    h.ncp.borrow_mut().inject(
        DEVICE_NWK,
        0x0104,
        TEMPERATURE,
        1,
        &[0x18, tsn, 0x0B, 0x00, 0x86],
    );
    let events = poll_for(&mut h, 500);
    assert!(reporting(&events).is_empty());
    assert_eq!(bound_clusters(&h), vec![TEMPERATURE], "the setup went on");
}

#[test]
fn a_frame_without_measures_costs_no_lookup_and_still_gets_its_default_response() {
    use solite_core::radio::{Activity, Sender};
    const UNKNOWN_CLUSTER: u16 = 0x0B05; // Diagnostics: 0x0B04 gives measures since spec 048
    // A report on a cluster that gives no measure, asking for a Default Response.
    let unknown_report =
        |tsn: u8| solite_sim::report_frame_with_control(0x08, tsn, &[(0x0000, 0x29, vec![1, 0])]);
    let mut h = started(Ncp::recording());
    h.ncp
        .borrow_mut()
        .inject(0x5555, 0x0104, UNKNOWN_CLUSTER, 1, &unknown_report(1));
    poll_for(&mut h, 500);
    assert!(
        !ids(&h).contains(&0x61),
        "no lookup for a frame without measures"
    );
    assert_eq!(
        sends_on(&h, 0x0104, UNKNOWN_CLUSTER).len(),
        1,
        "the Default Response"
    );
    assert!(
        h.radio.take_activity().is_empty(),
        "sender unknown: no activity"
    );
    // With the address in the packet, the activity is kept, still without a lookup.
    let mut ncp = ncp_at(19);
    ncp.packet_eui_wire = Some([0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F, 0x10, 0x11]);
    let mut h = started(ncp);
    h.ncp
        .borrow_mut()
        .inject(0x3333, 0x0104, UNKNOWN_CLUSTER, 1, &unknown_report(2));
    poll_for(&mut h, 500);
    assert!(!ids(&h).contains(&0x61));
    assert_eq!(
        h.radio.take_activity(),
        vec![Activity::Message {
            sender: Sender::Eui64([0x11, 0x10, 0x0F, 0x0E, 0x0D, 0x0C, 0x0B, 0x0A])
        }]
    );
}

// ---- Spec 009: enrollment of IAS zone devices ----

use solite_core::radio::{Enrollment, EnrollmentOutcome};
use solite_sim::{CieAnswer, Kind};

const IAS: u16 = 0x0500;
const OWN_WIRE: [u8; 8] = [0x56, 0x34, 0x12, 0xFE, 0xFF, 0xB9, 0x6F, 0x8C];
const BIND_CLUSTER: u16 = 0x0021;

/// A motion sensor like the SNZB-03: an IAS zone and a battery, always awake.
fn motion() -> SimDevice {
    let mut device = SimDevice::of_kind(
        Kind::Motion,
        [0x78, 0x56, 0x34, 0x12, 0x00, 0x4B, 0x12, 0x00],
        None,
        None,
    );
    device.nwk = DEVICE_NWK;
    device.auto_join = true;
    device.sleepy_ms = None;
    device
}

fn enrollments(events: &[PairingEvent]) -> Vec<Option<Enrollment>> {
    events
        .iter()
        .filter_map(|e| match e {
            PairingEvent::ReportingConfigured { enrollment, .. } => Some(*enrollment),
            _ => None,
        })
        .collect()
}

fn enrolled(zone_type: Option<u16>) -> Option<Enrollment> {
    Some(Enrollment {
        zone_type,
        outcome: EnrollmentOutcome::Enrolled,
    })
}

fn unconfirmed(zone_type: Option<u16>) -> Option<Enrollment> {
    Some(Enrollment {
        zone_type,
        outcome: EnrollmentOutcome::Unconfirmed,
    })
}

fn failed_enrollment() -> Option<Enrollment> {
    Some(Enrollment {
        zone_type: None,
        outcome: EnrollmentOutcome::Failed,
    })
}

/// What the host sent to the IAS Zone cluster, in order.
fn zone_sends(h: &Harness) -> Vec<SentMessage> {
    sends_on(h, 0x0104, IAS)
}

fn is_read(m: &SentMessage) -> bool {
    m.message[0] & 0x03 == 0 && m.message[2] == 0x00
}

fn is_write(m: &SentMessage) -> bool {
    m.message[0] & 0x03 == 0 && m.message[2] == 0x02
}

fn is_response(m: &SentMessage) -> bool {
    m.message[0] & 0x03 == 1 && m.message[2] == 0x00
}

fn enroll_request(tsn: u8, zone_type: u16) -> Vec<u8> {
    let mut frame = vec![0x09, tsn, 0x01];
    frame.extend_from_slice(&zone_type.to_le_bytes());
    frame.extend_from_slice(&[0x34, 0x12]);
    frame
}

#[test]
fn a_motion_sensor_not_enrolled_is_read_given_our_address_sent_the_response_and_read_before_the_binds()
 {
    let (h, events) = paired(motion());
    assert_eq!(enrollments(&events), vec![enrolled(Some(0x000D))]);
    let zone = zone_sends(&h);
    let kinds: Vec<&str> = zone
        .iter()
        .map(|m| {
            if is_read(m) {
                "read"
            } else if is_write(m) {
                "write"
            } else if is_response(m) {
                "response"
            } else {
                "other"
            }
        })
        .collect();
    assert_eq!(kinds, vec!["read", "write", "response", "read"]);
    // The first read asks for the zone state, zone type and CIE address.
    assert_eq!(&zone[0].message[3..], &[0x00, 0x00, 0x01, 0x00, 0x10, 0x00]);
    // The write carries the coordinator's own address.
    assert_eq!(&zone[1].message[3..6], &[0x10, 0x00, 0xF0]);
    assert_eq!(&zone[1].message[6..14], &OWN_WIRE);
    // The response: success, zone 23.
    assert_eq!(
        zone[2].message,
        vec![0x11, zone[2].message[1], 0x00, 0x00, 0x17]
    );
    assert_eq!(&zone[3].message[3..], &[0x00, 0x00]);
    // 500 ms before the response, 500 ms before the read that follows it.
    let times: Vec<u64> = {
        let ncp = h.ncp.borrow();
        ncp.sent
            .iter()
            .zip(&ncp.sent_at)
            .filter(|(m, _)| m.cluster == IAS)
            .map(|(_, at)| *at)
            .collect()
    };
    assert!(times[2] >= times[1] + 500, "{times:?}");
    assert!(times[3] >= times[2] + 500, "{times:?}");
    // The binds come after the enrollment.
    let sent = h.ncp.borrow().sent.clone();
    let last_zone = sent.iter().rposition(|m| m.cluster == IAS).unwrap();
    let first_bind = sent.iter().position(|m| m.cluster == BIND_CLUSTER).unwrap();
    assert!(first_bind > last_zone);
    assert_eq!(h.ncp.borrow().devices[0].cie, OWN_WIRE);
    assert_eq!(bound_clusters(&h), vec![POWER]);
}

#[test]
fn a_sensor_already_enrolled_with_our_address_is_not_written_to() {
    let (h, events) = paired(motion().enrolled_with(OWN_WIRE));
    assert_eq!(enrollments(&events), vec![enrolled(Some(0x000D))]);
    let zone = zone_sends(&h);
    assert_eq!(zone.len(), 1, "one read, nothing else");
    assert!(is_read(&zone[0]));
}

#[test]
fn a_sensor_enrolled_with_another_address_is_given_ours_and_enrolled_again() {
    let (h, events) = paired(motion().enrolled_with([1, 2, 3, 4, 5, 6, 7, 8]));
    assert_eq!(enrollments(&events), vec![enrolled(Some(0x000D))]);
    let zone = zone_sends(&h);
    assert!(zone.iter().any(is_write));
    assert!(zone.iter().any(is_response));
    assert_eq!(h.ncp.borrow().devices[0].cie, OWN_WIRE);
}

#[test]
fn a_zone_state_that_is_never_1_is_read_20_times_then_unconfirmed_with_its_zone_type() {
    let mut device = motion();
    device.zone_stays_unenrolled = true;
    let (h, events) = paired(device);
    assert_eq!(enrollments(&events), vec![unconfirmed(Some(0x000D))]);
    let zone = zone_sends(&h);
    // The first read, then 20 reads of the state; the response is sent once.
    assert_eq!(zone.iter().filter(|m| is_read(m)).count(), 21);
    assert_eq!(zone.iter().filter(|m| is_response(m)).count(), 1);
    // The setup goes on.
    assert_eq!(bound_clusters(&h), vec![POWER]);
}

#[test]
fn a_zone_state_poll_without_an_answer_ends_the_enrollment_unconfirmed_within_the_cap() {
    let mut device = motion();
    device.zone_reads_answered = 1; // the first read only
    let mut h = started(with_device(device));
    let started_at = h.clock.get();
    h.radio.permit_join(180).unwrap();
    let events = poll_until_idle(&mut h);
    assert_eq!(enrollments(&events), vec![unconfirmed(Some(0x000D))]);
    // One poll, not answered; no second one, and the whole thing within the 70 s cap.
    assert_eq!(zone_sends(&h).iter().filter(|m| is_read(m)).count(), 2);
    assert!(h.clock.get() - started_at < 70_000);
    assert!(!h.radio.pairing_busy());
}

#[test]
fn a_cie_write_refused_or_unanswered_fails_the_enrollment_and_the_binds_go_on() {
    let mut refused = motion();
    refused.cie_answer = CieAnswer::Refuse(0x88);
    let (h, events) = paired(refused);
    assert_eq!(enrollments(&events), vec![failed_enrollment()]);
    assert!(!zone_sends(&h).iter().any(is_response), "no response sent");
    assert_eq!(bound_clusters(&h), vec![POWER]);

    let mut silent = motion();
    silent.cie_answer = CieAnswer::Silent;
    let (h, events) = paired(silent);
    assert_eq!(enrollments(&events), vec![failed_enrollment()]);
    // The write is sent twice (one retry), as the requests of spec 005.
    assert_eq!(zone_sends(&h).iter().filter(|m| is_write(m)).count(), 2);
    assert_eq!(bound_clusters(&h), vec![POWER]);
}

#[test]
fn a_failed_enrollment_is_tried_once_more_when_the_device_next_sends_a_message() {
    let mut device = motion();
    device.ignore_zone_reads = 2; // the first read and its retry are not answered
    let (mut h, events) = paired(device);
    assert_eq!(enrollments(&events), vec![failed_enrollment()]);

    // Any message from the device: it is awake.
    h.ncp
        .borrow_mut()
        .inject(DEVICE_NWK, 0x0104, 0x0001, 1, &report_frame(0x50, &[]));
    let events = poll_until_idle(&mut h);
    assert_eq!(enrollments(&events), vec![enrolled(Some(0x000D))]);

    // Once only: a later message starts nothing.
    let sent = h.ncp.borrow().sent.len();
    h.ncp
        .borrow_mut()
        .inject(DEVICE_NWK, 0x0104, 0x0001, 1, &report_frame(0x51, &[]));
    let events = poll_for(&mut h, 500);
    assert!(enrollments(&events).is_empty());
    assert_eq!(h.ncp.borrow().sent.len(), sent);
}

/// A radio that resumed a network, listening.
fn listening() -> Harness {
    let mut ncp = Ncp::recording();
    ncp.network = Some(StoredNetwork {
        extended_pan_wire: [1, 2, 3, 4, 5, 6, 7, 8],
        pan_id: 0x4321,
        channel: 20,
        node_type: 1,
    });
    let mut h = harness(ncp);
    h.radio.identify().unwrap();
    h.radio.listen().unwrap();
    h
}

#[test]
fn a_zone_enroll_request_in_listen_mode_is_answered() {
    let mut h = listening();
    h.ncp
        .borrow_mut()
        .inject(DEVICE_NWK, 0x0104, IAS, 1, &enroll_request(0x42, 0x000D));
    poll_for(&mut h, 500);
    let sent = zone_sends(&h);
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].destination, DEVICE_NWK);
    assert_eq!(sent[0].message, vec![0x11, 0x42, 0x00, 0x00, 0x17]);
}

#[test]
fn a_zone_enroll_request_after_the_outcome_with_another_zone_type_is_answered_and_adds_nothing() {
    let (mut h, events) = paired(motion());
    assert_eq!(enrollments(&events), vec![enrolled(Some(0x000D))]);
    h.ncp
        .borrow_mut()
        .inject(DEVICE_NWK, 0x0104, IAS, 1, &enroll_request(0x43, 0x0015));
    let events = poll_for(&mut h, 500);
    let last = zone_sends(&h).pop().unwrap();
    assert_eq!(last.message, vec![0x11, 0x43, 0x00, 0x00, 0x17]);
    // No new outcome: nothing reaches the registry.
    assert!(enrollments(&events).is_empty());
}

#[test]
fn a_zone_enroll_request_after_pairing_gets_a_response_with_its_own_tsn_and_no_default_response() {
    let (mut h, _) = paired(motion());
    let before = zone_sends(&h).len();
    h.ncp
        .borrow_mut()
        .inject(DEVICE_NWK, 0x0104, IAS, 1, &enroll_request(0x77, 0x000D));
    poll_for(&mut h, 500);
    let sent = zone_sends(&h);
    assert_eq!(
        sent.len(),
        before + 1,
        "the response, and no Default Response"
    );
    let response = &sent[before];
    assert_eq!(response.message[1], 0x77);
    assert_eq!(
        response.message[0] & 0x10,
        0x10,
        "default response disabled"
    );
    assert_eq!(response.message[0] & 0x08, 0, "client to server");
    assert_eq!(response.destination_endpoint, 1);
}

#[test]
fn a_read_without_a_zone_type_then_an_enroll_request_with_one_gives_that_zone_type() {
    let mut device = motion();
    device.zone_type = None;
    device.zone_stays_unenrolled = true;
    let mut h = started(with_device(device));
    h.radio.permit_join(180).unwrap();
    let mut events = Vec::new();
    let mut sent_request = false;
    let end = h.clock.get() + 60_000;
    while (!sent_request || h.radio.pairing_busy()) && h.clock.get() < end {
        events.extend(h.radio.poll_pairing().unwrap());
        // The enrollment is under way once the response is sent: the request comes now.
        if !sent_request && zone_sends(&h).iter().any(is_response) {
            h.ncp
                .borrow_mut()
                .inject(DEVICE_NWK, 0x0104, IAS, 1, &enroll_request(0x44, 0x0015));
            sent_request = true;
        }
    }
    assert_eq!(enrollments(&events), vec![unconfirmed(Some(0x0015))]);
}

#[test]
fn no_answer_to_the_first_read_fails_the_enrollment_with_no_zone_and_the_setup_goes_on() {
    let mut device = motion();
    device.ignore_zone_reads = u32::MAX;
    let (h, events) = paired(device);
    assert_eq!(enrollments(&events), vec![failed_enrollment()]);
    assert_eq!(
        zone_sends(&h).len(),
        2,
        "the read and its retry, nothing else"
    );
    assert_eq!(bound_clusters(&h), vec![POWER]);
    assert_eq!(
        reporting(&events),
        vec![(vec![POWER], vec![], false)],
        "the reporting setup still runs"
    );
}

#[test]
fn the_reporting_line_says_how_the_enrollment_ended_and_nothing_after_no_descriptor() {
    let line = |ok: Vec<u16>, failed: Vec<u16>, no_descriptor: bool, enrollment| {
        PairingEvent::ReportingConfigured {
            eui64: DEVICE_EUI,
            ok,
            failed,
            no_descriptor,
            enrollment,
        }
        .line()
    };
    let motion = enrolled(Some(0x000D));
    assert_eq!(
        line(vec![POWER], vec![], false, motion),
        "reporting: 0x00124B0012345678 battery, enrolled (motion)"
    );
    assert_eq!(
        line(vec![], vec![], false, motion),
        "reporting: 0x00124B0012345678 none, enrolled (motion)"
    );
    assert_eq!(
        line(vec![POWER], vec![TEMPERATURE], false, motion),
        "reporting: 0x00124B0012345678 battery failed: temperature, enrolled (motion)"
    );
    assert_eq!(
        line(vec![], vec![], true, motion),
        "reporting: 0x00124B0012345678 failed: no descriptor"
    );
    assert_eq!(
        line(vec![POWER], vec![], false, failed_enrollment()),
        "reporting: 0x00124B0012345678 battery, enrollment failed"
    );
    assert_eq!(
        line(vec![POWER], vec![], false, unconfirmed(Some(0x000D))),
        "reporting: 0x00124B0012345678 battery, enrollment unconfirmed (motion)"
    );
    // The names of the zone types, and the number of one that is not known.
    for (zone_type, name) in [
        (0x0015, "contact"),
        (0x002A, "water leak"),
        (0x0028, "fire"),
        (0x002D, "zone 0x002D"),
    ] {
        assert_eq!(
            line(vec![], vec![], false, enrolled(Some(zone_type))),
            format!("reporting: 0x00124B0012345678 none, enrolled ({name})")
        );
    }
    assert_eq!(
        line(vec![], vec![], false, None),
        "reporting: 0x00124B0012345678 none"
    );
}

#[test]
fn an_unknown_own_address_never_stops_the_run_the_enrollment_fails_and_the_clusters_fail() {
    let mut h = started(with_device(motion().enrolled_with(OWN_WIRE)));
    // After the identification the radio cannot tell its address again.
    h.radio.forget_own_eui64();
    h.ncp.borrow_mut().eui64_fails = true;
    h.radio.permit_join(180).unwrap();
    let end = h.clock.get() + 60_000;
    let mut events = Vec::new();
    while h.clock.get() < end && (events.is_empty() || h.radio.pairing_busy()) {
        events.extend(h.radio.poll_pairing().expect("the run goes on"));
    }
    // Enrolled with an address that cannot be compared, so "not enrolled with us": the write
    // cannot be built, the enrollment fails; the bind cannot be built either, the cluster fails.
    assert_eq!(enrollments(&events), vec![failed_enrollment()]);
    assert_eq!(reporting(&events), vec![(vec![], vec![POWER], false)]);
    assert!(!zone_sends(&h).iter().any(is_write));
}

// ---- Spec 011: commands to a device ----

use solite_core::radio::zcl::Command;
use solite_core::radio::{CommandFailure, CommandHandle, CommandTarget, Radio};
use solite_sim::{FakeClock, SimTransport};

const RELAY_WIRE: [u8; 8] = [0x7A, 0x56, 0x34, 0x12, 0x00, 0x4B, 0x12, 0x00];
const RELAY_EUI: [u8; 8] = [0x00, 0x12, 0x4B, 0x00, 0x12, 0x34, 0x56, 0x7A];
const RELAY_NWK: u16 = 0x2001;
const ON_OFF: u16 = 0x0006;

/// A mains relay like the ZBMINIR2, always awake, joining when pairing opens.
fn relay() -> SimDevice {
    let mut device = SimDevice::of_kind(Kind::Relay, RELAY_WIRE, None, None);
    device.nwk = RELAY_NWK;
    device.auto_join = true;
    device
}

fn target(nwk: Option<u16>) -> CommandTarget {
    CommandTarget {
        eui64: RELAY_EUI,
        nwk,
        endpoint: 1,
    }
}

type Outcome = (CommandHandle, Result<(), CommandFailure>);

/// Polls for `ms` of fake time: the events, and the outcomes of commands as they finish.
fn poll_commands(h: &mut Harness, ms: u64) -> (Vec<PairingEvent>, Vec<Outcome>) {
    let end = h.clock.get() + ms;
    let (mut events, mut outcomes) = (Vec::new(), Vec::new());
    while h.clock.get() < end {
        events.extend(h.radio.poll_pairing().unwrap());
        outcomes.extend(h.radio.take_command_results());
    }
    (events, outcomes)
}

/// The commands (cluster-specific frames) the host sent on the On/Off cluster, in order.
fn command_frames(h: &Harness) -> Vec<SentMessage> {
    sends_on(h, 0x0104, ON_OFF)
        .into_iter()
        .filter(|m| m.message[0] & 0x03 == 0x01)
        .collect()
}

fn light_state_events(events: &[PairingEvent]) -> Vec<bool> {
    measure_events(events)
        .iter()
        .flat_map(|e| e.measures.iter())
        .filter(|m| m.kind == "light_state")
        .map(|m| m.value == Value::Bool(true))
        .collect()
}

#[test]
fn a_command_acknowledged_by_a_default_response_success_is_a_success() {
    let (mut h, _) = paired(relay());
    let handle = h
        .radio
        .send_command(target(None), Command::On, 12_000)
        .unwrap();
    assert_eq!(h.radio.commands_in_flight(), 1);
    let (_, outcomes) = poll_commands(&mut h, 300);
    assert_eq!(outcomes, vec![(handle, Ok(()))]);
    assert_eq!(h.radio.commands_in_flight(), 0);
    let sent = command_frames(&h);
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].destination, RELAY_NWK);
    assert_eq!(
        (sent[0].source_endpoint, sent[0].destination_endpoint),
        (1, 1)
    );
    assert_eq!(sent[0].message[0], 0x01);
    assert_eq!(sent[0].message[2], 0x01, "On");
}

#[test]
fn a_default_response_with_the_error_status_0x81_is_a_refusal_that_says_the_status() {
    let (mut h, _) = paired(relay());
    h.ncp.borrow_mut().devices[0].refuse_commands = Some(0x81);
    let handle = h
        .radio
        .send_command(target(None), Command::Off, 12_000)
        .unwrap();
    let (_, outcomes) = poll_commands(&mut h, 300);
    assert_eq!(outcomes, vec![(handle, Err(CommandFailure::Refused(0x81)))]);
    assert_eq!(
        CommandFailure::Refused(0x81).to_string(),
        "device refused the command (status 0x81)"
    );
}

#[test]
fn no_answer_is_sent_again_2_s_after_the_5_s_wait_and_then_it_is_the_device_not_answering() {
    // Silent: sent at t, again at t + 7 s, given up at t + 12 s.
    let (mut h, _) = paired(relay());
    h.ncp.borrow_mut().devices[0].silent = true;
    let handle = h
        .radio
        .send_command(target(None), Command::On, 12_000)
        .unwrap();
    let (_, outcomes) = poll_commands(&mut h, 13_000);
    assert_eq!(outcomes, vec![(handle, Err(CommandFailure::NoAnswer))]);
    assert_eq!(
        CommandFailure::NoAnswer.to_string(),
        "device did not answer"
    );
    assert_eq!(command_frames(&h).len(), 2);
    let times = h.ncp.borrow().sent_at.clone();
    let sends = h.ncp.borrow().sent.clone();
    let at: Vec<u64> = sends
        .iter()
        .zip(&times)
        .filter(|(m, _)| m.cluster == ON_OFF && m.message[0] & 0x03 == 0x01)
        .map(|(_, t)| *t)
        .collect();
    assert!((7_000..7_200).contains(&(at[1] - at[0])), "{at:?}");

    // The device wakes up between the two sends: the retry is the answer.
    let (mut h, _) = paired(relay());
    h.ncp.borrow_mut().devices[0].silent = true;
    let handle = h
        .radio
        .send_command(target(None), Command::On, 12_000)
        .unwrap();
    let (_, early) = poll_commands(&mut h, 6_000);
    assert!(early.is_empty(), "still waiting for the retry");
    h.ncp.borrow_mut().devices[0].silent = false;
    let (_, outcomes) = poll_commands(&mut h, 3_000);
    assert_eq!(outcomes, vec![(handle, Ok(()))]);
    assert_eq!(command_frames(&h).len(), 2);
}

#[test]
fn measures_from_another_device_keep_arriving_while_a_command_waits() {
    let mut ncp = Ncp::recording();
    ncp.devices = vec![relay(), SimDevice::sensor()];
    let mut h = started(ncp);
    h.radio.permit_join(180).unwrap();
    poll_until_idle(&mut h);
    h.ncp.borrow_mut().devices[0].silent = true;
    h.radio
        .send_command(target(None), Command::On, 12_000)
        .unwrap();
    h.ncp.borrow_mut().inject(
        DEVICE_NWK,
        0x0104,
        TEMPERATURE,
        1,
        &temperature_report(9, 1500),
    );
    let (events, outcomes) = poll_commands(&mut h, 300);
    assert!(outcomes.is_empty(), "the command still waits");
    assert_eq!(h.radio.commands_in_flight(), 1);
    let measures = measure_events(&events);
    assert_eq!(measures.len(), 1);
    assert_eq!(measures[0].measures[0].kind, "temperature");
}

#[test]
fn after_a_success_the_state_is_read_once_and_the_answer_is_the_devices_measure() {
    let (mut h, _) = paired(relay());
    // A relay that does not report: only the read tells its new state.
    h.ncp.borrow_mut().devices[0].configured_attributes.clear();
    h.radio
        .send_command(target(None), Command::On, 12_000)
        .unwrap();
    let (events, outcomes) = poll_commands(&mut h, 500);
    assert_eq!(outcomes.len(), 1);
    assert_eq!(light_state_events(&events), vec![true]);
    let reads: Vec<SentMessage> = sends_on(&h, 0x0104, ON_OFF)
        .into_iter()
        .filter(|m| m.message[0] & 0x03 == 0 && m.message[2] == 0x00)
        .collect();
    assert_eq!(reads.len(), 1, "one read, after the command");
    assert_eq!(reads[0].message[3..], [0x00, 0x00]);
    // Off: the next read says off.
    h.radio
        .send_command(target(None), Command::Off, 12_000)
        .unwrap();
    let (events, _) = poll_commands(&mut h, 500);
    assert_eq!(light_state_events(&events), vec![false]);
}

#[test]
fn a_busy_radio_is_retried_once_after_2_s_and_an_error_status_is_never_retried() {
    // Busy once: the retry gets through.
    let (mut h, _) = paired(relay());
    h.ncp.borrow_mut().busy_sends = 1;
    let handle = h
        .radio
        .send_command(target(None), Command::On, 12_000)
        .unwrap();
    let (_, outcomes) = poll_commands(&mut h, 3_000);
    assert_eq!(outcomes, vec![(handle, Ok(()))]);
    assert_eq!(command_frames(&h).len(), 2, "the busy send and its retry");

    // Busy twice: radio busy.
    let (mut h, _) = paired(relay());
    h.ncp.borrow_mut().busy_sends = 2;
    let handle = h
        .radio
        .send_command(target(None), Command::On, 12_000)
        .unwrap();
    let (_, outcomes) = poll_commands(&mut h, 3_000);
    assert_eq!(outcomes, vec![(handle, Err(CommandFailure::Busy))]);
    assert_eq!(CommandFailure::Busy.to_string(), "radio busy");

    // An error status: one send, no retry.
    let (mut h, _) = paired(relay());
    h.ncp.borrow_mut().devices[0].refuse_commands = Some(0x81);
    h.radio
        .send_command(target(None), Command::On, 12_000)
        .unwrap();
    poll_commands(&mut h, 8_000);
    assert_eq!(command_frames(&h).len(), 1);
}

#[test]
fn a_default_response_from_another_device_or_with_another_tsn_command_or_cluster_is_no_answer() {
    let (mut h, _) = paired(relay());
    h.ncp.borrow_mut().devices[0].silent = true;
    let handle = h
        .radio
        .send_command(target(None), Command::On, 12_000)
        .unwrap();
    poll_commands(&mut h, 100);
    let tsn = command_frames(&h)[0].message[1];
    let response = |tsn: u8, command: u8| [0x18, tsn, 0x0B, command, 0x00];
    {
        let mut ncp = h.ncp.borrow_mut();
        // Another device (its node address), another transaction, another command, another
        // cluster: none of them answers the command.
        ncp.inject(0x3333, 0x0104, ON_OFF, 1, &response(tsn, 0x01));
        ncp.inject(
            RELAY_NWK,
            0x0104,
            ON_OFF,
            1,
            &response(tsn.wrapping_add(1), 0x01),
        );
        ncp.inject(RELAY_NWK, 0x0104, ON_OFF, 1, &response(tsn, 0x00));
        ncp.inject(RELAY_NWK, 0x0104, 0x0008, 1, &response(tsn, 0x01));
    }
    let (_, outcomes) = poll_commands(&mut h, 300);
    assert!(outcomes.is_empty(), "{outcomes:?}");
    assert_eq!(h.radio.commands_in_flight(), 1);
    // The right one answers.
    h.ncp
        .borrow_mut()
        .inject(RELAY_NWK, 0x0104, ON_OFF, 1, &response(tsn, 0x01));
    let (_, outcomes) = poll_commands(&mut h, 300);
    assert_eq!(outcomes, vec![(handle, Ok(()))]);
}

/// A second radio on the same simulated radio and network, that has heard nothing of the devices
/// (a restart of the host).
fn restarted(h: &Harness) -> Radio<SimTransport<FakeClock>, Box<dyn FnMut() -> u64>> {
    let clock = FakeClock::default();
    let time = std::rc::Rc::clone(&clock.time);
    let transport = SimTransport::new(std::rc::Rc::clone(&h.ncp), Vec::new(), 1, clock);
    let mut radio = Radio::new(
        transport,
        Box::new(move || time.get()) as Box<dyn FnMut() -> u64>,
    );
    radio.identify().unwrap();
    radio.listen().unwrap();
    radio
}

#[test]
fn the_address_comes_from_the_registry_else_the_radios_table_and_unknown_is_not_sent() {
    let (h, _) = paired(relay());
    let mut radio = restarted(&h);
    let wait = |radio: &mut Radio<_, _>| {
        let mut outcomes = Vec::new();
        for _ in 0..200 {
            radio.poll().unwrap();
            outcomes.extend(radio.take_command_results());
        }
        outcomes
    };
    let lookups = |h: &Harness| {
        h.ncp
            .borrow()
            .commands
            .iter()
            .filter(|c| c.0 == 0x60)
            .count()
    };

    // The registry knows the address: no lookup.
    let first = radio
        .send_command(target(Some(RELAY_NWK)), Command::On, 12_000)
        .unwrap();
    assert_eq!(wait(&mut radio), vec![(first, Ok(()))]);
    assert_eq!(lookups(&h), 0);

    // Another restart, the registry knows nothing: the radio's address table does.
    let mut radio = restarted(&h);
    let second = radio
        .send_command(target(None), Command::On, 12_000)
        .unwrap();
    assert_eq!(wait(&mut radio), vec![(second, Ok(()))]);
    assert_eq!(lookups(&h), 1);
    assert_eq!(command_frames(&h).len(), 2, "one command each");

    // A device nobody knows: nothing is sent.
    let before = h.ncp.borrow().sent.len();
    let stranger = CommandTarget {
        eui64: [9; 8],
        nwk: None,
        endpoint: 1,
    };
    let third = radio.send_command(stranger, Command::On, 12_000).unwrap();
    assert_eq!(
        wait(&mut radio),
        vec![(third, Err(CommandFailure::UnknownAddress))]
    );
    assert_eq!(h.ncp.borrow().sent.len(), before);
    assert_eq!(
        CommandFailure::UnknownAddress.to_string(),
        "device address unknown"
    );
}

#[test]
fn the_messages_of_a_paired_relay_tell_its_address_and_the_endpoint_of_its_on_off_frames() {
    let (mut h, _) = paired(relay());
    let routes = h.radio.take_routes();
    let route = routes
        .iter()
        .find(|r| r.eui64 == RELAY_EUI)
        .expect("a route");
    assert_eq!(route.nwk, RELAY_NWK);
    assert_eq!(route.endpoint, Some(1), "from the On/Off configure answer");
    assert!(h.radio.take_routes().is_empty(), "taken once");
}

#[test]
fn more_commands_than_the_bound_are_refused_and_the_bound_is_8() {
    let (mut h, _) = paired(relay());
    h.ncp.borrow_mut().devices[0].silent = true;
    for _ in 0..solite_core::radio::MAX_COMMANDS {
        h.radio
            .send_command(target(None), Command::On, 12_000)
            .unwrap();
    }
    assert!(
        h.radio
            .send_command(target(None), Command::On, 12_000)
            .is_err()
    );
    assert_eq!(h.radio.commands_in_flight(), 8);
}

// Spec 017: pairing while serving. After `listen` the devices are not followed; `set_following`
// turns it on for a window and off again, `forget_finished` lets go of the devices done.

/// A radio that resumed a network, listening, with these devices that join by themselves when
/// joining opens.
fn listening_with(ncp_devices: Vec<SimDevice>, crowd: usize) -> Harness {
    let mut ncp = Ncp::recording();
    ncp.network = Some(StoredNetwork {
        extended_pan_wire: [1, 2, 3, 4, 5, 6, 7, 8],
        pan_id: 0x4321,
        channel: 20,
        node_type: 1,
    });
    ncp.devices = ncp_devices;
    ncp.crowd = crowd;
    let mut h = harness(ncp);
    h.radio.identify().unwrap();
    h.radio.listen().unwrap();
    h
}

fn kinds(events: &[PairingEvent]) -> Vec<&'static str> {
    events
        .iter()
        .filter_map(|e| match e {
            PairingEvent::Joined { .. } => Some("joined"),
            PairingEvent::Identified { .. } => Some("identified"),
            PairingEvent::ReportingConfigured { .. } => Some("reporting"),
            _ => None,
        })
        .collect()
}

#[test]
fn a_device_that_joins_while_only_listening_is_not_followed() {
    let mut h = listening_with(vec![SimDevice::sensor()], 0);
    h.radio.permit_join(30).unwrap();
    let events = poll_for(&mut h, 5_000);
    assert!(kinds(&events).is_empty(), "{events:?}");
    assert!(!h.radio.pairing_busy());
}

#[test]
fn following_turned_on_follows_the_join_as_pairing_does_and_off_it_stops() {
    let mut h = listening_with(vec![SimDevice::sensor()], 0);
    h.radio.set_following(true);
    h.radio.permit_join(30).unwrap();
    let events = poll_until_idle(&mut h);
    let seen = kinds(&events);
    assert!(seen.contains(&"joined"), "{seen:?}");
    assert!(seen.contains(&"identified"), "{seen:?}");
    assert!(seen.contains(&"reporting"), "{seen:?}");
    assert!(h.radio.busy_devices().is_empty());
    // Off again: another announce of the same device is not followed.
    h.radio.set_following(false);
    h.radio.forget_finished();
    h.ncp.borrow_mut().devices = vec![another_device()];
    h.radio.permit_join(30).unwrap();
    let events = poll_for(&mut h, 5_000);
    assert!(kinds(&events).is_empty(), "{events:?}");
}

/// A sensor with an address of its own, that joins by itself when joining opens.
fn another_device() -> SimDevice {
    let mut device = SimDevice::sensor();
    device.eui_wire = [0x99, 0x34, 0x56, 0x78, 0x00, 0x4B, 0x12, 0x00];
    device.nwk = 0x7777;
    device.auto_join = true;
    device
}

#[test]
fn forgetting_the_finished_devices_makes_room_for_the_next_window() {
    let mut device = SimDevice::sensor();
    device.announce = false;
    let mut h = listening_with(vec![device], 31);
    h.radio.set_following(true);
    h.radio.permit_join(30).unwrap();
    let events = poll_for(&mut h, 5_000);
    assert_eq!(joined_count(&events), 32);
    // Let every identification and setup finish.
    poll_for(&mut h, 120_000);
    assert!(!h.radio.pairing_busy());
    h.radio.close_joining().unwrap();
    h.radio.forget_finished();
    // A 33rd device joins: followed, no `TooManyDevices`.
    {
        let mut ncp = h.ncp.borrow_mut();
        let mut extra = another_device();
        extra.announce = false;
        ncp.devices = vec![extra];
        ncp.crowd = 0;
    }
    h.radio.permit_join(30).unwrap();
    let events = poll_for(&mut h, 5_000);
    assert_eq!(joined_count(&events), 1, "{events:?}");
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, PairingEvent::TooManyDevices { .. })),
        "{events:?}"
    );
}

#[test]
fn devices_still_being_identified_count_against_the_limit_and_are_kept_by_forget_finished() {
    let mut device = SimDevice::sensor();
    device.announce = false;
    let mut h = listening_with(vec![device], 32);
    h.radio.set_following(true);
    h.radio.permit_join(30).unwrap();
    let events = poll_for(&mut h, 3_000);
    assert_eq!(joined_count(&events), 32);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, PairingEvent::TooManyDevices { .. })),
        "the 33rd is refused while the 32 are being identified"
    );
    // Mid-setup devices are kept.
    assert!(h.radio.pairing_busy());
    let before = h.radio.busy_devices().len();
    assert!(before > 0);
    h.radio.forget_finished();
    assert_eq!(h.radio.busy_devices().len(), before);
}

// ---- Spec 031: the colour cluster ----

/// A mains colour bulb joining when pairing opens, at the relay's address.
fn colour_bulb() -> SimDevice {
    let mut device = SimDevice::of_kind(Kind::ColorLight, RELAY_WIRE, None, None);
    device.nwk = RELAY_NWK;
    device.auto_join = true;
    device
}

/// The measures of the events with one of these kinds, one entry per event.
fn colour_events(events: &[PairingEvent]) -> Vec<Vec<(&'static str, Value)>> {
    measure_events(events)
        .iter()
        .map(|e| {
            e.measures
                .iter()
                .filter(|m| m.kind.starts_with("color") || m.kind == "light_color_temp")
                .map(|m| (m.kind, m.value))
                .collect::<Vec<_>>()
        })
        .filter(|kinds| !kinds.is_empty())
        .collect()
}

#[test]
fn a_colour_cluster_message_tells_the_endpoint_of_the_device() {
    // A bulb with the colour cluster alone: only its Color Control messages can tell the route.
    let mut device = colour_bulb();
    device.input_clusters = vec![0x0000, 0x0003, 0x0300];
    let (mut h, _) = paired(device);
    let routes = h.radio.take_routes();
    let route = routes
        .iter()
        .find(|r| r.eui64 == RELAY_EUI)
        .expect("a route");
    assert_eq!(route.endpoint, Some(1));
}

#[test]
fn a_move_to_color_acknowledged_is_read_back_in_one_request_and_gives_one_colour_frame() {
    let (mut h, _) = paired(colour_bulb());
    // A bulb that does not report: only the read tells its new colour.
    h.ncp.borrow_mut().devices[0].configured_attributes.clear();
    let command = Command::MoveToColor {
        x: 0x3333,
        y: 0x9999,
    };
    h.radio.send_command(target(None), command, 12_000).unwrap();
    let (events, outcomes) = poll_commands(&mut h, 500);
    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].1, Ok(()));
    let reads: Vec<SentMessage> = sends_on(&h, 0x0104, 0x0300)
        .into_iter()
        .filter(|m| m.message[0] & 0x03 == 0 && m.message[2] == 0x00)
        .collect();
    assert_eq!(reads.len(), 1, "one read for X and Y");
    assert_eq!(reads[0].message[3..], [0x03, 0x00, 0x04, 0x00]);
    let colours = colour_events(&events);
    assert_eq!(
        colours.len(),
        1,
        "one frame carries the colour: {colours:?}"
    );
    let kinds: Vec<&str> = colours[0].iter().map(|(k, _)| *k).collect();
    assert_eq!(kinds, vec!["color_x", "color_y"]);
    // Hue and saturation, then a colour temperature: their own attributes are read.
    h.radio
        .send_command(
            target(None),
            Command::MoveToHueSaturation {
                hue: 84,
                saturation: 254,
            },
            12_000,
        )
        .unwrap();
    let (events, _) = poll_commands(&mut h, 500);
    let colours = colour_events(&events);
    assert_eq!(colours.len(), 1, "{colours:?}");
    assert_eq!(
        colours[0],
        vec![
            ("color_hue", Value::Number(119.0)),
            ("color_saturation", Value::Number(100.0)),
        ]
    );
    h.radio
        .send_command(target(None), Command::MoveToColorTemperature(250), 12_000)
        .unwrap();
    let (events, _) = poll_commands(&mut h, 500);
    assert_eq!(
        colour_events(&events),
        vec![vec![("light_color_temp", Value::Number(250.0))]]
    );
}

// ---- A remote's commands (spec 049) ----

use solite_core::radio::measures::REMOTE_ACTION;
use solite_sim::DeviceReport;

fn remote() -> SimDevice {
    let mut device = SimDevice::of_kind(
        Kind::Button,
        [0x78, 0x56, 0x34, 0x12, 0x00, 0x4B, 0x12, 0x00],
        None,
        None,
    );
    device.nwk = 0x5C3D;
    device.auto_join = true;
    device.sleepy_ms = None;
    device
}

#[test]
fn a_remotes_output_clusters_are_bound_and_never_configured() {
    let (h, events) = paired(remote());
    // Its battery (an input cluster, configured), then On/Off and Level, bound only; its data
    // are the battery's and its `action`.
    assert_eq!(bound_clusters(&h), vec![POWER, 0x0006, 0x0008]);
    assert_eq!(
        reporting(&events),
        vec![(vec![POWER, REMOTE_ACTION], vec![], false)]
    );
    assert!(sends_on(&h, 0x0104, 0x0006).is_empty());
    assert!(sends_on(&h, 0x0104, 0x0008).is_empty());
}

#[test]
fn a_light_that_also_sends_commands_has_its_output_clusters_left_unbound() {
    let mut device = remote();
    device.input_clusters = vec![0x0000, 0x0003, 0x0006];
    device.output_clusters = vec![0x0006, 0x0008];
    let (h, events) = paired(device);
    // Its input On/Off is set up as a light's; its outputs are not bound.
    assert_eq!(bound_clusters(&h), vec![0x0006]);
    assert_eq!(reporting(&events), vec![(vec![0x0006], vec![], false)]);
}

#[test]
fn a_remotes_presses_are_two_measure_events_when_equal_and_get_a_default_response() {
    let (mut h, _) = paired(remote());
    for _ in 0..2 {
        let sent = h.ncp.borrow_mut().device_report(
            0,
            &DeviceReport {
                action: Some("toggle".to_string()),
                ..DeviceReport::default()
            },
        );
        assert!(sent);
    }
    let events = poll_for(&mut h, 2_000);
    let actions: Vec<String> = measure_events(&events)
        .iter()
        .flat_map(|e| e.measures.iter().map(ToString::to_string))
        .collect();
    assert_eq!(actions, vec!["action toggle", "action toggle"]);
    // The frames asked for a Default Response (command 0x0B), sent on the cluster they came on.
    let answers = sends_on(&h, 0x0104, 0x0006);
    assert_eq!(answers.len(), 2);
    assert!(answers.iter().all(|m| m.message[2] == 0x0B));
}

// Spec 048: the scale of the electrical clusters.

fn plug() -> SimDevice {
    let mut device = SimDevice::of_kind(
        solite_sim::Kind::Plug,
        [0x78, 0x56, 0x34, 0x12, 0x00, 0x4B, 0x12, 0x00],
        None,
        None,
    );
    device.nwk = DEVICE_NWK;
    device.auto_join = true;
    device
}

fn measure_lines(events: &[PairingEvent]) -> Vec<String> {
    measure_events(events)
        .iter()
        .flat_map(|m| m.lines())
        .collect()
}

#[test]
fn the_scale_of_a_plug_is_read_once_its_reporting_is_set_up_and_its_values_are_scaled() {
    let mut h = started(with_device(plug()));
    h.radio.permit_join(180).unwrap();
    poll_for(&mut h, 30_000);
    // One Read Attributes of the four scale attributes of each cluster.
    let reads = sends_on(&h, 0x0104, 0x0B04);
    let reads: Vec<_> = reads.iter().filter(|m| m.message[2] == 0x00).collect();
    assert_eq!(reads.len(), 1, "one scale read of Electrical Measurement");
    assert_eq!(
        reads[0].message[3..],
        [0x04, 0x06, 0x05, 0x06, 0x02, 0x06, 0x03, 0x06]
    );
    let reads = sends_on(&h, 0x0104, 0x0702);
    assert_eq!(reads.iter().filter(|m| m.message[2] == 0x00).count(), 1);
    // The device reports, in its own units: 1205 tenths of a watt, 520 mA, 12340 Wh.
    let report = solite_sim::DeviceReport {
        power: Some(120.5),
        current: Some(0.52),
        energy: Some(12.34),
        ..solite_sim::DeviceReport::default()
    };
    h.ncp.borrow_mut().device_report(0, &report);
    let lines = measure_lines(&poll_for(&mut h, 1_000));
    assert!(
        lines.iter().any(|l| l.ends_with("power 121 W")),
        "{lines:?}"
    );
    assert!(
        lines.iter().any(|l| l.ends_with("current 0.52 A")),
        "{lines:?}"
    );
    assert!(
        lines.iter().any(|l| l.ends_with("energy 12.34 kWh")),
        "{lines:?}"
    );
}

#[test]
fn a_report_before_the_scale_is_known_publishes_nothing_and_the_next_one_carries_the_value() {
    // The plug is in the network already (after a restart): the radio knows no scale.
    let mut device = plug();
    device.auto_join = false;
    let mut h = started(with_device(device.in_network()));
    let report = |tsn| solite_sim::electrical_report(tsn, Some(120.5), None);
    h.ncp
        .borrow_mut()
        .inject(DEVICE_NWK, 0x0104, 0x0B04, 1, &report(1));
    let first = measure_lines(&poll_for(&mut h, 2_000));
    assert!(first.is_empty(), "no figure before the scale: {first:?}");
    // The scale was asked for, the device answered.
    let asks = sends_on(&h, 0x0104, 0x0B04);
    assert_eq!(asks.iter().filter(|m| m.message[2] == 0x00).count(), 1);
    h.ncp
        .borrow_mut()
        .inject(DEVICE_NWK, 0x0104, 0x0B04, 1, &report(2));
    let second = measure_lines(&poll_for(&mut h, 2_000));
    assert!(
        second.iter().any(|l| l.ends_with("power 121 W")),
        "{second:?}"
    );
}

// ---- A shutter (spec 050) ----

fn shutter() -> SimDevice {
    let mut device = SimDevice::of_kind(Kind::Shutter, RELAY_WIRE, None, None);
    device.nwk = RELAY_NWK;
    device.auto_join = true;
    device
}

fn position_lines(events: &[PairingEvent]) -> Vec<String> {
    measure_events(events)
        .iter()
        .flat_map(|e| e.measures.iter().map(ToString::to_string))
        .filter(|l| l.starts_with("shutter_position"))
        .collect()
}

#[test]
fn a_shutter_has_its_position_reporting_set_up_and_moves_10_points_a_second_reporting_each() {
    let (mut h, events) = paired(shutter());
    assert_eq!(reporting(&events), vec![(vec![0x0102], vec![], false)]);
    // Reporting of attribute 0x0008, uint8, 1 s to 300 s, change 1.
    let configure = sends_on(&h, 0x0104, 0x0102);
    let configure = configure.iter().find(|m| m.message[2] == 0x06).unwrap();
    assert_eq!(
        configure.message[3..],
        [0x00, 0x08, 0x00, 0x20, 0x01, 0x00, 0x2C, 0x01, 0x01]
    );
    // From closed (lift 100) to lift 70: three steps, a second apart.
    h.radio
        .send_command(target(None), Command::GoToLiftPercentage(70), 12_000)
        .unwrap();
    let (events, outcomes) = poll_commands(&mut h, 5_000);
    assert_eq!(outcomes.len(), 1);
    assert!(outcomes[0].1.is_ok(), "{outcomes:?}");
    assert_eq!(
        position_lines(&events),
        vec![
            "shutter_position 10 %",
            "shutter_position 20 %",
            "shutter_position 30 %"
        ]
    );
    // No On/Off state is read back from a shutter.
    assert!(sends_on(&h, 0x0104, ON_OFF).is_empty());
}

#[test]
fn a_stop_halts_a_moving_shutter_and_open_runs_it_to_the_end() {
    let (mut h, _) = paired(shutter());
    h.radio
        .send_command(target(None), Command::ShutterOpen, 12_000)
        .unwrap();
    let (events, _) = poll_commands(&mut h, 2_500);
    assert_eq!(
        position_lines(&events),
        vec!["shutter_position 10 %", "shutter_position 20 %"]
    );
    h.radio
        .send_command(target(None), Command::ShutterStop, 12_000)
        .unwrap();
    let (events, _) = poll_commands(&mut h, 5_000);
    assert!(position_lines(&events).is_empty(), "stopped");
    h.radio
        .send_command(target(None), Command::ShutterOpen, 12_000)
        .unwrap();
    let (events, _) = poll_commands(&mut h, 12_000);
    let lines = position_lines(&events);
    // It went on from where it stopped (position 20: lift 80).
    assert_eq!(lines.first().unwrap(), "shutter_position 30 %");
    assert_eq!(lines.last().unwrap(), "shutter_position 100 %");
}

// ---- A thermostat (spec 051) ----

fn thermostat() -> SimDevice {
    let mut device = SimDevice::of_kind(Kind::Thermostat, RELAY_WIRE, None, None);
    device.nwk = RELAY_NWK;
    device.auto_join = true;
    device
}

fn thermostat_lines(events: &[PairingEvent]) -> Vec<String> {
    measure_events(events)
        .iter()
        .flat_map(|e| e.measures.iter().map(ToString::to_string))
        .filter(|l| l.starts_with("setpoint") || l.starts_with("local_temperature"))
        .collect()
}

#[test]
fn a_thermostat_takes_the_setpoint_write_answers_it_and_reports_the_new_setpoint() {
    let (mut h, events) = paired(thermostat());
    assert_eq!(reporting(&events), vec![(vec![0x0201], vec![], false)]);
    let handle = h
        .radio
        .send_command(target(None), Command::WriteHeatingSetpoint(2050), 12_000)
        .unwrap();
    let (events, outcomes) = poll_commands(&mut h, 1_000);
    assert_eq!(outcomes, vec![(handle, Ok(()))]);
    assert_eq!(thermostat_lines(&events), vec!["setpoint 20.5 °C"]);
    // The write is a Write Attributes; no On/Off state is read back from a thermostat.
    let write = sends_on(&h, 0x0104, 0x0201)
        .into_iter()
        .find(|m| m.message[2] == 0x02)
        .expect("a Write Attributes");
    assert_eq!(write.message[3..], [0x12, 0x00, 0x29, 0x02, 0x08]);
    assert!(sends_on(&h, 0x0104, ON_OFF).is_empty());
}

#[test]
fn a_thermostat_that_refuses_the_write_fails_the_command_and_reports_nothing() {
    let mut device = thermostat();
    device.refuse_commands = Some(0x86);
    let (mut h, _) = paired(device);
    h.radio
        .send_command(target(None), Command::WriteHeatingSetpoint(2050), 12_000)
        .unwrap();
    let (events, outcomes) = poll_commands(&mut h, 1_000);
    assert_eq!(outcomes.len(), 1);
    assert!(
        matches!(outcomes[0].1, Err(CommandFailure::Refused(_))),
        "{outcomes:?}"
    );
    assert!(thermostat_lines(&events).is_empty());
}
