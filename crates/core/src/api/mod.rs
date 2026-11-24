//! Sowel's routes for devices, zones, equipments, recipes and settings (`src/api/routes/{devices,
//! zones,equipments,recipes,settings}.ts` of the Sowel repository), and Solite's backup, as a pure
//! function: a request in, a status and a JSON body out. The host (or the box) only moves bytes.
//! The time and the random bytes come from the caller.

mod backup;
mod devices;
mod equipments;
pub(crate) mod input;
mod orders;
mod pairing;
mod recipes;
mod settings;
mod slots;
pub mod static_files;
mod system;
mod zones;

pub use crate::home::{CHUNK_ITEMS, ListCursor};
pub use orders::order_response;
pub use slots::{MAX_WORKERS, SlotGuard, Slots};
pub use static_files::{StaticFile, content_type};

use serde_json::{Map, Value};

use crate::devices::{Random, to_json};
use crate::failure::Failure;
use crate::home::Home;
use crate::orders::OrderWait;

/// Request bodies over this many bytes are refused (413).
pub const MAX_BODY_BYTES: usize = 16 * 1024;

const PREFIX: &str = "/api/v1";

/// The routes this module serves, as (method, path): a segment starting with `:` is a
/// parameter. The `/ws` upgrade is the host's, not listed. The router is a `match` on path
/// segments, so a route added there and missing here is not caught by a test (the contract check
/// of spec 027 reads this table); every route listed is tested to answer something other than
/// the route-level `Not found`.
pub const ROUTES: &[(&str, &str)] = &[
    ("GET", "/api/v1/devices"),
    ("GET", "/api/v1/devices/:id"),
    ("PUT", "/api/v1/devices/:id"),
    ("DELETE", "/api/v1/devices/:id"),
    ("GET", "/api/v1/zones"),
    ("POST", "/api/v1/zones"),
    ("GET", "/api/v1/zones/aggregation"),
    ("GET", "/api/v1/zones/:id"),
    ("PUT", "/api/v1/zones/:id"),
    ("DELETE", "/api/v1/zones/:id"),
    ("GET", "/api/v1/equipments"),
    ("POST", "/api/v1/equipments"),
    ("GET", "/api/v1/equipments/:id"),
    ("PUT", "/api/v1/equipments/:id"),
    ("DELETE", "/api/v1/equipments/:id"),
    ("POST", "/api/v1/equipments/:id/data-bindings"),
    ("DELETE", "/api/v1/equipments/:id/data-bindings/:bindingId"),
    ("POST", "/api/v1/equipments/:id/order-bindings"),
    ("DELETE", "/api/v1/equipments/:id/order-bindings/:bindingId"),
    ("POST", "/api/v1/equipments/:id/orders/:alias"),
    ("GET", "/api/v1/recipes"),
    ("GET", "/api/v1/recipes/:id"),
    ("GET", "/api/v1/recipe-instances"),
    ("POST", "/api/v1/recipe-instances"),
    ("PUT", "/api/v1/recipe-instances/:id"),
    ("DELETE", "/api/v1/recipe-instances/:id"),
    ("POST", "/api/v1/recipe-instances/:id/enable"),
    ("POST", "/api/v1/recipe-instances/:id/disable"),
    ("POST", "/api/v1/recipe-instances/:id/actions"),
    ("GET", "/api/v1/recipe-instances/:id/log"),
    ("GET", "/api/v1/settings"),
    ("PUT", "/api/v1/settings"),
    ("GET", "/api/v1/settings/timezones"),
    ("GET", "/api/v1/system/timezone"),
    ("GET", "/api/v1/backup"),
    ("POST", "/api/v1/backup"),
    ("GET", "/api/v1/integrations/solite/permit-join"),
    ("POST", "/api/v1/integrations/solite/permit-join"),
];

