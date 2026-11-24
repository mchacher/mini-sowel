//! The changes of equipments and bindings, with Sowel's rules and texts
//! (`src/equipments/equipment-manager.ts`).

use super::{Home, new_id};
use crate::devices::{Id, Random};
use crate::equipments::{
    Alias, DataBinding, Equipment, EquipmentType, Extras, Field, MAX_BINDINGS, MAX_EQUIPMENTS,
    MAX_EXTRAS_BYTES, MAX_EXTRAS_TOTAL_BYTES, OrderBinding, Piece, StoredEquipment,
};
use crate::failure::{Failure, Outcome};
use crate::live::{EngineEvent, Topic};
use crate::recipes::Settle;

/// What `POST /equipments` gives. `kind` and `zone_id` are as written.
#[derive(Debug, Default)]
pub struct NewEquipment {
    pub name: String,
    pub kind: String,
    pub zone_id: String,
    pub icon: Option<String>,
    pub description: Option<String>,
    pub device_ids: Vec<String>,
}

/// What `PUT /equipments/:id` gives: each field absent (keep) or its new value; for `icon` and
/// `description` the inner `None` is JSON `null`.
#[derive(Debug, Default)]
pub struct EquipmentUpdate {
    pub name: Option<String>,
    pub kind: Option<String>,
    pub zone_id: Option<String>,
    pub icon: Option<Option<String>>,
    pub description: Option<Option<String>>,
    pub enabled: Option<bool>,
    /// The optional fields the body gave, each with its value or `None` for JSON `null`, of the
    /// shape `Field::parse` checked. Absent from the list: kept.
    pub extras: Vec<(Field, Option<Piece>)>,
}

/// The fields whose rules need what Solite does not have (Sowel's energy arbitration, metering
/// graph, solar forecast, timed commands, vehicles, and `invertDirection`, which Sowel applies to
/// the shutter orders and to an on/off order sent without a value): they are kept as stored,
/// never changed.
const REFUSED: [Field; 8] = [
    Field::EnergyProfile,
    Field::SolarProfile,
    Field::TimedCommand,
    Field::MeteringParentId,
    Field::UsualVehicleId,
    Field::SeparateSupply,
    Field::LocalProduction,
    Field::InvertDirection,
];

/// Sowel's `NON_SUBMETER_TYPES` (`src/equipments/metering.ts`): the house total, the two
/// production surfaces and a vehicle.
fn non_submeter(kind: EquipmentType) -> bool {
    matches!(
        kind,
        EquipmentType::MainEnergyMeter
            | EquipmentType::EnergyProductionMeter
            | EquipmentType::SolarPanel
            | EquipmentType::ElectricVehicle
    )
}

