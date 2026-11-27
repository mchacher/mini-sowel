//! The home's side of recipes (spec 012): the engine it holds, the events it hands over, the
//! changes made through the API, and what is written to `recipes.json`.

use serde_json::Value;

use super::events::EventQueue;
use super::{Home, ListCursor, new_id};
use crate::devices::Random;
use crate::failure::Outcome;
use crate::recipes::{RecipeEngine, RecipeInstance, Settle};

impl Home {
    /// The recipe instances, with their logs.
    pub fn recipes(&self) -> &RecipeEngine {
        &self.recipes
    }

    /// Loads a script recipe (spec 041): `GET /recipes` lists it from then on. The error is
    /// `Script recipe invalid: <reason>`.
    #[cfg(feature = "script-recipes")]
    pub fn load_script_recipe(
        &mut self,
        source: &str,
        runtimes: std::sync::Arc<dyn crate::recipes::script::ScriptRuntimes>,
        clock: crate::recipes::ScriptClock,
    ) -> Result<crate::recipes::script::ScriptSummary, String> {
        self.recipes.load_script(source, runtimes, clock)
    }

    /// The counters of the script recipes loaded, for the box's `scripts:` line.
    pub fn script_stats(&self) -> Vec<(String, solite_recipe_sdk::RunStats)> {
        self.recipes.script_stats()
    }

    /// The events waiting for the engine.
    pub fn events(&self) -> &EventQueue {
        &self.events
    }

    /// The time on the clock timers run on: the queue's monotonic clock when the host gave one,
    /// else `wall`.
    pub(super) fn mono(&self, wall: u64) -> u64 {
        self.orders.now().unwrap_or(wall)
    }

    /// Runs `f` with the engine taken out of the home, so that the engine can read the home it
    /// belongs to.
    fn engine_reads_home<R>(&mut self, f: impl FnOnce(&mut RecipeEngine, &Home) -> R) -> R {
        let mut engine = std::mem::replace(&mut self.recipes, RecipeEngine::placeholder());
        let result = f(&mut engine, self);
        self.recipes = engine;
        result
    }

    /// The same, for a change of which instances run: the interest is then the running
    /// instances' as they are.
    pub(super) fn with_engine<R>(&mut self, f: impl FnOnce(&mut RecipeEngine, &Home) -> R) -> R {
        let result = self.engine_reads_home(f);
        self.interest = self.recipes.interest();
        result
    }

    /// `POST /recipe-instances`: creates an instance, enabled, and starts it.
    pub fn create_recipe_instance(
        &mut self,
        recipe_id: &str,
        params: Value,
        wall: u64,
        random: Random,
    ) -> Outcome<RecipeInstance> {
        let id = new_id(random)?;
        let mono = self.mono(wall);
        let instance = self
            .with_engine(|engine, home| engine.create(home, recipe_id, params, id, wall, mono))?;
        self.recipes_changed = true;
        Ok(instance)
    }

    /// `PUT /recipe-instances/:id`.
    pub fn update_recipe_instance(
        &mut self,
        id: &str,
        params: Value,
        wall: u64,
    ) -> Outcome<RecipeInstance> {
        let mono = self.mono(wall);
        let result = self.with_engine(|engine, home| engine.update(home, id, params, wall, mono));
        // A script recipe that fails at once is 500 with the new settings kept: written.
        if result.is_ok() || result.as_ref().is_err_and(|f| f.status == 500) {
            self.recipes_changed = true;
        }
        result
    }

    /// `DELETE /recipe-instances/:id`.
    pub fn delete_recipe_instance(&mut self, id: &str) -> Outcome<()> {
        self.with_engine(|engine, _| engine.delete(id))?;
        self.recipes_changed = true;
        Ok(())
    }

    /// `POST /recipe-instances/:id/enable` and `/disable`.
    pub fn set_recipe_enabled(&mut self, id: &str, enabled: bool, wall: u64) -> Outcome<()> {
        let mono = self.mono(wall);
        let result =
            self.with_engine(|engine, home| engine.set_enabled(home, id, enabled, wall, mono));
        match result {
            Ok(true) => self.recipes_changed = true,
            Ok(false) => {}
            // A script recipe that fails at once is 500, the instance enabled: written.
            Err(failure) if failure.status == 500 && enabled => {
                self.recipes_changed = true;
                return Err(failure);
            }
            Err(failure) => return Err(failure),
        }
        Ok(())
    }

    /// Settles the instances after a change of the home that can make their settings stale
    /// (spec 024, R4); `recipes.json` is written when one changed. Never fails.
    pub(crate) fn settle_recipes(&mut self, change: Settle, wall: u64) {
        if self.recipes.is_empty() {
            return;
        }
        let mono = self.mono(wall);
        if self.with_engine(|engine, home| engine.settle(home, &change, wall, mono)) {
            self.recipes_changed = true;
        }
    }

    /// The instances read from storage, not started yet (`start_recipes`).
    pub fn load_recipes(&mut self, list: Vec<RecipeInstance>) {
        self.recipes.load(list);
    }

    /// Starts the enabled instances, once the home is loaded. The zones are computed first, so
    /// that an instance starts from the motion and luminosity as they are.
    pub fn start_recipes(&mut self, mono: u64, wall: u64) {
        self.recompute(wall);
        self.with_engine(|engine, home| engine.start_all(home, wall, mono));
    }

    /// One turn of the radio loop: the events waiting go to the running instances, then the
    /// timers that are due fire. `mono` is the monotonic clock of the timers, `wall` the clock of
    /// the dates written.
    pub fn run_recipes(&mut self, mono: u64, wall: u64) {
        let events = self.events.take();
        if self.recipes.is_empty() {
            return;
        }
        self.engine_reads_home(|engine, home| engine.turn(home, events, wall, mono));
        // A turn restarts failed scripts and calls leave new listen sets: what the running
        // instances listen to is read again when they changed.
        if self.recipes.take_interest_stale() {
            self.interest = self.recipes.interest();
        }
    }

    // ---- What is to be written ----

    /// The instances as `recipes.json` is written, chunk by chunk, if they changed since they
    /// were last taken; the flag is cleared.
    pub fn take_recipes_cursor(&mut self) -> Option<ListCursor> {
        if !self.recipes_changed {
            return None;
        }
        self.recipes_changed = false;
        Some(ListCursor::saved_recipes(self))
    }

    pub fn mark_recipes_changed(&mut self) {
        self.recipes_changed = true;
    }

    pub fn recipes_changed(&self) -> bool {
        self.recipes_changed
    }
}
