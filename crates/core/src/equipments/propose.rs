//! Recognition of a paired device (spec 027 R3): the equipment type and the bindings its data and
//! orders suggest, the way the interface's equipment form plans them (`deduceType` then
//! `computeBindingPlan` for one device, spec 019), extended to `light_color`. Built on demand for
//! the proposals of a later milestone, never kept and never run per measure.

use super::rules::{self, Side};
use super::{Alias, EquipmentType, MAX_BINDINGS};
use crate::devices::{DeviceWithDetails, Id};

/// Sowel's sensor data categories (`RELEVANT_DATA.sensor` without `battery`): a device that
/// reads one of them is proposed as a sensor.
const SENSOR_CATEGORIES: [&str; 12] = [
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
];

/// An equipment type and the bindings to make for a device: at most `MAX_BINDINGS` of each kind,
/// each as the id of the device's data (or order) and its alias.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proposal {
    pub kind: EquipmentType,
    pub data: Vec<(Id, Alias)>,
    pub orders: Vec<(Id, Alias)>,
}

/// The type to propose for a device, the first that fits (spec 027 R3.1); `None` for a device
/// that is none of them (a battery alone, nothing read yet).
fn kind_of(device: &DeviceWithDetails) -> Option<EquipmentType> {
    let data_has = |categories: &[&str]| {
        device
            .data
            .iter()
            .any(|d| categories.contains(&&*d.category))
    };
    let order_has = |categories: &[&str]| {
        device.orders.iter().any(|o| {
            o.category
                .as_deref()
                .is_some_and(|c| categories.contains(&c))
        })
    };
    let on_off = order_has(&["light_toggle", "toggle_power"]);
    if on_off
        && (data_has(&["light_color", "light_color_temp"])
            || order_has(&["set_color", "set_color_temp"]))
    {
        return Some(EquipmentType::LightColor);
    }
    if on_off && data_has(&["light_brightness"]) {
        return Some(EquipmentType::LightDimmable);
    }
    if on_off {
        return Some(EquipmentType::LightOnoff);
    }
    if data_has(&["shutter_position"]) || order_has(&["set_shutter_position"]) {
        return Some(EquipmentType::Shutter);
    }
    if data_has(&["setpoint"]) || order_has(&["set_setpoint"]) {
        return Some(EquipmentType::Thermostat);
    }
    if data_has(&["action"]) {
        return Some(EquipmentType::Button);
    }
    if data_has(&SENSOR_CATEGORIES) {
        return Some(EquipmentType::Sensor);
    }
    None
}

/// The equipment a device suggests: its type, and a binding for every data whose category the
/// type may bind and every order whose key it may bind (`rules`), under the alias Sowel's
/// `resolveAlias` gives, made unique (`<alias>_2`...). An alias past `MAX_ALIAS_CHARS` is left
/// out, as the form leaves it.
pub fn propose(device: &DeviceWithDetails) -> Option<Proposal> {
    let kind = kind_of(device)?;
    let mut data: Vec<(Id, Alias)> = Vec::new();
    for item in &device.data {
        if data.len() >= MAX_BINDINGS {
            break;
        }
        if !rules::relevant_data(kind, &item.category) {
            continue;
        }
        let base = rules::resolve_alias(
            &item.key,
            kind,
            Side::Data,
            Some(&item.category),
            item.value_type,
        );
        let used: Vec<&str> = data.iter().map(|(_, alias)| &**alias).collect();
        let alias = rules::unique_alias(&base, &used);
        if rules::alias_fits(&alias) {
            data.push((item.id, Alias::new(&alias)));
        }
    }
    let mut orders: Vec<(Id, Alias)> = Vec::new();
    for item in &device.orders {
        if orders.len() >= MAX_BINDINGS {
            break;
        }
        if !rules::relevant_order(kind, &item.key) {
            continue;
        }
        let base = rules::resolve_alias(
            &item.key,
            kind,
            Side::Order,
            item.category.as_deref(),
            item.value_type,
        );
        let used: Vec<&str> = orders.iter().map(|(_, alias)| &**alias).collect();
        let alias = rules::unique_alias(&base, &used);
        if rules::alias_fits(&alias) {
            orders.push((item.id, Alias::new(&alias)));
        }
    }
    Some(Proposal { kind, data, orders })
}
