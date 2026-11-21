//! The state machine of `motion-light`: a port of the recipe's `createInstance` (recipe
//! `src/index.ts` R:406-924) and of the helpers of Sowel's engine, with every quirk kept (see
//! `specs/012-motion-light/motion-light-reference.md`, section 4). Timers are deadlines on the
//! engine's monotonic clock, checked by `tick`; the home is read through `Cx`, orders go to the
//! home's queue and log lines to `Cx`.

use serde_json::Value;

use super::params::Params;
use super::sources::{MotionSource, is_motion_active, resolve_motion_sources};
use crate::devices::Id;
use crate::home::Home;
use crate::home::events::{Event, EventValue, Interest};
use crate::orders::{OrderRequest, commands_for};
use crate::recipes::duration::{format_duration, js_number};
use crate::recipes::{Cx, Level, Runner, StateKeys};

/// After the recipe switches the lights off, an `OFF` seen on the light within this long is its
/// own echo, not a manual change (the recipe's `TURN_OFF_GRACE_MS`).
const TURN_OFF_GRACE_MS: u64 = 2000;
/// The light goes off above the threshold plus this share (the recipe's
/// `LUX_HYSTERESIS_FACTOR`).
const LUX_HYSTERESIS_FACTOR: f64 = 0.1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OffKind {
    /// The timeout with no motion: the lights go off.
    Normal,
    /// The timeout that ends the override mode.
    OverrideClear,
}

#[derive(Debug, Clone, Copy)]
struct OffTimer {
    at: u64,
    kind: OffKind,
}

/// One running instance.
#[derive(Debug)]
pub struct MotionLight {
    p: Params,
    /// Resolved once, when the instance starts.
    sources: Vec<MotionSource>,
    off: Option<OffTimer>,
    /// The failsafe's deadline.
    failsafe: Option<u64>,
    /// The state key `overrideMode` is present exactly while this is true.
    override_mode: bool,
    lights_on_by_recipe: bool,
    last_known_light_on: bool,
    /// On the engine's monotonic clock.
    grace_until: u64,
    /// The state keys `timerExpiresAt` and `failsafeExpiresAt`, as wall-clock milliseconds.
    timer_expires_at: Option<u64>,
    failsafe_expires_at: Option<u64>,
}

/// JavaScript's `value === true || String(value).toUpperCase() === "ON"`.
fn is_on(value: &EventValue) -> bool {
    match value {
        EventValue::Bool(b) => *b,
        EventValue::Text(text) => text.to_uppercase() == "ON",
        _ => false,
    }
}

/// The light's `state` as the home has it now: the data bound with the category `light_state`,
/// else under the alias `state`; on when `true` or `"ON"` (the engine's `isAnyLightOn`).
fn light_is_on(home: &Home, light: &Id) -> bool {
    let Some(item) = home.equipments().get_by_id(light) else {
        return false;
    };
    let by_category = item.data_bindings.iter().find(|b| {
        home.devices
            .data_of(&b.device_data_id)
            .is_some_and(|(_, data)| &*data.category == "light_state")
    });
    let binding = by_category.or_else(|| item.data_bindings.iter().find(|b| b.alias == "state"));
    let Some((_, data)) = binding.and_then(|b| home.devices.data_of(&b.device_data_id)) else {
        return false;
    };
    match &data.value {
        Value::Bool(b) => *b,
        Value::String(text) => text.to_uppercase() == "ON",
        _ => false,
    }
}

fn any_light_on(home: &Home, lights: &[Id]) -> bool {
    lights.iter().any(|light| light_is_on(home, light))
}

