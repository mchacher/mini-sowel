//! Sowel's `EngineEvent`s that Solite sends over the WebSocket, with Sowel's names and
//! fields (`src/shared/types.ts` T:1819-1974). The events of what Solite does not have
//! (modes, activity, energy, most of system...) are not here: see decision D18.

use serde::Serialize;
use serde_json::Value;

use super::topics::{Topic, topic_of_type};
use crate::devices::{Device, DeviceStatus, Id};
use crate::equipments::{Equipment, EquipmentStatus};
use crate::orders::OrderSource;
use crate::zones::Zone;
use crate::zones::aggregate::ZoneAggregatedData;

/// An event as Sowel's bus carries it: `type` first, then the fields, in camelCase. Built only
/// when a connected client listens to its topic, serialized once, shared by the clients.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all_fields = "camelCase")]
pub enum EngineEvent {
    /// A device the home did not have: Sowel's `device.discovered`, the `Device` without its
    /// data and orders (`Device` serialises as `GET /devices/:id` gives it, minus those two).
    #[serde(rename = "device.discovered")]
    DeviceDiscovered { device: Device },
    #[serde(rename = "device.removed")]
    DeviceRemoved { device_id: Id, device_name: String },
    #[serde(rename = "device.status_changed")]
    DeviceStatusChanged {
        device_id: Id,
        device_name: String,
        status: DeviceStatus,
    },
    #[serde(rename = "device.data.updated")]
    DeviceDataUpdated {
        device_id: Id,
        device_name: String,
        data_id: Id,
        key: String,
        value: Value,
        previous: Value,
        /// ISO 8601. Sowel's optional `sourceTimestamp` is not sent.
        timestamp: String,
    },
    #[serde(rename = "zone.created")]
    ZoneCreated { zone: Zone },
    #[serde(rename = "zone.updated")]
    ZoneUpdated { zone: Zone },
    #[serde(rename = "zone.removed")]
    ZoneRemoved { zone_id: Id, zone_name: String },
    #[serde(rename = "zone.data.changed")]
    ZoneDataChanged {
        zone_id: Id,
        aggregated_data: Box<ZoneAggregatedData>,
    },
    #[serde(rename = "equipment.created")]
    EquipmentCreated { equipment: Equipment },
    #[serde(rename = "equipment.updated")]
    EquipmentUpdated { equipment: Equipment },
    #[serde(rename = "equipment.removed")]
    EquipmentRemoved {
        equipment_id: Id,
        equipment_name: String,
        zone_id: Id,
    },
    #[serde(rename = "equipment.data.changed")]
    EquipmentDataChanged {
        equipment_id: Id,
        alias: String,
        value: Value,
        previous: Value,
    },
    #[serde(rename = "equipment.status.changed")]
    EquipmentStatusChanged {
        equipment_id: Id,
        equipment_name: String,
        old_status: EquipmentStatus,
        new_status: EquipmentStatus,
    },
    #[serde(rename = "equipment.order.executed")]
    EquipmentOrderExecuted {
        equipment_id: Id,
        order_alias: String,
        value: Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        source: Option<OrderSource>,
    },
    #[serde(rename = "equipment.order.failed")]
    EquipmentOrderFailed {
        equipment_id: Id,
        order_alias: String,
        value: Value,
        error: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        source: Option<OrderSource>,
    },
    #[serde(rename = "recipe.instance.created")]
    RecipeInstanceCreated { instance_id: Id, recipe_id: String },
    #[serde(rename = "recipe.instance.removed")]
    RecipeInstanceRemoved { instance_id: Id, recipe_id: String },
    #[serde(rename = "recipe.instance.started")]
    RecipeInstanceStarted { instance_id: Id, recipe_id: String },
    #[serde(rename = "recipe.instance.stopped")]
    RecipeInstanceStopped { instance_id: Id, recipe_id: String },
    #[serde(rename = "recipe.instance.state.changed")]
    RecipeInstanceStateChanged { instance_id: Id, recipe_id: String },
    #[serde(rename = "recipe.instance.error")]
    RecipeInstanceError {
        instance_id: Id,
        recipe_id: String,
        error: String,
    },
    /// The keys of an accepted `PUT /settings`, or those an import changed (topic `system`).
    #[serde(rename = "settings.changed")]
    SettingsChanged { keys: Vec<String> },
    /// The root zone's sun data changed (topic `system`).
    #[serde(rename = "sunlight.changed")]
    SunlightChanged,
}

