//! Sowel's twelve optional `Equipment` fields (`src/shared/types.ts` T:419-500), kept as stored
//! instead of dropped (spec 027 R2.3). Solite gives them no behaviour: `Extras` holds them, writes
//! them back unchanged and serves them; the five that have rules without anything Solite lacks
//! can be set by `PUT` (`family`, `requireConfirmation`, `invertDirection`, `plugInHybrid`,
//! `trackOrderOutcome`), the other seven only kept. An object is held as the bytes it was written
//! with (`RawValue`), its inside unchecked.

use std::io;

use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

/// The most bytes the twelve fields of one equipment take, as written in JSON (keys included,
/// braces and commas included). Sowel-shaped objects for all twelve fit.
pub const MAX_EXTRAS_BYTES: usize = 1024;
/// The most bytes the fields of all equipments take together: the worst case grows by 8 KB, not
/// by 64 x 1 KB.
pub const MAX_EXTRAS_TOTAL_BYTES: usize = 8192;
/// The longest `meteringParentId` or `usualVehicleId`, in characters.
pub const MAX_ID_CHARS: usize = 64;

/// Sowel's `EquipmentFamily` (T:386), what a switch acts on or measures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EquipmentFamily {
    Lighting,
    Openings,
    Climate,
    Water,
    Energy,
    Sensors,
}

impl EquipmentFamily {
    /// Every value, in Sowel's order.
    pub const ALL: [EquipmentFamily; 6] = [
        EquipmentFamily::Lighting,
        EquipmentFamily::Openings,
        EquipmentFamily::Climate,
        EquipmentFamily::Water,
        EquipmentFamily::Energy,
        EquipmentFamily::Sensors,
    ];

    /// The family as Sowel writes it (`climate`).
    pub fn as_str(self) -> &'static str {
        match self {
            EquipmentFamily::Lighting => "lighting",
            EquipmentFamily::Openings => "openings",
            EquipmentFamily::Climate => "climate",
            EquipmentFamily::Water => "water",
            EquipmentFamily::Energy => "energy",
            EquipmentFamily::Sensors => "sensors",
        }
    }
}

/// A JSON object kept as written, key order and spacing included; compared by its bytes.
#[derive(Debug, Clone, Serialize)]
#[serde(transparent)]
pub struct RawObject(Box<RawValue>);

impl RawObject {
    /// `Some` for a text that is a JSON object, `None` for anything else.
    pub fn new(json: &str) -> Option<RawObject> {
        let raw: Box<RawValue> = serde_json::from_str(json).ok()?;
        raw.get().starts_with('{').then_some(RawObject(raw))
    }

    /// The object as written.
    pub fn get(&self) -> &str {
        self.0.get()
    }
}

impl PartialEq for RawObject {
    fn eq(&self, other: &RawObject) -> bool {
        self.get() == other.get()
    }
}

/// The twelve fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    EnergyProfile,
    RequireConfirmation,
    TrackOrderOutcome,
    MeteringParentId,
    SeparateSupply,
    LocalProduction,
    Family,
    PlugInHybrid,
    UsualVehicleId,
    SolarProfile,
    InvertDirection,
    TimedCommand,
}

/// A value of one field, of the shape its field takes.
#[derive(Debug, Clone, PartialEq)]
pub enum Piece {
    Bool(bool),
    Text(Box<str>),
    Family(EquipmentFamily),
    Object(RawObject),
}

impl Field {
    /// Every field, in the order Sowel lists them.
    pub const ALL: [Field; 12] = [
        Field::EnergyProfile,
        Field::RequireConfirmation,
        Field::TrackOrderOutcome,
        Field::MeteringParentId,
        Field::SeparateSupply,
        Field::LocalProduction,
        Field::Family,
        Field::PlugInHybrid,
        Field::UsualVehicleId,
        Field::SolarProfile,
        Field::InvertDirection,
        Field::TimedCommand,
    ];