/// Sends `ON` or `OFF` to every light, through its toggle order binding (the category
/// `light_toggle` or `toggle_power`, else the alias `state`). Fire and forget: the errors are
/// only the lights with no such binding, as the engine's `turnOnLights`. A disabled light gets
/// nothing and says nothing; an order the queue refuses (its bound is full) is dropped.
fn send(home: &Home, lights: &[Id], on: bool, now: u64) -> Vec<String> {
    let value = Value::String(if on { "ON" } else { "OFF" }.into());
    let mut errors = Vec::new();
    for light in lights {
        let Some(item) = home.equipments().get_by_id(light) else {
            errors.push(format!("{light}: no toggle order binding"));
            continue;
        };
        let by_category = item.order_bindings.iter().find(|b| {
            home.devices
                .order_of(&b.device_order_id)
                .is_some_and(|(_, order)| {
                    matches!(
                        order.category.as_deref(),
                        Some("light_toggle" | "toggle_power")
                    )
                })
        });
        let binding =
            by_category.or_else(|| item.order_bindings.iter().find(|b| b.alias == "state"));
        let Some(binding) = binding else {
            errors.push(format!("{light}: no toggle order binding"));
            continue;
        };
        if !item.equipment.enabled {
            continue;
        }
        let Ok(commands) = commands_for(
            &home.devices,
            std::slice::from_ref(binding),
            &binding.alias,
            &value,
        ) else {
            continue;
        };
        if commands.is_empty() {
            continue;
        }
        let request = OrderRequest {
            equipment: item.equipment.id,
            alias: binding.alias.to_string(),
            commands,
        };
        if home.orders().push_unattended(request, now).is_err() {
            log::warn!("recipe order for {light} dropped: the queue of recipe orders is full");
        }
    }
    errors
}

/// The root zone's `isDaylight`: mini-Sowel computes no sunrise or sunset, so it is always
/// `null`, read as night (spec 012, R6).
fn root_is_daylight(_home: &Home) -> Option<bool> {
    None
}

impl MotionLight {
    /// Starts an instance: reads the lights, then switches them to the recipe's idea of the
    /// state (`syncLightsOnStart`), whatever they were.
    pub fn start(params: Params, cx: &mut Cx<'_>) -> MotionLight {
        let on = any_light_on(cx.home, &params.lights);
        let sources = resolve_motion_sources(cx.home, &params.zone);
        let mut machine = MotionLight {
            p: params,
            sources,
            off: None,
            failsafe: None,
            override_mode: false,
            lights_on_by_recipe: on,
            last_known_light_on: on,
            grace_until: 0,
            timer_expires_at: None,
            failsafe_expires_at: None,
        };
        machine.sync_lights_on_start(cx);
        machine.grace_until = 0;
        machine
    }

    fn log(&self, cx: &mut Cx<'_>, level: Level, message: impl Into<String>) {
        cx.lines.push((level, message.into()));
    }

    // ---- What the home says ----

    fn zone_reading(&self, cx: &Cx<'_>) -> (bool, Option<i64>) {
        cx.home
            .zone_reading(&self.p.zone, cx.wall)
            .unwrap_or((false, None))
    }

    fn is_too_bright(&self, lux: Option<i64>) -> bool {
        match (self.p.lux_threshold, lux) {
            (Some(threshold), Some(lux)) => lux as f64 > threshold,
            _ => false,
        }
    }

    fn is_bright_enough_to_turn_off(&self, lux: Option<i64>) -> bool {
        match (self.p.lux_threshold, lux) {
            (Some(threshold), Some(lux)) => lux as f64 > threshold * (1.0 + LUX_HYSTERESIS_FACTOR),
            _ => false,
        }
    }

    /// Only the automatic turn-on and the start sync look at the daylight.
    fn is_daytime(&self, cx: &Cx<'_>) -> bool {
        self.p.disable_when_daylight && root_is_daylight(cx.home) == Some(true)
    }

    // ---- Timers and state keys ----

    fn delete_timer_key(&mut self) {
        self.timer_expires_at = None;
    }

    fn cancel_off_timer(&mut self) {
        self.off = None;
    }

    fn cancel_failsafe(&mut self) {
        self.failsafe = None;
        self.failsafe_expires_at = None;
    }

    fn clear_override_mode(&mut self) {
        self.override_mode = false;
    }

    fn arm_off_timer(&mut self, cx: &Cx<'_>, kind: OffKind) {
        self.off = Some(OffTimer {
            at: cx.now.saturating_add(self.p.timeout_ms),
            kind,
        });
        self.timer_expires_at = Some(cx.wall.saturating_add(self.p.timeout_ms));
    }

