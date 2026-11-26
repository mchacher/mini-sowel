//! Durations as Sowel writes and reads them (`src/recipes/engine/duration.ts`): `"10m"`, `"30s"`,
//! `"1h"`, or a number of milliseconds. Held as `u64` milliseconds (Sowel's JavaScript timers stop
//! at 24.8 days; these do not).

use serde_json::Value;

/// JavaScript's `String(value)` for what a parameter can hold, for the texts of the errors.
pub fn js_string(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.as_f64().map_or_else(|| n.to_string(), js_number),
        Value::String(text) => text.clone(),
        Value::Array(items) => items
            .iter()
            .map(|item| match item {
                Value::Null => String::new(),
                other => js_string(other),
            })
            .collect::<Vec<_>>()
            .join(","),
        Value::Object(_) => "[object Object]".to_string(),
    }
}

/// JavaScript's number to text: a whole number without a decimal point.
pub fn js_number(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e21 {
        format!("{value:.0}")
    } else {
        format!("{value}")
    }
}

fn bad_format(text: &str) -> String {
    format!("Invalid duration format: {text}. Use e.g. \"10m\", \"30s\", \"1h\"")
}

/// Sowel's `parseDuration`: a number is milliseconds (a negative one counts as 0, a fractional
/// one is rounded down); a string must be digits, optional spaces and `s`, `m` or `h`, and fit in
/// `u64` milliseconds. Anything else is an error with Sowel's text.
pub fn parse_duration(value: &Value) -> Result<u64, String> {
    match value {
        Value::Number(n) => {
            let ms = n.as_f64().unwrap_or(0.0);
            // `as` saturates: negative and NaN become 0, a huge value u64::MAX.
            Ok(ms.floor() as u64)
        }
        Value::String(text) => {
            let digits_end = text
                .find(|c: char| !c.is_ascii_digit())
                .unwrap_or(text.len());
            let (digits, rest) = text.split_at(digits_end);
            let unit = rest.trim_start_matches(char::is_whitespace);
            let factor: u64 = match unit {
                "s" => 1000,
                "m" => 60_000,
                "h" => 3_600_000,
                _ => return Err(bad_format(text)),
            };
            if digits.is_empty() {
                return Err(bad_format(text));
            }
            digits
                .parse::<u64>()
                .ok()
                .and_then(|n| n.checked_mul(factor))
                .ok_or_else(|| bad_format(text))
        }
        other => Err(format!("Invalid duration: {}", js_string(other))),
    }
}

/// JavaScript's `Math.round`: halves go up.
fn round(x: f64) -> u64 {
    (x + 0.5).floor() as u64
}

/// Sowel's `formatDuration`: `2h`, `10min` or `30s`, rounded.
pub fn format_duration(ms: u64) -> String {
    // Millisecond counts of a few days are exact in an f64.
    let value = ms as f64;
    if ms >= 3_600_000 {
        format!("{}h", round(value / 3_600_000.0))
    } else if ms >= 60_000 {
        format!("{}min", round(value / 60_000.0))
    } else {
        format!("{}s", round(value / 1000.0))
    }
}