/// What a `PUT` makes of the optional fields: `kind` is the resulting type, `type_given` whether
/// the body had a `type`. Sowel's rules on the resulting type and flags, with Sowel's codes
/// (`src/api/routes/equipments.ts`, `equipment-manager.ts`); nothing is changed here. `Ok(None)`:
/// the equipment ends with none.
fn resulting_extras(
    stored: &Equipment,
    kind: EquipmentType,
    type_given: bool,
    given: &[(Field, Option<Piece>)],
) -> Outcome<Option<Box<Extras>>> {
    let mut extras: Extras = stored.extras.as_deref().cloned().unwrap_or_default();
    let name = &stored.name;
    for (field, piece) in given {
        if !REFUSED.contains(field) {
            continue;
        }
        // Given back as stored is no change: so is `null` with nothing stored, and `false` for
        // the three flags whose default is `false`. Anything else would change what Solite cannot
        // check.
        let same = match (extras.get(*field), piece) {
            (None, None) => true,
            (None, Some(Piece::Bool(false))) => {
                matches!(
                    field,
                    Field::SeparateSupply | Field::LocalProduction | Field::InvertDirection
                )
            }
            (held, piece) => held == *piece,
        };
        if !same {
            return Err(Failure::new(
                400,
                format!("Not supported: {}", field.name()),
            ));
        }
    }
    let gave_true = |field: Field| {
        given
            .iter()
            .any(|(f, piece)| *f == field && *piece == Some(Piece::Bool(true)))
    };
    // Sowel checks the resulting (type, flag) pair when the body gives the flag or a type.
    let separate = extras.separate_supply == Some(true);
    let local = extras.local_production == Some(true);
    if (gave_true(Field::SeparateSupply) || type_given) && separate && non_submeter(kind) {
        return Err(Failure::coded(
            400,
            "SeparateSupplyNotApplicable",
            format!(
                "{name} is the house total or a production meter — the reconciliation reference \
                 cannot be outside itself"
            ),
        ));
    }
    if (gave_true(Field::LocalProduction) || gave_true(Field::SeparateSupply) || type_given)
        && local
        && (non_submeter(kind) || separate)
    {
        let message = if separate {
            format!("{name} is fed by a separate supply — its production never reaches the house")
        } else {
            format!(
                "{name} is the house total or a production meter — it cannot also carry local \
                 production"
            )
        };
        return Err(Failure::coded(400, "LocalProductionNotApplicable", message));
    }
    for (field, piece) in given {
        if REFUSED.contains(field) {
            continue;
        }
        if *field == Field::Family && piece.is_some() && kind != EquipmentType::Switch {
            return Err(Failure::coded(
                400,
                "FamilyNotApplicable",
                format!(
                    "{name} is a {} — only a switch can be given a family",
                    kind.as_str()
                ),
            ));
        }
        extras.set(*field, piece.clone());
    }
    // Only a switch carries a family: whatever stops being a switch loses it.
    if kind != EquipmentType::Switch {
        extras.family = None;
    }
    Ok(extras.into_option())
}

fn invalid_type(text: &str) -> Failure {
    Failure::new(400, format!("Invalid equipment type: {text}"))
}

fn not_found() -> Failure {
    Failure::new(404, "Equipment not found")
}

impl Home {
    /// `equipment.updated`, after a binding was added or removed.
    pub(super) fn announce_equipment_updated(&self, id: &str) {
        if let Some(item) = self.equipments.get(id) {
            self.live
                .emit(Topic::Equipments, || EngineEvent::EquipmentUpdated {
                    equipment: item.equipment.clone(),
                });
        }
    }

    /// Creates an equipment; with `device_ids`, binds every data and every order of each device
    /// with the key as alias (`createWithAutoBindings`). A device that does not exist is
    /// skipped (logged); so is a binding whose alias is taken or past `MAX_BINDINGS`.
    pub fn create_equipment(
        &mut self,
        input: NewEquipment,
        now: u64,
        random: Random,
    ) -> Outcome<StoredEquipment> {
        let kind = EquipmentType::parse(&input.kind).ok_or_else(|| invalid_type(&input.kind))?;
        let zone = self
            .zones
            .find(&input.zone_id)
            .map(|z| z.id)
            .ok_or_else(|| Failure::new(404, format!("Zone not found: {}", input.zone_id)))?;
        if self.equipments.list().len() >= MAX_EQUIPMENTS {
            return Err(Failure::new(
                400,
                format!("Too many equipments ({MAX_EQUIPMENTS})"),
            ));
        }
        let id = new_id(random)?;
        let mut item = StoredEquipment {
            equipment: Equipment {
                id,
                name: input.name.into(),
                zone_id: zone,
                kind,
                icon: input.icon.map(Into::into),
                description: input.description.map(Into::into),
                enabled: true,
                created_at: now,
                updated_at: now,
                // Sowel's create takes none of the optional fields.
                extras: None,
            },
            data_bindings: Vec::new(),
            order_bindings: Vec::new(),
        };
        for text in &input.device_ids {
            let Some(device) = Id::parse(text).and_then(|id| self.devices.get_by_id(&id)) else {
                log::warn!("equipment {}: device {text} not found, not bound", id);
                continue;
            };
            for data in &device.data {
                if item.data_bindings.iter().any(|b| b.alias == *data.key) {
                    continue;
                }
                if item.data_bindings.len() >= MAX_BINDINGS {
                    log::warn!(
                        "equipment {id}: more than {MAX_BINDINGS} data bindings, {} not bound",
                        data.key
                    );
                    continue;
                }
                item.data_bindings.push(DataBinding {
                    id: new_id(random)?,
                    device_data_id: data.id,
                    alias: Alias::new(&data.key),
                });
            }
            for order in &device.orders {
                if item.order_bindings.iter().any(|b| b.alias == *order.key) {
                    continue;
                }
                if item.order_bindings.len() >= MAX_BINDINGS {
                    log::warn!(
                        "equipment {id}: more than {MAX_BINDINGS} order bindings, {} not bound",
                        order.key
                    );
                    continue;
                }
                item.order_bindings.push(OrderBinding {
                    id: new_id(random)?,
                    device_order_id: order.id,
                    alias: Alias::new(&order.key),
                });
            }
        }
        // Memory: bindings have a bound, so no spare room is kept for them.
        item.data_bindings.shrink_to_fit();
        item.order_bindings.shrink_to_fit();
        self.equipments.list_mut().push(item.clone());
        self.equipments.list_mut().shrink_to_fit();
        self.equipments_changed = true;
        self.live
            .emit(Topic::Equipments, || EngineEvent::EquipmentCreated {
                equipment: item.equipment.clone(),
            });
        Ok(item)
    }

