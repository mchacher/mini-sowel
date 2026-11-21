//! Sowel's routes for devices, zones and equipments (`src/api/routes/{devices,zones,
//! equipments}.ts` of the Sowel repository), as a pure function: a request in, a status and a
//! JSON body out. The host (or the box) only moves bytes. The time and the random bytes come from
//! the caller.

mod devices;
mod equipments;
mod input;
mod zones;

pub use crate::home::{CHUNK_ITEMS, ListCursor};

use serde_json::{Map, Value};

use crate::devices::{Random, to_json};
use crate::failure::Failure;
use crate::home::Home;

/// Request bodies over this many bytes are refused (413).
pub const MAX_BODY_BYTES: usize = 16 * 1024;

const PREFIX: &str = "/api/v1";

/// One HTTP request, as the host read it.
#[derive(Debug)]
pub struct Request<'a> {
    pub method: &'a str,
    /// The path, with its query string if any (`GET /equipments` reads `type`).
    pub path: &'a str,
    /// The `Host` header, if any.
    pub host: Option<&'a str>,
    /// The port the server listens on: the only `Host` ports accepted.
    pub port: u16,
    /// At most `MAX_BODY_BYTES + 1` bytes are needed to see that it is too long.
    pub body: &'a [u8],
}

/// What a request is answered with: a whole response, or a list streamed from a cursor (status
/// 200, `application/json; charset=utf-8`).
#[derive(Debug)]
pub enum Reply {
    Whole(Response),
    List(ListCursor),
}

/// The answer: a status, and a JSON body (`None` for 204). The content type is
/// `application/json; charset=utf-8` whenever there is a body.
#[derive(Debug, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    pub body: Option<String>,
}

pub(crate) fn error(status: u16, message: &str) -> Response {
    Response {
        status,
        body: Some(Value::Object(Map::from_iter([("error".into(), message.into())])).to_string()),
    }
}

pub(crate) fn failure(failure: &Failure) -> Response {
    error(failure.status, &failure.message)
}

pub(crate) fn json<T: serde::Serialize>(status: u16, value: &T) -> Response {
    match to_json(value) {
        Some(body) => Response {
            status,
            body: Some(body),
        },
        None => error(500, "Internal error"),
    }
}

pub(crate) fn no_content() -> Response {
    Response {
        status: 204,
        body: None,
    }
}

pub(crate) fn method_not_allowed() -> Response {
    error(405, "Method not allowed")
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

/// Answers a request; the long lists are drained into one body (tests, tools). The host streams
/// them with `handle_stream`.
pub fn handle(home: &mut Home, request: &Request, now: u64, random: Random) -> Response {
    match handle_stream(home, request, now, random) {
        Reply::Whole(response) => response,
        Reply::List(mut cursor) => {
            let mut body = String::new();
            loop {
                match cursor.next_chunk(home) {
                    Ok(Some(chunk)) => body.push_str(&chunk),
                    Ok(None) => break,
                    Err(_) => return error(500, "Internal error"),
                }
            }
            Response {
                status: 200,
                body: Some(body),
            }
        }
    }
}

/// Answers a request. `now` is the time in milliseconds since 1970 (UTC); `random` gives the
/// bytes of the ids of what is created. After a change, the zone aggregation is brought up to
/// date. `GET /devices` and `GET /equipments` answer a cursor: the caller serves its chunks,
/// taking the home for each one and never holding it while writing.
pub fn handle_stream(home: &mut Home, request: &Request, now: u64, random: Random) -> Reply {
    if !host_allowed(request.host, request.port) {
        return Reply::Whole(error(403, "Forbidden host"));
    }
    if request.body.len() > MAX_BODY_BYTES {
        return Reply::Whole(error(413, "Body too large"));
    }
    let (path, query) = match request.path.split_once('?') {
        Some((path, query)) => (path, query),
        None => (request.path, ""),
    };
    let path = path.split('#').next().unwrap_or("");
    let Some(rest) = path
        .strip_prefix(PREFIX)
        .and_then(|rest| rest.strip_prefix('/'))
    else {
        return Reply::Whole(error(404, "Not found"));
    };
    let segments: Vec<&str> = rest.split('/').collect();
    // Silence alone can turn a device offline: the aggregation follows.
    if home.devices.refresh_status(now) {
        home.recompute(now);
    }
    if request.method == "GET" {
        match segments.as_slice() {
            ["devices"] => return Reply::List(ListCursor::devices(home, now)),
            ["equipments"] => {
                return Reply::List(ListCursor::equipments(
                    home,
                    equipments::type_filter(query),
                    now,
                ));
            }
            _ => {}
        }
    }
    let response = match segments.first().copied() {
        Some("devices") => devices::route(home, &segments, request, now),
        Some("zones") => zones::route(home, &segments, request, now, random),
        Some("equipments") => equipments::route(home, &segments, request, now, random),
        _ => error(404, "Not found"),
    };
    if request.method != "GET" && response.status < 300 {
        home.recompute(now);
    }
    Reply::Whole(response)
}
