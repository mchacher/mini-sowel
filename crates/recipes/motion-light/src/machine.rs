//! The state machine of `motion-light`: a port of the recipe's `createInstance` (recipe
//! `src/index.ts` R:406-924) and of the helpers of Sowel's engine, with every quirk kept (see
//! `specs/012-motion-light/motion-light-reference.md`, section 4). Timers are deadlines on the
//! engine's monotonic clock, checked by `tick`; the home is read, orders are sent and lines are
//! logged through the host in `Cx`.

use serde_json::Value;
use solite_recipe_sdk::bounds::MAX_LISTEN_DEVICES;
use solite_recipe_sdk::duration::{format_duration, js_number};
use solite_recipe_sdk::time::format_iso;
use solite_recipe_sdk::{
    Clock, Event, EventValue, Host, IdBytes, Level, ListenEntry, OrderStatus, format_id, parse_id,
};

use super::params::{MAX_BUTTONS, Params};
use super::sources::{MotionSource, is_motion_active, resolve_motion_sources};

/// What a call of an instance has: the host to read and command the home, and the clocks. The
/// lines go to the host's log, in the order they are written.
pub struct Cx<'a> {
    pub host: &'a dyn Host,
    /// The engine's monotonic clock, for timers.
    pub now: u64,
    /// The wall clock, for the dates written.
    pub wall: u64,
}

impl<'a> Cx<'a> {
    pub fn new(host: &'a dyn Host, clock: Clock) -> Cx<'a> {
        Cx {
            host,
            now: clock.mono,
            wall: clock.wall,
        }
    }

    fn log(&self, level: Level, message: impl AsRef<str>) {
        self.host.log(level, message.as_ref());
    }
}

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
    /// Resolved when the instance starts, and again at each settle (`refresh`).
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
fn light_is_on(host: &dyn Host, light: &IdBytes) -> bool {
    let Some(item) = host.equipment(&format_id(light)) else {
        return false;
    };
    let by_category = item
        .data_bindings
        .iter()
        .find(|b| b.category.as_deref() == Some("light_state"));
    let binding = by_category.or_else(|| item.data_bindings.iter().find(|b| b.alias == "state"));
    match binding.map(|b| &b.value) {
        Some(EventValue::Bool(b)) => *b,
        Some(EventValue::Text(text)) => text.to_uppercase() == "ON",
        _ => false,
    }
}

fn any_light_on(host: &dyn Host, lights: &[IdBytes]) -> bool {
    lights.iter().any(|light| light_is_on(host, light))
}

/// Sends `ON` or `OFF` to every light, through its toggle order binding (the category
/// `light_toggle` or `toggle_power`, else the alias `state`). Fire and forget: the errors are
/// only the lights with no such binding, as the engine's `turnOnLights`. A disabled light gets
/// nothing and says nothing; an order the queue refuses (its bound is full) is dropped by the
/// host.
fn send(host: &dyn Host, lights: &[IdBytes], on: bool) -> Vec<String> {
    let value = Value::String(if on { "ON" } else { "OFF" }.into());
    let mut errors = Vec::new();
    for light in lights {
        let light = format_id(light);
        let Some(item) = host.equipment(&light) else {
            errors.push(format!("{light}: no toggle order binding"));
            continue;
        };
        let by_category = item
            .order_bindings
            .iter()
            .find(|b| matches!(b.category.as_deref(), Some("light_toggle" | "toggle_power")));
        let binding =
            by_category.or_else(|| item.order_bindings.iter().find(|b| b.alias == "state"));
        let Some(binding) = binding else {
            errors.push(format!("{light}: no toggle order binding"));
            continue;
        };
        if host.order(&item.id, &binding.id, &value) == OrderStatus::NoBinding {
            errors.push(format!("{light}: no toggle order binding"));
        }
    }
    errors
}

/// The root zone's `isDaylight` (spec 026 R2.4), from the home's settings: `null` without a
/// location or before the clock is set, read as night.
fn root_is_daylight(host: &dyn Host) -> Option<bool> {
    host.sunlight().is_daylight
}

impl MotionLight {
    /// Starts an instance: reads the lights, then switches them to the recipe's idea of the
    /// state (`syncLightsOnStart`), whatever they were. `reading` is the zone's motion and
    /// luminosity as the start saw them.
    pub fn start(params: Params, reading: (bool, Option<i64>), cx: &mut Cx<'_>) -> MotionLight {
        let on = any_light_on(cx.host, &params.lights);
        let sources = resolve_motion_sources(cx.host, &format_id(&params.zone));
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
        machine.sync_lights_on_start(reading, cx);
        machine.grace_until = 0;
        machine
    }

    fn log(&self, cx: &mut Cx<'_>, level: Level, message: impl Into<String>) {
        cx.log(level, message.into());
    }

    // ---- What the home says ----

