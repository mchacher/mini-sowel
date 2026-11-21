//! `/devices` (Sowel's `src/api/routes/devices.ts`).

use serde_json::Value;

use super::input::{self, Fields};
use super::{Request, Response, error, failure, json, method_not_allowed, no_content};
use crate::devices::MAX_NAME_CHARS;
use crate::home::Home;

pub fn route(home: &mut Home, segments: &[&str], request: &Request, now: u64) -> Response {
    match (segments, request.method) {
        (["devices"], _) => method_not_allowed(),
        (["devices", id], "GET") => get(home, id),
        (["devices", id], "PUT") => update(home, id, request.body, now),
        (["devices", id], "DELETE") => delete(home, id),
        (["devices", _], _) => method_not_allowed(),
        _ => error(404, "Not found"),
    }
}

fn not_found() -> Response {
    error(404, "Device not found")
}

fn get(home: &Home, id: &str) -> Response {
    home.devices
        .get(id)
        .map_or_else(not_found, |device| json(200, device))
}

fn delete(home: &mut Home, id: &str) -> Response {
    if home.remove_device(id) {
        no_content()
    } else {
        not_found()
    }
}

/// `PUT`: `name` (1 to `MAX_NAME_CHARS` characters) and `zoneId` (a zone, or `null`). The whole
/// body is checked before anything changes.
fn update(home: &mut Home, id: &str, body: &[u8], now: u64) -> Response {
    let fields: Fields = match input::object(body) {
        Ok(fields) => fields,
        Err(e) => return failure(&e),
    };
    if let Err(e) = input::refuse_unknown(&fields, &["name", "zoneId"]) {
        return failure(&e);
    }
    if fields.is_empty() {
        return error(400, "Nothing to update");
    }
    let name = match fields.get("name") {
        None => None,
        Some(Value::String(name)) => {
            let name = name.trim();
            if name.is_empty() || name.chars().count() > MAX_NAME_CHARS {
                return error(400, "Invalid name");
            }
            Some(name.to_string())
        }
        Some(_) => return error(400, "Invalid name"),
    };
    let zone = match fields.get("zoneId") {
        None => None,
        Some(Value::Null) => Some(None),
        Some(Value::String(text)) => Some(Some(text.as_str())),
        Some(_) => return failure(&input::invalid_field("zoneId")),
    };
    if home.devices.get(id).is_none() {
        return not_found();
    }
    // The zone is checked before the name is written: a refused body changes nothing.
    if let Some(Some(text)) = zone
        && home.zones().find(text).is_none()
    {
        return error(404, "Zone not found");
    }
    if let Some(name) = name
        && home.devices.rename(id, &name, now).is_none()
    {
        return not_found();
    }
    match zone {
        Some(zone) => match home.set_device_zone(id, zone, now) {
            Ok(device) => json(200, &device),
            Err(e) => failure(&e),
        },
        None => home
            .devices
            .get(id)
            .map_or_else(not_found, |item| json(200, &item.device)),
    }
}
