//! `/integrations/solite/permit-join` (spec 017): opens and closes pairing while serving. Sowel has
//! no pairing route (decision D28); the field `time` is Zigbee2MQTT's `permit_join` request. The
//! route only records the request in the home's pairing window: the radio loop applies it at its
//! next turn.

use serde::Serialize;

use super::input;
use super::{Request, Response, error, failure, json, method_not_allowed};
use crate::home::Home;

/// The integration's id, as the devices' `integrationId`.
const INTEGRATION: &str = "solite";
/// Seconds of a `POST` with no `time`: the default of `--pair`.
const DEFAULT_TIME: i64 = 180;
/// The longest time the radio takes.
const MAX_TIME: i64 = 254;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct State<'a> {
    permit_join: bool,
    remaining: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<&'a str>,
}

fn state(home: &Home, status: u16) -> Response {
    let window = home.pairing();
    let (permit_join, remaining, error) = window.state(window.now());
    json(
        status,
        &State {
            permit_join,
            remaining,
            error,
        },
    )
}

pub fn route(home: &mut Home, segments: &[&str], request: &Request) -> Response {
    let ["integrations", id, action] = segments else {
        return error(404, "Not found");
    };
    if *action != "permit-join" {
        return error(404, "Not found");
    }
    if *id != INTEGRATION {
        return error(404, "Integration not found");
    }
    match request.method {
        "GET" => state(home, 200),
        "POST" => open_or_close(home, request.body),
        _ => method_not_allowed(),
    }
}

fn open_or_close(home: &mut Home, body: &[u8]) -> Response {
    let fields = match input::object(body) {
        Ok(fields) => fields,
        Err(e) => return failure(&e),
    };
    if let Err(e) = input::refuse_unknown(&fields, &["time"]) {
        return failure(&e);
    }
    let time = match input::integer(&fields, "time") {
        Ok(time) => time.unwrap_or(DEFAULT_TIME),
        Err(e) => return failure(&e),
    };
    let Ok(time) = u8::try_from(time) else {
        return failure(&input::invalid_field("time"));
    };
    if i64::from(time) > MAX_TIME {
        return failure(&input::invalid_field("time"));
    }
    home.pairing_mut().request(time);
    state(home, 202)
}
