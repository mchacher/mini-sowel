//! When a reading is stale and what an equipment's status is: Sowel's `isStaleBinding` and
//! `deriveEquipmentStatus` (`src/equipments/equipment-status.ts`), with the windows of
//! `STREAMING_TIMEOUT_MS` and the set `STREAMING_CATEGORIES` (`src/shared/constants.ts`).

use serde::Serialize;

use super::EquipmentType;
use crate::devices::{Device, DeviceStatus, ValueType, format_iso};

const MINUTE_MS: u64 = 60 * 1000;
/// The window of a streaming category Sowel gives none (never reached with Sowel's table, kept
/// as its `DEFAULT_STREAMING_TIMEOUT_MS`).
const DEFAULT_WINDOW_MS: u64 = 15 * MINUTE_MS;

/// Sowel's `EquipmentStatus`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EquipmentStatus {
    Online,
    Degraded,
    Offline,
}

/// Sowel's `EquipmentStatusReason`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusReason {
    /// Names of the bound devices that are offline.
    pub offline_devices: Vec<String>,
    /// Aliases of the stale bindings.
    pub stale_bindings: Vec<String>,
    /// The earliest `lastSeen` of the offline devices and `lastUpdated` of the stale bindings.
    pub offline_since: Option<String>,
}

/// The categories of a device that pushes a value at regular intervals, even unchanged
/// (`STREAMING_CATEGORIES`); silence past the window is an anomaly. `motion`, contacts, lights,
/// `battery` and `generic` are never here: a sensor that does not fire for weeks is normal.
fn is_streaming(category: &str) -> bool {
    matches!(
        category,
        "power"
            | "energy"
            | "voltage"
            | "current"
            | "temperature"
            | "temperature_outdoor"
            | "humidity"
            | "humidity_outdoor"
            | "pressure"
            | "luminosity"
            | "co2"
            | "voc"
            | "noise"
            | "ups_status"
            | "battery_runtime"
            | "ups_load"
            | "setpoint"
            | "pool_water_temperature"
            | "pool_temperature_setpoint"
    )
}

/// `ELECTRICAL_STREAMING_CATEGORIES`: Sowel judges them only on metering equipments; Solite has
/// no behaviour for the metering types (spec 027), so it never judges them.
fn is_electrical(category: &str) -> bool {
    matches!(category, "power" | "energy" | "voltage" | "current")
}

/// The window of a streaming category, in milliseconds (`STREAMING_TIMEOUT_MS`): above one hour
/// for the ambient ones, because a Zigbee sensor's longest allowed silence is an hour.
pub fn stale_window_ms(category: &str) -> u64 {
    match category {
        "temperature"
        | "temperature_outdoor"
        | "humidity"
        | "humidity_outdoor"
        | "pressure"
        | "luminosity"
        | "co2"
        | "voc"
        | "noise"
        | "pool_water_temperature" => 65 * MINUTE_MS,
        "setpoint" | "pool_temperature_setpoint" => 60 * MINUTE_MS,
        "ups_status" | "battery_runtime" | "ups_load" => 20 * MINUTE_MS,
        "power" => 2 * MINUTE_MS,
        "energy" => 10 * MINUTE_MS,
        "voltage" | "current" => 5 * MINUTE_MS,
        _ => DEFAULT_WINDOW_MS,
    }
}

/// Sowel's `isStaleBinding`: only a streaming category, never a boolean value, never a reading
/// without a date, the electrical categories never (no metering equipment here); stale when
/// more than the window has passed since `last_updated` (strictly).
pub fn is_stale(
    category: &str,
    value_type: ValueType,
    last_updated: Option<u64>,
    now: u64,
    _equipment: EquipmentType,
) -> bool {
    if !is_streaming(category) || value_type == ValueType::Boolean || is_electrical(category) {
        return false;
    }
    let Some(updated) = last_updated else {
        return false;
    };
    // A clock set back (`now` before `updated`) is never stale.
    now.saturating_sub(updated) > stale_window_ms(category)
}

/// One binding as the status sees it.
pub struct Reading<'a> {
    pub device: &'a Device,
    pub alias: &'a str,
    pub category: &'a str,
    pub value_type: ValueType,
    pub last_updated: Option<u64>,
}

/// Sowel's `deriveEquipmentStatus`: no data binding is `offline`; a `button` is `online`
/// (silence is its normal state); all the bound devices offline is `offline`; one offline or one
/// binding stale is `degraded`; a device whose status is `unknown` counts as online. The reason
/// comes with every status but `online`.
pub fn derive(
    readings: &[Reading],
    kind: EquipmentType,
    now: u64,
) -> (EquipmentStatus, Option<StatusReason>) {
    if readings.is_empty() {
        return (
            EquipmentStatus::Offline,
            Some(StatusReason {
                offline_devices: Vec::new(),
                stale_bindings: Vec::new(),
                offline_since: None,
            }),
        );
    }
    if kind == EquipmentType::Button {
        return (EquipmentStatus::Online, None);
    }
    // The distinct devices behind the bindings (one device backs several bindings).
    let mut devices: Vec<&Device> = Vec::new();
    for reading in readings {
        if !devices.iter().any(|d| d.id == reading.device.id) {
            devices.push(reading.device);
        }
    }
    let offline: Vec<&&Device> = devices
        .iter()
        .filter(|d| d.status == DeviceStatus::Offline)
        .collect();
    let stale: Vec<&Reading> = readings
        .iter()
        .filter(|r| is_stale(r.category, r.value_type, r.last_updated, now, kind))
        .collect();
    if offline.is_empty() && stale.is_empty() {
        return (EquipmentStatus::Online, None);
    }
    let earliest = offline
        .iter()
        .filter_map(|d| d.last_seen)
        .chain(stale.iter().filter_map(|r| r.last_updated))
        .min();
    let reason = StatusReason {
        offline_devices: offline.iter().map(|d| d.name.to_string()).collect(),
        stale_bindings: stale.iter().map(|r| r.alias.to_string()).collect(),
        offline_since: earliest.map(format_iso),
    };
    let status = if offline.len() == devices.len() {
        EquipmentStatus::Offline
    } else {
        EquipmentStatus::Degraded
    };
    (status, Some(reason))
}
