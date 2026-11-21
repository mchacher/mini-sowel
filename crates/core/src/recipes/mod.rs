//! Recipes (spec 012): Sowel's `RecipeInstance`, `RecipeLogEntry` and recipe routes, with the
//! recipe engine that holds the instances, gives them the events and the time, and keeps their
//! logs. The recipes are written in Rust and built in (`motion-light` only); loading recipes at
//! run time comes later. No I/O and no thread: the host's radio loop gives the events and the
//! time (`handle`, `tick`), a recipe's orders go to the home's queue, its timers are deadlines
//! checked on each turn.

pub mod duration;
pub mod motion_light;

use std::collections::VecDeque;
use std::io::{self, Write};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::devices::{Id, iso, iso_option};
use crate::failure::{Failure, Outcome};
use crate::home::Home;
use crate::home::events::{Event, Interest, cut};

/// Instances kept; one more is refused (400).
pub const MAX_INSTANCES: usize = 16;
/// Log entries kept per instance; the oldest goes first.
pub const MAX_LOG_ENTRIES: usize = 50;
/// A log message is cut at this many bytes.
pub const MAX_LOG_MESSAGE_BYTES: usize = 200;
/// The longest `params`, serialized, in bytes.
pub const MAX_PARAMS_BYTES: usize = 2048;
// Worst case of the bounds: 16 instances x 50 entries x about 260 bytes (200 of message, the rest
// of the entry) is about 208 KB of logs, plus 16 x 2 KB of params kept as text (never parsed
// while kept: 2 KB of nested JSON takes about 112 KB once parsed): about 240 KB. The ceiling test
// measures the params of 16 adversarial instances at about 2.5 KB each. It fits the box's PSRAM
// (2 MB); a home of a few instances holds a few KB.

/// A log line's level (Sowel's `RecipeLogEntry.level`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Info,
    Warn,
    Error,
}

/// Sowel's `RecipeInstance` (`src/shared/types.ts` T:834).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecipeInstance {
    pub id: Id,
    pub recipe_id: String,
    /// The parameters as the JSON text received (at most `MAX_PARAMS_BYTES`): kept as text, since
    /// parsed JSON takes far more memory; parsed only for a moment (validation, start).
    #[serde(with = "raw_json")]
    pub params: Box<str>,
    pub enabled: bool,
    #[serde(with = "iso")]
    pub created_at: u64,
}

/// `params` as it is written and read: the JSON text itself, never a parsed value.
pub(crate) mod raw_json {
    use serde::de::Error as _;
    use serde::ser::Error as _;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use serde_json::value::RawValue;

    pub fn serialize<S: Serializer>(text: &str, serializer: S) -> Result<S::Ok, S::Error> {
        let raw: &RawValue = serde_json::from_str(text).map_err(S::Error::custom)?;
        raw.serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Box<str>, D::Error> {
        let raw = Box::<RawValue>::deserialize(deserializer).map_err(D::Error::custom)?;
        Ok(raw.get().into())
    }
}

/// Sowel's `RecipeLogEntry`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecipeLogEntry {
    /// Counts from 1 since the program started.
    pub id: u64,
    pub instance_id: Id,
    #[serde(with = "iso")]
    pub timestamp: u64,
    pub message: Box<str>,
    pub level: Level,
}

/// The state keys of an instance (`state` of `GET /recipe-instances`): only the keys that are set.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StateKeys {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub override_mode: Option<bool>,
    #[serde(with = "iso_option", skip_serializing_if = "Option::is_none")]
    pub timer_expires_at: Option<u64>,
    #[serde(with = "iso_option", skip_serializing_if = "Option::is_none")]
    pub failsafe_expires_at: Option<u64>,
}

/// What a running instance gets with each event and each tick: the home to read, the time on the
/// engine's monotonic clock (`now`, for timers) and on the wall clock (`wall`, for the dates
/// written), and the lines to log.
pub struct Cx<'a> {
    pub home: &'a Home,
    pub now: u64,
    pub wall: u64,
    pub lines: Vec<(Level, String)>,
}

