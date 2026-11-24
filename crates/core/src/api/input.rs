//! Reading a request body: a JSON object, its fields checked one by one with the texts of the
//! errors (`Invalid JSON`, `Unknown field: f`, `Invalid field: f`, `Missing field: f`,
//! `Invalid name`...). A body is checked whole before anything changes.

use serde_json::{Map, Value};

use crate::failure::{Failure, Outcome};
use crate::zones::{MAX_DESCRIPTION_CHARS, MAX_ICON_CHARS, MAX_NAME_CHARS};

pub type Fields = Map<String, Value>;

fn bad(message: String) -> Failure {
    Failure::new(400, message)
}

/// The body as a JSON object (400 `Invalid JSON` for anything else).
pub fn object(body: &[u8]) -> Outcome<Fields> {
    match serde_json::from_slice::<Value>(body) {
        Ok(Value::Object(fields)) => Ok(fields),
        _ => Err(bad("Invalid JSON".into())),
    }
}

/// The refusal of a body that is no JSON object.
pub fn object_error() -> Failure {
    bad("Invalid JSON".into())
}

/// Refuses the first field that is not in `allowed`.
pub fn refuse_unknown(fields: &Fields, allowed: &[&str]) -> Outcome<()> {
    match fields.keys().find(|k| !allowed.contains(&k.as_str())) {
        Some(unknown) => Err(bad(format!("Unknown field: {unknown}"))),
        None => Ok(()),
    }
}

pub fn invalid_field(field: &str) -> Failure {
    bad(format!("Invalid field: {field}"))
}

pub fn missing_field(field: &str) -> Failure {
    bad(format!("Missing field: {field}"))
}

/// A string field, or absent.
pub fn string(fields: &Fields, field: &str) -> Outcome<Option<String>> {
    match fields.get(field) {
        None => Ok(None),
        Some(Value::String(text)) => Ok(Some(text.clone())),
        Some(_) => Err(invalid_field(field)),
    }
}

/// A string field, `null`, or absent: `None` absent, `Some(None)` null.
pub fn nullable_string(fields: &Fields, field: &str) -> Outcome<Option<Option<String>>> {
    match fields.get(field) {
        None => Ok(None),
        Some(Value::Null) => Ok(Some(None)),
        Some(Value::String(text)) => Ok(Some(Some(text.clone()))),
        Some(_) => Err(invalid_field(field)),
    }
}

/// A whole number field, or absent.
pub fn integer(fields: &Fields, field: &str) -> Outcome<Option<i64>> {
    match fields.get(field) {
        None => Ok(None),
        Some(Value::Number(n)) => n.as_i64().map(Some).ok_or_else(|| invalid_field(field)),
        Some(_) => Err(invalid_field(field)),
    }
}

pub fn boolean(fields: &Fields, field: &str) -> Outcome<Option<bool>> {
    match fields.get(field) {
        None => Ok(None),
        Some(Value::Bool(b)) => Ok(Some(*b)),
        Some(_) => Err(invalid_field(field)),
    }
}

/// A name: 1 to `MAX_NAME_CHARS` characters after trimming (400 `Invalid name`).
pub fn name(text: &str) -> Outcome<String> {
    let text = text.trim();
    if text.is_empty() || text.chars().count() > MAX_NAME_CHARS {
        return Err(bad("Invalid name".into()));
    }
    Ok(text.to_string())
}

/// An icon: at most `MAX_ICON_CHARS` characters (400 `Invalid icon`).
pub fn icon(text: &str) -> Outcome<String> {
    if text.chars().count() > MAX_ICON_CHARS {
        return Err(bad("Invalid icon".into()));
    }
    Ok(text.to_string())
}

/// A description: at most `MAX_DESCRIPTION_CHARS` characters (400 `Invalid description`).
pub fn description(text: &str) -> Outcome<String> {
    if text.chars().count() > MAX_DESCRIPTION_CHARS {
        return Err(bad("Invalid description".into()));
    }
    Ok(text.to_string())
}

/// An alias: 1 to `MAX_ALIAS_CHARS` characters after trimming, not blank (400 `Invalid alias`).
pub fn alias(text: &str) -> Outcome<String> {
    let text = text.trim();
    if text.is_empty() || text.chars().count() > crate::equipments::MAX_ALIAS_CHARS {
        return Err(bad("Invalid alias".into()));
    }
    Ok(text.to_string())
}

/// A text field that may be cleared: absent (`None`), `null` (`Some(None)`) or a value.
pub type Clearable = Option<Option<String>>;

/// `icon` and `description` of a body, validated.
pub fn text_fields(fields: &Fields) -> Outcome<(Clearable, Clearable)> {
    let icon_value = match nullable_string(fields, "icon")? {
        Some(Some(text)) => Some(Some(icon(&text)?)),
        other => other,
    };
    let description_value = match nullable_string(fields, "description")? {
        Some(Some(text)) => Some(Some(description(&text)?)),
        other => other,
    };
    Ok((icon_value, description_value))
}