    /// The field's name in JSON.
    pub fn name(self) -> &'static str {
        match self {
            Field::EnergyProfile => "energyProfile",
            Field::RequireConfirmation => "requireConfirmation",
            Field::TrackOrderOutcome => "trackOrderOutcome",
            Field::MeteringParentId => "meteringParentId",
            Field::SeparateSupply => "separateSupply",
            Field::LocalProduction => "localProduction",
            Field::Family => "family",
            Field::PlugInHybrid => "plugInHybrid",
            Field::UsualVehicleId => "usualVehicleId",
            Field::SolarProfile => "solarProfile",
            Field::InvertDirection => "invertDirection",
            Field::TimedCommand => "timedCommand",
        }
    }

    /// The field of this JSON name.
    pub fn from_name(name: &str) -> Option<Field> {
        Field::ALL.into_iter().find(|f| f.name() == name)
    }

    /// The value in this JSON text, `None` when it has not the field's shape (a boolean, a
    /// string of 1 to `MAX_ID_CHARS` characters, a family, a JSON object). `null` is no value.
    pub fn parse(self, json: &str) -> Option<Piece> {
        match self {
            Field::RequireConfirmation
            | Field::TrackOrderOutcome
            | Field::SeparateSupply
            | Field::LocalProduction
            | Field::PlugInHybrid
            | Field::InvertDirection => serde_json::from_str(json).ok().map(Piece::Bool),
            Field::MeteringParentId | Field::UsualVehicleId => {
                let text: String = serde_json::from_str(json).ok()?;
                let chars = text.chars().count();
                (1..=MAX_ID_CHARS)
                    .contains(&chars)
                    .then(|| Piece::Text(text.into()))
            }
            Field::Family => serde_json::from_str(json).ok().map(Piece::Family),
            Field::EnergyProfile | Field::SolarProfile | Field::TimedCommand => {
                RawObject::new(json).map(Piece::Object)
            }
        }
    }
}

/// Sowel's optional equipment fields; each `None` is absent (`null` is not stored).
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Extras {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub energy_profile: Option<RawObject>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub require_confirmation: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track_order_outcome: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metering_parent_id: Option<Box<str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub separate_supply: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_production: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub family: Option<EquipmentFamily>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plug_in_hybrid: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usual_vehicle_id: Option<Box<str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub solar_profile: Option<RawObject>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invert_direction: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timed_command: Option<RawObject>,
}

/// What `StoredEquipment` reads of the twelve fields: each as written, checked afterwards
/// (`RawValue` does not work through `flatten`, so it is read on its own).
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(super) struct RawExtras {
    #[serde(default)]
    energy_profile: Option<Box<RawValue>>,
    #[serde(default)]
    require_confirmation: Option<Box<RawValue>>,
    #[serde(default)]
    track_order_outcome: Option<Box<RawValue>>,
    #[serde(default)]
    metering_parent_id: Option<Box<RawValue>>,
    #[serde(default)]
    separate_supply: Option<Box<RawValue>>,
    #[serde(default)]
    local_production: Option<Box<RawValue>>,
    #[serde(default)]
    family: Option<Box<RawValue>>,
    #[serde(default)]
    plug_in_hybrid: Option<Box<RawValue>>,
    #[serde(default)]
    usual_vehicle_id: Option<Box<RawValue>>,
    #[serde(default)]
    solar_profile: Option<Box<RawValue>>,
    #[serde(default)]
    invert_direction: Option<Box<RawValue>>,
    #[serde(default)]
    timed_command: Option<Box<RawValue>>,
}

impl RawExtras {
    fn text(&self, field: Field) -> Option<&str> {
        let raw = match field {
            Field::EnergyProfile => &self.energy_profile,
            Field::RequireConfirmation => &self.require_confirmation,
            Field::TrackOrderOutcome => &self.track_order_outcome,
            Field::MeteringParentId => &self.metering_parent_id,
            Field::SeparateSupply => &self.separate_supply,
            Field::LocalProduction => &self.local_production,
            Field::Family => &self.family,
            Field::PlugInHybrid => &self.plug_in_hybrid,
            Field::UsualVehicleId => &self.usual_vehicle_id,
            Field::SolarProfile => &self.solar_profile,
            Field::InvertDirection => &self.invert_direction,
            Field::TimedCommand => &self.timed_command,
        };
        raw.as_deref().map(RawValue::get)
    }
}

/// A writer that only counts.
struct Count(usize);

