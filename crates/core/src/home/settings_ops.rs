//! The home's settings (spec 026): their change flag for the saver, the sun data computed from them
//! (one value cached per wall-clock minute), and the import of a backup document, checked on a copy
//! of the zones before anything changes.

use serde::Serialize;
use serde_json::{Map, Value};

use super::{Home, ListCursor};
use crate::devices::Id;
use crate::failure::{Failure, Outcome};
use crate::live::{EngineEvent, Topic};
use crate::settings::Settings;
use crate::settings::backup::{self, Backup, BackupZone, invalid};
use crate::settings::sun::{self, Sunlight};
use crate::zones::aggregate::ZoneAggregatedData;
use crate::zones::{NewZone, Zone, ZoneUpdate};

/// What `POST /backup` answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSummary {
    pub success: bool,
    /// The keys set after the import.
    pub settings: usize,
    pub zones_created: usize,
    pub zones_updated: usize,
    pub zones_unchanged: usize,
}

/// A zone's fields as the document gives them, compared with the zone's.
fn same(zone: &Zone, wanted: &BackupZone, parent: Option<Id>) -> bool {
    zone.name == wanted.name
        && zone.parent_id == parent
        && zone.icon == wanted.icon
        && zone.description == wanted.description
        && zone.display_order == wanted.display_order
}

/// A refusal of `Zones::create` or `update` for a zone of the document.
fn refused(failure: Failure, id: &Id) -> Failure {
    if failure.message.starts_with("Too many zones") {
        invalid("too many zones")
    } else {
        invalid(format_args!("{} in zone {id}", failure.message))
    }
}

impl Home {
    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// The settings read from `settings.json`; `repaired`: something was dropped, so the file is
    /// to be written again.
    pub fn load_settings(&mut self, settings: Settings, repaired: bool) {
        self.settings = settings;
        self.settings_changed |= repaired;
        self.forget_sunlight();
    }

    /// `PUT /settings` (R1.3): the body checked whole, then applied; the settings are marked to be
    /// written only when a value changed. `settings.changed` carries the keys of the body.
    pub fn put_settings(&mut self, entries: &Map<String, Value>) -> Outcome<()> {
        let checked = Settings::check(entries)?;
        let changed = self.settings.apply(checked);
        if !changed.is_empty() {
            self.settings_changed = true;
            self.forget_sunlight();
        }
        self.live
            .emit(Topic::System, || EngineEvent::SettingsChanged {
                keys: entries.keys().cloned().collect(),
            });
        Ok(())
    }

    /// True when the settings changed since they were last taken.
    pub fn settings_changed(&self) -> bool {
        self.settings_changed
    }

    /// `settings.json` as a cursor of one chunk if the settings changed, and the flag cleared.
    pub fn take_settings_cursor(&mut self) -> Option<ListCursor> {
        if !self.settings_changed {
            return None;
        }
        self.settings_changed = false;
        Some(ListCursor::saved_settings())
    }

    pub fn mark_settings_changed(&mut self) {
        self.settings_changed = true;
    }

    // ---- The sun ----

    fn forget_sunlight(&mut self) {
        *self.sun_cache.get_mut() = None;
    }

    /// The sun data at `now` (R2.3): computed at most once per wall-clock minute, the cache cleared
    /// by any settings change.
    pub fn sunlight(&self, now: u64) -> Sunlight {
        let minute = now / 60_000;
        if let Ok(cache) = self.sun_cache.try_borrow()
            && let Some((at, value)) = cache.as_ref()
            && *at == minute
        {
            return value.clone();
        }
        let value = sun::sunlight(&self.settings, now);
        if let Ok(mut cache) = self.sun_cache.try_borrow_mut() {
            *cache = Some((minute, value.clone()));
        }
        self.sun_computations
            .set(self.sun_computations.get().saturating_add(1));
        value
    }

    /// How many times the sun data was computed since the home was made (it stops at its
    /// maximum).
    pub fn sun_computations(&self) -> u32 {
        self.sun_computations.get()
    }

    /// The root zone's aggregate gets the sun data (Sowel's `injectSunlightData`).
    pub(super) fn inject_sunlight(&self, zone: &Id, data: &mut ZoneAggregatedData, now: u64) {
        if *zone != Id::ROOT_ZONE {
            return;
        }
        let sun = self.sunlight(now);
        data.sunrise = sun.sunrise;
        data.sunset = sun.sunset;
        data.is_daylight = sun.is_daylight;
    }