    pub fn update_equipment(
        &mut self,
        id: &str,
        input: EquipmentUpdate,
        now: u64,
    ) -> Outcome<Equipment> {
        if self.equipments.get(id).is_none() {
            return Err(not_found());
        }
        let kind = match &input.kind {
            None => None,
            Some(text) => Some(EquipmentType::parse(text).ok_or_else(|| invalid_type(text))?),
        };
        let zone = match &input.zone_id {
            None => None,
            Some(text) => Some(
                self.zones
                    .find(text)
                    .map(|z| z.id)
                    .ok_or_else(|| Failure::new(404, format!("Zone not found: {text}")))?,
            ),
        };
        // The optional fields are settled, with Sowel's rules, before anything is changed.
        let extras = {
            let Some(item) = self.equipments.get(id) else {
                return Err(not_found());
            };
            let stored = &item.equipment;
            let extras = resulting_extras(
                stored,
                kind.unwrap_or(stored.kind),
                kind.is_some(),
                &input.extras,
            )?;
            let bytes = extras.as_deref().map_or(0, Extras::json_len);
            if bytes > MAX_EXTRAS_BYTES
                || self.equipments.extras_bytes(Some(&stored.id)) + bytes > MAX_EXTRAS_TOTAL_BYTES
            {
                return Err(Failure::new(
                    400,
                    format!(
                        "Equipment fields too large ({MAX_EXTRAS_BYTES} bytes each, \
                         {MAX_EXTRAS_TOTAL_BYTES} in all)"
                    ),
                ));
            }
            extras
        };
        let Some(item) = self.equipments.get_mut(id) else {
            return Err(not_found());
        };
        let equipment = &mut item.equipment;
        equipment.extras = extras;
        if let Some(name) = input.name {
            equipment.name = name.into();
        }
        if let Some(kind) = kind {
            equipment.kind = kind;
        }
        if let Some(zone) = zone {
            equipment.zone_id = zone;
        }
        if let Some(icon) = input.icon {
            equipment.icon = icon.map(Into::into);
        }
        if let Some(description) = input.description {
            equipment.description = description.map(Into::into);
        }
        if let Some(enabled) = input.enabled {
            equipment.enabled = enabled;
        }
        equipment.updated_at = now;
        let updated = equipment.clone();
        self.equipments_changed = true;
        self.live
            .emit(Topic::Equipments, || EngineEvent::EquipmentUpdated {
                equipment: updated.clone(),
            });
        self.settle_recipes(Settle::default(), now);
        Ok(updated)
    }

    pub fn delete_equipment(&mut self, id: &str, now: u64) -> Outcome<()> {
        let Some(parsed) = Id::parse(id) else {
            return Err(not_found());
        };
        let list = self.equipments.list_mut();
        let Some(index) = list.iter().position(|e| e.equipment.id == parsed) else {
            return Err(not_found());
        };
        let removed = list.remove(index).equipment;
        self.equipments_changed = true;
        self.tracker.forget(&removed.id);
        self.live
            .emit(Topic::Equipments, || EngineEvent::EquipmentRemoved {
                equipment_id: removed.id,
                equipment_name: removed.name.to_string(),
                zone_id: removed.zone_id,
            });
        let equipment = Some((removed.id, removed.name.to_string()));
        self.settle_recipes(
            Settle {
                equipment,
                zone: None,
            },
            now,
        );
        Ok(())
    }

