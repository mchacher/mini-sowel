//! Sowel's device routes (`src/api/routes/devices.ts` of the Sowel repository), as a pure
//! function: a request in, a status and a JSON body out. The host (or the box) only moves bytes.
//! The time comes from the caller.

use serde_json::{Map, Value};

use crate::devices::{Devices, MAX_NAME_CHARS, to_json};

/// Request bodies over this many bytes are refused (413).
pub const MAX_BODY_BYTES: usize = 16 * 1024;

const PREFIX: &str = "/api/v1";

/// One HTTP request, as the host read it.
#[derive(Debug)]
pub struct Request<'a> {
    pub method: &'a str,
    /// The path, with its query string if any (the query is ignored).
    pub path: &'a str,
    /// The `Host` header, if any.
    pub host: Option<&'a str>,
    /// The port the server listens on: the only `Host` ports accepted.
    pub port: u16,
    /// At most `MAX_BODY_BYTES + 1` bytes are needed to see that it is too long.
    pub body: &'a [u8],
}

/// The answer: a status, and a JSON body (`None` for 204). The content type is
/// `application/json; charset=utf-8` whenever there is a body.
#[derive(Debug, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    pub body: Option<String>,
}

fn error(status: u16, message: &str) -> Response {
    Response {
        status,
        body: Some(Value::Object(Map::from_iter([("error".into(), message.into())])).to_string()),
    }
}

fn json<T: serde::Serialize>(status: u16, value: &T) -> Response {
    match to_json(value) {
        Some(body) => Response {
            status,
            body: Some(body),
        },
        None => error(500, "Internal error"),
    }
}

/// True for `127.0.0.1:<port>` and `localhost:<port>`: a page of another origin cannot reach the
/// API through a DNS name that points here.
fn host_allowed(host: Option<&str>, port: u16) -> bool {
    let Some(host) = host else {
        return false;
    };
    let host = host.trim().to_ascii_lowercase();
    host == format!("127.0.0.1:{port}") || host == format!("localhost:{port}")
}

/// Answers a request. `now` is the time in milliseconds since 1970 (UTC).
pub fn handle(devices: &mut Devices, request: &Request, now: u64) -> Response {
    if !host_allowed(request.host, request.port) {
        return error(403, "Forbidden host");
    }
    if request.body.len() > MAX_BODY_BYTES {
        return error(413, "Body too large");
    }
    let path = request.path.split(['?', '#']).next().unwrap_or("");
    let Some(rest) = path
        .strip_prefix(PREFIX)
        .and_then(|rest| rest.strip_prefix('/'))
    else {
        return error(404, "Not found");
    };
    let segments: Vec<&str> = rest.split('/').collect();
    devices.refresh_status(now);
    match (segments.as_slice(), request.method) {
        (["devices"], "GET") => list(devices),
        (["devices"], _) => error(405, "Method not allowed"),
        (["devices", id], "GET") => get(devices, id),
        (["devices", id], "PUT") => update(devices, id, request.body, now),
        (["devices", id], "DELETE") => delete(devices, id),
        (["devices", _], _) => error(405, "Method not allowed"),
        _ => error(404, "Not found"),
    }
}

fn not_found() -> Response {
    error(404, "Device not found")
}

fn list(devices: &Devices) -> Response {
    let mut items: Vec<_> = devices.list().iter().collect();
    // Stable: devices created at the same millisecond keep the order they were added in.
    items.sort_by_key(|d| d.device.created_at);
    json(200, &items)
}

fn get(devices: &Devices, id: &str) -> Response {
    devices
        .get(id)
        .map_or_else(not_found, |device| json(200, device))
}

fn delete(devices: &mut Devices, id: &str) -> Response {
    if devices.remove(id) {
        Response {
            status: 204,
            body: None,
        }
    } else {
        not_found()
    }
}

/// `PUT`: `name` (1 to `MAX_NAME_CHARS` characters) and `zoneId` (only `null` until zones
/// exist). The whole body is checked before anything changes.
fn update(devices: &mut Devices, id: &str, body: &[u8], now: u64) -> Response {
    let Ok(Value::Object(fields)) = serde_json::from_slice::<Value>(body) else {
        return error(400, "Invalid JSON");
    };
    if let Some(unknown) = fields.keys().find(|k| *k != "name" && *k != "zoneId") {
        return error(400, &format!("Unknown field: {unknown}"));
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
    if fields.get("zoneId").is_some_and(|zone| !zone.is_null()) {
        return error(400, "zoneId must be null");
    }
    if devices.get(id).is_none() {
        return not_found();
    }
    if let Some(name) = name {
        return devices
            .rename(id, &name, now)
            .map_or_else(not_found, |device| json(200, &device));
    }
    devices
        .get(id)
        .map_or_else(not_found, |item| json(200, &item.device))
}
