//! Sowel's binding rules for eight equipment types (spec 027 R2.2), ported from Sowel's
//! `ui/src/components/equipments/bindingUtils.ts`: which data categories and order keys a type
//! may bind (`RELEVANT_DATA`, `RELEVANT_ORDERS`, `BIND_ALL_ORDER_TYPES`), and the alias a binding
//! gets (`resolveAlias` and its maps, `uniqueAlias`). The other 23 types have no rules here:
//! nothing is proposed for them, their bindings are added by hand.
//!
//! Cut from Sowel's tables: every entry of the 23 other types, and the per-type alias entries of
//! `gate`, `heater`, `water_heater`, `water_valve`, `media_player`, `ev_charger` and
//! `electric_vehicle`.

use super::{EquipmentType, MAX_ALIAS_CHARS};
use crate::devices::ValueType;

/// Sowel's `THERMOSTAT_STATE_ALIAS`: a thermostat's boolean run state is bound under it.
pub const THERMOSTAT_STATE_ALIAS: &str = "state";

/// Whether this type has binding rules here: the eight types of spec 027 R2.2.
pub fn has_rules(kind: EquipmentType) -> bool {
    matches!(
        kind,
        EquipmentType::LightOnoff
            | EquipmentType::LightDimmable
            | EquipmentType::LightColor
            | EquipmentType::Shutter
            | EquipmentType::Switch
            | EquipmentType::Sensor
            | EquipmentType::Button
            | EquipmentType::Thermostat
    )
}

/// Sowel's `RELEVANT_DATA` for the eight types: the data categories a type may bind.
pub fn relevant_data(kind: EquipmentType, category: &str) -> bool {
    let categories: &[&str] = match kind {
        EquipmentType::LightOnoff | EquipmentType::Switch => &["light_state"],
        EquipmentType::LightDimmable => &["light_state", "light_brightness"],
        EquipmentType::LightColor => &[
            "light_state",
            "light_brightness",
            "light_color",
            "light_color_temp",
        ],
        EquipmentType::Shutter => &["shutter_position"],
        EquipmentType::Sensor => &[
            "temperature",
            "humidity",
            "pressure",
            "luminosity",
            "co2",
            "voc",
            "noise",
            "motion",
            "contact_door",
            "contact_window",
            "water_leak",
            "smoke",
            "battery",
        ],
        EquipmentType::Button => &["action", "battery"],
        EquipmentType::Thermostat => &[
            "temperature",
            "temperature_outdoor",
            "setpoint",
            "power",
            "operation_mode",
            "generic",
        ],
        _ => &[],
    };
    categories.contains(&category)
}

/// Sowel's `RELEVANT_ORDERS` and `BIND_ALL_ORDER_TYPES` for the eight types: the order keys a
/// type may bind (a thermostat binds every order its device has).
pub fn relevant_order(kind: EquipmentType, key: &str) -> bool {
    let keys: &[&str] = match kind {
        EquipmentType::Thermostat => return true,
        EquipmentType::LightOnoff | EquipmentType::Switch => {
            &["state", "on", "R1", "R2", "R3", "R4"]
        }
        EquipmentType::LightDimmable => &["state", "on", "brightness", "R1", "R2", "R3", "R4"],
        EquipmentType::LightColor => &[
            "state",
            "on",
            "brightness",
            "color",
            "color_temp",
            "R1",
            "R2",
            "R3",
            "R4",
        ],
        EquipmentType::Shutter => &["position", "state", "target_position"],
        _ => &[],
    };
    keys.contains(&key)
}

/// Sowel's `ORDER_CATEGORY_ALIASES`: the alias of an order category, whatever the type.
fn order_category_alias(category: &str) -> Option<&'static str> {
    Some(match category {
        "light_toggle" | "toggle_power" | "valve_toggle" | "pool_pump_toggle" | "shutter_move"
        | "pool_cover_move" => "state",
        "set_shutter_position" | "pool_cover_position" => "position",
        "set_brightness" => "brightness",
        "set_color_temp" => "color_temp",
        "set_color" => "color",
        "set_setpoint" | "set_pool_temperature_setpoint" => "setpoint",
        "gate_trigger" => "command",
        _ => return None,
    })
}

/// Sowel's `DATA_CATEGORY_ALIASES`: the alias of a data category, whatever the type.
fn data_category_alias(category: &str) -> Option<&'static str> {
    Some(match category {
        "light_state" | "cover_state" => "state",
        "shutter_position" => "position",
        "light_brightness" => "brightness",
        "light_color_temp" => "color_temp",
        "light_color" => "color",
        "setpoint" | "pool_temperature_setpoint" => "setpoint",
        "pool_water_temperature" => "temperature",
        "battery" => "battery",
        _ => return None,
    })
}

/// Sowel's `TYPE_CATEGORY_ALIASES` for the eight types: a thermostat's.
fn type_category_alias(kind: EquipmentType, category: &str) -> Option<&'static str> {
    match (kind, category) {
        (EquipmentType::Thermostat, "toggle_power") => Some("power"),
        (EquipmentType::Thermostat, "temperature") => Some("temperature"),
        (EquipmentType::Thermostat, "temperature_outdoor") => Some("outsideTemperature"),
        (EquipmentType::Thermostat, "operation_mode" | "set_operation_mode") => {
            Some("operationMode")
        }
        _ => None,
    }
}

/// Sowel's `STANDARD_ALIASES` for the eight types: per-type key aliases.
fn standard_alias(kind: EquipmentType, key: &str) -> Option<&'static str> {
    match (kind, key) {
        (EquipmentType::Thermostat, "targetTemperature") => Some("setpoint"),
        (EquipmentType::Thermostat, "insideTemperature") => Some("temperature"),
        (
            EquipmentType::LightOnoff
            | EquipmentType::LightDimmable
            | EquipmentType::LightColor
            | EquipmentType::Switch,
            "R1" | "R2" | "R3" | "R4",
        ) => Some("state"),
        _ => None,
    }
}

/// Which map of category aliases `resolve_alias` reads: the one of data or the one of orders.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Data,
    Order,
}

/// Sowel's `resolveAlias`: the alias of a device key for a type. Priority: a thermostat's boolean
/// `power` reading (`state`); the category, per type then global; the per-type key map; the key.
pub fn resolve_alias(
    key: &str,
    kind: EquipmentType,
    side: Side,
    category: Option<&str>,
    value_type: ValueType,
) -> String {
    if kind == EquipmentType::Thermostat
        && category == Some("power")
        && value_type == ValueType::Boolean
    {
        return THERMOSTAT_STATE_ALIAS.to_string();
    }
    if let Some(category) = category {
        if let Some(alias) = type_category_alias(kind, category) {
            return alias.to_string();
        }
        let global = match side {
            Side::Data => data_category_alias(category),
            Side::Order => order_category_alias(category),
        };
        if let Some(alias) = global {
            return alias.to_string();
        }
    }
    standard_alias(kind, key).unwrap_or(key).to_string()
}

/// Sowel's `uniqueAlias`: `base`, else `base_2`, `base_3`... the first not in `used`.
pub fn unique_alias(base: &str, used: &[&str]) -> String {
    if !used.contains(&base) {
        return base.to_string();
    }
    let mut n = 2u32;
    loop {
        let candidate = format!("{base}_{n}");
        if !used.contains(&candidate.as_str()) {
            return candidate;
        }
        n += 1;
    }
}

/// Whether an alias is short enough for the API (`MAX_ALIAS_CHARS`).
pub fn alias_fits(alias: &str) -> bool {
    alias.chars().count() <= MAX_ALIAS_CHARS
}
