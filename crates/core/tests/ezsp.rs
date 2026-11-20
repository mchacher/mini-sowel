//! EZSP: frames, negotiation, responses. Constants from UG100 and the enums of
//! zigbee-herdsman's ember adapter.

use mini_sowel_core::radio::ezsp::*;

/// A legacy response frame: sequence, frame control 0x80, id, parameters.
fn legacy_response(seq: u8, id: u8, params: &[u8]) -> Vec<u8> {
    let mut frame = vec![seq, 0x80, id];
    frame.extend_from_slice(params);
    frame
}

/// An extended response frame.
fn extended_response(seq: u8, id: u16, params: &[u8]) -> Vec<u8> {
    let mut frame = vec![seq, 0x80, 0x01];
    frame.extend_from_slice(&id.to_le_bytes());
    frame.extend_from_slice(params);
    frame
}

fn params_of(parsed: Result<Parsed, EzspError>) -> Vec<u8> {
    match parsed {
        Ok(Parsed::Response(params)) => params,
        other => panic!("expected a response, got {other:?}"),
    }
}

#[test]
fn legacy_version_command_with_desired_19_and_seq_0() {
    let mut ezsp = Ezsp::new();
    assert_eq!(ezsp.version_command(), vec![0x00, 0x00, 0x00, 0x13]);
}

#[test]
fn extended_version_command_with_seq_1_has_two_byte_control_and_id() {
    let frame = encode_command(1, ID_VERSION, &[0x13], true);
    assert_eq!(frame, vec![0x01, 0x00, 0x01, 0x00, 0x00, 0x13]);
}

#[test]
fn version_response_gives_protocol_stack_type_and_stack_version() {
    let version = parse_version(&[13, 2, 0x40, 0x74]).unwrap();
    assert_eq!(version.protocol, 13);
    assert_eq!(version.stack_type, 2);
    assert_eq!(version.stack_version, 0x7440);
    assert_eq!(format_stack_version(version.stack_version), "7.4.4.0");
}

/// Runs the two `version` exchanges against a radio at `radio_version`.
fn negotiate(radio_version: u8) -> Result<Version, EzspError> {
    let mut ezsp = Ezsp::new();
    ezsp.version_command();
    let first =
        params_of(ezsp.parse_response(&legacy_response(0, 0, &[radio_version, 2, 0x40, 0x74])));
    let second_command = ezsp.on_first_version(&first)?;
    // The second command is in the extended format, with the radio's version, sequence 1.
    assert_eq!(
        second_command,
        vec![1, 0x00, 0x01, 0x00, 0x00, radio_version]
    );
    let second = params_of(ezsp.parse_response(&extended_response(
        1,
        ID_VERSION,
        &[radio_version, 2, 0x40, 0x74],
    )));
    let version = ezsp.on_second_version(&second)?;
    assert_eq!(ezsp.protocol_version(), Some(radio_version));
    Ok(version)
}

#[test]
fn negotiation_with_a_radio_at_19_and_at_13_settles_on_that_version() {
    assert_eq!(negotiate(19).unwrap().protocol, 19);
    assert_eq!(negotiate(13).unwrap().protocol, 13);
}

#[test]
fn a_radio_at_12_or_at_20_is_refused_with_the_version_and_the_range() {
    for found in [12u8, 20] {
        let mut ezsp = Ezsp::new();
        ezsp.version_command();
        let params = params_of(ezsp.parse_response(&legacy_response(0, 0, &[found, 2, 0, 0x74])));
        let error = ezsp.on_first_version(&params).unwrap_err();
        assert_eq!(error, EzspError::UnsupportedVersion { found });
        let text = error.to_string();
        assert!(text.contains(&found.to_string()), "{text}");
        assert!(text.contains("13 to 19"), "{text}");
    }
}

#[test]
fn network_state_values_are_named() {
    let shown: Vec<String> = [0u8, 1, 2, 3, 6, 4]
        .iter()
        .map(|v| parse_network_state(&[*v]).unwrap().to_string())
        .collect();
    assert_eq!(
        shown,
        vec![
            "no network",
            "joining",
            "joined",
            "joined with no parent",
            "leaving",
            "unknown (4)"
        ]
    );
    assert_eq!(parse_network_state(&[0]).unwrap(), NetworkState::NoNetwork);
    assert_eq!(parse_network_state(&[4]).unwrap(), NetworkState::Unknown(4));
}

