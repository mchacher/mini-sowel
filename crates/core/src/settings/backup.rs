//! The export and import document of the home (spec 026 R3, decision D42): Solite's own JSON
//! `{"version":1,"settings":{...},"zones":[...]}`, the settings as `GET /settings` and the zones as
//! Sowel's `Zone` (`src/shared/types.ts`). No secret in it. The import is checked whole here (the
//! document's shape and each field's rule); the home applies it on a copy of its zones.

use serde::Serialize;
use serde_json::{Map, Value};

use super::{Entry, Settings};
use crate::api::input;
use crate::devices::Id;
use crate::failure::{Failure, Outcome};
use crate::zones::{MAX_ZONES, Zone};

/// The only version of the document.
pub const VERSION: u64 = 1;

const FIELDS: [&str; 3] = ["version", "settings", "zones"];
const ZONE_FIELDS: [&str; 8] = [
    "id",
    "name",
    "parentId",
    "icon",
    "description",
    "displayOrder",
    "createdAt",
    "updatedAt",
];

/// A zone of the document, checked by the rules of `POST /zones`.
#[derive(Debug, Clone)]
pub struct BackupZone {
    pub id: Id,
    pub name: String,
    /// The parent's id as written; `None` for the root zone only.
    pub parent: Option<String>,
    pub icon: Option<String>,
    pub description: Option<String>,
    pub display_order: i64,
}

/// A checked document: the settings (every key of `KEYS` given or unset) and at most `MAX_ZONES`
/// zones, ids unique.
#[derive(Debug)]
pub struct Backup {
    pub settings: Settings,
    pub zones: Vec<BackupZone>,
}

/// The document of `GET /backup`.
#[derive(Serialize)]
struct Document<'a> {
    version: u64,
    settings: Map<String, Value>,
    zones: &'a [Zone],
}

/// The compact JSON of the document.
pub fn export(settings: &Settings, zones: &[Zone]) -> Result<String, String> {
    serde_json::to_string(&Document {
        version: VERSION,
        settings: settings.to_map(),
        zones,
    })
    .map_err(|e| e.to_string())
}

/// 400 `Invalid backup: <reason>`.
pub fn invalid(reason: impl std::fmt::Display) -> Failure {
    Failure::new(400, format!("Invalid backup: {reason}"))
}

/// Reads and checks a document (R3.2) before anything changes.
pub fn parse(body: &[u8]) -> Outcome<Backup> {
    let Ok(Value::Object(fields)) = serde_json::from_slice::<Value>(body) else {
        return Err(invalid("not an object"));
    };
    if let Some(unknown) = fields.keys().find(|k| !FIELDS.contains(&k.as_str())) {
        return Err(invalid(format_args!("unknown field {unknown}")));
    }
    for field in FIELDS {
        if !fields.contains_key(field) {
            return Err(invalid(format_args!("missing field {field}")));
        }
    }
    let version = &fields["version"];
    if version.as_u64() != Some(VERSION) {
        return Err(invalid(format_args!("unsupported version {version}")));
    }
    let Value::Object(settings) = &fields["settings"] else {
        return Err(invalid("settings is not an object"));
    };
    let entries: Vec<Entry> = Settings::check(settings).map_err(|e| invalid(e.message))?;
    let Value::Array(list) = &fields["zones"] else {
        return Err(invalid("zones is not a list"));
    };
    if list.len() > MAX_ZONES {
        return Err(invalid("too many zones"));
    }
    let mut zones: Vec<BackupZone> = Vec::with_capacity(list.len());
    for item in list {
        let zone = zone(item)?;
        if zones.iter().any(|z| z.id == zone.id) {
            return Err(invalid(format_args!("duplicate zone {}", zone.id)));
        }
        zones.push(zone);
    }
    Ok(Backup {
        settings: Settings::from_entries(entries),
        zones,
    })
}

/// One zone of the document: Sowel's `Zone` fields only, under the rules and texts of
/// `POST /zones` (`<text> in zone <id>`).
fn zone(item: &Value) -> Outcome<BackupZone> {
    let Value::Object(fields) = item else {
        return Err(invalid("a zone is not an object"));
    };
    if let Some(unknown) = fields.keys().find(|k| !ZONE_FIELDS.contains(&k.as_str())) {
        return Err(invalid(format_args!("unknown field {unknown} in a zone")));
    }
    let Some(id) = fields.get("id").and_then(Value::as_str).and_then(Id::parse) else {
        return Err(invalid("invalid zone id"));
    };
    let in_zone = |e: Failure| invalid(format_args!("{} in zone {id}", e.message));
    let name = input::string(fields, "name")
        .map_err(in_zone)?
        .ok_or_else(|| in_zone(input::missing_field("name")))?;
    let name = input::name(&name).map_err(in_zone)?;
    let (icon, description) = input::text_fields(fields).map_err(in_zone)?;
    let display_order = input::integer(fields, "displayOrder")
        .map_err(in_zone)?
        .unwrap_or(0);
    for date in ["createdAt", "updatedAt"] {
        input::string(fields, date).map_err(in_zone)?;
    }
    let parent = input::nullable_string(fields, "parentId")
        .map_err(in_zone)?
        .flatten();
    if parent.is_none() != (id == Id::ROOT_ZONE) {
        return Err(invalid("only the root zone has no parent"));
    }
    Ok(BackupZone {
        id,
        name,
        parent,
        icon: icon.flatten(),
        description: description.flatten(),
        display_order,
    })
}
