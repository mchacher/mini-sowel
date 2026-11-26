//! The motion sensors of a zone, found when an instance starts and again at each settle, and
//! what counts as motion in a raw value (`resolveMotionSources`, `isMotionActive` of the recipe).

use solite_recipe_sdk::{EventValue, Host, IdBytes, parse_id};

/// A motion source: a device id and the key of its motion data.
pub type MotionSource = (IdBytes, Box<str>);

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

/// The motion sources of a zone and the zones below it: for each enabled equipment (the host's
/// `zone_equipment_ids`), each data binding whose data has the category `motion`. Each (device,
/// key) once. At most one per data of a device, so at most 64 x 16.
pub fn resolve_motion_sources(host: &dyn Host, zone: &str) -> Vec<MotionSource> {
    let mut sources: Vec<MotionSource> = Vec::new();
    for id in host.zone_equipment_ids(zone) {
        let Some(view) = host.equipment(&id) else {
            continue;
        };
        for binding in &view.data_bindings {
            if binding.category.as_deref() != Some("motion") {
                continue;
            }
            let (Some(device), Some(key)) = (&binding.device_id, &binding.key) else {
                continue;
            };
            let Some(device) = parse_id(device) else {
                continue;
            };
            if !sources.iter().any(|(d, k)| *d == device && **k == **key) {
                sources.push((device, key.as_str().into()));
            }
        }
    }
    sources
}
