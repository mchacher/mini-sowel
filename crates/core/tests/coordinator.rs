//! The coordinator against the scripted radio, with fake time: start or resume the network,
//! pairing, identification of the devices that join.

mod common;

use common::{Attr, FakeDevice, FakeNcp, Harness, SentMessage, StoredNetwork, harness};
use mini_sowel_core::radio::{Network, NetworkSettings, PairingEvent, RadioError};

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

fn ncp_at(protocol: u8) -> FakeNcp {
    let mut ncp = FakeNcp::new();
    ncp.protocol = protocol;
    ncp
}

fn with_device(device: FakeDevice) -> FakeNcp {
    let mut ncp = FakeNcp::new();
    ncp.device = Some(device);
    ncp
}

/// Identifies the radio and starts its network.
fn start(h: &mut Harness) -> Result<Network, RadioError> {
    h.radio.identify()?;
    h.radio.start_network(
        &NetworkSettings::default(),
        &mut scripted(draws()),
        &mut |_| Ok(()),
    )
}

fn started(ncp: FakeNcp) -> Harness {
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
    let mut h = harness(FakeNcp::new());
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
    let mut h = harness(FakeNcp::new());
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
    let h = started(FakeNcp::new());
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
    let mut ncp = FakeNcp::new();
    ncp.network_up_before_response = true;
    let mut h = harness(ncp);
    assert!(start(&mut h).unwrap().formed);
}