    fn zone_reading(&self, cx: &Cx<'_>) -> (bool, Option<i64>) {
        cx.host
            .zone_data(&format_id(&self.p.zone))
            .map_or((false, None), |zone| (zone.motion, zone.luminosity))
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
        self.p.disable_when_daylight && root_is_daylight(cx.host) == Some(true)
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
        let errors = send(cx.host, &self.p.lights, true);
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
        let errors = send(cx.host, &self.p.lights, false);
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
        let errors = send(cx.host, &self.p.lights, false);
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

    fn sync_lights_on_start(&mut self, reading: (bool, Option<i64>), cx: &mut Cx<'_>) {
        let (motion, lux) = reading;
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
        let lights_on = any_light_on(cx.host, &self.p.lights);
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
        if any_light_on(cx.host, &self.p.lights) {
            self.lights_on_by_recipe = false;
            self.last_known_light_on = false;
            self.grace_until = cx.now.saturating_add(TURN_OFF_GRACE_MS);
            let errors = send(cx.host, &self.p.lights, false);
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
                if any_light_on(cx.host, &self.p.lights) {
                    self.lights_on_by_recipe = false;
                    self.last_known_light_on = false;
                    self.grace_until = cx.now.saturating_add(TURN_OFF_GRACE_MS);
                    // The error list is ignored on this path.
                    let _ = send(cx.host, &self.p.lights, false);
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

impl MotionLight {
    /// An event the instance listens to.
    pub fn on_event(&mut self, event: &Event, cx: &mut Cx<'_>) {
        match event {
            Event::Zone {
                zone,
                motion,
                luminosity,
            } if parse_id(zone) == Some(self.p.zone) => {
                self.on_zone_changed(*motion, *luminosity, cx)
            }
            Event::Zone { .. } => {}
            Event::Equipment {
                equipment,
                alias,
                value,
            } => {
                let equipment = parse_id(equipment);
                if alias == "state" && equipment.is_some_and(|e| self.p.lights.contains(&e)) {
                    self.on_light_changed(value, cx);
                }
                if alias == "action" && equipment.is_some_and(|e| self.p.buttons.contains(&e)) {
                    self.on_button_action(cx);
                }
            }
            Event::Device { device, key, value } => {
                if self.p.max_on_ms.is_some()
                    && !self.sources.is_empty()
                    && parse_id(device).is_some_and(|device| {
                        self.sources
                            .iter()
                            .any(|(d, k)| *d == device && **k == **key)
                    })
                    && is_motion_active(value)
                {
                    self.reset_failsafe_timer(cx);
                }
            }
        }
    }

    /// The time passed: the timers that are due fire.
    pub fn tick(&mut self, cx: &mut Cx<'_>) {
        loop {
            let off_due = self.off.filter(|t| t.at <= cx.now);
            let failsafe_due = self.failsafe.filter(|at| *at <= cx.now);
            match (off_due, failsafe_due) {
                (None, None) => return,
                (Some(off), Some(at)) if at < off.at => self.fire_failsafe(cx),
                (Some(off), _) => self.fire_off_timer(off.kind, cx),
                (None, Some(_)) => self.fire_failsafe(cx),
            }
        }
    }

    /// The state keys, as the JSON object `GET /recipe-instances` serves: only the keys that are
    /// set, in this order (`overrideMode`, `timerExpiresAt`, `failsafeExpiresAt`). At most about
    /// 110 bytes, far under the SDK's `MAX_STATE_BYTES`.
    pub fn state_text(&self) -> String {
        let mut keys: Vec<String> = Vec::new();
        if self.override_mode {
            keys.push("\"overrideMode\":true".to_string());
        }
        if let Some(at) = self.timer_expires_at {
            keys.push(format!("\"timerExpiresAt\":\"{}\"", format_iso(at)));
        }
        if let Some(at) = self.failsafe_expires_at {
            keys.push(format!("\"failsafeExpiresAt\":\"{}\"", format_iso(at)));
        }
        format!("{{{}}}", keys.join(","))
    }

    /// The home changed and the instance is kept: takes again its sources and its buttons,
    /// without touching its timers or state. No order, no log.
    pub fn refresh(&mut self, host: &dyn Host, params: &Value) {
        self.sources = resolve_motion_sources(host, &format_id(&self.p.zone));
        if let Some(Value::Array(list)) = params.get("buttons") {
            // Bounded as at validation: at most `MAX_BUTTONS`.
            self.p.buttons = list
                .iter()
                .filter_map(Value::as_str)
                .filter_map(parse_id)
                .take(MAX_BUTTONS)
                .collect();
        } else {
            self.p.buttons.clear();
        }
    }

    /// What the instance listens to: its zone, its lights' `state`, its buttons' `action`, and,
    /// with a failsafe, its motion sources (at most `MAX_LISTEN_DEVICES` of them).
    pub fn listen(&self) -> Vec<ListenEntry> {
        let mut listen = vec![ListenEntry::Zone {
            zone: format_id(&self.p.zone),
        }];
        for light in &self.p.lights {
            listen.push(ListenEntry::Equipment {
                equipment: format_id(light),
                alias: "state".into(),
            });
        }
        for button in &self.p.buttons {
            listen.push(ListenEntry::Equipment {
                equipment: format_id(button),
                alias: "action".into(),
            });
        }
        if self.p.max_on_ms.is_some() {
            for (device, key) in self.sources.iter().take(MAX_LISTEN_DEVICES) {
                listen.push(ListenEntry::Device {
                    device: format_id(device),
                    key: key.to_string(),
                });
            }
        }
        listen
    }
}
