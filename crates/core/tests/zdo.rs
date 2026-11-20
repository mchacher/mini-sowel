//! ZDO messages: bytes in, types out.

use mini_sowel_core::radio::zdo::*;

#[test]
fn device_announce_payload_gives_node_address_and_eui64_most_significant_first() {
    let mut message = vec![0x01, 0x3D, 0x5C];
    message.extend_from_slice(&[0x78, 0x56, 0x34, 0x12, 0x00, 0x4B, 0x12, 0x00]);
    message.push(0x80); // capabilities
    let announce = parse_device_announce(&message).unwrap();
    assert_eq!(announce.nwk, 0x5C3D);
    assert_eq!(
        announce.eui64,
        [0x00, 0x12, 0x4B, 0x00, 0x12, 0x34, 0x56, 0x78]
    );
    assert_eq!(
        parse_device_announce(&message[..10]),
        Err(ZdoError::TooShort)
    );
}

#[test]
fn active_endpoints_request_is_the_transaction_then_the_node_address_little_endian() {
    assert_eq!(
        active_endpoints_request(0x21, 0x5C3D),
        vec![0x21, 0x3D, 0x5C]
    );
}

#[test]
fn active_endpoints_response_with_success_gives_the_endpoints() {
    let message = [0x21, 0x00, 0x3D, 0x5C, 0x02, 0x01, 0x0A];
    let response = parse_active_endpoints_response(&message).unwrap();
    assert_eq!(response.tsn, 0x21);
    assert_eq!(response.nwk, 0x5C3D);
    assert_eq!(response.endpoints, vec![1, 10]);
}

#[test]
fn active_endpoints_response_with_a_failure_status_or_too_short_is_an_error() {
    assert_eq!(
        parse_active_endpoints_response(&[0x21, 0x80, 0x3D, 0x5C, 0x00]),
        Err(ZdoError::Status(0x80))
    );
    // The count says 3 endpoints, the message has 1.
    assert_eq!(
        parse_active_endpoints_response(&[0x21, 0x00, 0x3D, 0x5C, 0x03, 0x01]),
        Err(ZdoError::TooShort)
    );
}
