//! ZDO messages: bytes in, types out.

use solite_core::radio::zdo::*;

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

#[test]
fn simple_descriptor_request_is_the_tsn_node_address_and_endpoint_and_its_response_lists_the_clusters()
 {
    assert_eq!(
        simple_descriptor_request(0x30, 0x5C3D, 1),
        vec![0x30, 0x3D, 0x5C, 0x01]
    );
    let mut message = vec![0x30, 0x00, 0x3D, 0x5C];
    let mut descriptor = vec![0x01, 0x04, 0x01, 0x02, 0x03, 0x00]; // ep, profile, device id, version
    descriptor.push(4);
    for cluster in [0x0000u16, 0x0001, 0x0402, 0x0405] {
        descriptor.extend_from_slice(&cluster.to_le_bytes());
    }
    descriptor.push(1);
    descriptor.extend_from_slice(&0x0019u16.to_le_bytes());
    message.push(descriptor.len() as u8);
    message.extend(descriptor);
    let parsed = parse_simple_descriptor_response(&message).unwrap();
    assert_eq!(parsed.tsn, 0x30);
    assert_eq!(parsed.nwk, 0x5C3D);
    assert_eq!(parsed.endpoint, 1);
    assert_eq!(parsed.profile, 0x0104);
    assert_eq!(parsed.input_clusters, vec![0x0000, 0x0001, 0x0402, 0x0405]);
    assert_eq!(parsed.output_clusters, vec![0x0019]);
    // A failure status, and a message cut short.
    assert_eq!(
        parse_simple_descriptor_response(&[0x30, 0x81, 0x3D, 0x5C, 0x00]),
        Err(ZdoError::Status(0x81))
    );
    assert_eq!(
        parse_simple_descriptor_response(&message[..message.len() - 3]),
        Err(ZdoError::TooShort)
    );
}

#[test]
fn bind_request_is_source_eui64_endpoint_cluster_mode_3_destination_eui64_and_endpoint_1() {
    let source = [0x00, 0x12, 0x4B, 0x00, 0x12, 0x34, 0x56, 0x78];
    let own = [0x8C, 0x6F, 0xB9, 0xFF, 0xFE, 0x12, 0x34, 0x56];
    assert_eq!(
        bind_request(0x31, source, 1, 0x0402, own, 1),
        vec![
            0x31, // tsn
            0x78, 0x56, 0x34, 0x12, 0x00, 0x4B, 0x12, 0x00, // source, little-endian
            0x01, // source endpoint
            0x02, 0x04, // cluster
            0x03, // destination: 64-bit address and endpoint
            0x56, 0x34, 0x12, 0xFE, 0xFF, 0xB9, 0x6F, 0x8C, // destination, little-endian
            0x01, // destination endpoint
        ]
    );
}

#[test]
fn bind_response_success_gives_the_tsn_and_a_failure_is_an_error() {
    assert_eq!(parse_bind_response(&[0x31, 0x00]), Ok(0x31));
    assert_eq!(
        parse_bind_response(&[0x31, 0x84]),
        Err(ZdoError::Status(0x84))
    );
    assert_eq!(parse_bind_response(&[0x31]), Err(ZdoError::TooShort));
}