impl EngineEvent {
    /// The `type` of every event Solite sends, in the order of the enum. A unit test proves it
    /// complete (one name per variant); the contract check (spec 027) compares it with Sowel's.
    pub const NAMES: &'static [&'static str] = &[
        "device.discovered",
        "device.removed",
        "device.status_changed",
        "device.data.updated",
        "zone.created",
        "zone.updated",
        "zone.removed",
        "zone.data.changed",
        "equipment.created",
        "equipment.updated",
        "equipment.removed",
        "equipment.data.changed",
        "equipment.status.changed",
        "equipment.order.executed",
        "equipment.order.failed",
        "recipe.instance.created",
        "recipe.instance.removed",
        "recipe.instance.started",
        "recipe.instance.stopped",
        "recipe.instance.state.changed",
        "recipe.instance.error",
        "settings.changed",
        "sunlight.changed",
    ];

    /// The `type` field.
    pub fn type_name(&self) -> &'static str {
        match self {
            EngineEvent::DeviceDiscovered { .. } => "device.discovered",
            EngineEvent::DeviceRemoved { .. } => "device.removed",
            EngineEvent::DeviceStatusChanged { .. } => "device.status_changed",
            EngineEvent::DeviceDataUpdated { .. } => "device.data.updated",
            EngineEvent::ZoneCreated { .. } => "zone.created",
            EngineEvent::ZoneUpdated { .. } => "zone.updated",
            EngineEvent::ZoneRemoved { .. } => "zone.removed",
            EngineEvent::ZoneDataChanged { .. } => "zone.data.changed",
            EngineEvent::EquipmentCreated { .. } => "equipment.created",
            EngineEvent::EquipmentUpdated { .. } => "equipment.updated",
            EngineEvent::EquipmentRemoved { .. } => "equipment.removed",
            EngineEvent::EquipmentDataChanged { .. } => "equipment.data.changed",
            EngineEvent::EquipmentStatusChanged { .. } => "equipment.status.changed",
            EngineEvent::EquipmentOrderExecuted { .. } => "equipment.order.executed",
            EngineEvent::EquipmentOrderFailed { .. } => "equipment.order.failed",
            EngineEvent::RecipeInstanceCreated { .. } => "recipe.instance.created",
            EngineEvent::RecipeInstanceRemoved { .. } => "recipe.instance.removed",
            EngineEvent::RecipeInstanceStarted { .. } => "recipe.instance.started",
            EngineEvent::RecipeInstanceStopped { .. } => "recipe.instance.stopped",
            EngineEvent::RecipeInstanceStateChanged { .. } => "recipe.instance.state.changed",
            EngineEvent::RecipeInstanceError { .. } => "recipe.instance.error",
            EngineEvent::SettingsChanged { .. } => "settings.changed",
            EngineEvent::SunlightChanged => "sunlight.changed",
        }
    }

    /// The topic a client must have subscribed to for this event.
    pub fn topic(&self) -> Topic {
        topic_of_type(self.type_name())
    }

    /// Sowel's dedup key (`src/api/websocket.ts` W:249-289): of two events with the same key in
    /// one batch, only the last is sent. The other events are never deduped.
    pub fn dedup_key(&self) -> Option<String> {
        match self {
            EngineEvent::DeviceDataUpdated { device_id, key, .. } => {
                Some(format!("d:{device_id}:{key}"))
            }
            EngineEvent::DeviceStatusChanged { device_id, .. } => Some(format!("ds:{device_id}")),
            EngineEvent::EquipmentDataChanged {
                equipment_id,
                alias,
                ..
            } => Some(format!("e:{equipment_id}:{alias}")),
            EngineEvent::EquipmentStatusChanged { equipment_id, .. } => {
                Some(format!("es:{equipment_id}"))
            }
            EngineEvent::ZoneDataChanged { zone_id, .. } => Some(format!("z:{zone_id}")),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::*;
    use crate::devices::{Devices, PowerSource};
    use crate::equipments::Equipments;
    use crate::zones::Zones;
    use crate::zones::aggregate::compute;

    /// A number per variant, by a `match` with no wildcard: a variant added to `EngineEvent`
    /// does not compile until it has an arm here, and then the test below makes the author give
    /// it a sample and a name in `NAMES`.
    fn index(event: &EngineEvent) -> usize {
        match event {
            EngineEvent::DeviceDiscovered { .. } => 0,
            EngineEvent::DeviceRemoved { .. } => 1,
            EngineEvent::DeviceStatusChanged { .. } => 2,
            EngineEvent::DeviceDataUpdated { .. } => 3,
            EngineEvent::ZoneCreated { .. } => 4,
            EngineEvent::ZoneUpdated { .. } => 5,
            EngineEvent::ZoneRemoved { .. } => 6,
            EngineEvent::ZoneDataChanged { .. } => 7,
            EngineEvent::EquipmentCreated { .. } => 8,
            EngineEvent::EquipmentUpdated { .. } => 9,
            EngineEvent::EquipmentRemoved { .. } => 10,
            EngineEvent::EquipmentDataChanged { .. } => 11,
            EngineEvent::EquipmentStatusChanged { .. } => 12,
            EngineEvent::EquipmentOrderExecuted { .. } => 13,
            EngineEvent::EquipmentOrderFailed { .. } => 14,
            EngineEvent::RecipeInstanceCreated { .. } => 15,
            EngineEvent::RecipeInstanceRemoved { .. } => 16,
            EngineEvent::RecipeInstanceStarted { .. } => 17,
            EngineEvent::RecipeInstanceStopped { .. } => 18,
            EngineEvent::RecipeInstanceStateChanged { .. } => 19,
            EngineEvent::RecipeInstanceError { .. } => 20,
            EngineEvent::SettingsChanged { .. } => 21,
            EngineEvent::SunlightChanged => 22,
        }
    }

    /// How many arms `index` has: one more than its last number.
    const VARIANTS: usize = 23;

    /// One event of each variant, in the order of `index`.
    fn samples() -> Vec<EngineEvent> {
        let id = Id::ROOT_ZONE;
        let device = Device {
            id,
            source_device_id: "0x00124b0012345601".into(),
            name: "Device".into(),
            manufacturer: None,
            model: None,
            zone_id: None,
            status: DeviceStatus::Online,
            power_source: PowerSource::Mains,
            last_seen: None,
            created_at: 0,
            updated_at: 0,
        };
        let (zones, _) = Zones::from_list(Vec::new(), 0);
        let zone = zones.list()[0].clone();
        let equipment = Equipment {
            id,
            name: "Equipment".into(),
            zone_id: id,
            kind: crate::equipments::EquipmentType::Sensor,
            icon: None,
            description: None,
            enabled: true,
            created_at: 0,
            updated_at: 0,
            extras: None,
        };
        let aggregated = compute(&zones, &Equipments::new(), &Devices::new(), 0)
            .into_iter()
            .next()
            .map(|(_, data)| data)
            .expect("the root zone has an aggregate");
        let text = String::new;
        vec![
            EngineEvent::DeviceDiscovered { device },
            EngineEvent::DeviceRemoved {
                device_id: id,
                device_name: text(),
            },
            EngineEvent::DeviceStatusChanged {
                device_id: id,
                device_name: text(),
                status: DeviceStatus::Offline,
            },
            EngineEvent::DeviceDataUpdated {
                device_id: id,
                device_name: text(),
                data_id: id,
                key: text(),
                value: Value::Null,
                previous: Value::Null,
                timestamp: text(),
            },
            EngineEvent::ZoneCreated { zone: zone.clone() },
            EngineEvent::ZoneUpdated { zone },
            EngineEvent::ZoneRemoved {
                zone_id: id,
                zone_name: text(),
            },
            EngineEvent::ZoneDataChanged {
                zone_id: id,
                aggregated_data: Box::new(aggregated),
            },
            EngineEvent::EquipmentCreated {
                equipment: equipment.clone(),
            },
            EngineEvent::EquipmentUpdated { equipment },
            EngineEvent::EquipmentRemoved {
                equipment_id: id,
                equipment_name: text(),
                zone_id: id,
            },
            EngineEvent::EquipmentDataChanged {
                equipment_id: id,
                alias: text(),
                value: Value::Null,
                previous: Value::Null,
            },
            EngineEvent::EquipmentStatusChanged {
                equipment_id: id,
                equipment_name: text(),
                old_status: EquipmentStatus::Online,
                new_status: EquipmentStatus::Offline,
            },
            EngineEvent::EquipmentOrderExecuted {
                equipment_id: id,
                order_alias: text(),
                value: Value::Null,
                source: None,
            },
            EngineEvent::EquipmentOrderFailed {
                equipment_id: id,
                order_alias: text(),
                value: Value::Null,
                error: text(),
                source: None,
            },
            EngineEvent::RecipeInstanceCreated {
                instance_id: id,
                recipe_id: text(),
            },
            EngineEvent::RecipeInstanceRemoved {
                instance_id: id,
                recipe_id: text(),
            },
            EngineEvent::RecipeInstanceStarted {
                instance_id: id,
                recipe_id: text(),
            },
            EngineEvent::RecipeInstanceStopped {
                instance_id: id,
                recipe_id: text(),
            },
            EngineEvent::RecipeInstanceStateChanged {
                instance_id: id,
                recipe_id: text(),
            },
            EngineEvent::RecipeInstanceError {
                instance_id: id,
                recipe_id: text(),
                error: text(),
            },
            EngineEvent::SettingsChanged { keys: Vec::new() },
            EngineEvent::SunlightChanged,
        ]
    }

    #[test]
    fn names_lists_one_name_per_variant() {
        let samples = samples();
        assert_eq!(samples.len(), VARIANTS, "a sample per variant");
        for (position, event) in samples.iter().enumerate() {
            assert_eq!(
                index(event),
                position,
                "samples follow the order of `index`"
            );
            assert_eq!(
                EngineEvent::NAMES[position],
                event.type_name(),
                "NAMES[{position}] is the `type` of its variant"
            );
            // The name written on the wire is the one listed.
            let wire = serde_json::to_value(event).expect("an event serialises");
            assert_eq!(wire["type"], event.type_name());
        }
        assert_eq!(
            EngineEvent::NAMES.len(),
            VARIANTS,
            "no name without a variant"
        );
    }
}
