//! ASH: codec, decoder, session. Reference byte sequences come from UG101's examples (RST,
//! RSTACK, ERROR, ACK, NAK) and were checked against the algorithm of zigbee-herdsman's ember
//! adapter (CRC-CCITT 0x1021 from 0xFFFF; LFSR seed 0x42, polynomial 0xB8).

use solite_core::radio::ash::*;

fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn data(frm: u8, ack: u8, retransmit: bool, payload: &[u8]) -> AshFrame {
    AshFrame::Data {
        frm,
        ack,
        retransmit,
        payload: payload.to_vec(),
    }
}

fn decode_all(bytes: &[u8]) -> Vec<DecodeEvent> {
    Decoder::new().feed(bytes)
}

// ---------------------------------------------------------------- codec

#[test]
fn rst_is_encoded_with_the_cancel_byte_first() {
    assert_eq!(hex(&encode(&AshFrame::Rst)), "1A C0 38 BC 7E");
}

#[test]
fn rstack_version_2_power_on_is_decoded() {
    let events = decode_all(&[0xC1, 0x02, 0x02, 0x9B, 0x7B, 0x7E]);
    assert_eq!(
        events,
        vec![DecodeEvent::Frame(AshFrame::RstAck {
            version: 2,
            code: 2
        })]
    );
}

#[test]
fn error_version_2_code_0x51_is_decoded() {
    let events = decode_all(&[0xC2, 0x02, 0x51, 0xA8, 0xBD, 0x7E]);
    assert_eq!(
        events,
        vec![DecodeEvent::Frame(AshFrame::Error {
            version: 2,
            code: 0x51
        })]
    );
}

#[test]
fn ack_with_ack_number_1_is_encoded() {
    let bytes = encode(&AshFrame::Ack {
        ack: 1,
        not_ready: false,
    });
    assert_eq!(hex(&bytes), "81 60 59 7E");
}

#[test]
fn nak_with_ack_number_6_is_encoded() {
    let bytes = encode(&AshFrame::Nak {
        ack: 6,
        not_ready: false,
    });
    assert_eq!(hex(&bytes), "A6 34 DC 7E");
}

#[test]
fn data_frame_payload_is_randomised() {
    // Sequence 42 21 A8 54 ...: 00 00 00 02 becomes 42 21 A8 56.
    let bytes = encode(&data(0, 0, false, &[0x00, 0x00, 0x00, 0x02]));
    assert_eq!(hex(&bytes), "00 42 21 A8 56 8D EA 7E");
}

#[test]
fn legacy_version_command_frame_matches_the_reference_bytes() {
    let bytes = encode(&data(0, 0, false, &[0x00, 0x00, 0x00, 0x13]));
    assert_eq!(hex(&bytes), "00 42 21 A8 47 8F FA 7E");
}

#[test]
fn a_220_byte_data_frame_with_every_byte_value_round_trips() {
    // Two payloads of 220 bytes that together hold the 256 byte values.
    for payload in [
        (0..220u16).map(|i| i as u8).collect::<Vec<u8>>(),
        (36..256u16).map(|i| i as u8).collect::<Vec<u8>>(),
    ] {
        assert_eq!(payload.len(), 220);
        let frame = data(5, 3, true, &payload);
        assert_eq!(decode_all(&encode(&frame)), vec![DecodeEvent::Frame(frame)]);
    }
}

#[test]
fn reserved_bytes_in_a_payload_are_stuffed_and_decoded_back() {
    let payload = [0x7E, 0x7D, 0x11, 0x13, 0x18, 0x1A];
    let frame = data(0, 0, false, &payload);
    let bytes = encode(&frame);
    // Between the frame's own flag and the end there is no bare reserved byte but the flag.
    let (last, body) = bytes.split_last().unwrap();
    assert_eq!(*last, 0x7E);
    let mut iter = body.iter();
    while let Some(&b) = iter.next() {
        if b == 0x7D {
            // The byte after an escape has bit 5 flipped: it is one of the reserved bytes
            // xor 0x20.
            let next = *iter.next().unwrap();
            assert!([0x5E, 0x5D, 0x31, 0x33, 0x38, 0x3A].contains(&next));
        } else {
            assert!(![0x7E, 0x11, 0x13, 0x18, 0x1A].contains(&b));
        }
    }
    assert_eq!(decode_all(&bytes), vec![DecodeEvent::Frame(frame)]);
}

