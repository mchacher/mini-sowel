//! `/system/timezone` (Sowel's `src/api/routes/system.ts`): the home's time zone as
//! `{tz, source, offsetHours}`. Sowel's other `/system` routes (version, update, mode...) are
//! not Solite's.

use serde::Serialize;
use serde_json::{Number, Value};

use super::{Request, Response, error, json, method_not_allowed};
use crate::home::Home;
use crate::settings;

pub fn route(home: &Home, segments: &[&str], request: &Request, now: u64) -> Response {
    match (segments, request.method) {
        (["system", "timezone"], "GET") => timezone(home, now),
        (["system", "timezone"], _) => method_not_allowed(),
        _ => error(404, "Not found"),
    }
}

/// Sowel's `{tz, source, offsetHours}`, its keys in Sowel's order.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Info<'a> {
    tz: &'a str,
    source: &'static str,
    offset_hours: Value,
}

/// With `home.timezone` set: the setting as `tz`, `source` `env` (the setting is the box's `TZ`,
/// decision D44), the offset from UTC now; unset: Sowel's fallback without a `TZ` and a
/// location, `UTC` (Sowel derives `auto` from the location, which the box has no room for).
fn timezone(home: &Home, now: u64) -> Response {
    let set = home
        .settings()
        .get(settings::TIMEZONE)
        .and_then(|name| settings::tz::find(name).map(|zone| (name, zone)));
    let info = match set {
        Some((name, zone)) => {
            let seconds = zone.offset_at(i64::try_from(now).unwrap_or(i64::MAX));
            Info {
                tz: name,
                source: "env",
                offset_hours: hours(seconds),
            }
        }
        None => Info {
            tz: "UTC",
            source: "fallback",
            offset_hours: Value::from(0),
        },
    };
    json(200, &info)
}

/// Seconds as hours: a whole number when it is one (`1`, `-3`), else a fraction (`5.5`).
fn hours(seconds: i64) -> Value {
    if seconds % 3600 == 0 {
        return Value::from(seconds / 3600);
    }
    // At most a few thousand seconds: exact in an `f64`.
    #[allow(clippy::cast_precision_loss)]
    Number::from_f64(seconds as f64 / 3600.0).map_or(Value::Null, Value::Number)
}
