//! The parameters of `motion-light` and Sowel's `validate` (recipe `src/index.ts` R:324-400), with
//! its order and its exact messages. A failure is the message; the caller wraps it as
//! `Invalid params: <message>`.

use serde_json::Value;

use crate::devices::Id;
use crate::equipments::EquipmentType;
use crate::home::Home;
use crate::recipes::duration::parse_duration;

/// Lights, and buttons, an instance can have (spec 012, R8.4).
pub const MAX_LIGHTS: usize = 16;
pub const MAX_BUTTONS: usize = 16;

/// The timeout when none is given.
const DEFAULT_TIMEOUT: &str = "10m";

/// Parameters that passed `validate`, resolved.
#[derive(Debug, Clone, PartialEq)]
pub struct Params {
    pub zone: Id,
    pub lights: Vec<Id>,
    pub buttons: Vec<Id>,
    pub timeout_ms: u64,
    pub lux_threshold: Option<f64>,
    pub max_on_ms: Option<u64>,
    pub disable_when_daylight: bool,
}

/// JavaScript's `!value`: absent, `null`, `false`, `0` and `""` are falsy.
fn falsy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => true,
        Some(Value::Bool(b)) => !b,
        Some(Value::Number(n)) => n.as_f64().is_none_or(|n| n == 0.0),
        Some(Value::String(text)) => text.is_empty(),
        Some(_) => false,
    }
}

/// `undefined`, `null` and `""`: no value given.
fn not_given(value: Option<&Value>) -> bool {
    matches!(value, None | Some(Value::Null))
        || matches!(value, Some(Value::String(t)) if t.is_empty())
}

/// JavaScript's `Number(text)` for a decimal number: a trimmed empty text is 0, a sign, digits, a
/// point and an exponent are read, anything else is not a number.
fn number_of_text(text: &str) -> Option<f64> {
    let text = text.trim();
    if text.is_empty() {
        return Some(0.0);
    }
    if !text
        .chars()
        .all(|c| c.is_ascii_digit() || matches!(c, '+' | '-' | '.' | 'e' | 'E'))
    {
        return None;
    }
    text.parse::<f64>().ok().filter(|n| !n.is_nan())
}

/// JavaScript's `Number(value)` for what a parameter can hold (`None` is NaN).
fn number_of(value: &Value) -> Option<f64> {
    match value {
        Value::Number(n) => n.as_f64(),
        Value::Bool(b) => Some(f64::from(u8::from(*b))),
        Value::String(text) => number_of_text(text),
        _ => None,
    }
}

/// The strings of a list parameter (what is not a string is dropped).
fn strings(list: &[Value]) -> Vec<&str> {
    list.iter().filter_map(Value::as_str).collect()
}

/// The params text without the buttons that are gone or have no `action` data binding any more,
/// and the name of each (the id for an equipment that does not exist), once per id. `None` when
/// every button is fine (or the text is not a JSON object with a `buttons` list).
pub fn prune_stale_buttons(text: &str, home: &Home) -> Option<(Box<str>, Vec<String>)> {
    let mut value = serde_json::from_str::<Value>(text).ok()?;
    let Some(Value::Array(list)) = value.as_object_mut()?.get_mut("buttons") else {
        return None;
    };
    let mut stale: Vec<(String, String)> = Vec::new();
    let before = list.len();
    list.retain(|entry| {
        let Some(id) = entry.as_str() else {
            return true;
        };
        let item = Id::parse(id).and_then(|id| home.equipments().get_by_id(&id));
        let name = match item {
            Some(item) if item.data_bindings.iter().any(|b| b.alias == "action") => return true,
            Some(item) => item.equipment.name.to_string(),
            None => id.to_string(),
        };
        if !stale.iter().any(|(seen, _)| seen == id) {
            stale.push((id.to_string(), name));
        }
        false
    });
    if list.len() == before {
        return None;
    }
    // Shorter than the text it came from: it stays under `MAX_PARAMS_BYTES`.
    let text = serde_json::to_string(&value).ok()?.into_boxed_str();
    Some((text, stale.into_iter().map(|(_, name)| name).collect()))
}

