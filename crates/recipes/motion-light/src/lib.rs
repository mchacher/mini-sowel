//! The `motion-light` recipe, the port of Sowel's `sowel-recipe-motion-light` 1.1.2 with the same
//! id, slots, messages and behaviour (spec 012), written against the recipe SDK (spec 041): it
//! reads and commands the home through `Host` only, so the core links it in like any recipe. No
//! `unsafe`.

mod info;
mod machine;
mod params;
mod sources;

pub use info::{ID, NAME, info};
pub use machine::{Cx, MotionLight};
pub use params::{MAX_BUTTONS, MAX_LIGHTS, Params, prune_stale_buttons, validate};
pub use sources::{MotionSource, is_motion_active, resolve_motion_sources};

use serde_json::Value;
use solite_recipe_sdk::{
    Change, Clock, Event, Host, Level, LogLine, Pruned, Recipe, RecipeError, Update,
};

/// The recipe: the instances it serves, by the handle the engine gave them. Bounded by the
/// engine's instances (16); the engine stops an instance before it starts another past its bound.
#[derive(Default)]
pub struct MotionLightRecipe {
    instances: Vec<(u32, MotionLight)>,
}

impl MotionLightRecipe {
    pub fn new() -> Self {
        MotionLightRecipe::default()
    }

    fn instance(&mut self, handle: u32) -> Result<&mut MotionLight, String> {
        self.instances
            .iter_mut()
            .find(|(h, _)| *h == handle)
            .map(|(_, machine)| machine)
            .ok_or_else(|| format!("unknown handle {handle}"))
    }
}

fn update_of(machine: &MotionLight) -> Update {
    Update {
        state: machine.state_text(),
        listen: machine.listen(),
    }
}

impl Recipe for MotionLightRecipe {
    fn info(&self) -> Value {
        info()
    }

    fn validate(&self, host: &dyn Host, params: &Value) -> Result<(), RecipeError> {
        validate(params, host)
            .map(|_| ())
            .map_err(RecipeError::from)
    }

    fn start(
        &mut self,
        host: &dyn Host,
        handle: u32,
        params: &Value,
        clock: Clock,
    ) -> Result<Update, RecipeError> {
        let params = validate(params, host)?;
        // The check of every start: a zone with no motion sensor never triggers.
        let zone = host.zone_data(&solite_recipe_sdk::format_id(&params.zone));
        if zone.as_ref().map_or(0, |data| data.motion_sensors) == 0 {
            host.log(
                Level::Warn,
                "Zone has no motion sensors — recipe will never trigger",
            );
        }
        let reading = zone.map_or((false, None), |data| (data.motion, data.luminosity));
        let machine = MotionLight::start(params, reading, &mut Cx::new(host, clock));
        let update = update_of(&machine);
        self.instances.retain(|(h, _)| *h != handle);
        self.instances.push((handle, machine));
        Ok(update)
    }

    fn stop(&mut self, handle: u32) {
        self.instances.retain(|(h, _)| *h != handle);
    }

    fn on_event(
        &mut self,
        host: &dyn Host,
        handle: u32,
        event: &Event,
        clock: Clock,
    ) -> Result<Option<Update>, RecipeError> {
        let machine = self.instance(handle)?;
        let before = machine.state_text();
        machine.on_event(event, &mut Cx::new(host, clock));
        Ok((machine.state_text() != before).then(|| update_of(machine)))
    }

    fn tick(
        &mut self,
        host: &dyn Host,
        handle: u32,
        clock: Clock,
    ) -> Result<Option<Update>, RecipeError> {
        let machine = self.instance(handle)?;
        let before = machine.state_text();
        machine.tick(&mut Cx::new(host, clock));
        Ok((machine.state_text() != before).then(|| update_of(machine)))
    }

    fn refresh(
        &mut self,
        host: &dyn Host,
        handle: u32,
        params: &Value,
    ) -> Result<Update, RecipeError> {
        let machine = self.instance(handle)?;
        machine.refresh(host, params);
        Ok(update_of(machine))
    }

    /// The recipe's own part of a settle: the legacy `light` key, and the buttons that are gone or
    /// have no `action` data binding any more (spec 028). A lost light restarts a running
    /// instance, buttons only do not.
    fn prune(&self, host: &dyn Host, params: &Value, change: &Change) -> Option<Pruned> {
        let mut params = params.clone();
        let mut lines: Vec<LogLine> = Vec::new();
        let mut restart = false;
        let mut changed = false;
        if let Some(gone) = &change.equipment {
            let slot = params.as_object_mut()?.get_mut("light");
            if let Some(slot @ Value::String(_)) = slot
                && slot.as_str() == Some(gone.id.as_str())
            {
                *slot = Value::String(String::new());
                changed = true;
                restart = true;
                // The engine logged the removal when it took the id out of a slot.
                if change.pruned.is_empty() {
                    lines.push(LogLine {
                        level: Level::Warn,
                        message: format!("Equipment removed from settings: {}", gone.name),
                    });
                }
            }
            // A lost light restarts, whatever slot of the engine's it left.
            restart |= change.pruned.iter().any(|slot| slot != "buttons");
        }
        let stale = prune_stale_buttons(&mut params, host);
        if !stale.is_empty() {
            changed = true;
            lines.extend(stale.into_iter().map(|name| LogLine {
                level: Level::Warn,
                message: format!("Button removed from settings: {name}"),
            }));
        }
        (changed || restart).then_some(Pruned {
            params,
            lines,
            restart,
        })
    }
}
