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
