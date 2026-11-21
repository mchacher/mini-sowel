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

/// The orders of a cluster.
pub fn orders_for_cluster(cluster: u16) -> &'static [OrderSpec] {
    match cluster {
        0x0006 => &[STATE_ORDER],
        0x0008 => &[BRIGHTNESS_ORDER],
        _ => &[],
    }
}

/// The data key of a measure of the radio (its `kind`): `light_state` is `state`, `motion` is
/// `occupancy`... `None` for a measure that is no data (`alarm`, `tamper`: IAS zones come with
/// their enrollment).
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
        _ => return None,
    })
}

/// Where a data key comes in the table of spec 008 (R2): the order data are listed in.
pub fn data_rank(key: &str) -> usize {
    const ORDER: [&str; 9] = [
        "temperature",
        "humidity",
        "pressure",
        "illuminance",
        "occupancy",
        "state",
        "brightness",
        "battery",
        "voltage",
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
