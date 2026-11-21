//! Reading what a device measures, generically, from the standard Zigbee clusters: no device
//! definition. The measures take the names of Sowel's `DataCategory`
//! (`src/shared/types.ts` of the Sowel repository). Bytes in, measures out; no I/O.

use std::fmt;

use super::zcl::{AttributeRecord, ReportingRecord};

/// A frame gives at most this many measures: the rest is dropped.
pub const MAX_MEASURES_PER_FRAME: usize = 16;

const TYPE_BOOLEAN: u8 = 0x10;
const TYPE_BITMAP8: u8 = 0x18;
const TYPE_UINT8: u8 = 0x20;
const TYPE_UINT16: u8 = 0x21;
const TYPE_INT16: u8 = 0x29;

const CLUSTER_POWER: u16 = 0x0001;
const CLUSTER_ON_OFF: u16 = 0x0006;
const CLUSTER_LEVEL: u16 = 0x0008;
const CLUSTER_ILLUMINANCE: u16 = 0x0400;
const CLUSTER_PRESSURE: u16 = 0x0403;
const CLUSTER_TEMPERATURE: u16 = 0x0402;
const CLUSTER_HUMIDITY: u16 = 0x0405;
const CLUSTER_OCCUPANCY: u16 = 0x0406;

/// What a measure is worth.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Value {
    Number(f64),
    Bool(bool),
}

/// One measure of a device.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Measure {
    /// A name of Sowel's `DataCategory` (`temperature`, `humidity`, `battery`...), or `alarm`
    /// and `tamper` for an IAS zone.
    pub kind: &'static str,
    pub value: Value,
    /// `°C`, `%`, `hPa`, `lx`, `V`; empty when the value has none.
    pub unit: &'static str,
}

impl fmt::Display for Measure {
    /// `temperature 19.52 °C`, `light_state on`, `motion true`: trailing zeros are dropped.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ", self.kind)?;
        match self.value {
            Value::Number(number) => write!(f, "{number}")?,
            Value::Bool(on) if self.kind == "light_state" => {
                write!(f, "{}", if on { "on" } else { "off" })?;
            }
            Value::Bool(on) => write!(f, "{on}")?,
        }
        if !self.unit.is_empty() {
            write!(f, " {}", self.unit)?;
        }
        Ok(())
    }
}

fn number(kind: &'static str, value: f64, unit: &'static str) -> Option<Measure> {
    Some(Measure {
        kind,
        value: Value::Number(value),
        unit,
    })
}

fn flag(kind: &'static str, value: bool) -> Option<Measure> {
    Some(Measure {
        kind,
        value: Value::Bool(value),
        unit: "",
    })
}

/// Reads one attribute as a measure, or `None` when the cluster or attribute is not one of the
/// table, the type is not the table's, the bytes are not the right size, or the value is invalid
/// or implausible (an automation never acts on it).
pub fn decode(cluster: u16, attribute: u16, data_type: u8, bytes: &[u8]) -> Option<Measure> {
    let u16_value = || Some(u16::from_le_bytes([*bytes.first()?, *bytes.get(1)?]));
    let i16_value = || u16_value().map(|v| v as i16);
    let u8_value = || bytes.first().copied();
    match (cluster, attribute, data_type) {
        (CLUSTER_TEMPERATURE, 0x0000, TYPE_INT16) => {
            let raw = i16_value()?;
            if raw == i16::MIN {
                return None;
            }
            let degrees = f64::from(raw) / 100.0;
            (-50.0..=150.0)
                .contains(&degrees)
                .then(|| number("temperature", degrees, "°C"))?
        }
        (CLUSTER_HUMIDITY, 0x0000, TYPE_UINT16) => {
            let raw = u16_value()?;
            if raw == u16::MAX {
                return None;
            }
            let percent = f64::from(raw) / 100.0;
            (0.0..=100.0)
                .contains(&percent)
                .then(|| number("humidity", percent, "%"))?
        }
        // kPa times 10 is hPa.
        (CLUSTER_PRESSURE, 0x0000, TYPE_INT16) => {
            let raw = i16_value()?;
            (raw != i16::MIN).then(|| number("pressure", f64::from(raw), "hPa"))?
        }
        (CLUSTER_ILLUMINANCE, 0x0000, TYPE_UINT16) => {
            let raw = u16_value()?;
            if raw == u16::MAX {
                return None;
            }
            let lux = if raw == 0 {
                0.0
            } else {
                10f64.powf((f64::from(raw) - 1.0) / 10_000.0).round()
            };
            number("luminosity", lux, "lx")
        }
        (CLUSTER_OCCUPANCY, 0x0000, TYPE_BITMAP8) => flag("motion", u8_value()? & 1 != 0),
        (CLUSTER_ON_OFF, 0x0000, TYPE_BOOLEAN) => flag("light_state", u8_value()? != 0),
        (CLUSTER_LEVEL, 0x0000, TYPE_UINT8) => {
            let percent = (f64::from(u8_value()?) / 254.0 * 100.0).round();
            number("light_brightness", percent.min(100.0), "%")
        }
        (CLUSTER_POWER, 0x0021, TYPE_UINT8) => {
            let raw = u8_value()?;
            (raw != u8::MAX).then(|| number("battery", (f64::from(raw) / 2.0).round(), "%"))?
        }
        (CLUSTER_POWER, 0x0020, TYPE_UINT8) => {
            let raw = u8_value()?;
            (raw != u8::MAX).then(|| number("voltage", f64::from(raw) / 10.0, "V"))?
        }
        _ => None,
    }
}

