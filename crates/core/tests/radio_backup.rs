//! The text of the Zigbee network's backup (spec 021): the host's format, byte for byte, shared by
//! the host's `network.json` and the firmware's `zigbee.json`.

use solite_core::radio::backup::{is_network, pan_label, text};
use solite_core::radio::{Network, NetworkKeys};

fn network(pan_id: u16, extended: u8) -> Network {
    Network {
        pan_id,
        extended_pan_id: [extended; 8],
        channel: 15,
        formed: true,
        from_backup: false,
        keys: None,
    }
}

fn keys() -> NetworkKeys {
    NetworkKeys {
        network_key: [0xAB; 16],
        trust_center_key: [0x01; 16],
    }
}

#[test]
fn the_text_of_a_formed_network_is_the_hosts_format_byte_for_byte() {
    let expected = "{\n  \"panId\": \"0x1A2B\",\n  \"extendedPanId\": \"0xDDDDDDDDDDDDDDDD\",\n  \
                    \"channel\": 15,\n  \"networkKey\": \"ABABABABABABABABABABABABABABABAB\",\n  \
                    \"trustCenterKey\": \"01010101010101010101010101010101\"\n}\n";
    assert_eq!(text(&network(0x1A2B, 0xDD), &keys()), expected);
}

#[test]
fn is_network_is_true_for_the_same_pan_and_extended_pan_and_false_for_another() {
    let backup = text(&network(0x1A2B, 0xDD), &keys());
    assert!(is_network(&backup, &network(0x1A2B, 0xDD)));
    assert!(!is_network(&backup, &network(0x1A2C, 0xDD)));
    assert!(!is_network(&backup, &network(0x1A2B, 0xEE)));
    assert!(!is_network("not a backup", &network(0x1A2B, 0xDD)));
}

#[test]
fn the_pan_of_a_backup_names_the_file_it_is_kept_under() {
    let old = "{\n  \"panId\": \"0x4321\",\n  \"extendedPanId\": \"0x0102030405060708\"\n}\n";
    assert_eq!(pan_label(old), "4321");
    assert_eq!(pan_label("garbage"), "unknown");
}

// ---- Reading a backup back (spec 035, R7) ----

use solite_core::radio::backup::parse;

#[test]
fn a_backup_text_is_read_back_with_its_extended_pan_id_in_the_files_order() {
    let formed = Network {
        pan_id: 0x1A2B,
        extended_pan_id: [1, 2, 3, 4, 5, 6, 7, 8],
        channel: 25,
        formed: true,
        from_backup: false,
        keys: None,
    };
    let restored = parse(&text(&formed, &keys())).expect("a valid backup");
    assert_eq!(restored.pan_id, 0x1A2B);
    assert_eq!(restored.extended_pan_id, [1, 2, 3, 4, 5, 6, 7, 8]);
    assert_eq!(restored.channel, 25);
    assert_eq!(restored.keys, keys());
}

#[test]
fn a_backup_that_does_not_parse_or_holds_an_invalid_network_gives_none() {
    let good = text(&network(0x1A2B, 0xDD), &keys());
    assert!(parse(&good).is_some());
    let broken = |from: &str, to: &str| good.replace(from, to);
    for (what, text) in [
        ("empty", String::new()),
        ("garbage", "not json at all".to_string()),
        ("pan 0", broken("0x1A2B", "0x0000")),
        ("pan FFFF", broken("0x1A2B", "0xFFFF")),
        ("pan not hex", broken("0x1A2B", "0xZZZZ")),
        (
            "extended all 0",
            broken("DDDDDDDDDDDDDDDD", "0000000000000000"),
        ),
        (
            "extended all FF",
            broken("DDDDDDDDDDDDDDDD", "FFFFFFFFFFFFFFFF"),
        ),
        ("extended short", broken("DDDDDDDDDDDDDDDD", "DDDD")),
        ("channel 10", broken("\"channel\": 15", "\"channel\": 10")),
        ("channel 27", broken("\"channel\": 15", "\"channel\": 27")),
        ("channel missing", broken("\"channel\": 15,", "")),
        (
            "network key short",
            broken("ABABABABABABABABABABABABABABABAB", "ABAB"),
        ),
        ("trust key missing", broken("trustCenterKey", "other")),
    ] {
        assert_eq!(parse(&text), None, "{what}");
    }
}
