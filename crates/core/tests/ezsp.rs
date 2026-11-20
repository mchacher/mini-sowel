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
fn a_callback_frame_between_command_and_response_is_ignored() {
    let mut ezsp = Ezsp::new();
    ezsp.version_command();
    // Callback bit (0x10) set, any sequence.
    let callback = [0x77, 0x90, 0x19];
    assert_eq!(ezsp.parse_response(&callback), Ok(Parsed::Callback));
    let params = params_of(ezsp.parse_response(&legacy_response(0, 0, &[13, 2, 0, 0x74])));
    assert_eq!(params, vec![13, 2, 0, 0x74]);
}

#[test]
fn responses_with_truncated_or_overflow_bit_or_without_response_bit_are_errors() {
    let mut ezsp = Ezsp::new();
    ezsp.version_command();
    assert_eq!(
        ezsp.parse_response(&[0, 0x82, 0, 13, 2, 0, 0]),
        Err(EzspError::Truncated)
    );
    assert_eq!(
        ezsp.parse_response(&[0, 0x81, 0, 13, 2, 0, 0]),
        Err(EzspError::Overflow)
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
