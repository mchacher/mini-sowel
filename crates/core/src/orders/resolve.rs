//! An order of an equipment turned into the commands of its devices, with Sowel's rules
//! (`runOrder`, `resolveOrderValue` in `src/equipments/equipment-manager.ts`, `coerceOnOff` and
//! `resolveWireValue` in `src/shared/order-wire-value.ts`) and the checks Sowel leaves to its
//! integration.

use serde_json::Value;

use super::{DeviceCommand, OrderRequest};
use crate::devices::{DeviceOrder, Devices, ValueType};
use crate::equipments::OrderBinding;
use crate::failure::{Failure, Outcome};
use crate::home::Home;
use crate::radio::zcl::Command;

/// A command of the On/Off cluster from the value of a boolean order. An empty value (missing,
/// `null`, `""`) means `true` (Sowel's `resolveOrderValue`). Strings are trimmed and compared
/// without case; `1` and `0` count as `true` and `false`; `"toggle"` is the one value Sowel does
/// not know. Anything else is `None`.
fn state_command(
    value: &Value,
    value_on: Option<&str>,
    value_off: Option<&str>,
) -> Option<Command> {
    match value {
        Value::Null => Some(Command::On),
        Value::Bool(true) => Some(Command::On),
        Value::Bool(false) => Some(Command::Off),
        Value::String(text) => {
            if text.is_empty() {
                return Some(Command::On);
            }
            let text = text.trim().to_lowercase();
            if value_on.is_some_and(|on| on.to_lowercase() == text) {
                return Some(Command::On);
            }
            if value_off.is_some_and(|off| off.to_lowercase() == text) {
                return Some(Command::Off);
            }
            match text.as_str() {
                "on" | "true" | "1" => Some(Command::On),
                "off" | "false" | "0" => Some(Command::Off),
                "toggle" => Some(Command::Toggle),
                _ => None,
            }
        }
        Value::Number(number) => match number.as_f64() {
            Some(1.0) => Some(Command::On),
            Some(0.0) => Some(Command::Off),
            _ => None,
        },
        _ => None,
    }
}

/// A whole number from a JSON number or a numeric string (shared by `brightness` and
/// `color_temp`, each with its own range); `None` for anything else.
fn whole_number(value: &Value) -> Option<f64> {
    let number = match value {
        Value::Number(number) => number.as_f64()?,
        Value::String(text) => text.trim().parse::<f64>().ok()?,
        _ => return None,
    };
    (number.is_finite() && number.fract() == 0.0).then_some(number)
}

/// The level of a brightness order: a whole number or a numeric string, 0 to 255, 255 counting as
/// 254 (as Zigbee2MQTT), inside the order's own bounds.
fn level_command(value: &Value, min: Option<i64>, max: Option<i64>) -> Option<Command> {
    let number = whole_number(value)?;
    if !(0.0..=255.0).contains(&number) {
        return None;
    }
    // A whole number from 0 to 255: nothing is lost.
    let level = (number as u8).min(254);
    let in_bounds =
        min.is_none_or(|m| i64::from(level) >= m) && max.is_none_or(|m| i64::from(level) <= m);
    in_bounds.then_some(Command::MoveToLevel(level))
}

/// The colour temperature of a `color_temp` order: a whole number or a numeric string inside the
/// order's bounds (150 to 500 mireds).
fn color_temp_command(value: &Value, min: Option<i64>, max: Option<i64>) -> Option<Command> {
    let number = whole_number(value)?;
    if !(1.0..=f64::from(u16::MAX)).contains(&number) {
        return None;
    }
    // A whole number from 1 to 65535: nothing is lost.
    let mireds = number as u16;
    let in_bounds =
        min.is_none_or(|m| i64::from(mireds) >= m) && max.is_none_or(|m| i64::from(mireds) <= m);
    in_bounds.then_some(Command::MoveToColorTemperature(mireds))
}

/// A JSON number (not a string) of the object, inside `0..=max`.
fn colour_field(object: &serde_json::Map<String, Value>, key: &str, max: f64) -> Option<f64> {
    let number = object.get(key)?.as_f64()?;
    (0.0..=max).contains(&number).then_some(number)
}

/// A `color` order: an object with `x` and `y` (0 to 1: Move to Color) or else `hue` (0 to 360)
/// and `saturation` (0 to 100: Move to Hue and Saturation); a valid `x` and `y` win when both
/// forms are given, other fields are ignored.
fn color_command(value: &Value) -> Option<Command> {
    let object = value.as_object()?;
    if let (Some(x), Some(y)) = (
        colour_field(object, "x", 1.0),
        colour_field(object, "y", 1.0),
    ) {
        // 0 to 65535 after the rounding: nothing is lost; 0xFEFF is the largest valid value.
        let raw = |v: f64| ((v * 65535.0).round() as u16).min(0xFEFF);
        return Some(Command::MoveToColor {
            x: raw(x),
            y: raw(y),
        });
    }
    let hue = colour_field(object, "hue", 360.0)?;
    let saturation = colour_field(object, "saturation", 100.0)?;
    // 0 to 254 after the rounding: nothing is lost.
    Some(Command::MoveToHueSaturation {
        hue: (hue * 254.0 / 360.0).round() as u8,
        saturation: (saturation * 254.0 / 100.0).round() as u8,
    })
}

/// The command of a `shutter_move` order: `OPEN`, `CLOSE` or `STOP` (trimmed, any case); nothing
/// else, not even an empty value.
fn shutter_move_command(value: &Value) -> Option<Command> {
    match value.as_str()?.trim().to_uppercase().as_str() {
        "OPEN" => Some(Command::ShutterOpen),
        "CLOSE" => Some(Command::ShutterClose),
        "STOP" => Some(Command::ShutterStop),
        _ => None,
    }
}

