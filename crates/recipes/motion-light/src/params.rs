//! The parameters of `motion-light` and Sowel's `validate` (recipe `src/index.ts` R:324-400), with
//! its order and its exact messages. A failure is the message; the caller wraps it as
//! `Invalid params: <message>`.

use serde_json::Value;
use solite_recipe_sdk::duration::parse_duration;
use solite_recipe_sdk::{Host, IdBytes, format_id, parse_id};

/// Lights, and buttons, an instance can have (spec 012, R8.4).
pub const MAX_LIGHTS: usize = 16;
pub const MAX_BUTTONS: usize = 16;

/// The timeout when none is given.
const DEFAULT_TIMEOUT: &str = "10m";

/// Parameters that passed `validate`, resolved.
#[derive(Debug, Clone, PartialEq)]
pub struct Params {
    /// Ids are kept as their bytes (16 each): at most 1 + 16 + 16 per instance.
    pub zone: IdBytes,
    pub lights: Vec<IdBytes>,
    pub buttons: Vec<IdBytes>,
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

/// Takes out of the params the buttons that are gone or have no `action` data binding any more,
/// once per id, and gives the name of each (the id as written for an equipment that does not
/// exist). Empty when every button is fine (or the params are not an object with a `buttons`
/// list).
pub fn prune_stale_buttons(params: &mut Value, host: &dyn Host) -> Vec<String> {
    let Some(Value::Array(list)) = params.as_object_mut().and_then(|o| o.get_mut("buttons")) else {
        return Vec::new();
    };
    let mut stale: Vec<(String, String)> = Vec::new();
    let before = list.len();
    list.retain(|entry| {
        let Some(id) = entry.as_str() else {
            return true;
        };
        let name = match host.equipment(id) {
            Some(item) if item.data_bindings.iter().any(|b| b.alias == "action") => return true,
            Some(item) => item.name,
            None => id.to_string(),
        };
        if !stale.iter().any(|(seen, _)| seen == id) {
            stale.push((id.to_string(), name));
        }
        false
    });
    if list.len() == before {
        return Vec::new();
    }
    stale.into_iter().map(|(_, name)| name).collect()
}

/// Sowel's `validate`, against the home as the host shows it.
pub fn validate(params: &Value, host: &dyn Host) -> Result<Params, String> {
    let get = |key: &str| params.get(key);
    // 1, 2: the zone.
    let zone_text = match get("zone") {
        Some(Value::String(text)) if !text.is_empty() => text.as_str(),
        _ => return Err("Zone parameter is required".into()),
    };
    let zone_id = || format!("Zone not found: {zone_text}");
    let zone = parse_id(zone_text).ok_or_else(zone_id)?;
    host.zone_data(&format_id(&zone)).ok_or_else(zone_id)?;
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
        let item = host
            .equipment(text)
            .ok_or_else(|| format!("Light equipment not found: {text}"))?;
        if item.kind != "light_onoff" {
            return Err(format!(
                "Light \"{}\" is type \"{}\" but this recipe requires light_onoff",
                item.name, item.kind
            ));
        }
        if parse_id(&item.zone_id) != Some(zone) {
            return Err(format!(
                "Light \"{}\" does not belong to the selected zone",
                item.name
            ));
        }
        if !item.order_bindings.iter().any(|b| b.alias == "state") {
            return Err(format!(
                "Light \"{}\" has no \"state\" order binding",
                item.name
            ));
        }
        light_ids
            .push(parse_id(&item.id).ok_or_else(|| format!("Light equipment not found: {text}"))?);
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
            let item = host
                .equipment(text)
                .ok_or_else(|| format!("Button equipment not found: {text}"))?;
            if !item.data_bindings.iter().any(|b| b.alias == "action") {
                return Err(format!(
                    "Button \"{}\" has no \"action\" data binding",
                    item.name
                ));
            }
            button_ids.push(
                parse_id(&item.id).ok_or_else(|| format!("Button equipment not found: {text}"))?,
            );
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
