//! The events a recipe gets, as the core's `home/events.rs` has them (Sowel's `zone.data.changed`,
//! `equipment.data.changed` and `device.data.updated`), with ids as canonical lower-case texts,
//! and what a recipe says it listens to.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::bounds::MAX_VALUE_TEXT_BYTES;

/// The text cut to at most `max` bytes, on a character boundary.
pub fn cut(text: &str, max: usize) -> &str {
    if text.len() <= max {
        return text;
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// The value of a data in an event or a view: a boolean, a number or a short text (anything else,
/// a list or an object, is `Null`: no recipe reads one).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum EventValue {
    Null,
    Bool(bool),
    Number(f64),
    Text(String),
}

impl EventValue {
    /// A JSON value, text cut at `MAX_VALUE_TEXT_BYTES` (on a character boundary).
    pub fn from_json(value: &Value) -> EventValue {
        match value {
            Value::Bool(b) => EventValue::Bool(*b),
            Value::Number(n) => n.as_f64().map_or(EventValue::Null, EventValue::Number),
            Value::String(text) => EventValue::Text(cut(text, MAX_VALUE_TEXT_BYTES).to_string()),
            _ => EventValue::Null,
        }
    }
}

/// One event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Event {
    /// `zone.data.changed`: the zone's aggregate changed; what recipes read of it.
    Zone {
        zone: String,
        motion: bool,
        luminosity: Option<i64>,
    },
    /// `equipment.data.changed`: a data bound under `alias` was updated (changed or not).
    Equipment {
        equipment: String,
        alias: String,
        value: EventValue,
    },
    /// `device.data.updated`: a device data was updated (changed or not).
    Device {
        device: String,
        key: String,
        value: EventValue,
    },
}

/// One thing an instance listens to; the engine hands it only the events that match, and tells
/// the home to queue them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ListenEntry {
    Zone { zone: String },
    Equipment { equipment: String, alias: String },
    Device { device: String, key: String },
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn an_event_is_json_with_a_type_and_a_value_of_any_short_kind() {
        let event = Event::Equipment {
            equipment: "e".into(),
            alias: "state".into(),
            value: EventValue::Text("ON".into()),
        };
        let text = serde_json::to_string(&event).unwrap();
        assert_eq!(
            text,
            r#"{"type":"equipment","equipment":"e","alias":"state","value":"ON"}"#
        );
        assert_eq!(serde_json::from_str::<Event>(&text).unwrap(), event);
        for value in [
            EventValue::Null,
            EventValue::Bool(true),
            EventValue::Number(2.5),
            EventValue::Text("x".into()),
        ] {
            let text = serde_json::to_string(&value).unwrap();
            assert_eq!(serde_json::from_str::<EventValue>(&text).unwrap(), value);
        }
        let zone = Event::Zone {
            zone: "z".into(),
            motion: true,
            luminosity: None,
        };
        assert_eq!(
            serde_json::to_string(&zone).unwrap(),
            r#"{"type":"zone","zone":"z","motion":true,"luminosity":null}"#
        );
    }

    #[test]
    fn a_value_is_cut_at_32_bytes_on_a_character_boundary_and_a_list_is_null() {
        let long = "é".repeat(20); // 40 bytes
        let EventValue::Text(text) = EventValue::from_json(&json!(long)) else {
            panic!("not a text");
        };
        assert_eq!(text.len(), 32);
        assert_eq!(EventValue::from_json(&json!([1])), EventValue::Null);
        assert_eq!(EventValue::from_json(&json!({"a": 1})), EventValue::Null);
    }
}
