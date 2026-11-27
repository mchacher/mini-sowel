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
mod swap;

use std::sync::Arc;

pub use limits::{
    HeldCounter, MAX_ALIAS_BYTES, MAX_LOG_LINES_PER_CALL, MAX_SCRIPT_BYTES, MAX_TIMERS, NewRuntime,
    Reason, RefusedFlag, SCRIPT_CALL_LIMIT_MS, SCRIPT_LOAD_LIMIT_MS, SCRIPT_MEMORY_LIMIT,
    SCRIPT_STACK_BYTES, SYSTEM_MEMORY_LIMIT, ScriptRuntimes, SystemRuntimes,
};
pub use load::{MAX_INSTALLED_ID_CHARS, RESERVED_IDS, ScriptSummary};
pub use runtime::ScriptRecipe;
pub use slots::{MAX_SLOT_EQUIPMENTS, SlotDef, SlotKind};
pub use swap::{Replaced, UpdateFailure};

use super::{RecipeEngine, ScriptClock, ScriptOrigin, ScriptSource};
use load::Rules;

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
        self.load_script_as(source, runtimes, clock, ScriptSource::BuiltIn)
    }

    /// The same for an installed script (spec 042): the stricter rules of R1.3 and R1.4, and its
    /// `version` must be the signed header's (`version differs from the signed one`).
    pub fn load_script_as(
        &mut self,
        source: &str,
        runtimes: Arc<dyn ScriptRuntimes>,
        clock: ScriptClock,
        origin: ScriptSource,
    ) -> Result<ScriptSummary, String> {
        let invalid = |reason: String| format!("Script recipe invalid: {reason}");
        let rules = rules_of(origin);
        let (recipe, summary) = load::load(source, &runtimes, &clock, rules).map_err(invalid)?;
        if let ScriptSource::Installed { version } = origin
            && summary
                .version
                .as_deref()
                .and_then(crate::update::Version::parse)
                != Some(version)
        {
            return Err("version differs from the signed one".to_string());
        }
        let text: Arc<str> = Arc::from(source);
        let limit = runtimes.memory_limit();
        let rebuild = rebuild_of(&text, &runtimes, &clock, rules);
        let kept = ScriptOrigin {
            source: origin,
            text,
            limit,
            declared: summary.version.clone(),
        };
        self.add_script(Box::new(recipe), rebuild, clock, kept)
            .map_err(invalid)?;
        Ok(summary)
    }
}

/// The rules a script of this origin is held to.
pub(crate) fn rules_of(origin: ScriptSource) -> Rules {
    match origin {
        ScriptSource::BuiltIn => Rules::BUILT_IN,
        ScriptSource::Installed { .. } => Rules::INSTALLED,
    }
}

/// How the runtime is made again after a failure: from the same source, under the same rules.
pub(crate) fn rebuild_of(
    text: &Arc<str>,
    runtimes: &Arc<dyn ScriptRuntimes>,
    clock: &ScriptClock,
    rules: Rules,
) -> super::Rebuild {
    let (text, runtimes, clock) = (Arc::clone(text), Arc::clone(runtimes), Arc::clone(clock));
    Box::new(move || {
        load::load(&text, &runtimes, &clock, rules)
            .map(|(recipe, _)| Box::new(recipe) as Box<dyn solite_recipe_sdk::Recipe + Send>)
    })
}
