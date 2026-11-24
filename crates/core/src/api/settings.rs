//! `/settings` (Sowel's `src/api/routes/settings.ts`): the home's six keys (spec 026 R1), and
//! `/settings/timezones`, Solite's own list of the time zones the box knows (decision D41).

use serde_json::Value;

use super::{Request, Response, error, failure, json, method_not_allowed};
use crate::home::Home;
use crate::settings::tz::TIME_ZONES;

pub fn route(home: &mut Home, segments: &[&str], request: &Request) -> Response {
    match (segments, request.method) {
        (["settings"], "GET") => json(200, &home.settings().to_map()),
        (["settings"], "PUT") => put(home, request.body),
        (["settings"], _) => method_not_allowed(),
        (["settings", "timezones"], "GET") => {
            let names: Vec<&str> = TIME_ZONES.iter().map(|(name, _)| *name).collect();
            json(200, &names)
        }
        (["settings", "timezones"], _) => method_not_allowed(),
        _ => error(404, "Not found"),
    }
}

fn put(home: &mut Home, body: &[u8]) -> Response {
    let Ok(Value::Object(entries)) = serde_json::from_slice::<Value>(body) else {
        return error(400, "Invalid body");
    };
    match home.put_settings(&entries) {
        Ok(()) => json(200, &serde_json::json!({ "success": true })),
        Err(e) => failure(&e),
    }
}