/// A running instance of a recipe.
pub trait Runner: Send {
    /// An event the instance listens to (see `listen`). A failure is logged and never stops the
    /// others.
    fn on_event(&mut self, event: &Event, cx: &mut Cx<'_>) -> Result<(), String>;
    /// The time passed: timers that are due fire.
    fn tick(&mut self, cx: &mut Cx<'_>) -> Result<(), String>;
    fn state(&self) -> StateKeys;
    /// Adds what the instance listens to.
    fn listen(&self, interest: &mut Interest);
}

/// An instance, running or not, with its log.
struct Slot {
    instance: RecipeInstance,
    runner: Option<Box<dyn Runner>>,
    /// At most `MAX_LOG_ENTRIES`, the oldest first.
    log: VecDeque<RecipeLogEntry>,
}

/// The instances, at most `MAX_INSTANCES`, in the order they were created.
#[derive(Default)]
pub struct RecipeEngine {
    slots: Vec<Slot>,
    next_log_id: u64,
}

impl std::fmt::Debug for RecipeEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RecipeEngine")
            .field("instances", &self.slots.len())
            .finish()
    }
}

/// Counts the bytes written to it and keeps none.
struct Counter(usize);

impl Write for Counter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0 += bytes.len();
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// The parameters serialized, kept as text; `None` when they are over `MAX_PARAMS_BYTES`. The
/// size is counted first, so that nothing large is ever built.
fn params_text(params: &Value) -> Option<Box<str>> {
    let mut counter = Counter(0);
    if serde_json::to_writer(&mut counter, params).is_err() || counter.0 > MAX_PARAMS_BYTES {
        return None;
    }
    serde_json::to_string(params)
        .ok()
        .map(String::into_boxed_str)
}

fn not_found() -> Failure {
    Failure::new(404, "Instance not found")
}

/// The recipes there are.
pub fn recipe_infos() -> Vec<Value> {
    vec![motion_light::info()]
}

/// One recipe's definition, `None` for a recipe that does not exist.
pub fn recipe_info(id: &str) -> Option<Value> {
    (id == motion_light::ID).then(motion_light::info)
}

/// Checks the parameters of a recipe against the home, with the recipe's messages.
fn validate(recipe: &str, params: &Value, home: &Home) -> Result<(), String> {
    match recipe {
        motion_light::ID => motion_light::validate(params, home).map(|_| ()),
        other => Err(format!("Recipe not found: {other}")),
    }
}

impl RecipeEngine {
    pub fn new() -> Self {
        RecipeEngine::default()
    }

    // ---- Reading ----

