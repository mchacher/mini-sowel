//! The home's side of the recipe installs (spec 042): the catalogue and the files it knows of, the
//! tickets of the recipe jobs taken under the lock (R3.1), the steps a job takes on the engine, the
//! boot load of the installed files (R3.7), `GET /system/resources` and the figures it reads.

use serde_json::{Value, json};

use super::Home;
use crate::devices::format_iso;
use crate::failure::Failure;
use crate::memory::{
    InternalResources, PsramResources, Resources, ScriptResources, StorageResources,
};
use crate::recipes::install::catalogue::{DEFAULT_CATALOGUE_URL, Entry};
use crate::recipes::install::{
    CatalogueState, FETCH_STACK_BYTES, MAX_REFUSED, RECIPE_INTERNAL_MARGIN, RecipeInstalls,
    Refused, SCRIPT_PSRAM_RESERVE,
};
use crate::settings;

/// The unit of the figures in the messages: KiB, the needed rounded up and the free rounded down.
#[cfg_attr(not(feature = "script-recipes"), allow(dead_code))]
fn kib_down(bytes: usize) -> usize {
    bytes / 1024
}

#[cfg_attr(not(feature = "script-recipes"), allow(dead_code))]
fn kib_up(bytes: usize) -> usize {
    bytes.div_ceil(1024)
}

#[cfg_attr(not(feature = "script-recipes"), allow(dead_code))]
impl Home {
    /// The recipe key and the memory probe of the program (spec 042 R1.5, R4.4): until set, no key
    /// (installs refused) and no memory read.
    pub fn set_recipe_installs(&mut self, installs: RecipeInstalls) {
        self.install.installs = installs;
    }

    pub fn recipe_installs(&self) -> &RecipeInstalls {
        &self.install.installs
    }

    /// The catalogue's address: the setting, or the default when unset or empty (R2.2).
    pub(crate) fn catalogue_url(&self) -> String {
        self.settings
            .get(settings::RECIPES_CATALOGUE)
            .filter(|url| !url.is_empty())
            .unwrap_or(DEFAULT_CATALOGUE_URL)
            .to_string()
    }

    /// The storage figures, read by the thread that wrote or removed a file (R5.2) and kept.
    pub fn set_storage_figures(&mut self, used: u64, total: Option<u64>) {
        self.install.storage = StorageResources { used, total };
    }

    /// The files the factory reset asks the saver to remove (R3.9), taken.
    pub fn take_removals(&mut self) -> Vec<String> {
        std::mem::take(&mut self.install.removals)
    }

    /// Puts back the files a removal failed for, to be tried again.
    pub fn restore_removals(&mut self, names: Vec<String>) {
        for name in names {
            if !self.install.removals.contains(&name) {
                self.install.removals.push(name);
            }
        }
    }

    /// True while files wait for the saver to remove them.
    pub fn has_removals(&self) -> bool {
        !self.install.removals.is_empty()
    }

    /// Keeps a catalogue read: the list, when, and no error.
    pub(crate) fn keep_catalogue(&mut self, entries: Vec<Entry>, wall: u64) {
        self.install.catalogue = CatalogueState::new(entries, wall);
    }

    /// A read that failed: the last list is kept, with the reason.
    pub(crate) fn catalogue_failed(&mut self, reason: String) {
        self.install.catalogue.error = Some(reason);
    }

    /// The catalogue's entry for an id, if one is held.
    pub(crate) fn catalogue_entry(&self, id: &str) -> Option<Entry> {
        self.install
            .catalogue
            .entries()
            .iter()
            .find(|entry| entry.id == id)
            .cloned()
    }

    /// True when a catalogue read is held (even an empty list).
    pub(crate) fn catalogue_held(&self) -> bool {
        self.install.catalogue.read_at.is_some()
    }

    pub(crate) fn refuse_file(&mut self, id: &str, reason: &str) {
        log::warn!("recipes: script {id}: {reason}");
        self.install.refused.retain(|refused| refused.id != id);
        if self.install.refused.len() < MAX_REFUSED {
            self.install.refused.push(Refused {
                id: id.to_string(),
                reason: reason.to_string(),
            });
        }
    }

    /// `GET /recipes/store` (R2.3): what the box holds, never the network.
    pub fn recipe_store(&self) -> Value {
        let state = &self.install;
        let recipes: Vec<Value> = state
            .catalogue
            .entries()
            .iter()
            .map(|entry| {
                let mut value = serde_json::to_value(entry).unwrap_or(Value::Null);
                if let Value::Object(map) = &mut value {
                    let refused = state
                        .refused
                        .iter()
                        .find(|refused| refused.id == entry.id)
                        .map(|refused| refused.reason.clone());
                    map.insert("installed".into(), self.installed_text(&entry.id));
                    map.insert("refused".into(), json!(refused));
                    map.insert("builtIn".into(), json!(self.is_built_in_recipe(&entry.id)));
                }
                value
            })
            .collect();
        json!({
            "recipes": recipes,
            "readAt": state.catalogue.read_at.map(format_iso),
            "error": state.catalogue.error,
        })
    }