    fn start_off_timer(&mut self, cx: &Cx<'_>) {
        self.cancel_off_timer();
        self.arm_off_timer(cx, OffKind::Normal);
    }

    fn start_off_timer_for_override_clear(&mut self, cx: &Cx<'_>) {
        self.cancel_off_timer();
        self.arm_off_timer(cx, OffKind::OverrideClear);
    }

    /// Armed once; an armed failsafe is left as it is.
    fn start_failsafe_timer(&mut self, cx: &Cx<'_>) {
        let Some(max) = self.p.max_on_ms else {
            return;
        };
        if self.failsafe.is_some() {
            return;
        }
        self.failsafe = Some(cx.now.saturating_add(max));
        self.failsafe_expires_at = Some(cx.wall.saturating_add(max));
    }

    /// Restarts an armed failsafe with its full duration; it never arms one.
    fn reset_failsafe_timer(&mut self, cx: &Cx<'_>) {
        let Some(max) = self.p.max_on_ms else {
            return;
        };
        if self.failsafe.is_none() {
            return;
        }
        self.failsafe = Some(cx.now.saturating_add(max));
        self.failsafe_expires_at = Some(cx.wall.saturating_add(max));
    }

    // ---- Primitive actions ----

    fn turn_on(&mut self, cx: &mut Cx<'_>) {
        self.lights_on_by_recipe = true;
        self.last_known_light_on = true;
        let errors = send(cx.home, &self.p.lights, true, cx.now);
        if !errors.is_empty() {
            self.log(
                cx,
                Level::Error,
                format!("Error turning on some lights: {}", errors.join("; ")),
            );
        }
        self.log(
            cx,
            Level::Info,
            format!(
                "Motion detected — {} light(s) turned on",
                self.p.lights.len()
            ),
        );
        self.start_failsafe_timer(cx);
    }

    fn turn_off(&mut self, cx: &mut Cx<'_>, reason: &str) {
        self.lights_on_by_recipe = false;
        self.last_known_light_on = false;
        self.grace_until = cx.now.saturating_add(TURN_OFF_GRACE_MS);
        let errors = send(cx.home, &self.p.lights, false, cx.now);
        if !errors.is_empty() {
            self.log(
                cx,
                Level::Error,
                format!("Error turning off some lights: {}", errors.join("; ")),
            );
        }
        self.log(cx, Level::Info, reason);
        self.cancel_failsafe();
        self.clear_override_mode();
    }

    /// Like `turn_off`, but the override mode is left as it is.
    fn turn_off_failsafe(&mut self, cx: &mut Cx<'_>) {
        self.lights_on_by_recipe = false;
        self.last_known_light_on = false;
        self.grace_until = cx.now.saturating_add(TURN_OFF_GRACE_MS);
        let errors = send(cx.home, &self.p.lights, false, cx.now);
        if !errors.is_empty() {
            self.log(
                cx,
                Level::Error,
                format!("Error turning off some lights: {}", errors.join("; ")),
            );
        }
        self.log(
            cx,
            Level::Warn,
            format!(
                "Failsafe: lights forced off after {} max on duration",
                format_duration(self.p.max_on_ms.unwrap_or(0))
            ),
        );
    }

    // ---- Start sync ----

    fn sync_lights_on_start(&mut self, cx: &mut Cx<'_>) {
        let (motion, lux) = self.zone_reading(cx);
        if motion && !self.is_too_bright(lux) && !self.is_daytime(cx) {
            self.turn_on(cx);
        } else if self.is_daytime(cx) {
            self.turn_off(cx, "Recipe activated — daytime, lights off");
        } else if motion {
            self.turn_off(
                cx,
                "Recipe activated — luminosity above threshold, lights off",
            );
        } else {
            self.turn_off(cx, "Recipe activated — no motion, lights off");
        }
    }

    // ---- Handlers ----