#[test]
fn crc_of_c0_is_38_bc() {
    assert_eq!(crc16(&[0xC0]), 0x38BC);
}

// -------------------------------------------------------------- decoder

#[test]
fn two_frames_in_one_chunk_give_two_frames() {
    let mut bytes = encode(&AshFrame::Ack {
        ack: 1,
        not_ready: false,
    });
    bytes.extend(encode(&AshFrame::Nak {
        ack: 6,
        not_ready: false,
    }));
    assert_eq!(
        decode_all(&bytes),
        vec![
            DecodeEvent::Frame(AshFrame::Ack {
                ack: 1,
                not_ready: false
            }),
            DecodeEvent::Frame(AshFrame::Nak {
                ack: 6,
                not_ready: false
            }),
        ]
    );
}

#[test]
fn a_frame_fed_one_byte_at_a_time_gives_one_frame() {
    let frame = data(2, 1, false, &[1, 2, 3, 4, 0x7E, 0x11]);
    let mut decoder = Decoder::new();
    let mut events = Vec::new();
    for byte in encode(&frame) {
        events.extend(decoder.feed(&[byte]));
    }
    assert_eq!(events, vec![DecodeEvent::Frame(frame)]);
}

#[test]
fn a_bad_crc_is_an_error_and_the_next_frame_is_decoded() {
    let mut bytes = encode(&AshFrame::Ack {
        ack: 1,
        not_ready: false,
    });
    bytes[1] ^= 0xFF; // corrupt the CRC
    bytes.extend(encode(&AshFrame::Ack {
        ack: 2,
        not_ready: false,
    }));
    assert_eq!(
        decode_all(&bytes),
        vec![
            DecodeEvent::Error(AshError::BadCrc),
            DecodeEvent::Frame(AshFrame::Ack {
                ack: 2,
                not_ready: false
            }),
        ]
    );
}

#[test]
fn a_cancel_byte_inside_a_frame_drops_it_and_the_next_is_decoded() {
    let mut bytes = vec![0xC1, 0x02, 0x1A];
    bytes.extend(encode(&AshFrame::Ack {
        ack: 3,
        not_ready: false,
    }));
    assert_eq!(
        decode_all(&bytes),
        vec![
            DecodeEvent::Error(AshError::Cancelled),
            DecodeEvent::Frame(AshFrame::Ack {
                ack: 3,
                not_ready: false
            }),
        ]
    );
}

#[test]
fn a_substitute_byte_inside_a_frame_drops_it_at_the_flag() {
    let mut bytes = vec![0xC1, 0x02, 0x18, 0x02, 0x9B, 0x7B, 0x7E];
    bytes.extend(encode(&AshFrame::Ack {
        ack: 3,
        not_ready: false,
    }));
    assert_eq!(
        decode_all(&bytes),
        vec![
            DecodeEvent::Error(AshError::Substituted),
            DecodeEvent::Frame(AshFrame::Ack {
                ack: 3,
                not_ready: false
            }),
        ]
    );
}

#[test]
fn a_frame_longer_than_223_bytes_is_an_error_and_the_next_is_decoded() {
    // 400 bytes without a flag, then a flag, then a good frame.
    let mut bytes = vec![0x01; 400];
    bytes.push(0x7E);
    bytes.extend(encode(&AshFrame::Ack {
        ack: 4,
        not_ready: false,
    }));
    assert_eq!(
        decode_all(&bytes),
        vec![
            DecodeEvent::Error(AshError::TooLong),
            DecodeEvent::Frame(AshFrame::Ack {
                ack: 4,
                not_ready: false
            }),
        ]
    );
}

