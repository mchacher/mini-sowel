//! `/backup` (spec 026 R3, decision D42): the home's settings and zones exported as one JSON
//! document, and imported back. Both ways the document is at most `MAX_BODY_BYTES`, so every export
//! can be imported.

use super::{MAX_BODY_BYTES, Request, Response, error, failure, json, method_not_allowed};
use crate::home::Home;
use crate::settings::backup;

pub fn route(home: &mut Home, segments: &[&str], request: &Request, now: u64) -> Response {
    match (segments, request.method) {
        (["backup"], "GET") => export(home),
        (["backup"], "POST") => {
            match backup::parse(request.body).and_then(|doc| home.import_backup(doc, now)) {
                Ok(summary) => json(200, &summary),
                Err(e) => failure(&e),
            }
        }
        (["backup"], _) => method_not_allowed(),
        _ => error(404, "Not found"),
    }
}

fn export(home: &Home) -> Response {
    let Ok(body) = home.export_backup() else {
        return error(500, "Internal error");
    };
    if body.len() > MAX_BODY_BYTES {
        let message = format!(
            "Backup too large: {} bytes, {MAX_BODY_BYTES} at most (shorten the rooms' descriptions)",
            body.len()
        );
        return error(413, &message);
    }
    Response {
        status: 200,
        body: Some(body),
    }
}
