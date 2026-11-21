//! Durations as Sowel writes and reads them (spec 012, R3, R8.8).

use serde_json::json;
use solite_core::recipes::duration::{format_duration, parse_duration};

const FORMAT: &str = ". Use e.g. \"10m\", \"30s\", \"1h\"";

#[test]
fn a_duration_is_digits_and_s_m_or_h_or_milliseconds_and_the_errors_have_sowels_texts() {
    for (value, ms) in [
        (json!("10m"), 600_000),
        (json!("30s"), 30_000),
        (json!("1h"), 3_600_000),
        (json!("5 m"), 300_000),
        (json!("5\tm"), 300_000),
        (json!(120_000), 120_000),
        (json!(0), 0),
    ] {
        assert_eq!(parse_duration(&value), Ok(ms), "{value}");
    }
    for (value, text) in [
        (
            json!("10M"),
            format!("Invalid duration format: 10M{FORMAT}"),
        ),
        (json!("1d"), format!("Invalid duration format: 1d{FORMAT}")),
        (
            json!("1.5m"),
            format!("Invalid duration format: 1.5m{FORMAT}"),
        ),
        (
            json!(" 5m"),
            format!("Invalid duration format:  5m{FORMAT}"),
        ),
        (
            json!("5m "),
            format!("Invalid duration format: 5m {FORMAT}"),
        ),
        (json!("m"), format!("Invalid duration format: m{FORMAT}")),
        (json!(""), format!("Invalid duration format: {FORMAT}")),
        (json!(true), "Invalid duration: true".to_string()),
        (json!(null), "Invalid duration: null".to_string()),
        (json!(["a", "b"]), "Invalid duration: a,b".to_string()),
        (json!({}), "Invalid duration: [object Object]".to_string()),
    ] {
        assert_eq!(parse_duration(&value), Err(text), "{value}");
    }
}

#[test]
fn a_negative_number_is_zero_a_fraction_is_rounded_down_and_a_huge_string_is_the_format_error() {
    assert_eq!(parse_duration(&json!(-5)), Ok(0));
    assert_eq!(parse_duration(&json!(1500.5)), Ok(1500));
    let huge = "99999999999999999999h";
    assert_eq!(
        parse_duration(&json!(huge)),
        Err(format!("Invalid duration format: {huge}{FORMAT}"))
    );
    // The largest that fits.
    assert_eq!(
        parse_duration(&json!("5124095576030h")),
        Ok(5_124_095_576_030 * 3_600_000)
    );
    assert!(parse_duration(&json!("5124095576031h")).is_err());
    // More than 24.8 days (2^31 ms) is held as it is.
    assert_eq!(parse_duration(&json!("700h")), Ok(700 * 3_600_000));
}

#[test]
fn a_duration_is_written_in_hours_minutes_or_seconds_rounded() {
    for (ms, text) in [
        (30_000, "30s"),
        (600_000, "10min"),
        (5_400_000, "2h"),
        (7_200_000, "2h"),
        (1_500, "2s"),
        (1_499, "1s"),
        (0, "0s"),
        (59_999, "60s"),
        (90_000, "2min"),
    ] {
        assert_eq!(format_duration(ms), text, "{ms}");
    }
}
