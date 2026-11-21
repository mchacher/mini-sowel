//! Sowel's `EngineEvent`s that mini-Sowel sends over the WebSocket, with Sowel's names and
//! fields (`src/shared/types.ts` T:1819-1974). The events of what mini-Sowel does not have
//! (`device.discovered`, modes, activity, energy, system...) are not here: see decision D18.

use serde::Serialize;
use serde_json::Value;

use super::topics::{Topic, topic_of_type};
use crate::devices::{DeviceStatus, Id};
use crate::equipments::{Equipment, EquipmentStatus};
use crate::orders::OrderSource;
use crate::zones::Zone;
use crate::zones::aggregate::ZoneAggregatedData;

/// An event as Sowel's bus carries it: `type` first, then the fields, in camelCase. Built only
/// when a connected client listens to its topic, serialized once, shared by the clients.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all_fields = "camelCase")]
pub enum EngineEvent {
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
}

impl EngineEvent {
    /// The `type` field.
    pub fn type_name(&self) -> &'static str {
        match self {
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