    /// Emits `sunlight.changed` when the sun data differs from the last the aggregation saw.
    pub(super) fn note_sunlight(&mut self, now: u64) {
        let sun = self.sunlight(now);
        if sun != self.sun_seen {
            self.sun_seen = sun;
            self.live
                .emit(Topic::System, || EngineEvent::SunlightChanged);
        }
    }

    // ---- Backup ----

    /// `GET /backup`: the document's compact JSON.
    pub fn export_backup(&self) -> Result<String, String> {
        backup::export(&self.settings, self.zones.list())
    }

    /// `POST /backup` (R3.2 to R3.4): the zones of the document applied to a copy (new zones
    /// first, in passes, then the zones the home has, in the document's order), the settings
    /// replaced whole; nothing changes when any of it is refused.
    pub fn import_backup(&mut self, document: Backup, now: u64) -> Outcome<ImportSummary> {
        let mut zones = self.zones.clone();
        // At most one per zone of the document (`MAX_ZONES`).
        let mut created: Vec<Zone> = Vec::new();
        let mut updated: Vec<Zone> = Vec::new();
        let mut pending: Vec<&BackupZone> = document
            .zones
            .iter()
            .filter(|z| zones.get(&z.id).is_none())
            .collect();
        let new_ids: Vec<Id> = pending.iter().map(|z| z.id).collect();
        while let Some(first) = pending.first() {
            let first = first.id;
            let before = pending.len();
            let mut rest = Vec::new();
            for wanted in pending {
                if wanted
                    .parent
                    .as_deref()
                    .and_then(|p| zones.find(p))
                    .is_none()
                {
                    rest.push(wanted);
                    continue;
                }
                let zone = zones
                    .create(
                        NewZone {
                            name: wanted.name.clone(),
                            parent: wanted.parent.clone(),
                            icon: wanted.icon.clone(),
                            description: wanted.description.clone(),
                            display_order: wanted.display_order,
                        },
                        now,
                        wanted.id,
                    )
                    .map_err(|e| refused(e, &wanted.id))?;
                created.push(zone);
            }
            if rest.len() == before {
                return Err(invalid(format_args!("parent not found for zone {first}")));
            }
            pending = rest;
        }
        let mut unchanged = 0;
        for wanted in document.zones.iter().filter(|z| !new_ids.contains(&z.id)) {
            let parent = wanted
                .parent
                .as_deref()
                .and_then(|p| zones.find(p))
                .map(|z| z.id);
            let Some(zone) = zones.get(&wanted.id) else {
                continue;
            };
            if same(zone, wanted, parent) {
                unchanged += 1;
                continue;
            }
            let zone = zones
                .update(
                    &wanted.id.to_string(),
                    ZoneUpdate {
                        name: Some(wanted.name.clone()),
                        parent: wanted.parent.clone().map(Some),
                        icon: Some(wanted.icon.clone()),
                        description: Some(wanted.description.clone()),
                        display_order: Some(wanted.display_order),
                    },
                    now,
                )
                .map_err(|e| refused(e, &wanted.id))?;
            updated.push(zone);
        }
        // Everything was checked: the copy replaces the zones, the settings are replaced.
        let changed = self.settings.differences(&document.settings);
        if !created.is_empty() || !updated.is_empty() {
            self.zones = zones;
            self.zones_changed = true;
        }
        if !changed.is_empty() {
            self.settings = document.settings;
            self.settings_changed = true;
            self.forget_sunlight();
            self.live
                .emit(Topic::System, || EngineEvent::SettingsChanged {
                    keys: changed.iter().map(|k| (*k).to_string()).collect(),
                });
        }
        let summary = ImportSummary {
            success: true,
            settings: self.settings.count(),
            zones_created: created.len(),
            zones_updated: updated.len(),
            zones_unchanged: unchanged,
        };
        for zone in created {
            self.live
                .emit(Topic::Zones, || EngineEvent::ZoneCreated { zone });
        }
        for zone in updated {
            self.live
                .emit(Topic::Zones, || EngineEvent::ZoneUpdated { zone });
        }
        Ok(summary)
    }
}