impl io::Write for Count {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0 += bytes.len();
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Extras {
    /// The fields of a stored equipment, each checked for its shape: a field of the wrong shape
    /// is dropped with a warning naming the equipment. `None` when no field is left.
    pub(super) fn from_raw(raw: &RawExtras, equipment: &str) -> Option<Box<Extras>> {
        let mut extras = Extras::default();
        for field in Field::ALL {
            let Some(text) = raw.text(field) else {
                continue;
            };
            // `null` is Sowel's "absent / default": not stored.
            if text == "null" {
                continue;
            }
            match field.parse(text) {
                Some(piece) => {
                    extras.set(field, Some(piece));
                }
                None => log::warn!(
                    "equipment {equipment}: {} has the wrong shape, dropped",
                    field.name()
                ),
            }
        }
        extras.into_option()
    }

    /// The value of a field, if set.
    pub fn get(&self, field: Field) -> Option<Piece> {
        match field {
            Field::EnergyProfile => self.energy_profile.clone().map(Piece::Object),
            Field::RequireConfirmation => self.require_confirmation.map(Piece::Bool),
            Field::TrackOrderOutcome => self.track_order_outcome.map(Piece::Bool),
            Field::MeteringParentId => self.metering_parent_id.clone().map(Piece::Text),
            Field::SeparateSupply => self.separate_supply.map(Piece::Bool),
            Field::LocalProduction => self.local_production.map(Piece::Bool),
            Field::Family => self.family.map(Piece::Family),
            Field::PlugInHybrid => self.plug_in_hybrid.map(Piece::Bool),
            Field::UsualVehicleId => self.usual_vehicle_id.clone().map(Piece::Text),
            Field::SolarProfile => self.solar_profile.clone().map(Piece::Object),
            Field::InvertDirection => self.invert_direction.map(Piece::Bool),
            Field::TimedCommand => self.timed_command.clone().map(Piece::Object),
        }
    }

    /// Sets a field, or clears it with `None`. False (nothing changed) when the piece is not of
    /// the field's shape.
    pub fn set(&mut self, field: Field, piece: Option<Piece>) -> bool {
        match (field, piece) {
            (Field::EnergyProfile, None) => self.energy_profile = None,
            (Field::EnergyProfile, Some(Piece::Object(v))) => self.energy_profile = Some(v),
            (Field::RequireConfirmation, None) => self.require_confirmation = None,
            (Field::RequireConfirmation, Some(Piece::Bool(v))) => {
                self.require_confirmation = Some(v);
            }
            (Field::TrackOrderOutcome, None) => self.track_order_outcome = None,
            (Field::TrackOrderOutcome, Some(Piece::Bool(v))) => self.track_order_outcome = Some(v),
            (Field::MeteringParentId, None) => self.metering_parent_id = None,
            (Field::MeteringParentId, Some(Piece::Text(v))) => self.metering_parent_id = Some(v),
            (Field::SeparateSupply, None) => self.separate_supply = None,
            (Field::SeparateSupply, Some(Piece::Bool(v))) => self.separate_supply = Some(v),
            (Field::LocalProduction, None) => self.local_production = None,
            (Field::LocalProduction, Some(Piece::Bool(v))) => self.local_production = Some(v),
            (Field::Family, None) => self.family = None,
            (Field::Family, Some(Piece::Family(v))) => self.family = Some(v),
            (Field::PlugInHybrid, None) => self.plug_in_hybrid = None,
            (Field::PlugInHybrid, Some(Piece::Bool(v))) => self.plug_in_hybrid = Some(v),
            (Field::UsualVehicleId, None) => self.usual_vehicle_id = None,
            (Field::UsualVehicleId, Some(Piece::Text(v))) => self.usual_vehicle_id = Some(v),
            (Field::SolarProfile, None) => self.solar_profile = None,
            (Field::SolarProfile, Some(Piece::Object(v))) => self.solar_profile = Some(v),
            (Field::InvertDirection, None) => self.invert_direction = None,
            (Field::InvertDirection, Some(Piece::Bool(v))) => self.invert_direction = Some(v),
            (Field::TimedCommand, None) => self.timed_command = None,
            (Field::TimedCommand, Some(Piece::Object(v))) => self.timed_command = Some(v),
            _ => return false,
        }
        true
    }

    /// `None` when no field is set (an equipment then holds no `Extras` at all).
    pub fn into_option(self) -> Option<Box<Extras>> {
        (self != Extras::default()).then(|| Box::new(self))
    }

    /// The bytes of the fields as written in JSON (an object with the set fields); 0 for none.
    pub fn json_len(&self) -> usize {
        if *self == Extras::default() {
            return 0;
        }
        let mut count = Count(0);
        // Writing to a counter cannot fail; a failure would read as 0 bytes, past no bound, but
        // the serialisation of these fields has no failing path either.
        let _ = serde_json::to_writer(&mut count, self);
        count.0
    }

    /// The fields that fit: those of `self` in the order of `Field::ALL`, each kept unless it
    /// would take the equipment past `MAX_EXTRAS_BYTES`, or all equipments past
    /// `MAX_EXTRAS_TOTAL_BYTES` (`used` counts the ones before, and takes this one's bytes). A
    /// field left out is logged with the equipment's name.
    pub(super) fn bounded(self, equipment: &str, used: &mut usize) -> Option<Box<Extras>> {
        let mut kept = Extras::default();
        let mut kept_len = 0;
        for field in Field::ALL {
            let Some(piece) = self.get(field) else {
                continue;
            };
            let mut candidate = kept.clone();
            candidate.set(field, Some(piece));
            let len = candidate.json_len();
            if len > MAX_EXTRAS_BYTES || *used - kept_len + len > MAX_EXTRAS_TOTAL_BYTES {
                log::warn!(
                    "equipment {equipment}: {} is past the bound of the optional fields \
                     ({MAX_EXTRAS_BYTES} bytes each equipment, {MAX_EXTRAS_TOTAL_BYTES} in all), \
                     dropped",
                    field.name()
                );
                continue;
            }
            *used = *used - kept_len + len;
            kept_len = len;
            kept = candidate;
        }
        kept.into_option()
    }
}
