//! Script recipes (spec 041): recipes written in JavaScript, in Sowel's script-recipe format, run
//! by QuickJS through `rquickjs`, one runtime and one context per recipe, shared by its
//! instances. The loader reads a source text, the shim (`shim.js`) implements the script API in
//! the recipe's own context, `ScriptRecipe` is one more implementation of the SDK's `Recipe`
//! trait (the engine serves it like a built-in recipe), `slots` is the host's validation of an
//! instance's settings, and `limits` holds the bounds: memory, time, stack, collector.
//!
//! The core never touches hardware: the runtimes come from the `ScriptRuntimes` adapter (the
//! host's default is `SystemRuntimes`, the firmware gives its own allocator), and the time limits
//! read the monotonic clock the engine is given.

mod limits;
mod load;
mod runtime;
mod slots;

use std::sync::Arc;

pub use limits::{
    MAX_ALIAS_BYTES, MAX_LOG_LINES_PER_CALL, MAX_SCRIPT_BYTES, MAX_TIMERS, NewRuntime, Reason,
    RefusedFlag, SCRIPT_CALL_LIMIT_MS, SCRIPT_LOAD_LIMIT_MS, SCRIPT_MEMORY_LIMIT,
    SCRIPT_STACK_BYTES, SYSTEM_MEMORY_LIMIT, ScriptRuntimes, SystemRuntimes,
};
pub use load::ScriptSummary;
pub use runtime::ScriptRecipe;
pub use slots::{MAX_SLOT_EQUIPMENTS, SlotDef, SlotKind};

use super::{RecipeEngine, ScriptClock};

impl RecipeEngine {
    /// Loads a script recipe from its source (spec 041, R1, R3.8): the runtime is made, the script
    /// run once, its definition checked. After that `GET /recipes` lists it, after the built-in
    /// recipes and the scripts loaded before it. The error is `Script recipe invalid: <reason>`.
    /// `clock` is the monotonic clock, in milliseconds, of the time limits.
    pub fn load_script(
        &mut self,
        source: &str,
        runtimes: Arc<dyn ScriptRuntimes>,
        clock: ScriptClock,
    ) -> Result<ScriptSummary, String> {
        let invalid = |reason: String| format!("Script recipe invalid: {reason}");
        let (recipe, summary) = load::load(source, &runtimes, &clock).map_err(invalid)?;
        let text: Arc<str> = Arc::from(source);
        let (rebuild_runtimes, rebuild_clock) = (Arc::clone(&runtimes), Arc::clone(&clock));
        // The runtime made again after a failure comes from the same source.
        let rebuild: super::Rebuild = Box::new(move || {
            load::load(&text, &rebuild_runtimes, &rebuild_clock)
                .map(|(recipe, _)| Box::new(recipe) as Box<dyn solite_recipe_sdk::Recipe + Send>)
        });
        self.add_script(Box::new(recipe), rebuild, clock)
            .map_err(invalid)?;
        Ok(summary)
    }
}