/// Sowel's `validate`, against the home as it is.
pub fn validate(params: &Value, home: &Home) -> Result<Params, String> {
    let get = |key: &str| params.get(key);
    // 1, 2: the zone.
    let zone_text = match get("zone") {
        Some(Value::String(text)) if !text.is_empty() => text.as_str(),
        _ => return Err("Zone parameter is required".into()),
    };
    let zone = home
        .zones()
        .find(zone_text)
        .map(|z| z.id)
        .ok_or_else(|| format!("Zone not found: {zone_text}"))?;
    // 4: the lights; the legacy `light` only when `lights` is not a list.
    let lights: Vec<&str> = match get("lights") {
        Some(Value::Array(list)) => strings(list),
        _ => match get("light") {
            Some(Value::String(text)) if !text.is_empty() => vec![text.as_str()],
            _ => Vec::new(),
        },
    };
    if lights.is_empty() {
        return Err("At least one light is required".into());
    }
    if lights.len() > MAX_LIGHTS {
        return Err(format!("At most {MAX_LIGHTS} lights"));
    }
    // 5: each light, in order.
    let mut light_ids = Vec::with_capacity(lights.len());
    for text in lights {
        let item = Id::parse(text)
            .and_then(|id| home.equipments().get_by_id(&id))
            .ok_or_else(|| format!("Light equipment not found: {text}"))?;
        let equipment = &item.equipment;
        if equipment.kind != EquipmentType::LightOnoff {
            return Err(format!(
                "Light \"{}\" is type \"{}\" but this recipe requires light_onoff",
                equipment.name,
                equipment.kind.as_str()
            ));
        }
        if equipment.zone_id != zone {
            return Err(format!(
                "Light \"{}\" does not belong to the selected zone",
                equipment.name
            ));
        }
        if !item.order_bindings.iter().any(|b| b.alias == "state") {
            return Err(format!(
                "Light \"{}\" has no \"state\" order binding",
                equipment.name
            ));
        }
        light_ids.push(equipment.id);
    }
    // 6: the timeout.
    let timeout_value = match get("timeout") {
        Some(value) if !falsy(Some(value)) => value.clone(),
        _ => Value::String(DEFAULT_TIMEOUT.into()),
    };
    let timeout_ms = parse_duration(&timeout_value)?;
    // 7: the lux threshold.
    let lux_threshold = match get("luxThreshold") {
        value if not_given(value) => None,
        Some(value) => match number_of(value) {
            Some(n) if n >= 0.0 => Some(n),
            _ => return Err("luxThreshold must be a non-negative number".into()),
        },
        None => None,
    };
    // 8: the failsafe.
    let max_on_ms = match get("maxOnDuration") {
        Some(value) if !not_given(Some(value)) => Some(parse_duration(value)?),
        _ => None,
    };
    // 9: the buttons; what is not a list is ignored.
    let mut button_ids = Vec::new();
    if let Some(Value::Array(list)) = get("buttons") {
        let buttons = strings(list);
        if buttons.len() > MAX_BUTTONS {
            return Err(format!("At most {MAX_BUTTONS} buttons"));
        }
        for text in buttons {
            let item = Id::parse(text)
                .and_then(|id| home.equipments().get_by_id(&id))
                .ok_or_else(|| format!("Button equipment not found: {text}"))?;
            if !item.data_bindings.iter().any(|b| b.alias == "action") {
                return Err(format!(
                    "Button \"{}\" has no \"action\" data binding",
                    item.equipment.name
                ));
            }
            button_ids.push(item.equipment.id);
        }
    }
    Ok(Params {
        zone,
        lights: light_ids,
        buttons: button_ids,
        timeout_ms,
        lux_threshold,
        max_on_ms,
        disable_when_daylight: matches!(get("disableWhenDaylight"), Some(Value::Bool(true))),
    })
}
