//! The home's side of recipes (spec 012): the engine it holds, the events it hands over, the
//! changes made through the API, and what is written to `recipes.json`.

use serde_json::Value;

use super::events::EventQueue;
use super::{Home, ListCursor, new_id};
use crate::devices::Random;
use crate::failure::Outcome;
use crate::recipes::{RecipeEngine, RecipeInstance};

impl Home {
    /// The recipe instances, with their logs.
    pub fn recipes(&self) -> &RecipeEngine {
        &self.recipes
    }

    /// The events waiting for the engine.
    pub fn events(&self) -> &EventQueue {
        &self.events
    }

    /// The time on the clock timers run on: the queue's monotonic clock when the host gave one,
    /// else `wall`.
    fn mono(&self, wall: u64) -> u64 {
        self.orders.now().unwrap_or(wall)
    }

    /// Runs `f` with the engine taken out of the home, so that the engine can read the home it
    /// belongs to.
    fn engine_reads_home<R>(&mut self, f: impl FnOnce(&mut RecipeEngine, &Home) -> R) -> R {
        let mut engine = std::mem::take(&mut self.recipes);
        let result = f(&mut engine, self);
        self.recipes = engine;
        result
    }

    /// The same, for a change of which instances run: the interest is then the running
    /// instances' as they are.
    fn with_engine<R>(&mut self, f: impl FnOnce(&mut RecipeEngine, &Home) -> R) -> R {
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
        let instance =
            self.with_engine(|engine, home| engine.update(home, id, params, wall, mono))?;
        self.recipes_changed = true;
        Ok(instance)
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
        let changed =
            self.with_engine(|engine, home| engine.set_enabled(home, id, enabled, wall, mono))?;
        if changed {
            self.recipes_changed = true;
        }
        Ok(())
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
        // The running instances do not change here: the interest stays.
        self.engine_reads_home(|engine, home| {
            for event in &events {
                engine.handle(home, event, wall, mono);
            }
            engine.tick(home, wall, mono);
        });
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
