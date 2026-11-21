//! ZCL frames (Read and Report Attributes, Configure Reporting, Default Response, IAS zone):
//! bytes in, types out; and the Default Response the coordinator sends, against the scripted
//! radio.

use mini_sowel_core::radio::NetworkSettings;
use mini_sowel_core::radio::measures::{self, Value};
use mini_sowel_core::radio::zcl::*;
use mini_sowel_sim::{
    Harness, Ncp, SentMessage, harness, report_frame, report_frame_with_control, temperature_report,
};

/// A Read Attributes response frame with the given records (already encoded).
fn response(tsn: u8, records: &[u8]) -> Vec<u8> {
    let mut frame = vec![0x18, tsn, 0x01];
    frame.extend_from_slice(records);
    frame
}

fn string_record(id: u16, text: &[u8]) -> Vec<u8> {
    let mut bytes = id.to_le_bytes().to_vec();
    bytes.extend_from_slice(&[0x00, 0x42, text.len() as u8]);
    bytes.extend_from_slice(text);
    bytes
}

#[test]
fn read_attributes_request_for_manufacturer_and_model_is_10_tsn_00_04_00_05_00() {
    assert_eq!(
        read_attributes_request(
            0x2A,
            &[ATTRIBUTE_MANUFACTURER_NAME, ATTRIBUTE_MODEL_IDENTIFIER]
        ),
        vec![0x10, 0x2A, 0x00, 0x04, 0x00, 0x05, 0x00]
    );
}

#[test]
fn response_with_two_strings_gives_manufacturer_and_model() {
    let mut records = string_record(0x0004, b"eWeLink");
    records.extend(string_record(0x0005, b"TH01"));
    let parsed = parse_read_attributes_response(&response(0x2A, &records)).unwrap();
    assert_eq!(parsed.tsn, 0x2A);
    assert_eq!(parsed.string(ATTRIBUTE_MANUFACTURER_NAME), Some("eWeLink"));
    assert_eq!(parsed.string(ATTRIBUTE_MODEL_IDENTIFIER), Some("TH01"));
}

#[test]
fn response_with_manufacturer_unsupported_gives_manufacturer_unknown_and_model_read() {
    let mut records = vec![0x04, 0x00, 0x86]; // status 0x86: unsupported attribute
    records.extend(string_record(0x0005, b"TH01"));
    let parsed = parse_read_attributes_response(&response(1, &records)).unwrap();
    assert_eq!(parsed.string(ATTRIBUTE_MANUFACTURER_NAME), None);
    assert_eq!(parsed.string(ATTRIBUTE_MODEL_IDENTIFIER), Some("TH01"));
}

#[test]
fn a_string_longer_than_the_frame_is_unknown_and_does_not_panic() {
    // Length 20, 3 bytes of text.
    let records = [0x04, 0x00, 0x00, 0x42, 20, b'a', b'b', b'c'];
    let parsed = parse_read_attributes_response(&response(1, &records)).unwrap();
    assert_eq!(parsed.string(ATTRIBUTE_MANUFACTURER_NAME), None);
    // Cut in the middle of a record header, or right after the type: no panic either.
    for cut in 3..records.len() {
        let _ = parse_read_attributes_response(&response(1, &records[..cut]));
    }
}

#[test]
fn a_default_response_is_not_a_read_response() {
    assert_eq!(
        parse_read_attributes_response(&[0x18, 0x2A, 0x0B, 0x00, 0x00]),
        Err(ZclError::NotReadResponse)
    );
    assert_eq!(
        parse_read_attributes_response(&[0x18]),
        Err(ZclError::TooShort)
    );
}

/// A temperature record: attribute 0, int16.
fn temperature_record(hundredths: i16) -> (u16, u8, Vec<u8>) {
    (0x0000, 0x29, hundredths.to_le_bytes().to_vec())
}

/// The temperatures (in hundredths) the measures of a frame on cluster 0x0402 give.
fn temperatures(frame: &[u8]) -> Vec<f64> {
    let report = parse_attribute_report(frame).unwrap();
    measures::from_records(0x0402, &report.records)
        .into_iter()
        .map(|m| match m.value {
            Value::Number(n) => n,
            Value::Bool(_) => panic!("{m:?}"),
        })
        .collect()
}

#[test]
fn report_attributes_with_two_temperature_records_gives_two_values() {
    let frame = report_frame(7, &[temperature_record(1952), temperature_record(1960)]);
    let report = parse_attribute_report(&frame).unwrap();
    assert_eq!(report.tsn, 7);
    assert_eq!(report.records.len(), 2);
    assert_eq!(report.records[0].data_type, 0x29);
    assert_eq!(report.records[0].value, 1952i16.to_le_bytes().to_vec());
    assert_eq!(temperatures(&frame), vec![19.52, 19.6]);
}

