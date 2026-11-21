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

/// The level of a brightness order: a whole number or a numeric string, 0 to 255, 255 counting as
/// 254 (as Zigbee2MQTT), inside the order's own bounds.
fn level_command(value: &Value, min: Option<i64>, max: Option<i64>) -> Option<Command> {
    let number = match value {
        Value::Number(number) => number.as_f64()?,
        Value::String(text) => text.trim().parse::<f64>().ok()?,
        _ => return None,
    };
    if !number.is_finite() || number.fract() != 0.0 || !(0.0..=255.0).contains(&number) {
        return None;
    }
    // A whole number from 0 to 255: nothing is lost.
    let level = (number as u8).min(254);
    let in_bounds =
        min.is_none_or(|m| i64::from(level) >= m) && max.is_none_or(|m| i64::from(level) <= m);
    in_bounds.then_some(Command::MoveToLevel(level))
}

fn command_for(order: &DeviceOrder, value: &Value) -> Option<Command> {
    match order.value_type {
        ValueType::Boolean => {
            state_command(value, order.value_on.as_deref(), order.value_off.as_deref())
        }
        ValueType::Number => level_command(value, order.min, order.max),
    }
}

/// The address `0x` and 16 hexadecimal digits, most significant byte first.
fn parse_eui64(text: &str) -> Option<[u8; 8]> {
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
const DEFAULT_ENDPOINT: u8 = 1;

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
        commands,
    })
}