#[test]
fn a_version_response_with_stack_type_1_is_an_error() {
    let mut ezsp = Ezsp::new();
    ezsp.version_command();
    let params = params_of(ezsp.parse_response(&legacy_response(0, 0, &[13, 1, 0x40, 0x74])));
    assert_eq!(
        ezsp.on_first_version(&params),
        Err(EzspError::UnsupportedStackType(1))
    );
}

#[test]
fn a_callback_frame_between_command_and_response_is_returned_apart_and_the_response_still_matches()
{
    let mut ezsp = Ezsp::new();
    ezsp.version_command();
    // Callback bit (0x10) set, any sequence.
    let callback = [0x77, 0x90, 0x01, 0x19, 0x00, 0x15];
    assert_eq!(
        ezsp.parse_response(&callback),
        Ok(Parsed::Callback(Callback {
            id: 0x19,
            params: vec![0x15]
        }))
    );
    let params = params_of(ezsp.parse_response(&legacy_response(0, 0, &[13, 2, 0, 0x74])));
    assert_eq!(params, vec![13, 2, 0, 0x74]);
}

#[test]
fn responses_with_truncated_bit_or_without_response_bit_are_errors() {
    let mut ezsp = Ezsp::new();
    ezsp.version_command();
    assert_eq!(
        ezsp.parse_response(&[0, 0x82, 0, 13, 2, 0, 0]),
        Err(EzspError::Truncated)
    );
    assert_eq!(
        ezsp.parse_response(&[0, 0x00, 0, 13, 2, 0, 0]),
        Err(EzspError::NotAResponse)
    );
}

#[test]
fn get_eui64_response_is_read_little_endian() {
    let eui = parse_eui64(&[0x56, 0x34, 0x12, 0xFE, 0xFF, 0xB9, 0x6F, 0x8C]).unwrap();
    assert_eq!(u64::from_be_bytes(eui), 0x8C6F_B9FF_FE12_3456);
}

#[test]
fn a_wrong_sequence_a_wrong_frame_id_or_a_short_frame_is_an_error_not_a_panic() {
    let mut ezsp = Ezsp::new();
    ezsp.version_command();
    assert_eq!(
        ezsp.parse_response(&legacy_response(5, 0, &[13, 2, 0, 0x74])),
        Err(EzspError::WrongSequence {
            expected: 0,
            got: 5
        })
    );
    assert_eq!(
        ezsp.parse_response(&legacy_response(0, 0x18, &[0])),
        Err(EzspError::WrongFrameId {
            expected: 0,
            got: 0x18
        })
    );
    for short in [&[][..], &[0], &[0, 0x80]] {
        assert_eq!(ezsp.parse_response(short), Err(EzspError::TooShort));
    }
    assert!(matches!(
        parse_version(&[13, 2]),
        Err(EzspError::BadLength { .. })
    ));
    assert!(matches!(
        parse_network_state(&[]),
        Err(EzspError::BadLength { .. })
    ));
    assert!(matches!(
        parse_eui64(&[1, 2, 3]),
        Err(EzspError::BadLength { .. })
    ));
}

#[test]
fn the_response_format_is_read_from_the_response_itself() {
    // The command went out in the legacy format; an extended response is still understood.
    let mut ezsp = Ezsp::new();
    ezsp.version_command();
    let params =
        params_of(ezsp.parse_response(&extended_response(0, ID_VERSION, &[13, 2, 0x40, 0x74])));
    assert_eq!(params, vec![13, 2, 0x40, 0x74]);
    // And a legacy response is understood once the commands are extended.
    let first = params;
    ezsp.on_first_version(&first).unwrap();
    let params = params_of(ezsp.parse_response(&legacy_response(1, 0, &[13, 2, 0x40, 0x74])));
    assert_eq!(params, vec![13, 2, 0x40, 0x74]);
}

#[test]
fn a_response_with_the_overflow_flag_is_used_and_the_flag_is_reported_once() {
    let mut ezsp = Ezsp::new();
    ezsp.version_command();
    let params = params_of(ezsp.parse_response(&[0, 0x81, 0, 13, 2, 0x40, 0x74]));
    assert_eq!(params, vec![13, 2, 0x40, 0x74]);
    assert!(ezsp.take_overflow());
    assert!(!ezsp.take_overflow());
}

// ---- commands and callbacks of the coordinator (spec 005) ----

