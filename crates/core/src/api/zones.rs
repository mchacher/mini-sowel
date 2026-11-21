//! `/zones` (Sowel's `src/api/routes/zones.ts`). Unknown fields are refused. The route
//! `zones/aggregation` is matched before `zones/:id`.

use super::input::{self, Fields};
use super::{Request, Response, error, failure, json, method_not_allowed, no_content};
use crate::devices::Random;
use crate::failure::Outcome;
use crate::home::{Aggregation, Home};
use crate::zones::{NewZone, ZoneUpdate};

const FIELDS: [&str; 5] = ["name", "parentId", "icon", "description", "displayOrder"];

pub fn route(
    home: &mut Home,
    segments: &[&str],
    request: &Request,
    now: u64,
    random: Random,
) -> Response {
    match (segments, request.method) {
        (["zones"], "GET") => match home.zones().tree() {
            Some(tree) => json(200, &[tree]),
            None => error(500, "Internal error"),
        },
        (["zones"], "POST") => create(home, request.body, now, random),
        (["zones"], _) => method_not_allowed(),
        (["zones", "aggregation"], "GET") => json(200, &Aggregation(home.aggregate(now))),
        (["zones", "aggregation"], _) => method_not_allowed(),
        (["zones", id], "GET") => match home.zones().with_children(id) {
            Some(tree) => json(200, &tree),
            None => error(404, "Zone not found"),
        },
        (["zones", id], "PUT") => update(home, id, request.body, now),
        (["zones", id], "DELETE") => match home.delete_zone(id, now) {
            Ok(()) => no_content(),
            Err(e) => failure(&e),
        },
        (["zones", _], _) => method_not_allowed(),
        _ => error(404, "Not found"),
    }
}

/// The fields of a body, checked: the fields every zone body can hold.
struct Parsed {
    name: Option<String>,
    parent: Option<Option<String>>,
    icon: Option<Option<String>>,
    description: Option<Option<String>>,
    display_order: Option<i64>,
}

fn parse(fields: &Fields) -> Outcome<Parsed> {
    input::refuse_unknown(fields, &FIELDS)?;
    let name = input::string(fields, "name")?;
    let parent = input::nullable_string(fields, "parentId")?;
    let display_order = input::integer(fields, "displayOrder")?;
    let (icon, description) = input::text_fields(fields)?;
    let name = match name {
        Some(text) => Some(input::name(&text)?),
        None => None,
    };
    Ok(Parsed {
        name,
        parent,
        icon,
        description,
        display_order,
    })
}

fn create(home: &mut Home, body: &[u8], now: u64, random: Random) -> Response {
    let result = input::object(body).and_then(|fields| {
        let parsed = parse(&fields)?;
        let name = parsed.name.ok_or_else(|| input::missing_field("name"))?;
        home.create_zone(
            NewZone {
                name,
                // `null` means the root, as an absent `parentId`.
                parent: parsed.parent.flatten(),
                icon: parsed.icon.flatten(),
                description: parsed.description.flatten(),
                display_order: parsed.display_order.unwrap_or(0),
            },
            now,
            random,
        )
    });
    match result {
        Ok(zone) => json(201, &zone),
        Err(e) => failure(&e),
    }
}

fn update(home: &mut Home, id: &str, body: &[u8], now: u64) -> Response {
    let result = input::object(body).and_then(|fields| {
        let parsed = parse(&fields)?;
        home.update_zone(
            id,
            ZoneUpdate {
                name: parsed.name,
                parent: parsed.parent,
                icon: parsed.icon,
                description: parsed.description,
                display_order: parsed.display_order,
            },
            now,
        )
    });
    match result {
        Ok(zone) => json(200, &zone),
        Err(e) => failure(&e),
    }
}
