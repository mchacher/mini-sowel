//! The network setting and the link chosen at boot (spec 015 R5).

use solite_core::network::{
    Backoff, ConsoleCommand, Fallback, Link, MAX_CONSOLE_LINE, MAX_SETTING_BYTES, MAX_WIFI_BYTES,
    NetworkSetting, WIFI_RETRY_MAX_MS, WifiRefusal, boot_lines, boot_lines_radio, choose,
    connected_line, loaded_line, no_settings_line, parse, parse_console_line, refused_line,
    retry_line, wifi_reason, wifi_settings,
};

#[test]
fn a_valid_link_is_read_without_reason() {
    for (text, setting) in [
        (r#"{"link": "wifi"}"#, NetworkSetting::Wifi),
        (r#"{"link": "ethernet"}"#, NetworkSetting::Ethernet),
        (r#"{"link": "auto"}"#, NetworkSetting::Auto),
        (r#"{"link": "wifi", "x": 1}"#, NetworkSetting::Wifi),
    ] {
        assert_eq!(parse(Some(text)), (setting, None), "{text}");
    }
}

#[test]
fn a_missing_or_wrong_setting_falls_back_to_auto_with_its_reason() {
    let long = " ".repeat(MAX_SETTING_BYTES + 1);
    let cases: Vec<(Option<&str>, Fallback)> = vec![
        (None, Fallback::Missing),
        (Some("{}"), Fallback::Missing),
        (Some(""), Fallback::Empty),
        (Some("  \n "), Fallback::Empty),
        (Some(long.as_str()), Fallback::TooLong),
        (Some("not json"), Fallback::Invalid),
        (Some("null"), Fallback::Invalid),
        (Some(r#"{"link": 5}"#), Fallback::Invalid),
        (Some(r#"{"link": "lte"}"#), Fallback::Unknown("lte".into())),
        (
            Some(r#"{"link": "WiFi"}"#),
            Fallback::Unknown("WiFi".into()),
        ),
    ];
    for (text, reason) in cases {
        assert_eq!(
            parse(text),
            (NetworkSetting::Auto, Some(reason)),
            "{text:?}"
        );
    }
}

#[test]
fn a_setting_of_exactly_the_limit_is_parsed() {
    let padding = " ".repeat(MAX_SETTING_BYTES - r#"{"link":"wifi"}"#.len());
    let text = format!(r#"{{"link":"wifi"}}{padding}"#);
    assert_eq!(text.len(), MAX_SETTING_BYTES);
    assert_eq!(parse(Some(&text)), (NetworkSetting::Wifi, None));
}

#[test]
fn every_reason_has_a_log_line() {
    let (_, reason) = parse(Some(r#"{"link": "WiFi"}"#));
    assert_eq!(
        reason.unwrap().message(),
        r#"network: setting unknown "WiFi", auto"#
    );
    assert_eq!(Fallback::Missing.message(), "network: no setting, auto");
    assert_eq!(Fallback::Empty.message(), "network: setting empty, auto");
    assert_eq!(
        Fallback::Invalid.message(),
        "network: setting invalid, auto"
    );
    assert_eq!(
        Fallback::TooLong.message(),
        "network: setting over 256 bytes, auto"
    );
}

#[test]
fn auto_follows_the_cable_and_the_other_settings_win_over_it() {
    assert_eq!(choose(NetworkSetting::Auto, true), Link::Ethernet);
    assert_eq!(choose(NetworkSetting::Auto, false), Link::Wifi);
    assert_eq!(choose(NetworkSetting::Wifi, true), Link::Wifi);
    assert_eq!(choose(NetworkSetting::Ethernet, false), Link::Ethernet);
}

#[test]
fn the_boot_lines_give_the_reason_then_the_choice() {
    use solite_core::network::boot_lines;
    assert_eq!(
        boot_lines(Some(r#"{"link": "auto"}"#), false),
        ["network: wifi (setting auto), no link started"]
    );
    assert_eq!(
        boot_lines(None, false),
        [
            "network: no setting, auto",
            "network: wifi (setting auto), no link started"
        ]
    );
    assert_eq!(
        boot_lines(Some(r#"{"link": "ethernet"}"#), false),
        ["network: ethernet (setting ethernet), no link started"]
    );
    assert_eq!(
        boot_lines(Some(r#"{"link": "WiFi"}"#), true),
        [
            r#"network: setting unknown "WiFi", auto"#,
            "network: ethernet (setting auto), no link started"
        ]
    );
}

// ---- The Wi-Fi settings (spec 022 R1) ----

const SSID: &str = "Zz-Distinct-Net";
const PASSWORD: &str = "Zq9-distinct-pass";

fn settings_text(ssid: &str, password: &str) -> String {
    format!(r#"{{"ssid":"{ssid}","password":"{password}"}}"#)
}

#[test]
fn a_nominal_wifi_settings_text_is_read() {
    let settings = wifi_settings(r#"{"ssid":"Home","password":"abcdefgh"}"#).unwrap();
    assert_eq!(settings.ssid(), "Home");
    assert_eq!(settings.password(), "abcdefgh");
}

#[test]
fn an_open_network_has_an_empty_password_and_says_so() {
    let settings = wifi_settings(r#"{"ssid":"Cafe","password":""}"#).unwrap();
    assert_eq!(settings.password(), "");
    assert!(loaded_line(&settings).contains("open network"));
}

#[test]
fn a_raw_key_of_64_hex_digits_is_accepted() {
    let key = "0123456789abcdefABCDEF0123456789abcdef0123456789abcdef0123456789";
    assert_eq!(key.len(), 64);
    assert!(wifi_settings(&settings_text("Home", key)).is_ok());
}

#[test]
fn a_missing_wifi_file_has_its_line() {
    assert_eq!(
        no_settings_line(),
        "wifi: no settings (/data/wifi.json missing), no link started: sh scripts/box.sh wifi"
    );
}

#[test]
fn empty_and_blank_wifi_texts_are_refused_as_empty() {
    assert_eq!(wifi_settings(""), Err(WifiRefusal::Empty));
    assert_eq!(wifi_settings(" \n\t "), Err(WifiRefusal::Empty));
}

#[test]
fn a_wifi_text_over_256_bytes_is_refused_as_too_long_before_parsing() {
    // At the bound: parsed; one byte past it: refused as too long.
    let json = r#"{"ssid":"Home","password":"abcdefgh"}"#;
    let exact = format!("{json}{}", " ".repeat(MAX_WIFI_BYTES - json.len()));
    assert_eq!(exact.len(), MAX_WIFI_BYTES);
    assert!(wifi_settings(&exact).is_ok());
    let over = format!("{exact} ");
    assert_eq!(wifi_settings(&over), Err(WifiRefusal::TooLong));
}

#[test]
fn a_wifi_text_that_is_not_an_object_of_two_strings_is_invalid() {
    for text in [
        "not json",
        "[]",
        r#"["ssid","password"]"#,
        r#"{"ssid":5,"password":"abcdefgh"}"#,
        r#"{"ssid":"Home"}"#,
    ] {
        assert_eq!(wifi_settings(text), Err(WifiRefusal::Invalid), "{text}");
    }
}

#[test]
fn an_extra_wifi_field_is_invalid() {
    assert_eq!(
        wifi_settings(r#"{"ssid":"Home","password":"abcdefgh","link":"wifi"}"#),
        Err(WifiRefusal::Invalid)
    );
}

#[test]
fn the_ssid_is_1_to_32_bytes() {
    assert_eq!(
        wifi_settings(&settings_text("", "abcdefgh")),
        Err(WifiRefusal::InvalidSsid)
    );
    assert_eq!(
        wifi_settings(&settings_text(&"a".repeat(33), "abcdefgh")),
        Err(WifiRefusal::InvalidSsid)
    );
    // 16 two-byte characters: 32 bytes.
    let utf8 = "é".repeat(16);
    assert_eq!(utf8.len(), 32);
    assert_eq!(
        wifi_settings(&settings_text(&utf8, "abcdefgh"))
            .unwrap()
            .ssid(),
        utf8
    );
}

#[test]
fn the_password_is_empty_8_to_63_printable_or_64_hex() {
    let bad = |p: &str| wifi_settings(&settings_text("Home", p));
    assert_eq!(bad("abcdefg"), Err(WifiRefusal::InvalidPassword));
    assert_eq!(bad(&"z".repeat(64)), Err(WifiRefusal::InvalidPassword));
    assert!(bad(&"p".repeat(63)).is_ok());
    assert!(bad(&"p".repeat(8)).is_ok());
    assert_eq!(
        wifi_settings("{\"ssid\":\"Home\",\"password\":\"abcdefg\\u0007h\"}"),
        Err(WifiRefusal::InvalidPassword)
    );
}

#[test]
fn neither_debug_nor_any_wifi_line_shows_the_password_or_the_ssid() {
    let settings = wifi_settings(&settings_text(SSID, PASSWORD)).unwrap();
    let lines = [
        format!("{settings:?}"),
        loaded_line(&settings),
        no_settings_line(),
        refused_line(WifiRefusal::InvalidPassword),
        retry_line("wrong password or refused", false, 1000),
        connected_line("192.168.1.20".parse().unwrap(), "solite-a1b2c3", 6, -60),
        format!("{:?}", ConsoleCommand::WifiSet(settings.clone())),
    ];
    for line in lines {
        assert!(!line.contains(PASSWORD), "{line}");
        assert!(!line.contains(SSID), "{line}");
    }
    assert!(format!("{settings:?}").contains("password: 17 characters"));
}

// ---- The link's lines and retries (spec 022 R2) ----

#[test]
fn the_backoff_doubles_up_to_60_seconds_and_resets() {
    let mut backoff = Backoff::new();
    let seconds: Vec<u64> = (0..8).map(|_| backoff.next_delay_ms() / 1000).collect();
    assert_eq!(seconds, [1, 2, 4, 8, 16, 32, 60, 60]);
    assert_eq!(WIFI_RETRY_MAX_MS, 60_000);
    backoff.reset();
    assert_eq!(backoff.next_delay_ms(), 1000);
}

#[test]
fn a_disconnection_code_is_named() {
    assert_eq!(wifi_reason(201), "network not found");
    for code in [15, 202, 204, 2] {
        assert_eq!(wifi_reason(code), "wrong password or refused", "{code}");
    }
    assert_eq!(wifi_reason(200), "beacon lost");
    assert_eq!(wifi_reason(999), "reason 999");
}

#[test]
fn the_retry_line_says_not_connected_before_a_first_connection_and_lost_after() {
    assert_eq!(
        retry_line("network not found", false, 4000),
        "wifi: not connected (network not found), retry in 4 s"
    );
    assert_eq!(
        retry_line("beacon lost", true, 1000),
        "wifi: lost (beacon lost), retry in 1 s"
    );
}

#[test]
fn the_connected_line_names_address_name_channel_and_signal() {
    assert_eq!(
        connected_line("192.168.1.20".parse().unwrap(), "solite-a1b2c3", 6, -61),
        "wifi: connected, address 192.168.1.20, name solite-a1b2c3, channel 6, rssi -61 dBm"
    );
}

#[test]
fn the_radio_image_starts_wifi_for_auto_and_wifi() {
    assert_eq!(
        boot_lines_radio(Some(r#"{"link":"wifi"}"#), false),
        ["network: wifi (setting wifi)"]
    );
    assert_eq!(
        boot_lines_radio(None, false),
        [
            "network: no setting, auto".to_string(),
            "network: wifi (setting auto)".to_string()
        ]
    );
    for line in boot_lines_radio(Some(r#"{"link":"auto"}"#), false) {
        assert!(!line.contains("no link started"), "{line}");
    }
}

#[test]
fn the_radio_image_starts_no_link_for_ethernet() {
    assert_eq!(
        boot_lines_radio(Some(r#"{"link":"ethernet"}"#), false),
        ["network: ethernet (setting ethernet), not in this image, no link started"]
    );
}

#[test]
fn the_other_images_keep_their_boot_line_byte_for_byte() {
    assert_eq!(
        boot_lines(Some(r#"{"link":"wifi"}"#), false),
        ["network: wifi (setting wifi), no link started"]
    );
}

// ---- The console's lines (spec 022 R3) ----

fn wifi_set_line(total: usize) -> Vec<u8> {
    // `wifi-set ` and a JSON object padded with spaces to `total` bytes in all.
    let json = r#"{"ssid":"Home","password":"abcdefgh"}"#;
    let mut line = format!("wifi-set {json}").into_bytes();
    line.resize(total, b' ');
    line
}

#[test]
fn a_console_line_of_512_bytes_is_read_and_one_of_513_is_too_long() {
    assert_eq!(MAX_CONSOLE_LINE, 512);
    assert_eq!(
        parse_console_line(&vec![b'x'; 512]),
        ConsoleCommand::Unknown
    );
    assert_eq!(
        parse_console_line(&wifi_set_line(512)),
        ConsoleCommand::Refused(WifiRefusal::TooLong)
    );
    assert_eq!(
        parse_console_line(&vec![b'x'; 513]),
        ConsoleCommand::TooLong
    );
    assert_eq!(
        parse_console_line(&wifi_set_line(513)),
        ConsoleCommand::TooLong
    );
}

#[test]
fn an_unknown_command_or_bytes_that_are_not_utf8_are_unknown() {
    assert_eq!(parse_console_line(b"reboot"), ConsoleCommand::Unknown);
    assert_eq!(
        parse_console_line(b"wifi-forget now"),
        ConsoleCommand::Unknown
    );
    assert_eq!(
        parse_console_line(&[0xff, 0xfe, b'x']),
        ConsoleCommand::Unknown
    );
}

#[test]
fn wifi_set_gives_the_settings_with_or_without_a_carriage_return() {
    let expected = wifi_settings(r#"{"ssid":"Home","password":"abcdefgh"}"#).unwrap();
    for line in [
        &b"wifi-set {\"ssid\":\"Home\",\"password\":\"abcdefgh\"}"[..],
        &b"wifi-set {\"ssid\":\"Home\",\"password\":\"abcdefgh\"}\r"[..],
        &b"wifi-set {\"ssid\":\"Home\",\"password\":\"abcdefgh\"}\r\n"[..],
    ] {
        assert_eq!(
            parse_console_line(line),
            ConsoleCommand::WifiSet(expected.clone())
        );
    }
}

#[test]
fn a_wifi_set_that_is_refused_says_why() {
    assert_eq!(
        parse_console_line(br#"wifi-set {"ssid":"Home","password":"short"}"#),
        ConsoleCommand::Refused(WifiRefusal::InvalidPassword)
    );
    assert_eq!(
        parse_console_line(b"wifi-set"),
        ConsoleCommand::Refused(WifiRefusal::Empty)
    );
}

#[test]
fn wifi_forget_alone_forgets() {
    assert_eq!(
        parse_console_line(b"wifi-forget"),
        ConsoleCommand::WifiForget
    );
    assert_eq!(
        parse_console_line(b"wifi-forget\r"),
        ConsoleCommand::WifiForget
    );
}

#[test]
fn an_empty_console_line_is_empty() {
    assert_eq!(parse_console_line(b""), ConsoleCommand::Empty);
    assert_eq!(parse_console_line(b"\r"), ConsoleCommand::Empty);
}

#[test]
fn no_console_command_shows_the_password_or_the_ssid_in_debug() {
    let line = format!("wifi-set {}", settings_text(SSID, PASSWORD));
    let shown = format!("{:?}", parse_console_line(line.as_bytes()));
    assert!(shown.starts_with("WifiSet"), "{shown}");
    assert!(
        !shown.contains(PASSWORD) && !shown.contains(SSID),
        "{shown}"
    );
    let refused = format!(
        "{:?}",
        parse_console_line(format!("wifi-set {}", settings_text(SSID, "short")).as_bytes())
    );
    assert!(!refused.contains(SSID), "{refused}");
}
