//! What a recipe may ask of the home, and nothing else (spec 041, R2.2). The engine implements it
//! over the home (`NativeHost`, in the core), which every recipe the core runs reaches through. Ids are
//! canonical lower-case texts; an id written otherwise (capitals) is read as the same id, and the
//! views give it back in its canonical form.

use serde_json::Value;

use crate::Level;
use crate::view::{EquipmentView, Sunlight, ZoneData};

/// What `Host::order` answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderStatus {
    /// The order is in the queue.
    Sent,
    /// No such equipment, or no such order binding on it.
    NoBinding,
    /// The equipment is disabled: nothing is sent, and nothing is said.
    Disabled,
    /// The queue of recipe orders is full: the order is dropped (the engine logs it).
    QueueFull,
    /// The value is refused: the binding's devices have no command for it.
    Refused,
}

/// The home, as a recipe reads and commands it.
pub trait Host {
    /// The equipment with this id, `None` when there is none.
    fn equipment(&self, id: &str) -> Option<EquipmentView>;
    /// The ids of the enabled equipments of a zone and of the zones below it, in the home's order
    /// (at most `MAX_ZONE_EQUIPMENT_IDS`); empty for an unknown zone.
    fn zone_equipment_ids(&self, zone: &str) -> Vec<String>;
    /// What a zone reads now, `None` for a zone that does not exist.
    fn zone_data(&self, zone: &str) -> Option<ZoneData>;
    fn sunlight(&self) -> Sunlight;
    /// The home's offset from UTC, in seconds east, at this instant (milliseconds since 1970):
    /// what a time of day is read in (`UTC` unless the home has a time zone).
    fn utc_offset_seconds(&self, _utc_ms: u64) -> i64 {
        0
    }
    /// Sends `value` through that one order binding of that equipment.
    fn order(&self, equipment: &str, binding: &str, value: &Value) -> OrderStatus;
    /// A line for the instance's log.
    fn log(&self, level: Level, message: &str);
}
