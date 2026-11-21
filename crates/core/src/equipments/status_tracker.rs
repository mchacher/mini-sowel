//! The last status of each equipment, to tell when one changes: Sowel's `EquipmentStatusTracker`
//! (`src/equipments/equipment-status-tracker.ts`). The first time an equipment is seen its status
//! is only remembered: no event.

use super::view::status_of;
use super::{EquipmentStatus, Equipments, MAX_EQUIPMENTS};
use crate::devices::{Devices, Id};

/// A status that changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transition {
    pub equipment: Id,
    pub name: String,
    pub old: EquipmentStatus,
    pub new: EquipmentStatus,
}

/// The statuses seen last, at most one per equipment (so at most `MAX_EQUIPMENTS`).
#[derive(Debug, Default)]
pub struct StatusTracker {
    last: Vec<(Id, EquipmentStatus)>,
}

impl StatusTracker {
    pub fn new() -> Self {
        StatusTracker::default()
    }

    /// Forgets an equipment (it was deleted).
    pub fn forget(&mut self, equipment: &Id) {
        self.last.retain(|(id, _)| id != equipment);
    }

    /// Compares the status of every equipment as of `now` with the last seen, remembers the new
    /// ones, and returns the changes (none for an equipment seen for the first time).
    pub fn check(
        &mut self,
        devices: &Devices,
        equipments: &Equipments,
        now: u64,
    ) -> Vec<Transition> {
        let mut changes = Vec::new();
        for item in equipments.list() {
            let id = item.equipment.id;
            // Looked up binding by binding, no index: an index of every data would be the largest
            // allocation of a recompute.
            let (status, _) = status_of(devices, item, now);
            match self.last.iter().position(|(known, _)| *known == id) {
                Some(at) if self.last[at].1 == status => {}
                Some(at) => {
                    changes.push(Transition {
                        equipment: id,
                        name: item.equipment.name.to_string(),
                        old: self.last[at].1,
                        new: status,
                    });
                    self.last[at].1 = status;
                }
                None if self.last.len() < MAX_EQUIPMENTS => self.last.push((id, status)),
                None => {}
            }
        }
        // An equipment that is gone (deleted in a way that did not say so) is forgotten.
        self.last
            .retain(|(id, _)| equipments.list().iter().any(|e| e.equipment.id == *id));
        changes
    }
}
