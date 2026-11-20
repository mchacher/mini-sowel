//! ZCL Read Attributes: bytes in, types out.

use mini_sowel_core::radio::zcl::*;

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
