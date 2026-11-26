//! What a recipe sees of the home: Sowel's names and shapes, as JSON (`camelCase`).

use serde::{Deserialize, Serialize};

use crate::event::EventValue;

/// An equipment as a recipe sees it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EquipmentView {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub enabled: bool,
    pub zone_id: String,
    /// At most 16 (the core's bound).
    pub data_bindings: Vec<DataBindingView>,
    /// At most 16 (the core's bound).
    pub order_bindings: Vec<OrderBindingView>,
}

/// A data binding with the data behind it; `category`, `deviceId` and `key` are absent for a
/// binding whose device data is gone, and its value is `null`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataBindingView {
    pub id: String,
    pub alias: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    pub value: EventValue,
}

/// An order binding; `category` is absent when its device order has none or is gone.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderBindingView {
    pub id: String,
    pub alias: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
}

/// What recipes read of a zone: `motion` and `luminosity` from the zone reading, `motionSensors`
/// from the zone's aggregate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZoneData {
    pub motion: bool,
    pub luminosity: Option<i64>,
    pub motion_sensors: u32,
}

/// Sowel's `getSunlight`: `null`s when unknown.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sunlight {
    pub is_daylight: Option<bool>,
    pub sunrise: Option<String>,
    pub sunset: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bounds::MAX_VALUE_TEXT_BYTES;

    /// The size one view stays under (the largest the engine builds is about 4 KB).
    const MAX_TEXT_BYTES: usize = 8 * 1024;

    /// The largest view the core allows: 16 data bindings with the longest keys, aliases,
    /// categories and values, and 16 order bindings with the longest names.
    #[test]
    fn the_largest_equipment_view_at_the_cores_bounds_fits_one_text() {
        let id = "00000000-0000-4000-8000-000000000001".to_string();
        let view = EquipmentView {
            id: id.clone(),
            name: "n".repeat(64),
            kind: "electric_vehicle".into(),
            enabled: true,
            zone_id: id.clone(),
            data_bindings: (0..16)
                .map(|_| DataBindingView {
                    id: id.clone(),
                    alias: "a".repeat(32),
                    category: Some("c".repeat(32)),
                    device_id: Some(id.clone()),
                    key: Some("k".repeat(32)),
                    value: EventValue::Text("v".repeat(MAX_VALUE_TEXT_BYTES)),
                })
                .collect(),
            order_bindings: (0..16)
                .map(|_| OrderBindingView {
                    id: id.clone(),
                    alias: "a".repeat(32),
                    category: Some("c".repeat(32)),
                })
                .collect(),
        };
        let text = serde_json::to_string(&view).unwrap();
        assert!(text.len() < MAX_TEXT_BYTES, "{} bytes", text.len());
        assert_eq!(serde_json::from_str::<EquipmentView>(&text).unwrap(), view);
    }

    #[test]
    fn a_zone_and_the_sun_are_json_with_sowels_names() {
        let zone = ZoneData {
            motion: true,
            luminosity: Some(40),
            motion_sensors: 2,
        };
        assert_eq!(
            serde_json::to_string(&zone).unwrap(),
            r#"{"motion":true,"luminosity":40,"motionSensors":2}"#
        );
        assert_eq!(
            serde_json::to_string(&Sunlight::default()).unwrap(),
            r#"{"isDaylight":null,"sunrise":null,"sunset":null}"#
        );
    }
}