    pub fn len(&self) -> usize {
        self.slots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// The instances, oldest first.
    pub fn instances(&self) -> impl Iterator<Item = &RecipeInstance> {
        self.slots.iter().map(|s| &s.instance)
    }

    pub fn get(&self, id: &Id) -> Option<&RecipeInstance> {
        self.slots
            .iter()
            .find(|s| s.instance.id == *id)
            .map(|s| &s.instance)
    }

    /// True while the instance runs (enabled, and started without failure).
    pub fn is_running(&self, id: &Id) -> bool {
        self.slots
            .iter()
            .any(|s| s.instance.id == *id && s.runner.is_some())
    }

    /// The state keys of an instance: none when it does not run.
    pub fn state_of(&self, id: &Id) -> Option<StateKeys> {
        self.slots.iter().find(|s| s.instance.id == *id).map(|s| {
            s.runner
                .as_ref()
                .map_or_else(StateKeys::default, |r| r.state())
        })
    }

    /// The last `limit` entries of an instance's log, newest first; none for an unknown id.
    pub fn log(&self, id: &str, limit: usize) -> Vec<&RecipeLogEntry> {
        let Some(id) = Id::parse(id) else {
            return Vec::new();
        };
        self.slots
            .iter()
            .find(|s| s.instance.id == id)
            .map(|s| s.log.iter().rev().take(limit).collect())
            .unwrap_or_default()
    }

    /// What the running instances listen to.
    pub fn interest(&self) -> Interest {
        let mut interest = Interest::new();
        for runner in self.slots.iter().filter_map(|s| s.runner.as_ref()) {
            runner.listen(&mut interest);
        }
        interest
    }

    fn index_of(&self, id: &str) -> Option<usize> {
        let id = Id::parse(id)?;
        self.slots.iter().position(|s| s.instance.id == id)
    }

    // ---- The log ----

    fn push_log(slot: &mut Slot, next_id: &mut u64, wall: u64, level: Level, message: &str) {
        if slot.log.len() >= MAX_LOG_ENTRIES {
            slot.log.pop_front();
        }
        *next_id += 1;
        slot.log.push_back(RecipeLogEntry {
            id: *next_id,
            instance_id: slot.instance.id,
            timestamp: wall,
            message: cut(message, MAX_LOG_MESSAGE_BYTES).into(),
            level,
        });
    }

    fn push_lines(slot: &mut Slot, next_id: &mut u64, wall: u64, lines: Vec<(Level, String)>) {
        for (level, message) in lines {
            RecipeEngine::push_log(slot, next_id, wall, level, &message);
        }
    }

    // ---- Starting and stopping ----

    /// Starts the instance at `index` (it is stopped): validates, builds the runner, runs its
    /// start. A failure is the instance's log line `<failed>: <error>`, and the instance stays
    /// stopped.
    fn start_slot(&mut self, index: usize, home: &Home, mono: u64, wall: u64, failed: &str) {
        let Some(slot) = self.slots.get_mut(index) else {
            return;
        };
        let mut cx = Cx {
            home,
            now: mono,
            wall,
            lines: Vec::new(),
        };
        let result: Result<Box<dyn Runner>, String> = match slot.instance.recipe_id.as_str() {
            motion_light::ID => {
                // The parsed value lives for this validation only.
                let parsed = serde_json::from_str::<Value>(&slot.instance.params)
                    .map_err(|e| format!("params are not JSON: {e}"));
                parsed
                    .and_then(|value| motion_light::validate(&value, home))
                    .map(|params| {
                        // The check of every start: a zone with no motion sensor never triggers.
                        let sensors = home
                            .zone_aggregate(&params.zone, wall)
                            .map_or(0, |data| data.motion_sensors);
                        if sensors == 0 {
                            cx.lines.push((
                                Level::Warn,
                                "Zone has no motion sensors — recipe will never trigger"
                                    .to_string(),
                            ));
                        }
                        Box::new(motion_light::MotionLight::start(params, &mut cx))
                            as Box<dyn Runner>
                    })
            }
            other => Err(format!("Recipe not found: {other}")),
        };
        let lines = std::mem::take(&mut cx.lines);
        RecipeEngine::push_lines(slot, &mut self.next_log_id, wall, lines);
        match result {
            Ok(runner) => slot.runner = Some(runner),
            Err(error) => {
                slot.runner = None;
                let message = format!("{failed}: {error}");
                RecipeEngine::push_log(slot, &mut self.next_log_id, wall, Level::Error, &message);
            }
        }
    }

    // ---- Changes ----

    /// Creates an instance (enabled) and starts it: 404 `Recipe not found: <id>`, 400 for too
    /// many instances, params too large or invalid. A failure to start is only logged.
    pub fn create(
        &mut self,
        home: &Home,
        recipe_id: &str,
        params: Value,
        id: Id,
        wall: u64,
        mono: u64,
    ) -> Outcome<RecipeInstance> {
        if recipe_info(recipe_id).is_none() {
            return Err(Failure::new(404, format!("Recipe not found: {recipe_id}")));
        }
        if self.slots.len() >= MAX_INSTANCES {
            return Err(Failure::new(
                400,
                format!("Too many recipe instances (max {MAX_INSTANCES})"),
            ));
        }
        let text = params_text(&params)
            .ok_or_else(|| Failure::new(400, "Invalid params: params too large"))?;
        validate(recipe_id, &params, home)
            .map_err(|e| Failure::new(400, format!("Invalid params: {e}")))?;
        drop(params);
        let instance = RecipeInstance {
            id,
            recipe_id: recipe_id.to_string(),
            params: text,
            enabled: true,
            created_at: wall,
        };
        let mut slot = Slot {
            instance: instance.clone(),
            runner: None,
            log: VecDeque::new(),
        };
        RecipeEngine::push_log(
            &mut slot,
            &mut self.next_log_id,
            wall,
            Level::Info,
            "Instance created",
        );
        self.slots.push(slot);
        let index = self.slots.len() - 1;
        self.start_slot(index, home, mono, wall, "Failed to start");
        Ok(instance)
    }

    /// Replaces the parameters: validated first (the running instance is untouched when they are
    /// refused), then the instance is stopped, saved, and started again if it is enabled.
    pub fn update(
        &mut self,
        home: &Home,
        id: &str,
        params: Value,
        wall: u64,
        mono: u64,
    ) -> Outcome<RecipeInstance> {
        let index = self.index_of(id).ok_or_else(not_found)?;
        let recipe_id = self.slots[index].instance.recipe_id.clone();
        if recipe_info(&recipe_id).is_none() {
            return Err(Failure::new(404, format!("Recipe not found: {recipe_id}")));
        }
        let text = params_text(&params)
            .ok_or_else(|| Failure::new(400, "Invalid params: params too large"))?;
        validate(&recipe_id, &params, home)
            .map_err(|e| Failure::new(400, format!("Invalid params: {e}")))?;
        drop(params);
        let slot = &mut self.slots[index];
        slot.runner = None;
        slot.instance.params = text;
        let enabled = slot.instance.enabled;
        let instance = slot.instance.clone();
        if enabled {
            self.start_slot(index, home, mono, wall, "Failed to restart");
        }
        Ok(instance)
    }

    /// Stops an instance, forgets it and its log.
    pub fn delete(&mut self, id: &str) -> Outcome<()> {
        let index = self.index_of(id).ok_or_else(not_found)?;
        self.slots.remove(index);
        Ok(())
    }

    /// Enables or disables an instance. Enabling one that is enabled already does nothing;
    /// otherwise it starts (a failure is only logged), and disabling stops it. True when the
    /// instance changed.
    pub fn set_enabled(
        &mut self,
        home: &Home,
        id: &str,
        enabled: bool,
        wall: u64,
        mono: u64,
    ) -> Outcome<bool> {
        let index = self.index_of(id).ok_or_else(not_found)?;
        let slot = &mut self.slots[index];
        if slot.instance.enabled == enabled {
            return Ok(false);
        }
        slot.instance.enabled = enabled;
        if enabled {
            self.start_slot(index, home, mono, wall, "Failed to start");
        } else {
            slot.runner = None;
        }
        Ok(true)
    }

    /// `POST /recipe-instances/:id/actions`: no recipe declares an action, so a running instance
    /// accepts any and ignores it; one that does not run (or does not exist) is 400.
    pub fn action(&self, id: &str) -> Outcome<()> {
        let running = Id::parse(id).is_some_and(|id| self.is_running(&id));
        if running {
            Ok(())
        } else {
            Err(Failure::new(400, "Instance not running"))
        }
    }

    // ---- Loading ----

    /// Takes the instances read from storage, not started: at most `MAX_INSTANCES`, an id seen
    /// twice dropped (the log says so).
    pub fn load(&mut self, list: Vec<RecipeInstance>) {
        for instance in list {
            if self.slots.len() >= MAX_INSTANCES {
                log::warn!(
                    "recipes: more than {MAX_INSTANCES} instances, {} dropped",
                    instance.id
                );
                continue;
            }
            if self.slots.iter().any(|s| s.instance.id == instance.id) {
                log::warn!("recipes: instance {} is there twice, dropped", instance.id);
                continue;
            }
            self.slots.push(Slot {
                instance,
                runner: None,
                log: VecDeque::new(),
            });
        }
    }

    /// Starts every enabled instance that is not running: the host's start, after the home was
    /// loaded. An unknown recipe or oversized parameters are kept and stay stopped.
    pub fn start_all(&mut self, home: &Home, wall: u64, mono: u64) {
        for index in 0..self.slots.len() {
            let slot = &mut self.slots[index];
            if !slot.instance.enabled || slot.runner.is_some() {
                continue;
            }
            if recipe_info(&slot.instance.recipe_id).is_none() {
                let message = format!(
                    "Failed to restore: Recipe not found: {}",
                    slot.instance.recipe_id
                );
                RecipeEngine::push_log(slot, &mut self.next_log_id, wall, Level::Error, &message);
                continue;
            }
            if slot.instance.params.len() > MAX_PARAMS_BYTES {
                RecipeEngine::push_log(
                    slot,
                    &mut self.next_log_id,
                    wall,
                    Level::Error,
                    "Failed to restore: params too large",
                );
                continue;
            }
            self.start_slot(index, home, mono, wall, "Failed to restore");
        }
    }

    // ---- Events and time ----

    fn run(
        &mut self,
        home: &Home,
        wall: u64,
        mono: u64,
        mut call: impl FnMut(&mut dyn Runner, &mut Cx<'_>) -> Result<(), String>,
    ) {
        for slot in &mut self.slots {
            let Some(runner) = slot.runner.as_mut() else {
                continue;
            };
            let mut cx = Cx {
                home,
                now: mono,
                wall,
                lines: Vec::new(),
            };
            let result = call(runner.as_mut(), &mut cx);
            let lines = std::mem::take(&mut cx.lines);
            RecipeEngine::push_lines(slot, &mut self.next_log_id, wall, lines);
            // A handler that fails never stops the others, nor the loop.
            if let Err(error) = result {
                log::error!("recipe instance {}: {error}", slot.instance.id);
                let message = format!("Error: {error}");
                RecipeEngine::push_log(slot, &mut self.next_log_id, wall, Level::Error, &message);
            }
        }
    }

    /// Gives an event to every running instance.
    pub fn handle(&mut self, home: &Home, event: &Event, wall: u64, mono: u64) {
        self.run(home, wall, mono, |runner, cx| runner.on_event(event, cx));
    }

    /// The time passed: the timers that are due fire.
    pub fn tick(&mut self, home: &Home, wall: u64, mono: u64) {
        self.run(home, wall, mono, |runner, cx| runner.tick(cx));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A recipe that fails on every event, and one that counts the events it gets.
    struct Failing;
    impl Runner for Failing {
        fn on_event(&mut self, _: &Event, _: &mut Cx<'_>) -> Result<(), String> {
            Err("boom".into())
        }
        fn tick(&mut self, _: &mut Cx<'_>) -> Result<(), String> {
            Err("boom".into())
        }
        fn state(&self) -> StateKeys {
            StateKeys::default()
        }
        fn listen(&self, _: &mut Interest) {}
    }

    struct Counting(std::sync::Arc<std::sync::atomic::AtomicUsize>);
    impl Runner for Counting {
        fn on_event(&mut self, _: &Event, _: &mut Cx<'_>) -> Result<(), String> {
            self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        }
        fn tick(&mut self, _: &mut Cx<'_>) -> Result<(), String> {
            self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        }
        fn state(&self) -> StateKeys {
            StateKeys::default()
        }
        fn listen(&self, _: &mut Interest) {}
    }

    fn slot(n: u8, runner: Box<dyn Runner>) -> Slot {
        Slot {
            instance: RecipeInstance {
                id: Id::from_random([n; 16]),
                recipe_id: "motion-light".into(),
                params: "null".into(),
                enabled: true,
                created_at: 0,
            },
            runner: Some(runner),
            log: VecDeque::new(),
        }
    }

    #[test]
    fn an_instance_whose_handler_fails_does_not_stop_the_others() {
        let home = Home::new(1);
        let count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut engine = RecipeEngine::new();
        engine.slots.push(slot(1, Box::new(Failing)));
        engine
            .slots
            .push(slot(2, Box::new(Counting(count.clone()))));
        let event = Event::Zone {
            zone: Id::ROOT_ZONE,
            motion: true,
            luminosity: None,
        };
        engine.handle(&home, &event, 1, 1);
        engine.tick(&home, 2, 2);
        assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 2);
        // The failure is in the failing instance's log, not in the other's.
        let failing = engine.slots[0].instance.id.to_string();
        let healthy = engine.slots[1].instance.id.to_string();
        assert_eq!(engine.log(&failing, 50).len(), 2);
        assert_eq!(engine.log(&failing, 50)[0].level, Level::Error);
        assert_eq!(engine.log(&failing, 50)[0].message.as_ref(), "Error: boom");
        assert!(engine.log(&healthy, 50).is_empty());
    }
}