#[test]
fn an_unknown_control_byte_is_an_error() {
    // 0xC3 is not a defined frame; the CRC is valid so only the control byte is wrong.
    let crc = crc16(&[0xC3]).to_be_bytes();
    let bytes = [0xC3, crc[0], crc[1], 0x7E];
    assert_eq!(
        decode_all(&bytes),
        vec![DecodeEvent::Error(AshError::UnknownControl(0xC3))]
    );
}

#[test]
fn stray_xon_and_xoff_are_discarded_inside_and_between_frames() {
    let clean = encode(&AshFrame::Ack {
        ack: 1,
        not_ready: false,
    });
    let mut bytes = vec![0x11, 0x13];
    bytes.push(clean[0]);
    bytes.push(0x13);
    bytes.extend_from_slice(&clean[1..]);
    bytes.push(0x11);
    assert_eq!(
        decode_all(&bytes),
        vec![DecodeEvent::Frame(AshFrame::Ack {
            ack: 1,
            not_ready: false
        })]
    );
}

// -------------------------------------------------------------- session

/// RSTACK with the power-on reset code (the radio started by itself).
const RSTACK: [u8; 6] = [0xC1, 0x02, 0x02, 0x9B, 0x7B, 0x7E];

/// RSTACK with the software reset code 0x0B: the answer to our RST.
fn rstack_software() -> Vec<u8> {
    encode(&AshFrame::RstAck {
        version: 2,
        code: 0x0B,
    })
}

fn connected() -> Session {
    let mut session = Session::new();
    session.reset(0);
    session.receive(&rstack_software(), 0);
    assert!(session.is_connected());
    session
}

fn ack_bytes(ack: u8) -> Vec<u8> {
    encode(&AshFrame::Ack {
        ack,
        not_ready: false,
    })
}

fn frames(bytes: &[u8]) -> Vec<AshFrame> {
    decode_all(bytes)
        .into_iter()
        .filter_map(|e| match e {
            DecodeEvent::Frame(f) => Some(f),
            DecodeEvent::Error(_) => None,
        })
        .collect()
}

#[test]
fn reset_writes_rst_and_the_session_is_connected_after_rstack() {
    let mut session = Session::new();
    assert_eq!(hex(&session.reset(0)), "1A C0 38 BC 7E");
    assert!(!session.is_connected());
    let out = session.receive(&rstack_software(), 10);
    assert_eq!(out.events, vec![Event::Connected]);
    assert!(session.is_connected());
}

#[test]
fn sending_before_connected_is_refused_and_writes_nothing() {
    let mut session = Session::new();
    assert_eq!(session.send(vec![1, 2, 3], 0), Err(AshError::NotConnected));
    session.reset(0);
    assert_eq!(session.send(vec![1, 2, 3], 0), Err(AshError::NotConnected));
}

#[test]
fn send_uses_frame_numbers_in_turn_and_wraps_from_7_to_0() {
    let mut session = connected();
    let mut seen = Vec::new();
    for i in 0..9u8 {
        let bytes = session.send(vec![i, i, i], 0).unwrap();
        match &frames(&bytes)[0] {
            AshFrame::Data { frm, ack, .. } => seen.push((*frm, *ack)),
            other => panic!("not a DATA frame: {other:?}"),
        }
        session.receive(&ack_bytes((seen.last().unwrap().0 + 1) % 8), 0);
    }
    let numbers: Vec<u8> = seen.iter().map(|(f, _)| *f).collect();
    assert_eq!(numbers, vec![0, 1, 2, 3, 4, 5, 6, 7, 0]);
    assert!(seen.iter().all(|(_, ack)| *ack == 0));
}

