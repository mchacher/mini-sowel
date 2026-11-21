//! The network setting and the link chosen at boot (spec 015 R5).

use solite_core::network::{Fallback, Link, MAX_SETTING_BYTES, NetworkSetting, choose, parse};

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