#[test]
fn a_report_cut_in_the_middle_of_a_record_gives_the_complete_records_and_does_not_panic() {
    let frame = report_frame(7, &[temperature_record(1952), temperature_record(1960)]);
    for cut in 0..frame.len() {
        let _ = parse_attribute_report(&frame[..cut]);
    }
    let cut = &frame[..frame.len() - 1];
    assert_eq!(temperatures(cut), vec![19.52]);
}

#[test]
fn a_report_whose_first_record_is_a_string_then_a_temperature_gives_the_temperature() {
    let mut records = vec![(0x0004, 0x42, vec![3, b'a', b'b', b'c'])];
    records.push(temperature_record(2100));
    let frame = report_frame(1, &records);
    let report = parse_attribute_report(&frame).unwrap();
    assert_eq!(report.records.len(), 2);
    assert_eq!(temperatures(&frame), vec![21.0]);
    // A string with a 2-byte length (0x44) is skipped the same way.
    let records = [
        (0x0004, 0x44, vec![2, 0, b'a', b'b']),
        temperature_record(2100),
    ];
    assert_eq!(temperatures(&report_frame(1, &records)), vec![21.0]);
}

#[test]
fn a_report_with_a_record_of_unknown_length_then_a_temperature_gives_nothing_after_it() {
    // 0x48 is an array: its size is not known.
    let records = [
        temperature_record(1000),
        (0x0010, 0x48, vec![0x20, 0x01, 0x00, 0x05]),
        temperature_record(2100),
    ];
    let frame = report_frame(1, &records);
    assert_eq!(temperatures(&frame), vec![10.0]);
}

#[test]
fn a_read_attributes_response_record_with_status_86_then_a_temperature_record_gives_the_temperature()
 {
    let mut frame = vec![0x18, 5, 0x01];
    frame.extend_from_slice(&[0x01, 0x00, 0x86]); // attribute 1, unsupported
    frame.extend_from_slice(&[0x00, 0x00, 0x00, 0x29]); // attribute 0, success, int16
    frame.extend_from_slice(&2100i16.to_le_bytes());
    assert_eq!(temperatures(&frame), vec![21.0]);
}

#[test]
fn configure_reporting_request_for_occupancy_has_no_change_field() {
    let frame = configure_reporting_request(
        3,
        &[ReportingRecord {
            attribute: 0x0000,
            data_type: 0x18,
            min_interval: 0,
            max_interval: 3600,
            change: None,
        }],
    );
    assert_eq!(
        frame,
        vec![
            0x10, 3, 0x06, 0x00, 0x00, 0x00, 0x18, 0x00, 0x00, 0x10, 0x0E
        ]
    );
}

#[test]
fn configure_reporting_request_for_battery_percentage_has_a_uint8_change_of_2() {
    let frame = configure_reporting_request(
        4,
        &[ReportingRecord {
            attribute: 0x0021,
            data_type: 0x20,
            min_interval: 3600,
            max_interval: 62_000,
            change: Some(2),
        }],
    );
    assert_eq!(
        frame,
        vec![
            0x10, 4, 0x06, 0x00, 0x21, 0x00, 0x20, 0x10, 0x0E, 0x30, 0xF2, 0x02
        ]
    );
}

#[test]
fn configure_reporting_response_with_the_single_byte_00_accepts_everything() {
    let response = parse_configure_reporting_response(&[0x18, 9, 0x07, 0x00]).unwrap();
    assert_eq!(response.tsn, 9);
    assert!(response.refused.is_empty());
}

#[test]
fn default_response_86_to_configure_reporting_is_a_failure_of_the_cluster() {
    let response = parse_default_response(&[0x18, 9, 0x0B, 0x06, 0x86]).unwrap();
    assert_eq!(
        response,
        DefaultResponse {
            tsn: 9,
            command: 0x06,
            status: 0x86
        }
    );
    // It is not a Configure Reporting Response.
    assert_eq!(
        parse_configure_reporting_response(&[0x18, 9, 0x0B, 0x06, 0x86]),
        Err(ZclError::WrongCommand)
    );
}

