//! Which data and orders a standard cluster gives a device (the table of spec 008, R2). The keys
//! are Zigbee2MQTT's property names, as Sowel's Zigbee2MQTT plugin uses them, so Sowel's aliases
//! match; the categories are Sowel's `DataCategory` and `OrderCategory`.

use super::model::ValueType;

/// A data a cluster gives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DataSpec {
    pub key: &'static str,
    pub value_type: ValueType,
    pub category: &'static str,
    pub unit: Option<&'static str>,
}

/// An order a cluster gives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OrderSpec {
    pub key: &'static str,
    pub value_type: ValueType,
    pub category: &'static str,
    pub min: Option<i64>,
    pub max: Option<i64>,
    pub value_on: Option<&'static str>,
    pub value_off: Option<&'static str>,
}

const fn number(key: &'static str, category: &'static str, unit: Option<&'static str>) -> DataSpec {
    DataSpec {
        key,
        value_type: ValueType::Number,
        category,
        unit,
    }
}

const TEMPERATURE: DataSpec = number("temperature", "temperature", Some("°C"));
const HUMIDITY: DataSpec = number("humidity", "humidity", Some("%"));
const PRESSURE: DataSpec = number("pressure", "pressure", Some("hPa"));
const ILLUMINANCE: DataSpec = number("illuminance", "luminosity", Some("lx"));
const OCCUPANCY: DataSpec = DataSpec {
    key: "occupancy",
    value_type: ValueType::Boolean,
    category: "motion",
    unit: None,
};
const fn flag(key: &'static str, category: &'static str) -> DataSpec {
    DataSpec {
        key,
        value_type: ValueType::Boolean,
        category,
        unit: None,
    }
}

const CONTACT: DataSpec = flag("contact", "contact_door");
const WATER_LEAK: DataSpec = flag("water_leak", "water_leak");
const SMOKE: DataSpec = flag("smoke", "smoke");
const TAMPER: DataSpec = flag("tamper", "generic");
const BATTERY_LOW: DataSpec = flag("battery_low", "generic");
const STATE: DataSpec = DataSpec {
    key: "state",
    value_type: ValueType::Boolean,
    category: "light_state",
    unit: None,
};
const BRIGHTNESS: DataSpec = number("brightness", "light_brightness", None);
const BATTERY: DataSpec = number("battery", "battery", Some("%"));
const VOLTAGE: DataSpec = number("voltage", "voltage", Some("mV"));

const STATE_ORDER: OrderSpec = OrderSpec {
    key: "state",
    value_type: ValueType::Boolean,
    category: "light_toggle",
    min: None,
    max: None,
    value_on: Some("ON"),
    value_off: Some("OFF"),
};
const BRIGHTNESS_ORDER: OrderSpec = OrderSpec {
    key: "brightness",
    value_type: ValueType::Number,
    category: "set_brightness",
    min: Some(0),
    max: Some(254),
    value_on: None,
    value_off: None,
};

/// The data of a cluster, in the order of the table; none for a cluster that gives none.
pub fn data_for_cluster(cluster: u16) -> &'static [DataSpec] {
    match cluster {
        0x0402 => &[TEMPERATURE],
        0x0405 => &[HUMIDITY],
        0x0403 => &[PRESSURE],
        0x0400 => &[ILLUMINANCE],
        0x0406 => &[OCCUPANCY],
        0x0006 => &[STATE],
        0x0008 => &[BRIGHTNESS],
        0x0001 => &[BATTERY, VOLTAGE],
        _ => &[],
    }
}

/// The data of an IAS zone type (spec 009, R2): the zone's own alarm; none for a zone type that
/// is not known.
pub fn data_for_zone_type(zone_type: u16) -> &'static [DataSpec] {
    match zone_type {
        0x000D => &[OCCUPANCY],
        0x0015 => &[CONTACT],
        0x002A => &[WATER_LEAK],
        0x0028 => &[SMOKE],
        _ => &[],
    }
}

/// The data a zone status bit adds the first time a notification has it set (spec 009, R4.5).
pub fn data_for_zone_flag(kind: &str) -> Option<DataSpec> {
    zone_flag_spec(kind).copied()
}

/// The kinds of zone status bit `data_for_zone_flag` knows.
const ZONE_FLAGS: [&str; 2] = ["tamper", "battery_low"];

fn zone_flag_spec(kind: &str) -> Option<&'static DataSpec> {
    match kind {
        "tamper" => Some(&TAMPER),
        "battery_low" => Some(&BATTERY_LOW),
        _ => None,
    }
}

/// Every data spec of the catalog: the clusters' (every cluster number is tried, so one added to
/// `data_for_cluster` is found), the IAS zone types' and the zone flags'. The contract check
/// (spec 027) reads their categories.
pub fn all_data() -> impl Iterator<Item = &'static DataSpec> {
    (0..=u16::MAX)
        .flat_map(|cluster| data_for_cluster(cluster).iter())
        .chain((0..=u16::MAX).flat_map(|zone_type| data_for_zone_type(zone_type).iter()))
        .chain(ZONE_FLAGS.iter().filter_map(|kind| zone_flag_spec(kind)))
}

/// Every order spec of the catalog (every cluster number is tried).
pub fn all_orders() -> impl Iterator<Item = &'static OrderSpec> {
    (0..=u16::MAX).flat_map(|cluster| orders_for_cluster(cluster).iter())
}

/// The data keys the `alarm` of a zone sets, whichever the device has.
pub const ZONE_KEYS: [&str; 4] = ["occupancy", "contact", "water_leak", "smoke"];

/// The orders of a cluster.
pub fn orders_for_cluster(cluster: u16) -> &'static [OrderSpec] {
    match cluster {
        0x0006 => &[STATE_ORDER],
        0x0008 => &[BRIGHTNESS_ORDER],
        _ => &[],
    }
}

/// The data key of a measure of the radio (its `kind`): `light_state` is `state`, `motion` is
/// `occupancy`... `None` for a measure that is no data of its own (`alarm`: the registry gives it
/// to the zone's data, see `ZONE_KEYS`).
pub fn key_for_measure(kind: &str) -> Option<&'static str> {
    Some(match kind {
        "temperature" => "temperature",
        "humidity" => "humidity",
        "pressure" => "pressure",
        "luminosity" => "illuminance",
        "motion" => "occupancy",
        "light_state" => "state",
        "light_brightness" => "brightness",
        "battery" => "battery",
        "voltage" => "voltage",
        "tamper" => "tamper",
        "battery_low" => "battery_low",
        _ => return None,
    })
}

/// Where a data key comes in the table of spec 008 (R2): the order data are listed in.
pub fn data_rank(key: &str) -> usize {
    const ORDER: [&str; 14] = [
        "temperature",
        "humidity",
        "pressure",
        "illuminance",
        "occupancy",
        "contact",
        "water_leak",
        "smoke",
        "state",
        "brightness",
        "battery",
        "voltage",
        "tamper",
        "battery_low",
    ];
    ORDER.iter().position(|k| *k == key).unwrap_or(ORDER.len())
}

/// Where an order key comes in the table: `state` then `brightness`.
pub fn order_rank(key: &str) -> usize {
    match key {
        "state" => 0,
        "brightness" => 1,
        _ => 2,
    }
}