    #[cfg(feature = "script-recipes")]
    fn installed_text(&self, id: &str) -> Value {
        json!(self.recipes.installed_version(id).map(|v| v.to_string()))
    }

    #[cfg(not(feature = "script-recipes"))]
    fn installed_text(&self, _id: &str) -> Value {
        Value::Null
    }

    #[cfg(feature = "script-recipes")]
    fn is_built_in_recipe(&self, id: &str) -> bool {
        self.recipes.is_built_in(id)
    }

    #[cfg(not(feature = "script-recipes"))]
    fn is_built_in_recipe(&self, _id: &str) -> bool {
        false
    }

    // ---- The memory rules (R4) ----

    /// R4.1: the PSRAM free now, less a runtime's limit, must leave the reserve. `None` when the
    /// platform reads no memory.
    pub(crate) fn memory_refusal(&self, id: &str, limit: usize) -> Option<String> {
        let psram = self.install.installs.probe().read()?.stats.psram?;
        let needed = limit + SCRIPT_PSRAM_RESERVE;
        (psram.free < needed).then(|| {
            format!(
                "Not enough memory for {id}: {} KB free, {} KB needed",
                kib_down(psram.free),
                kib_up(needed)
            )
        })
    }

    /// R4.2: the largest block of internal RAM must hold a job's thread and its TLS session; an
    /// uninstall (`tls` false) opens no session and needs only its thread's stack. When the
    /// program keeps the thread from the start (spec 045 R3), its stack is not counted: an
    /// install needs the session's margin, an uninstall nothing.
    pub(crate) fn internal_refusal(&self, tls: bool) -> Option<Failure> {
        let reading = self.install.installs.probe().read()?;
        let stack = if self.install.installs.job_thread_kept() {
            0
        } else {
            FETCH_STACK_BYTES
        };
        let needed = if tls {
            stack + RECIPE_INTERNAL_MARGIN
        } else {
            stack
        };
        (reading.stats.internal_largest < needed).then(|| {
            Failure::new(
                503,
                format!(
                    "Not enough internal memory: largest free block {} KB, {} KB needed",
                    kib_down(reading.stats.internal_largest),
                    kib_up(needed)
                ),
            )
        })
    }

    // ---- GET /system/resources (R5) ----

    pub fn resources(&self) -> Resources {
        let reading = self.install.installs.probe().read();
        let internal = reading.map(|r| InternalResources {
            free: r.stats.internal_free,
            min_free: r.internal_min,
            largest_block: r.stats.internal_largest,
        });
        let psram = reading.and_then(|r| {
            r.stats.psram.map(|p| PsramResources {
                free: p.free,
                min_free: r.psram_min,
                total: p.total,
                reserve: SCRIPT_PSRAM_RESERVE,
            })
        });
        Resources {
            internal,
            psram,
            storage: self.install.storage,
            scripts: self.script_resources(),
        }
    }

    #[cfg(feature = "script-recipes")]
    fn script_resources(&self) -> Vec<ScriptResources> {
        let mut rows: Vec<ScriptResources> = self
            .recipes
            .script_rows()
            .into_iter()
            .map(|row| ScriptResources {
                source: if row.installed {
                    "installed"
                } else {
                    "builtIn"
                },
                state: if row.failed.is_some() {
                    "failed"
                } else {
                    "ready"
                },
                reason: row.failed,
                id: row.id,
                version: row.version,
                held: row.held,
                limit: row.limit,
                instances: row.instances,
                calls: row.calls,
                average_us: row.average_us,
                max_us: row.max_us,
            })
            .collect();
        for refused in &self.install.refused {
            rows.push(ScriptResources {
                id: refused.id.clone(),
                version: None,
                source: "installed",
                state: "refused",
                reason: Some(refused.reason.clone()),
                held: 0,
                limit: 0,
                instances: self
                    .recipes
                    .instances()
                    .filter(|instance| instance.recipe_id == refused.id)
                    .count(),
                calls: 0,
                average_us: 0,
                max_us: 0,
            });
        }
        rows
    }

    #[cfg(not(feature = "script-recipes"))]
    fn script_resources(&self) -> Vec<ScriptResources> {
        Vec::new()
    }
}