#[test]
fn configure_reporting_request_for_temperature_is_direction_0_attribute_type_29_30_3600_change_20_little_endian()
 {
    let frame = configure_reporting_request(
        0x21,
        &[ReportingRecord {
            attribute: 0x0000,
            data_type: 0x29,
            min_interval: 30,
            max_interval: 3600,
            change: Some(20),
        }],
    );
    assert_eq!(
        frame,
        vec![
            0x10, 0x21, 0x06, // control, tsn, command
            0x00, // direction
            0x00, 0x00, // attribute
            0x29, // type
            0x1E, 0x00, // min 30
            0x10, 0x0E, // max 3600
            0x14, 0x00, // change 20
        ]
    );
}

#[test]
fn configure_reporting_response_all_success_is_accepted_and_one_failure_names_its_attribute() {
    let all = parse_configure_reporting_response(&[
        0x18, 1, 0x07, 0x00, 0x00, 0x21, 0x00, 0x00, 0x00, 0x20, 0x00,
    ])
    .unwrap();
    assert!(all.refused.is_empty());
    let one = parse_configure_reporting_response(&[
        0x18, 1, 0x07, 0x00, 0x00, 0x21, 0x00, 0x8C, 0x00, 0x20, 0x00,
    ])
    .unwrap();
    assert_eq!(one.refused, vec![(0x0020, 0x8C)]);
}

#[test]
fn ias_zone_status_change_0001_is_an_alarm_and_0004_a_tamper() {
    // Cluster-specific, from the server, command 0: zone status, extended status, zone id, delay.
    let frame = |status: u16| {
        let mut f = vec![0x09, 1, 0x00];
        f.extend_from_slice(&status.to_le_bytes());
        f.extend_from_slice(&[0, 0, 0, 0]);
        f
    };
    let kinds = |status: u16| {
        let zone = parse_zone_status_change(&frame(status)).unwrap();
        measures::from_zone_status(zone)
            .into_iter()
            .map(|m| (m.kind, m.value))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        kinds(0x0001),
        vec![("alarm", Value::Bool(true)), ("tamper", Value::Bool(false))]
    );
    assert_eq!(
        kinds(0x0004),
        vec![("alarm", Value::Bool(false)), ("tamper", Value::Bool(true))]
    );
    assert_eq!(
        parse_zone_status_change(&report_frame(1, &[])),
        Err(ZclError::WrongCommand)
    );
}

#[test]
fn the_default_response_to_a_report_echoes_the_tsn_and_is_none_when_the_device_disabled_it() {
    // A report that asks for one: from the server (0x08), default response not disabled.
    let report = report_frame_with_control(0x08, 0x42, &[temperature_record(1952)]);
    assert_eq!(
        default_response(0x0402, &report),
        Some(vec![0x10, 0x42, 0x0B, 0x0A, 0x00])
    );
    // The device disabled it (0x18): none.
    assert_eq!(
        default_response(0x0402, &temperature_report(0x42, 1952)),
        None
    );
    // A manufacturer-specific frame: the code is echoed.
    let mut with_code = vec![0x0C, 0x34, 0x12, 0x43, 0x0A];
    with_code.extend_from_slice(&[0x00, 0x00, 0x29, 0xA0, 0x07]);
    assert_eq!(
        default_response(0x0402, &with_code),
        Some(vec![0x14, 0x34, 0x12, 0x43, 0x0B, 0x0A, 0x00])
    );
    // A Default Response never gets one.
    assert_eq!(default_response(0x0402, &[0x08, 1, 0x0B, 0x06, 0x00]), None);
}

#[test]
fn a_single_failure_status_byte_is_a_failure_of_the_whole_command() {
    let response = parse_configure_reporting_response(&[0x18, 9, 0x07, 0x86]).unwrap();
    assert_eq!(response.failed, Some(0x86));
    assert!(response.refused.is_empty());
    let accepted = parse_configure_reporting_response(&[0x18, 9, 0x07, 0x00]).unwrap();
    assert_eq!(accepted.failed, None);
}

#[test]
fn manufacturer_specific_frames_give_no_attributes_and_no_zone_status() {
    // The standard report of a temperature, with the manufacturer-specific bit and a code.
    let mut report = vec![0x1C, 0x34, 0x12, 0x01, 0x0A];
    report.extend_from_slice(&[0x00, 0x00, 0x29, 0xA0, 0x07]);
    assert_eq!(
        parse_attribute_report(&report),
        Err(ZclError::ManufacturerSpecific)
    );
    let zone = [0x0D, 0x34, 0x12, 0x01, 0x00, 0x01, 0x00, 0, 0, 0, 0];
    assert_eq!(
        parse_zone_status_change(&zone),
        Err(ZclError::ManufacturerSpecific)
    );
}