#[test]
fn incoming_data_in_sequence_is_acknowledged_and_delivered() {
    let mut session = connected();
    let out = session.receive(&encode(&data(0, 0, false, &[9, 8, 7])), 0);
    assert_eq!(hex(&out.write), hex(&ack_bytes(1)));
    assert_eq!(session.next_payload(), Some(vec![9, 8, 7]));
    let out = session.receive(&encode(&data(1, 0, false, &[6, 5, 4])), 0);
    assert_eq!(hex(&out.write), hex(&ack_bytes(2)));
    assert_eq!(session.next_payload(), Some(vec![6, 5, 4]));
}

#[test]
fn incoming_data_out_of_sequence_gets_a_nak_and_is_not_delivered() {
    let mut session = connected();
    let out = session.receive(&encode(&data(3, 0, false, &[9, 8, 7])), 0);
    assert_eq!(
        frames(&out.write),
        vec![AshFrame::Nak {
            ack: 0,
            not_ready: false
        }]
    );
    assert_eq!(session.next_payload(), None);
}

#[test]
fn the_same_data_received_twice_is_acknowledged_twice_and_delivered_once() {
    let mut session = connected();
    let first = session.receive(&encode(&data(0, 0, false, &[1, 2, 3])), 0);
    let second = session.receive(&encode(&data(0, 0, true, &[1, 2, 3])), 0);
    assert_eq!(hex(&first.write), hex(&ack_bytes(1)));
    assert_eq!(hex(&second.write), hex(&ack_bytes(1)));
    assert_eq!(session.next_payload(), Some(vec![1, 2, 3]));
    assert_eq!(session.next_payload(), None);
}

#[test]
fn no_ack_within_1_6_seconds_resends_with_the_retransmit_flag() {
    let mut session = connected();
    session.send(vec![1, 2, 3], 100).unwrap();
    assert!(session.poll(1699).write.is_empty());
    let out = session.poll(1700);
    assert_eq!(frames(&out.write), vec![data(0, 0, true, &[1, 2, 3])]);
}

#[test]
fn five_resends_then_link_lost_on_the_sixth_timeout() {
    let mut session = connected();
    session.send(vec![1, 2, 3], 0).unwrap();
    let mut now = 0;
    for _ in 0..5 {
        now += 1600;
        let out = session.poll(now);
        assert_eq!(frames(&out.write), vec![data(0, 0, true, &[1, 2, 3])]);
        assert!(out.events.is_empty());
    }
    now += 1600;
    let out = session.poll(now);
    assert!(out.write.is_empty());
    assert_eq!(out.events, vec![Event::Failed(AshFailure::LinkLost)]);
    assert_eq!(
        session.send(vec![1, 2, 3], now),
        Err(AshError::NotConnected)
    );
}

#[test]
fn an_error_frame_is_a_failure_with_its_code() {
    let mut session = connected();
    let out = session.receive(&[0xC2, 0x02, 0x51, 0xA8, 0xBD, 0x7E], 0);
    assert_eq!(
        out.events,
        vec![Event::Failed(AshFailure::ErrorFrame { code: 0x51 })]
    );
}

#[test]
fn rstack_once_connected_is_a_failure_with_its_reset_code() {
    let mut session = connected();
    let out = session.receive(&RSTACK, 0);
    assert_eq!(
        out.events,
        vec![Event::Failed(AshFailure::UnexpectedReset { code: 2 })]
    );
}

#[test]
fn nine_payloads_delivered_and_none_read_drop_the_oldest_and_report_it() {
    let mut session = connected();
    let mut events = Vec::new();
    for i in 0..9u8 {
        let out = session.receive(&encode(&data(i % 8, 0, false, &[i, i, i])), 0);
        events.extend(out.events);
    }
    assert_eq!(events, vec![Event::Error(AshError::DeliveredOverflow)]);
    let mut kept = Vec::new();
    while let Some(p) = session.next_payload() {
        kept.push(p[0]);
    }
    assert_eq!(kept, vec![1, 2, 3, 4, 5, 6, 7, 8]);
}