#[test]
fn form_network_parameters_are_20_bytes_in_the_order_of_the_reference() {
    let ext = [1, 2, 3, 4, 5, 6, 7, 8];
    let bytes = NetworkParameters::new(ext, 0x1A2B, 15).encode();
    assert_eq!(
        bytes,
        vec![
            1, 2, 3, 4, 5, 6, 7, 8, // extended PAN
            0x2B, 0x1A, // PAN, little-endian
            5,    // power
            15,   // channel
            0, 0, 0, 0, // join method, network manager (2), update id
            0x00, 0xF8, 0xFF, 0x07, // channels 0x07FFF800
        ]
    );
}

#[test]
fn set_initial_security_state_is_43_bytes_with_bitmask_0b84_keys_sequence_0_and_blank_address() {
    let tc = [0xAA; 16];
    let network = [0xBB; 16];
    let bytes = initial_security_state(&tc, &network);
    assert_eq!(bytes.len(), 43);
    assert_eq!(&bytes[..2], &[0x84, 0x0B]);
    assert_eq!(&bytes[2..18], &tc);
    assert_eq!(&bytes[18..34], &network);
    assert_eq!(bytes[34], 0);
    assert_eq!(&bytes[35..], &[0xFF; 8]);
}

#[test]
fn stack_status_network_up_is_one_byte_in_ezsp_13_and_four_in_14_and_later() {
    assert_eq!(
        parse_stack_status(false, &[0x90]),
        Ok(StackStatus::NetworkUp)
    );
    assert_eq!(
        parse_stack_status(true, &[0x15, 0, 0, 0]),
        Ok(StackStatus::NetworkUp)
    );
    assert_eq!(
        parse_stack_status(false, &[0x91]),
        Ok(StackStatus::NetworkDown)
    );
    assert_eq!(
        parse_stack_status(true, &[0x16, 0, 0, 0]),
        Ok(StackStatus::NetworkDown)
    );
    // A short callback is an error, not a panic.
    assert_eq!(parse_stack_status(true, &[0x15]), Err(EzspError::TooShort));
}

#[test]
fn trust_center_join_handler_gives_node_id_address_status_and_decision() {
    let mut bytes = vec![0x3D, 0x5C];
    bytes.extend_from_slice(&[0x78, 0x56, 0x34, 0x12, 0x00, 0x4B, 0x12, 0x00]);
    bytes.extend_from_slice(&[1, 0, 0x00, 0x00]); // status, decision, parent
    bytes.push(0xEE); // a field a newer stack appends
    let join = parse_trust_center_join(&bytes).unwrap();
    assert_eq!(join.node_id, 0x5C3D);
    assert_eq!(join.eui64, [0x00, 0x12, 0x4B, 0x00, 0x12, 0x34, 0x56, 0x78]);
    assert_eq!(join.status, 1);
    assert_eq!(join.decision, 0);
}

fn aps_bytes(profile: u16, cluster: u16) -> Vec<u8> {
    ApsFrame {
        profile,
        cluster,
        source_endpoint: 1,
        destination_endpoint: 1,
        options: 0x1140,
        group_id: 0,
        sequence: 7,
    }
    .encode()
}

#[test]
fn incoming_message_handler_in_13_and_14_layouts_gives_the_same_message() {
    let payload = [0x18, 0x42, 0x01, 0xAA];
    // EZSP 13: type, APS, lqi, rssi, sender, binding index, address index, payload.
    let mut v13 = vec![0x00];
    v13.extend(aps_bytes(0x0104, 0x0000));
    v13.extend_from_slice(&[0xC8, 0xD0, 0x3D, 0x5C, 0xFF, 0xFF]);
    v13.push(payload.len() as u8);
    v13.extend_from_slice(&payload);
    // EZSP 14+: type, APS, packet info (18 bytes), payload.
    let mut v14 = vec![0x00];
    v14.extend(aps_bytes(0x0104, 0x0000));
    v14.extend_from_slice(&[0x3D, 0x5C]);
    v14.extend_from_slice(&[0; 8]);
    v14.extend_from_slice(&[0xFF, 0xFF, 0xC8, 0xD0]);
    v14.extend_from_slice(&[0; 4]);
    v14.push(payload.len() as u8);
    v14.extend_from_slice(&payload);
    v14.push(0xEE); // trailing byte

    let a = parse_incoming_message(false, &v13).unwrap();
    let b = parse_incoming_message(true, &v14).unwrap();
    assert_eq!(a, b);
    assert_eq!(a.aps.profile, 0x0104);
    assert_eq!(a.aps.cluster, 0x0000);
    assert_eq!(a.aps.source_endpoint, 1);
    assert_eq!(a.aps.destination_endpoint, 1);
    assert_eq!(a.sender, 0x5C3D);
    assert_eq!(a.payload, payload);
}