#[test]
fn twenty_unused_callbacks_then_a_trust_center_join_do_not_lose_the_join() {
    let mut device = FakeDevice::sensor();
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
    let mut ncp = FakeNcp::new();
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
    let mut h = harness(FakeNcp::new());
    h.radio.identify().unwrap();
    let network = h
        .radio
        .start_network(
            &NetworkSettings::default(),
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
    let mut h = harness(FakeNcp::new());
    h.radio.identify().unwrap();
    let network = h
        .radio
        .start_network(
            &NetworkSettings::default(),
            &mut scripted(values),
            &mut |_| Ok(()),
        )
        .unwrap();
    assert_eq!(network.extended_pan_id, [8, 7, 6, 5, 4, 3, 2, 1]);
}

#[test]
fn a_random_source_that_fails_stops_the_start_before_the_network_is_formed() {
    let mut h = harness(FakeNcp::new());
    h.radio.identify().unwrap();
    let error = h
        .radio
        .start_network(
            &NetworkSettings::default(),
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
    let mut ncp = FakeNcp::new();
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
    let mut ncp = FakeNcp::new();
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
    let mut ncp = FakeNcp::new();
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
    let mut h = harness(FakeNcp::new());
    let error = h
        .radio
        .start_network(
            &NetworkSettings::default(),
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
    let mut h = started(FakeNcp::new());
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
    let mut h = started(FakeNcp::new());
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
    let mut device = FakeDevice::sensor();
    device.announce_twice = true;
    let mut h = started(with_device(device));
    h.radio.permit_join(30).unwrap();
    let events = poll_until_idle(&mut h);
    assert_eq!(joined_count(&events), 1, "{events:?}");
}

#[test]
fn a_device_that_announces_and_answers_the_endpoint_request_and_the_basic_read_is_identified() {
    for protocol in [13, 19] {
        let mut h = started(with_device_at(protocol, FakeDevice::sensor()));
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

fn with_device_at(protocol: u8, device: FakeDevice) -> FakeNcp {
    let mut ncp = ncp_at(protocol);
    ncp.device = Some(device);
    ncp
}

#[test]
fn a_trust_center_join_without_announce_starts_the_identification_5_seconds_later() {
    let mut device = FakeDevice::sensor();
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
    let mut device = FakeDevice::sensor();
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
    let mut device = FakeDevice::sensor();
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
    let mut device = FakeDevice::sensor();
    device.manufacturer = Attr::Unsupported;
    let mut h = started(with_device(device));
    h.radio.permit_join(30).unwrap();
    let events = poll_until_idle(&mut h);
    assert_eq!(identified(&events), vec![(None, some("TH01"))]);
}

#[test]
fn a_device_joining_one_second_before_the_window_ends_is_identified_after_the_window_closed() {
    let mut device = FakeDevice::sensor();
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
    let mut device = FakeDevice::sensor();
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
    let mut device = FakeDevice::sensor();
    device.endpoint_status = 0x80;
    let mut h = started(with_device(device));
    h.radio.permit_join(30).unwrap();
    let events = poll_until_idle(&mut h);
    assert_eq!(identified(&events), vec![(some("eWeLink"), some("TH01"))]);
    assert_eq!(h.ncp.borrow().sent[1].destination_endpoint, 1);
}

#[test]
fn a_device_silent_on_the_basic_read_is_asked_twice_then_identified_with_both_unknown() {
    let mut device = FakeDevice::sensor();
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
    let mut device = FakeDevice::sensor();
    device.ignore_reads = 1;
    let mut h = started(with_device(device));
    h.radio.permit_join(30).unwrap();
    let events = poll_until_idle(&mut h);
    assert_eq!(identified(&events), vec![(some("eWeLink"), some("TH01"))]);
}

#[test]
fn a_trust_center_join_denied_or_a_device_that_left_gives_no_event() {
    for (status, decision) in [(1u8, 2u8), (2, 0)] {
        let mut device = FakeDevice::sensor();
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
    let mut device = FakeDevice::sensor();
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
    let mut ncp = with_device(FakeDevice::sensor());
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
    let mut ncp = with_device(FakeDevice::sensor());
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
    let mut device = FakeDevice::sensor();
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
    let mut ncp = with_device(FakeDevice::sensor());
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
    let mut device = FakeDevice::sensor();
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
    use mini_sowel_core::radio::network_line;
    let network = Network {
        pan_id: 0x1A2B,
        extended_pan_id: [0xDD; 8],
        channel: 15,
        formed: true,
        keys: None,
    };
    assert_eq!(
        network_line(&network, false),
        "network: formed, channel 15, PAN 0x1A2B, extended PAN 0xDDDDDDDDDDDDDDDD"
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
    let mut h = harness(FakeNcp::new());
    h.radio.identify().unwrap();
    let mut seen = Vec::new();
    let ncp = std::rc::Rc::clone(&h.ncp);
    let network = h
        .radio
        .start_network(
            &NetworkSettings::default(),
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
    let mut ncp = FakeNcp::new();
    ncp.refuse_concentrator = true;
    let mut h = harness(ncp);
    h.radio.identify().unwrap();
    let mut keys = None;
    let error = h
        .radio
        .start_network(
            &NetworkSettings::default(),
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
    let mut h = harness(FakeNcp::new());
    h.radio.identify().unwrap();
    let error = h
        .radio
        .start_network(
            &NetworkSettings::default(),
            &mut scripted(draws()),
            &mut |_| Err("cannot write /x".to_string()),
        )
        .unwrap_err();
    assert_eq!(error.to_string(), "cannot write /x");
    assert!(!ids(&h).contains(&0x28));
}

#[test]
fn a_late_response_with_an_older_sequence_number_is_ignored_not_an_error() {
    let mut ncp = FakeNcp::new();
    ncp.stale_response_first = true;
    let mut h = harness(ncp);
    assert!(start(&mut h).unwrap().formed);
}

// Reporting setup and measures (spec 006).

use common::{ConfigureAnswer, report_frame, temperature_report};
use mini_sowel_core::radio::measures::{Measure, Value};
use mini_sowel_core::radio::{MeasureEvent, Sender};

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
fn paired(device: FakeDevice) -> (Harness, Vec<PairingEvent>) {
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
    }
}

#[test]
fn a_paired_fake_sensor_has_bind_and_configure_reporting_done_for_the_three_clusters() {
    let (h, events) = paired(FakeDevice::sensor());
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
    let mut device = FakeDevice::sensor();
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
    let mut device = FakeDevice::sensor();
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
    let mut device = FakeDevice::sensor();
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
    let mut device = FakeDevice::sensor();
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
    let mut device = FakeDevice::sensor();
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
    let mut h = started(FakeNcp::new());
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
    let (_h, events) = paired(FakeDevice::sensor());
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
    let mut ncp = FakeNcp::new();
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
    let mut h = harness(FakeNcp::new());
    h.radio.identify().unwrap();
    let error = h.radio.listen().unwrap_err();
    assert_eq!(error, RadioError::NoNetwork);
    assert_eq!(error.to_string(), "no network, pair a device first");
    assert!(!ids(&h).contains(&0x1E));
}

#[test]
fn listen_resumes_the_network_and_never_forms_one() {
    let mut ncp = FakeNcp::new();
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
    let mut device = FakeDevice::sensor();
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
    let mut device = FakeDevice::sensor();
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
    let mut h = started(FakeNcp::new());
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
    let mut device = FakeDevice::sensor();
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