/// Who can reach the server: it decides which `Host` headers are accepted and whether the
/// pairing route is open (spec 022, D33).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reach<'a> {
    /// The Mac host: `Host` is `127.0.0.1:<port>` or `localhost:<port>`.
    Loopback,
    /// The box, on the home's network: private addresses and the box's own name (`solite-<6
    /// hex>`) are accepted, and the pairing route is off.
    Lan { name: &'a str },
}

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
    /// Who can reach the server.
    pub reach: Reach<'a>,
    /// The `Content-Type` header, if any.
    pub content_type: Option<&'a str>,
    /// At most `MAX_BODY_BYTES + 1` bytes are needed to see that it is too long.
    pub body: &'a [u8],
}

/// What a request is answered with: a whole response, a list streamed from a cursor (status
/// 200, `application/json; charset=utf-8`), or an order to wait for.
#[derive(Debug)]
pub enum Reply {
    Whole(Response),
    List(ListCursor),
    /// An order put in the home's queue (spec 011): the caller releases the home, then waits for
    /// the radio loop's answer with `OrderWait::wait` and turns it into a response with
    /// `order_response`.
    Order(OrderWait),
    /// A file of the interface (`/`, `/assets/<name>`): the host opens it under its `--ui` folder
    /// and streams it. The path is already checked.
    Static(StaticFile),
    /// A refusal (404, 405) of a path of the interface: a whole response like `Whole`, but the host
    /// adds the interface's security headers to it.
    StaticError(Response),
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
    match failure.code {
        None => error(failure.status, &failure.message),
        Some(code) => Response {
            status: failure.status,
            body: Some(
                Value::Object(Map::from_iter([
                    ("error".into(), code.into()),
                    ("message".into(), failure.message.as_str().into()),
                ]))
                .to_string(),
            ),
        },
    }
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

/// The suffixes a home network gives the box's name, besides the bare name.
const NAME_SUFFIXES: [&str; 4] = [".local", ".lan", ".home", ".home.arpa"];

/// True when `host` is an IPv4 address of `10/8`, `172.16/12`, `192.168/16` or `127/8`.
fn private_ipv4(host: &str) -> bool {
    let mut octets = [0u8; 4];
    let mut count = 0;
    for part in host.split('.') {
        if count == 4 || part.is_empty() || part.len() > 3 {
            return false;
        }
        let Ok(octet) = part.parse::<u8>() else {
            return false;
        };
        octets[count] = octet;
        count += 1;
    }
    count == 4
        && match octets {
            [10, ..] | [127, ..] => true,
            [172, second, ..] => (16..=31).contains(&second),
            [192, 168, ..] => true,
            _ => false,
        }
}

/// True when the `Host` header is one the server accepts for this reach. Under `Loopback`:
/// `127.0.0.1:<port>` and `localhost:<port>`, so a page of another origin cannot reach the API
/// through a DNS name that points here. Under `Lan`, without case: a private IPv4 address,
/// `localhost` or the box's name (alone or with a home suffix), the port, when given, being the
/// server's; anything else (a public address, an IPv6 literal, another name) is refused, which
/// also stops DNS rebinding.
pub fn host_allowed(host: Option<&str>, port: u16, reach: Reach) -> bool {
    let Some(host) = host else {
        return false;
    };
    let host = host.trim().to_ascii_lowercase();
    match reach {
        Reach::Loopback => {
            host == format!("127.0.0.1:{port}") || host == format!("localhost:{port}")
        }
        Reach::Lan { name } => {
            let (name_part, given_port) = match host.split_once(':') {
                Some((name_part, given)) => (name_part, Some(given)),
                None => (host.as_str(), None),
            };
            if given_port.is_some_and(|given| given.parse::<u16>().ok() != Some(port)) {
                return false;
            }
            let name = name.to_ascii_lowercase();
            private_ipv4(name_part)
                || name_part == "localhost"
                || (!name.is_empty()
                    && (name_part == name
                        || NAME_SUFFIXES
                            .iter()
                            .any(|suffix| name_part.strip_suffix(suffix) == Some(name.as_str()))))
        }
    }
}

/// Answers a request; the long lists are drained into one body (tests, tools). The host streams
/// them with `handle_stream`.
pub fn handle(home: &mut Home, request: &Request, now: u64, random: Random) -> Response {
    match dispatch(home, request, now, random, false) {
        Reply::Whole(response) => response,
        // Not reachable: without the streaming handler, an order is refused before it is queued.
        Reply::Order(_) => error(501, ORDERS_NEED_STREAMING),
        // The interface's files are the host's to send.
        Reply::Static(_) => error(404, "Not found"),
        Reply::StaticError(response) => response,
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
    dispatch(home, request, now, random, true)
}

/// The interface's answer for the box's server (spec 023 R4.6): `None` when the path is not the
/// interface's (`/`, `/assets/<name>`), left to `handle_stream`; else, in the order of
/// `handle_stream`, 403 for a `Host` the reach refuses, then 405, 404 or the file to send. The
/// box's server calls it on its own task, without the home; the host keeps `handle_stream`.
pub fn static_reply(request: &Request) -> Option<Reply> {
    let reply = static_files::route(request)?;
    if !host_allowed(request.host, request.port, request.reach) {
        return Some(Reply::Whole(error(403, "Forbidden host")));
    }
    Some(reply)
}

/// What the plain `handle` answers to an order: it cannot wait for the radio, so it does not
/// queue the order.
const ORDERS_NEED_STREAMING: &str = "Orders need the streaming handler";

/// True when the body of a POST or PUT is declared as JSON (`application/json`, parameters such
/// as `charset` allowed), the only type Fastify's JSON parser takes. A request of another type
/// cannot come from a cross-origin "simple" form POST.
fn json_content_type(value: Option<&str>) -> bool {
    value.is_some_and(|v| {
        v.split(';')
            .next()
            .is_some_and(|t| t.trim().eq_ignore_ascii_case("application/json"))
    })
}

fn dispatch(
    home: &mut Home,
    request: &Request,
    now: u64,
    random: Random,
    streaming: bool,
) -> Reply {
    if !host_allowed(request.host, request.port, request.reach) {
        return Reply::Whole(error(403, "Forbidden host"));
    }
    // The interface's files come before the API's checks: they take no body and no JSON.
    if let Some(reply) = static_files::route(request) {
        return reply;
    }
    if request.body.len() > MAX_BODY_BYTES {
        return Reply::Whole(error(413, "Body too large"));
    }
    // Every POST and PUT of the API takes a JSON body (the only routes without one are GET and
    // DELETE).
    if matches!(request.method, "POST" | "PUT") && !json_content_type(request.content_type) {
        return Reply::Whole(error(415, "Unsupported Media Type"));
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
            ["recipe-instances"] => return Reply::List(ListCursor::recipe_instances(home)),
            _ => {}
        }
    }
    if let ["equipments", id, "orders", alias] = segments.as_slice() {
        if request.method == "POST" {
            return orders::route(home, id, alias, request.body, now, streaming);
        }
        return Reply::Whole(method_not_allowed());
    }
    let response = match segments.first().copied() {
        Some("devices") => devices::route(home, &segments, request, now),
        Some("zones") => zones::route(home, &segments, request, now, random),
        Some("equipments") => equipments::route(home, &segments, request, now, random),
        // The box does not open pairing from the API (D28).
        Some("integrations") if matches!(request.reach, Reach::Lan { .. }) => {
            error(404, "Pairing from the API is off on the box")
        }
        Some("integrations") => pairing::route(home, &segments, request),
        Some("recipes" | "recipe-instances") => {
            recipes::route(home, &segments, request, now, random)
        }
        Some("settings") => settings::route(home, &segments, request),
        Some("system") => system::route(home, &segments, request, now),
        Some("backup") => backup::route(home, &segments, request, now),
        _ => error(404, "Not found"),
    };
    if request.method != "GET" && response.status < 300 {
        home.recompute(now);
    }
    Reply::Whole(response)
}