    pub fn add_data_binding(
        &mut self,
        equipment: &str,
        data: &str,
        alias: &str,
        random: Random,
    ) -> Outcome<DataBinding> {
        let Some(item) = self.equipments.get(equipment) else {
            return Err(not_found());
        };
        let data_id = Id::parse(data)
            .filter(|id| self.devices.data_of(id).is_some())
            .ok_or_else(|| Failure::new(404, format!("DeviceData not found: {data}")))?;
        if item.data_bindings.iter().any(|b| b.alias == alias) {
            return Err(Failure::new(
                409,
                format!("Alias \"{alias}\" already exists on this equipment"),
            ));
        }
        if item.data_bindings.len() >= MAX_BINDINGS {
            return Err(Failure::new(
                400,
                format!("Too many bindings ({MAX_BINDINGS})"),
            ));
        }
        let binding = DataBinding {
            id: new_id(random)?,
            device_data_id: data_id,
            alias: Alias::new(alias),
        };
        if let Some(item) = self.equipments.get_mut(equipment) {
            item.data_bindings.push(binding.clone());
            item.data_bindings.shrink_to_fit();
        }
        self.equipments_changed = true;
        self.announce_equipment_updated(equipment);
        Ok(binding)
    }

    pub fn remove_data_binding(&mut self, equipment: &str, binding: &str, now: u64) -> Outcome<()> {
        let Some(item) = self.equipments.get_mut(equipment) else {
            return Err(not_found());
        };
        let Some(index) =
            Id::parse(binding).and_then(|id| item.data_bindings.iter().position(|b| b.id == id))
        else {
            return Err(Failure::new(404, "DataBinding not found"));
        };
        item.data_bindings.remove(index);
        self.equipments_changed = true;
        self.announce_equipment_updated(equipment);
        self.settle_recipes(Settle::default(), now);
        Ok(())
    }

    pub fn add_order_binding(
        &mut self,
        equipment: &str,
        order: &str,
        alias: &str,
        random: Random,
    ) -> Outcome<OrderBinding> {
        let Some(item) = self.equipments.get(equipment) else {
            return Err(not_found());
        };
        let order_id = Id::parse(order)
            .filter(|id| self.devices.order_of(id).is_some())
            .ok_or_else(|| Failure::new(404, format!("DeviceOrder not found: {order}")))?;
        if item.order_bindings.iter().any(|b| b.alias == alias) {
            return Err(Failure::new(
                409,
                format!("OrderBinding for alias \"{alias}\" with this device order already exists"),
            ));
        }
        if item.order_bindings.len() >= MAX_BINDINGS {
            return Err(Failure::new(
                400,
                format!("Too many bindings ({MAX_BINDINGS})"),
            ));
        }
        let binding = OrderBinding {
            id: new_id(random)?,
            device_order_id: order_id,
            alias: Alias::new(alias),
        };
        if let Some(item) = self.equipments.get_mut(equipment) {
            item.order_bindings.push(binding.clone());
            item.order_bindings.shrink_to_fit();
        }
        self.equipments_changed = true;
        self.announce_equipment_updated(equipment);
        Ok(binding)
    }

    pub fn remove_order_binding(
        &mut self,
        equipment: &str,
        binding: &str,
        now: u64,
    ) -> Outcome<()> {
        let Some(item) = self.equipments.get_mut(equipment) else {
            return Err(not_found());
        };
        let Some(index) =
            Id::parse(binding).and_then(|id| item.order_bindings.iter().position(|b| b.id == id))
        else {
            return Err(Failure::new(404, "OrderBinding not found"));
        };
        item.order_bindings.remove(index);
        self.equipments_changed = true;
        self.announce_equipment_updated(equipment);
        self.settle_recipes(Settle::default(), now);
        Ok(())
    }
}