/// The measures of the records of one frame, for the cluster it came on: at most
/// `MAX_MEASURES_PER_FRAME`.
pub fn from_records(cluster: u16, records: &[AttributeRecord]) -> Vec<Measure> {
    records
        .iter()
        .filter_map(|r| decode(cluster, r.attribute, r.data_type, &r.value))
        .take(MAX_MEASURES_PER_FRAME)
        .collect()
}

/// The measures of an IAS zone status: `alarm` (bit 0) and `tamper` (bit 2).
pub fn from_zone_status(status: u16) -> Vec<Measure> {
    [
        ("alarm", status & 0x0001 != 0),
        ("tamper", status & 0x0004 != 0),
    ]
    .into_iter()
    .filter_map(|(kind, on)| flag(kind, on))
    .collect()
}

/// A cluster the coordinator sets up reporting for, with the attributes to report.
#[derive(Debug)]
pub struct ReportingCluster {
    pub cluster: u16,
    /// What the cluster measures, shown in the `reporting:` line.
    pub name: &'static str,
    pub attributes: &'static [ReportingRecord],
}

const fn record(
    attribute: u16,
    data_type: u8,
    min_interval: u16,
    max_interval: u16,
    change: Option<u16>,
) -> ReportingRecord {
    ReportingRecord {
        attribute,
        data_type,
        min_interval,
        max_interval,
        change,
    }
}

/// The clusters set up on pairing, in the order they are set up: a sleepy device is awake for a
/// few seconds only, so the measures that matter most come first. Eight entries: the bound of
/// the setup's work per device.
pub const REPORTING: [ReportingCluster; 8] = [
    ReportingCluster {
        cluster: CLUSTER_TEMPERATURE,
        name: "temperature",
        attributes: &[record(0x0000, TYPE_INT16, 30, 3600, Some(20))],
    },
    ReportingCluster {
        cluster: CLUSTER_HUMIDITY,
        name: "humidity",
        attributes: &[record(0x0000, TYPE_UINT16, 30, 3600, Some(100))],
    },
    ReportingCluster {
        cluster: CLUSTER_PRESSURE,
        name: "pressure",
        attributes: &[record(0x0000, TYPE_INT16, 30, 3600, Some(1))],
    },
    ReportingCluster {
        cluster: CLUSTER_ILLUMINANCE,
        name: "luminosity",
        attributes: &[record(0x0000, TYPE_UINT16, 30, 3600, Some(10))],
    },
    ReportingCluster {
        cluster: CLUSTER_OCCUPANCY,
        name: "motion",
        attributes: &[record(0x0000, TYPE_BITMAP8, 0, 3600, None)],
    },
    ReportingCluster {
        cluster: CLUSTER_ON_OFF,
        name: "light_state",
        attributes: &[record(0x0000, TYPE_BOOLEAN, 0, 3600, None)],
    },
    ReportingCluster {
        cluster: CLUSTER_LEVEL,
        name: "light_brightness",
        attributes: &[record(0x0000, TYPE_UINT8, 1, 3600, Some(1))],
    },
    ReportingCluster {
        cluster: CLUSTER_POWER,
        name: "battery",
        attributes: &[
            record(0x0021, TYPE_UINT8, 3600, 62_000, Some(2)),
            record(0x0020, TYPE_UINT8, 3600, 62_000, Some(1)),
        ],
    },
];

/// The reporting setup of a cluster, if it is one of the table.
pub fn reporting_cluster(cluster: u16) -> Option<&'static ReportingCluster> {
    REPORTING.iter().find(|r| r.cluster == cluster)
}

/// The name shown for a cluster in the `reporting:` line: what it measures, or its number.
pub fn cluster_name(cluster: u16) -> String {
    reporting_cluster(cluster).map_or_else(|| format!("0x{cluster:04X}"), |r| r.name.to_string())
}

/// The clusters of `inputs` that are set up, in the table's order.
pub fn clusters_to_set_up(inputs: &[u16]) -> Vec<u16> {
    REPORTING
        .iter()
        .map(|r| r.cluster)
        .filter(|c| inputs.contains(c))
        .collect()
}
