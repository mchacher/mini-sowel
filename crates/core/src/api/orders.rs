//! `POST /equipments/:id/orders/:alias` (Sowel's `src/api/routes/equipments.ts`): the order is
//! validated and resolved under the home's lock, put in the home's queue, and the caller waits for
//! the radio loop's answer without the lock.

use std::time::Duration;

use serde_json::Value;

use super::{Reply, Response, error, failure, json};
use crate::home::Home;
use crate::orders::{DEADLINE_MS, OrderResult, OrderWait, prepare};

/// How long the caller waits for the answer: the order's deadline, and a moment for the radio
/// loop to give its answer at the deadline.
pub const WAIT_MS: u64 = DEADLINE_MS + 500;

/// The value of the body: `Null` when the body is empty or has no `value`; the body is a JSON
/// object otherwise (400 `Invalid JSON`). Other fields are ignored, as Sowel's route does.
fn value_of(body: &[u8]) -> Result<Value, Response> {
    if body.iter().all(u8::is_ascii_whitespace) {
        return Ok(Value::Null);
    }
    match serde_json::from_slice::<Value>(body) {
        Ok(Value::Object(mut fields)) => Ok(fields.remove("value").unwrap_or(Value::Null)),
        _ => Err(error(400, "Invalid JSON")),
    }
}

pub fn route(
    home: &mut Home,
    id: &str,
    alias: &str,
    body: &[u8],
    now: u64,
    streaming: bool,
) -> Reply {
    let value = match value_of(body) {
        Ok(value) => value,
        Err(response) => return Reply::Whole(response),
    };
    let request = match prepare(home, id, alias, &value) {
        Ok(request) => request,
        Err(e) => return Reply::Whole(failure(&e)),
    };
    if !streaming {
        return Reply::Whole(error(501, "Orders need the streaming handler"));
    }
    // The queue's clock (monotonic, given by the host) when it has one, else `now`.
    let at = home.orders().now().unwrap_or(now);
    match home.orders().push_waited(request, at) {
        Ok(wait) => Reply::Order(wait),
        Err(_) => Reply::Whole(error(503, "Busy")),
    }
}

/// The answer of an order: `200 {"success":true}`, or `502 {"error": <reason>}`. Waits for the
/// radio loop at most `WAIT_MS`; the home must not be held.
pub fn order_response(wait: OrderWait) -> Response {
    let result: OrderResult = wait.wait(Duration::from_millis(WAIT_MS));
    match result {
        Ok(()) => json(200, &serde_json::json!({ "success": true })),
        Err(reason) => error(502, &reason),
    }
}