#[test]
fn the_default_response_status_follows_the_kind_of_command() {
    // Read Attributes request from a device (client to server, default response not disabled).
    assert_eq!(
        default_response(0x0000, &[0x00, 0x21, 0x00, 0x00, 0x00]),
        Some(vec![0x18, 0x21, 0x0B, 0x00, 0x82])
    );
    // A cluster-specific command sent to us (a toggle of the on/off cluster).
    assert_eq!(
        default_response(0x0006, &[0x01, 0x22, 0x02]),
        Some(vec![0x18, 0x22, 0x0B, 0x02, 0x81])
    );
    // A cluster-specific notification from the device: success.
    assert_eq!(
        default_response(0x0500, &[0x09, 0x23, 0x00, 0x01, 0x00, 0, 0, 0, 0]),
        Some(vec![0x10, 0x23, 0x0B, 0x00, 0x00])
    );
    // A response (Read Attributes Response) asking for one: success.
    assert_eq!(
        default_response(0x0000, &[0x08, 0x24, 0x01]),
        Some(vec![0x10, 0x24, 0x0B, 0x01, 0x00])
    );
    // A command that has its own response (IAS Zone Enroll Request): none.
    assert_eq!(
        default_response(0x0500, &[0x09, 0x25, 0x01, 0x15, 0x00, 0x34, 0x12]),
        None
    );
}

// The Default Response the coordinator sends, against the scripted radio.

const SENDER: u16 = 0x2222;

fn started(protocol: u8) -> Harness {
    let mut ncp = Ncp::recording();
    ncp.protocol = protocol;
    let mut h = harness(ncp);
    h.radio.identify().unwrap();
    h.radio
        .start_network(
            &NetworkSettings::default(),
            &mut |buffer: &mut [u8]| -> Result<(), String> {
                buffer.fill(0x11);
                if buffer.len() == 2 {
                    buffer.copy_from_slice(&[0x2B, 0x1A]);
                }
                Ok(())
            },
            &mut |_| Ok(()),
        )
        .unwrap();
    h
}

fn poll(h: &mut Harness) {
    let end = h.clock.get() + 500;
    while h.clock.get() < end {
        h.radio.poll_pairing().unwrap();
    }
}

fn sent(h: &Harness) -> Vec<SentMessage> {
    h.ncp.borrow().sent.clone()
}

#[test]
fn a_default_response_received_gets_nothing_back() {
    let mut h = started(13);
    // From the server, default response not disabled, even so.
    h.ncp
        .borrow_mut()
        .inject(SENDER, 0x0104, 0x0402, 1, &[0x08, 0x10, 0x0B, 0x06, 0x00]);
    poll(&mut h);
    assert!(sent(&h).is_empty());
}

#[test]
fn the_same_report_received_twice_with_the_same_tsn_gets_one_default_response() {
    let mut h = started(13);
    let report = report_frame_with_control(0x08, 0x42, &[temperature_record(1952)]);
    for _ in 0..2 {
        h.ncp
            .borrow_mut()
            .inject(SENDER, 0x0104, 0x0402, 3, &report);
        poll(&mut h);
    }
    let sent = sent(&h);
    assert_eq!(sent.len(), 1, "{sent:?}");
    // To the sender, on the cluster, from the endpoint addressed (1) to the endpoint that sent (3).
    assert_eq!(sent[0].destination, SENDER);
    assert_eq!((sent[0].profile, sent[0].cluster), (0x0104, 0x0402));
    assert_eq!(
        (sent[0].source_endpoint, sent[0].destination_endpoint),
        (1, 3)
    );
    assert_eq!(sent[0].message, vec![0x10, 0x42, 0x0B, 0x0A, 0x00]);
    // Another transaction number gets its own.
    let next = report_frame_with_control(0x08, 0x43, &[temperature_record(1952)]);
    h.ncp.borrow_mut().inject(SENDER, 0x0104, 0x0402, 3, &next);
    poll(&mut h);
    assert_eq!(self::sent(&h).len(), 2);
}

#[test]
fn a_report_with_default_response_disabled_or_a_broadcast_gets_no_default_response() {
    let mut h = started(13);
    h.ncp.borrow_mut().inject(
        SENDER,
        0x0104,
        0x0402,
        1,
        &temperature_report(0x42, 1952), // 0x18: disabled
    );
    let asks = report_frame_with_control(0x08, 0x44, &[temperature_record(1952)]);
    h.ncp
        .borrow_mut()
        .inject_kind(0x04, SENDER, 0x0104, 0x0402, 1, &asks); // broadcast
    h.ncp
        .borrow_mut()
        .inject_kind(0x02, SENDER, 0x0104, 0x0402, 1, &asks); // multicast
    poll(&mut h);
    assert!(sent(&h).is_empty());
}
