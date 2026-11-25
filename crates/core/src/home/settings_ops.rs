//! The home's settings (spec 026): their change flag for the saver, the sun data computed from them
//! (one value cached per wall-clock minute).

use serde_json::{Map, Value};

use super::{Home, ListCursor};
use crate::devices::Id;
use crate::failure::Outcome;
use crate::live::{EngineEvent, Topic};
use crate::settings::Settings;
use crate::settings::sun::{self, Sunlight};
use crate::zones::aggregate::ZoneAggregatedData;

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

    pub(super) fn forget_sunlight(&mut self) {
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
}