    fn on_zone_changed(&mut self, motion: bool, lux: Option<i64>, cx: &mut Cx<'_>) {
        if self.override_mode {
            if motion {
                self.cancel_off_timer();
                self.delete_timer_key();
                self.reset_failsafe_timer(cx);
                self.log(
                    cx,
                    Level::Info,
                    "Motion detected but override mode active — ignoring",
                );
            } else if self.off.is_none() {
                self.start_off_timer_for_override_clear(cx);
            }
            return;
        }
        let lights_on = any_light_on(cx.home, &self.p.lights);
        if lights_on && self.is_bright_enough_to_turn_off(lux) {
            if !self.lights_on_by_recipe {
                if cx.now < self.grace_until {
                    return;
                }
                self.override_mode = true;
                self.start_failsafe_timer(cx);
                self.log(
                    cx,
                    Level::Info,
                    "Light turned on manually above lux threshold — entering override mode",
                );
            } else {
                self.cancel_off_timer();
                self.delete_timer_key();
                self.turn_off(cx, "Luminosity above threshold — lights turned off");
            }
            return;
        }
        if motion && !lights_on {
            if cx.now < self.grace_until {
                return;
            }
            if self.lights_on_by_recipe {
                self.lights_on_by_recipe = false;
                self.override_mode = true;
                self.start_off_timer_for_override_clear(cx);
                self.log(
                    cx,
                    Level::Info,
                    "Light turned off manually while motion active — entering override mode",
                );
                return;
            }
            if self.is_too_bright(lux) {
                if let (Some(lux), Some(threshold)) = (lux, self.p.lux_threshold) {
                    self.log(
                        cx,
                        Level::Info,
                        format!(
                            "Motion detected but luminosity {} exceeds threshold {} — not turning on",
                            js_number(lux as f64),
                            js_number(threshold)
                        ),
                    );
                }
                return;
            }
            if self.is_daytime(cx) {
                self.log(
                    cx,
                    Level::Info,
                    "Motion detected but daytime — not turning on",
                );
                return;
            }
            self.turn_on(cx);
        } else if motion && lights_on {
            self.cancel_off_timer();
            self.delete_timer_key();
            self.reset_failsafe_timer(cx);
        } else if !motion && lights_on && self.off.is_none() {
            self.start_off_timer(cx);
        }
    }

    fn on_light_changed(&mut self, value: &EventValue, cx: &mut Cx<'_>) {
        let light_on = is_on(value);
        if light_on == self.last_known_light_on {
            return;
        }
        self.last_known_light_on = light_on;
        if self.override_mode {
            return;
        }
        let (motion, _) = self.zone_reading(cx);
        if light_on && !motion {
            self.start_off_timer(cx);
            self.start_failsafe_timer(cx);
            self.log(
                cx,
                Level::Info,
                format!(
                    "Light turned on externally — turning off in {}",
                    format_duration(self.p.timeout_ms)
                ),
            );
        } else if light_on {
            self.cancel_off_timer();
            self.delete_timer_key();
        } else {
            if cx.now < self.grace_until {
                return;
            }
            if self.lights_on_by_recipe && motion {
                self.lights_on_by_recipe = false;
                self.override_mode = true;
                self.cancel_off_timer();
                self.cancel_failsafe();
                self.delete_timer_key();
                self.start_off_timer_for_override_clear(cx);
                self.log(
                    cx,
                    Level::Info,
                    "Light turned off manually while motion active — entering override mode",
                );
                return;
            }
            self.lights_on_by_recipe = false;
            if self.off.is_some() || self.failsafe.is_some() {
                self.cancel_off_timer();
                self.cancel_failsafe();
                self.delete_timer_key();
                self.log(
                    cx,
                    Level::Info,
                    "Light turned off externally — timers cancelled",
                );
            }
        }
    }