/// The command of a `set_shutter_position` order: a whole number from 0 to 100, 100 open, inside
/// the order's bounds; the ZCL counts the other way (`100 - value`).
fn shutter_position_command(value: &Value, min: Option<i64>, max: Option<i64>) -> Option<Command> {
    let number = whole_number(value)?;
    if !(0.0..=100.0).contains(&number) {
        return None;
    }
    // A whole number from 0 to 100: nothing is lost.
    let position = number as u8;
    let in_bounds = min.is_none_or(|m| i64::from(position) >= m)
        && max.is_none_or(|m| i64::from(position) <= m);
    in_bounds.then_some(Command::GoToLiftPercentage(100 - position))
}

/// The command of a `set_setpoint` order: a number (not a string) in degrees inside the order's
/// bounds, written in hundredths of a degree.
fn setpoint_command(value: &Value, min: Option<i64>, max: Option<i64>) -> Option<Command> {
    let degrees = value.as_f64().filter(|d| d.is_finite())?;
    let in_bounds =
        min.is_none_or(|m| degrees >= m as f64) && max.is_none_or(|m| degrees <= m as f64);
    // Inside 5..=30 (or the bounds): the hundredths fit an i16.
    in_bounds.then(|| Command::WriteHeatingSetpoint((degrees * 100.0).round() as i16))
}

/// The command of an order for a value, chosen by the order's category (`set_color_temp`,
/// `set_color`), else by its type.
fn command_for(order: &DeviceOrder, value: &Value) -> Option<Command> {
    match (order.category.as_deref(), order.value_type) {
        (Some("set_color_temp"), ValueType::Number) => {
            color_temp_command(value, order.min, order.max)
        }
        (Some("set_color"), ValueType::Json) => color_command(value),
        (Some("shutter_move"), ValueType::Text) => shutter_move_command(value),
        (Some("set_setpoint"), ValueType::Number) => setpoint_command(value, order.min, order.max),
        (Some("set_shutter_position"), ValueType::Number) => {
            shutter_position_command(value, order.min, order.max)
        }
        (_, ValueType::Boolean) => {
            state_command(value, order.value_on.as_deref(), order.value_off.as_deref())
        }
        (_, ValueType::Number) => level_command(value, order.min, order.max),
        (_, ValueType::Json | ValueType::Text) => None,
    }
}

/// The address `0x` and 16 hexadecimal digits, most significant byte first.
pub(crate) fn parse_eui64(text: &str) -> Option<[u8; 8]> {
    let digits = text.strip_prefix("0x")?;
    if digits.len() != 16 || !digits.is_ascii() {
        return None;
    }
    let mut eui64 = [0u8; 8];
    for (byte, pair) in eui64.iter_mut().zip(digits.as_bytes().chunks(2)) {
        *byte = u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok()?;
    }
    Some(eui64)
}

/// Endpoint of a device whose frames have not told it.
pub(crate) const DEFAULT_ENDPOINT: u8 = 1;

/// The commands of the devices behind these order bindings (all under `alias`), one per device
/// that still exists: Sowel sends an order to every binding of the alias, and skips a device that
/// is gone. 400 `Invalid value for <alias>` when a device's order does not accept the value.
pub fn commands_for(
    devices: &Devices,
    bindings: &[OrderBinding],
    alias: &str,
    value: &Value,
) -> Outcome<Vec<DeviceCommand>> {
    let mut commands = Vec::new();
    for binding in bindings {
        let Some((device, order)) = devices.order_of(&binding.device_order_id) else {
            continue;
        };
        let command = command_for(order, value)
            .ok_or_else(|| Failure::new(400, format!("Invalid value for {alias}")))?;
        let Some(eui64) = parse_eui64(&device.source_device_id) else {
            continue;
        };
        let route = devices
            .get_by_id(&device.id)
            .map(|d| d.route)
            .unwrap_or_default();
        commands.push(DeviceCommand {
            eui64,
            nwk: route.nwk,
            endpoint: route.endpoint.unwrap_or(DEFAULT_ENDPOINT),
            command,
        });
    }
    Ok(commands)
}

/// Validates an order and resolves it into the commands of the devices bound under `alias`
/// (`POST /equipments/:id/orders/:alias`). The errors are Sowel's: 404 `Equipment not found`, 400
/// `Equipment is disabled`, 404 `Order alias not found: <alias>`; and 400 `Invalid value for
/// <alias>` for a value the order does not accept. `value` is `Null` when the body gave none.
pub fn prepare(
    home: &Home,
    equipment_id: &str,
    alias: &str,
    value: &Value,
) -> Outcome<OrderRequest> {
    let item = home
        .equipments()
        .get(equipment_id)
        .ok_or_else(|| Failure::new(404, "Equipment not found"))?;
    if !item.equipment.enabled {
        return Err(Failure::new(400, "Equipment is disabled"));
    }
    let bindings: Vec<&OrderBinding> = item
        .order_bindings
        .iter()
        .filter(|b| b.alias == alias)
        .collect();
    if bindings.is_empty() {
        return Err(Failure::new(404, format!("Order alias not found: {alias}")));
    }
    let bindings: Vec<OrderBinding> = bindings.into_iter().cloned().collect();
    let commands = commands_for(&home.devices, &bindings, alias, value)?;
    Ok(OrderRequest {
        equipment: item.equipment.id,
        alias: alias.to_string(),
        value: value.clone(),
        source: None,
        commands,
    })
}