#[test]
fn message_sent_handler_in_13_and_14_gives_status_and_tag() {
    // EZSP 13: type, destination, APS, tag (1 byte), status (1 byte).
    let mut v13 = vec![0x00, 0x3D, 0x5C];
    v13.extend(aps_bytes(0, 5));
    v13.extend_from_slice(&[0x2A, 0x66]); // tag 42, delivery failed
    // EZSP 14+: status (4 bytes), type, destination, APS, tag (2 bytes).
    let mut v14 = vec![0x02, 0x0C, 0x00, 0x00, 0x00, 0x3D, 0x5C];
    v14.extend(aps_bytes(0, 5));
    v14.extend_from_slice(&[0x2A, 0x00]);

    let a = parse_message_sent(false, &v13).unwrap();
    let b = parse_message_sent(true, &v14).unwrap();
    assert_eq!(a, b);
    assert_eq!(a.tag, 42);
    assert_eq!(a.status, STATUS_DELIVERY_FAILED);
    assert_eq!(a.destination, 0x5C3D);
}

#[test]
fn message_sent_handler_ezsp_13_with_a_trailing_length_byte_is_parsed() {
    let mut v13 = vec![0x00, 0x3D, 0x5C];
    v13.extend(aps_bytes(0, 5));
    v13.extend_from_slice(&[0x2A, 0x00, 0x00]); // tag, status OK, length of contents 0
    let sent = parse_message_sent(false, &v13).unwrap();
    assert_eq!(sent.status, STATUS_OK);
    assert_eq!(sent.tag, 42);
}

#[test]
fn send_unicast_in_13_has_a_one_byte_tag_and_in_14_a_two_byte_tag() {
    let aps = ApsFrame {
        profile: 0x0104,
        cluster: 0x0000,
        source_endpoint: 1,
        destination_endpoint: 2,
        options: 0x1140,
        group_id: 0,
        sequence: 0,
    };
    let message = [0x10, 0x05, 0x00, 0x04, 0x00];
    let v13 = send_unicast(false, 0x5C3D, &aps, 0x0105, &message);
    let v14 = send_unicast(true, 0x5C3D, &aps, 0x0105, &message);
    let head = [
        0x00, // type: direct
        0x3D, 0x5C, // destination
        0x04, 0x01, 0x00, 0x00, 0x01, 0x02, 0x40, 0x11, 0x00, 0x00, 0x00, // APS
    ];
    let mut expected13 = head.to_vec();
    expected13.push(0x05); // tag cut to one byte
    expected13.push(5);
    expected13.extend_from_slice(&message);
    let mut expected14 = head.to_vec();
    expected14.extend_from_slice(&[0x05, 0x01]); // tag in two bytes
    expected14.push(5);
    expected14.extend_from_slice(&message);
    assert_eq!(v13, expected13);
    assert_eq!(v14, expected14);
}

#[test]
fn a_response_with_no_parameters_is_a_response() {
    // clearTransientLinkKeys answers with nothing: it must be awaited and matched.
    let mut ezsp = Ezsp::new();
    ezsp.version_command();
    let first = params_of(ezsp.parse_response(&legacy_response(0, 0, &[13, 2, 0x40, 0x74])));
    ezsp.on_first_version(&first).unwrap(); // extended format from now on, sequence 1
    let command = ezsp.command(ID_CLEAR_TRANSIENT_LINK_KEYS, &[]);
    assert_eq!(command, vec![2, 0x00, 0x01, 0x6B, 0x00]);
    let response = extended_response(2, ID_CLEAR_TRANSIENT_LINK_KEYS, &[]);
    assert_eq!(ezsp.parse_response(&response), Ok(Parsed::Response(vec![])));
}

#[test]
fn statuses_of_ezsp_13_are_read_as_the_statuses_of_14_and_later() {
    assert_eq!(
        read_status(false, &[0x93, 9]),
        Ok((STATUS_NOT_JOINED, &[9u8][..]))
    );
    assert_eq!(
        read_status(true, &[0x17, 0, 0, 0, 9]),
        Ok((STATUS_NOT_JOINED, &[9u8][..]))
    );
    assert_eq!(read_status(false, &[0xA1]).unwrap().0, STATUS_BUSY);
    assert!(status_is_busy(STATUS_BUSY));
    assert!(!status_is_busy(STATUS_OK));
}
