//! The motion sensors of a zone, found when an instance starts and again at each settle, and
//! what counts as motion in a raw value (`resolveMotionSources`, `isMotionActive` of the recipe).

use crate::devices::Id;
use crate::home::{Home, events::EventValue};

/// A motion source: a device and the key of its motion data.
pub type MotionSource = (Id, Box<str>);

/// Sowel's `isMotionActive`: a boolean as it is, a number other than 0, a text (trimmed, any
/// case) among `true`, `on`, `1`, `occupied`, `detected`; anything else is not motion.
pub fn is_motion_active(value: &EventValue) -> bool {
    match value {
        EventValue::Bool(b) => *b,
        EventValue::Number(n) => *n != 0.0,
        EventValue::Text(text) => matches!(
            text.trim().to_lowercase().as_str(),
            "true" | "on" | "1" | "occupied" | "detected"
        ),
        EventValue::Null => false,
    }
}

/// The motion sources of a zone and the zones below it: for each enabled equipment, each data
/// binding whose data has the category `motion`. Each (device, key) once. At most one per data
/// of a device, so at most 64 x 16.
pub fn resolve_motion_sources(home: &Home, zone: &Id) -> Vec<MotionSource> {
    let mut sources: Vec<MotionSource> = Vec::new();
    for item in home.equipments().list() {
        if !item.equipment.enabled || !home.zones().is_within(&item.equipment.zone_id, zone) {
            continue;
        }
        for binding in &item.data_bindings {
            let Some((device, data)) = home.devices.data_of(&binding.device_data_id) else {
                continue;
            };
            if &*data.category != "motion" {
                continue;
            }
            if !sources
                .iter()
                .any(|(d, k)| *d == device.id && **k == *data.key)
            {
                sources.push((device.id, data.key.as_ref().into()));
            }
        }
    }
    sources
}