    fn on_button_action(&mut self, cx: &mut Cx<'_>) {
        if any_light_on(cx.home, &self.p.lights) {
            self.lights_on_by_recipe = false;
            self.last_known_light_on = false;
            self.grace_until = cx.now.saturating_add(TURN_OFF_GRACE_MS);
            let errors = send(cx.home, &self.p.lights, false, cx.now);
            if !errors.is_empty() {
                self.log(
                    cx,
                    Level::Error,
                    format!("Error turning off some lights: {}", errors.join("; ")),
                );
            }
            self.cancel_off_timer();
            self.cancel_failsafe();
            self.delete_timer_key();
            self.override_mode = true;
            self.log(
                cx,
                Level::Info,
                "Button pressed — lights off, entering override mode",
            );
            self.start_off_timer_for_override_clear(cx);
        } else {
            self.clear_override_mode();
            self.turn_on(cx);
        }
    }

    // ---- Timers firing ----

    fn fire_off_timer(&mut self, kind: OffKind, cx: &mut Cx<'_>) {
        self.off = None;
        self.delete_timer_key();
        match kind {
            OffKind::Normal => {
                let reason = format!(
                    "No motion for {} — lights turned off",
                    format_duration(self.p.timeout_ms)
                );
                self.turn_off(cx, &reason);
            }
            OffKind::OverrideClear => {
                if any_light_on(cx.home, &self.p.lights) {
                    self.lights_on_by_recipe = false;
                    self.last_known_light_on = false;
                    self.grace_until = cx.now.saturating_add(TURN_OFF_GRACE_MS);
                    // The error list is ignored on this path.
                    let _ = send(cx.home, &self.p.lights, false, cx.now);
                }
                self.clear_override_mode();
                self.cancel_failsafe();
                self.log(
                    cx,
                    Level::Info,
                    format!(
                        "No motion for {} — override cleared, lights off",
                        format_duration(self.p.timeout_ms)
                    ),
                );
            }
        }
    }

    fn fire_failsafe(&mut self, cx: &mut Cx<'_>) {
        self.cancel_failsafe();
        self.cancel_off_timer();
        self.delete_timer_key();
        self.turn_off_failsafe(cx);
    }
}

impl Runner for MotionLight {
    fn on_event(&mut self, event: &Event, cx: &mut Cx<'_>) -> Result<(), String> {
        match event {
            Event::Zone {
                zone,
                motion,
                luminosity,
            } if *zone == self.p.zone => self.on_zone_changed(*motion, *luminosity, cx),
            Event::Zone { .. } => {}
            Event::Equipment {
                equipment,
                alias,
                value,
            } => {
                if *alias == "state" && self.p.lights.contains(equipment) {
                    self.on_light_changed(value, cx);
                }
                if *alias == "action" && self.p.buttons.contains(equipment) {
                    self.on_button_action(cx);
                }
            }
            Event::Device { device, key, value } => {
                if self.p.max_on_ms.is_some()
                    && !self.sources.is_empty()
                    && self
                        .sources
                        .iter()
                        .any(|(d, k)| d == device && **k == **key)
                    && is_motion_active(value)
                {
                    self.reset_failsafe_timer(cx);
                }
            }
        }
        Ok(())
    }

    fn tick(&mut self, cx: &mut Cx<'_>) -> Result<(), String> {
        loop {
            let off_due = self.off.filter(|t| t.at <= cx.now);
            let failsafe_due = self.failsafe.filter(|at| *at <= cx.now);
            match (off_due, failsafe_due) {
                (None, None) => return Ok(()),
                (Some(off), Some(at)) if at < off.at => self.fire_failsafe(cx),
                (Some(off), _) => self.fire_off_timer(off.kind, cx),
                (None, Some(_)) => self.fire_failsafe(cx),
            }
        }
    }

    fn state(&self) -> StateKeys {
        StateKeys {
            override_mode: self.override_mode.then_some(true),
            timer_expires_at: self.timer_expires_at,
            failsafe_expires_at: self.failsafe_expires_at,
        }
    }

    fn listen(&self, interest: &mut Interest) {
        interest.add_zone(self.p.zone);
        for light in &self.p.lights {
            interest.add_equipment(*light, "state");
        }
        for button in &self.p.buttons {
            interest.add_equipment(*button, "action");
        }
        if self.p.max_on_ms.is_some() {
            for (device, key) in &self.sources {
                interest.add_device(*device, key);
            }
        }
    }
}