#[test]
fn a_nak_for_the_frame_in_flight_resends_it() {
    let mut session = connected();
    session.send(vec![4, 5, 6], 0).unwrap();
    let out = session.receive(
        &encode(&AshFrame::Nak {
            ack: 0,
            not_ready: false,
        }),
        50,
    );
    assert_eq!(frames(&out.write), vec![data(0, 0, true, &[4, 5, 6])]);
}

#[test]
fn no_rstack_within_5_seconds_of_rst_is_a_failure() {
    let mut session = Session::new();
    session.reset(1000);
    assert!(session.poll(5999).events.is_empty());
    assert_eq!(
        session.poll(6000).events,
        vec![Event::Failed(AshFailure::NoResetAnswer)]
    );
}

#[test]
fn a_second_payload_waits_for_the_ack_of_the_first() {
    let mut session = connected();
    assert!(!session.send(vec![1, 1, 1], 0).unwrap().is_empty());
    assert!(session.send(vec![2, 2, 2], 0).unwrap().is_empty());
    let out = session.receive(&ack_bytes(1), 10);
    assert_eq!(frames(&out.write), vec![data(1, 0, false, &[2, 2, 2])]);
}

#[test]
fn error_frame_and_power_on_rstack_before_our_rstack_are_ignored() {
    let mut session = Session::new();
    session.reset(0);
    let error = [0xC2, 0x02, 0x51, 0xA8, 0xBD, 0x7E];
    assert!(session.receive(&error, 10).events.is_empty());
    assert!(session.receive(&RSTACK, 20).events.is_empty());
    assert!(!session.is_connected());
    let out = session.receive(&rstack_software(), 30);
    assert_eq!(out.events, vec![Event::Connected]);
}

#[test]
fn ug101_data_example_frm_2_ack_5_is_encoded_and_decoded() {
    let frame = data(2, 5, false, &[0x00, 0x00, 0x00, 0x02]);
    let bytes = [0x25, 0x42, 0x21, 0xA8, 0x56, 0xA6, 0x09, 0x7E];
    assert_eq!(hex(&encode(&frame)), hex(&bytes));
    assert_eq!(decode_all(&bytes), vec![DecodeEvent::Frame(frame)]);
}

#[test]
fn out_of_sequence_data_twice_gets_one_nak_then_silence_until_one_in_sequence() {
    let mut session = connected();
    let first = session.receive(&encode(&data(3, 0, false, &[1, 2, 3])), 0);
    assert_eq!(
        frames(&first.write),
        vec![AshFrame::Nak {
            ack: 0,
            not_ready: false
        }]
    );
    let second = session.receive(&encode(&data(4, 0, false, &[1, 2, 3])), 0);
    assert!(second.write.is_empty());
    assert_eq!(session.next_payload(), None);
    // The frame in sequence is accepted and acknowledged, and ends the silence.
    let ok = session.receive(&encode(&data(0, 0, false, &[7, 7, 7])), 0);
    assert_eq!(hex(&ok.write), hex(&ack_bytes(1)));
    assert_eq!(session.next_payload(), Some(vec![7, 7, 7]));
    let again = session.receive(&encode(&data(5, 0, false, &[1, 2, 3])), 0);
    assert_eq!(frames(&again.write).len(), 1);
}

#[test]
fn a_frame_dropped_by_the_decoder_while_connected_gets_one_nak() {
    let mut session = connected();
    let mut bad = ack_bytes(1);
    bad[1] ^= 0xFF;
    let out = session.receive(&bad, 0);
    assert_eq!(
        frames(&out.write),
        vec![AshFrame::Nak {
            ack: 0,
            not_ready: false
        }]
    );
    assert_eq!(out.events, vec![Event::Error(AshError::BadCrc)]);
    // A second bad frame: still silent.
    assert!(session.receive(&bad, 0).write.is_empty());
}
