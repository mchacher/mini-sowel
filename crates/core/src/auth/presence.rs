//! The confirmation at the box (R6, D48): a person who is at the box, not on the network, has
//! confirmed. The core asks an adapter; the Mac host confirms always, the box's firmware gives its
//! own (the 10 minutes after a power-on, the button once it is wired). Times are on a monotonic
//! uptime clock given to the adapter, never the wall clock, which jumps when it is set.

use std::sync::{Arc, Mutex};

/// A button press confirms for this long, in milliseconds.
pub const CONFIRM_WINDOW_MS: u64 = 120_000;
/// After a power-on boot the box confirms for this long, in milliseconds (until its button is
/// wired).
pub const BOOT_CONFIRM_MS: u64 = 600_000;

/// The uptime in milliseconds (the same shape as the orders' clock).
type Clock = Arc<dyn Fn() -> u64 + Send + Sync>;

/// Whether a person at the box has confirmed, and what to tell the one who has not.
pub trait Presence: Send {
    /// True while a person at the box has confirmed.
    fn confirmed(&self) -> bool;
    /// What the person must do (the text of the refusal).
    fn hint(&self) -> &str;
}

/// The default: nothing can confirm.
#[derive(Debug, Default, Clone, Copy)]
pub struct NeverConfirmed;

impl Presence for NeverConfirmed {
    fn confirmed(&self) -> bool {
        false
    }

    fn hint(&self) -> &str {
        "Confirmation at the box is not available"
    }
}

/// The Mac host (loopback only): the person at the Mac is at the host.
#[derive(Debug, Default, Clone, Copy)]
pub struct AlwaysConfirmed;

impl Presence for AlwaysConfirmed {
    fn confirmed(&self) -> bool {
        true
    }

    fn hint(&self) -> &str {
        ""
    }
}

/// The box until its button is wired: confirmed during `BOOT_CONFIRM_MS` after a power-on boot
/// (power-on, external reset, USB: the reasons of D32's pairing window), never after a software
/// restart or a crash.
pub struct BootPresence {
    power_on: bool,
    clock: Clock,
}

impl BootPresence {
    /// `power_on`: the boot's reason was a power-on one; `clock`: the uptime in milliseconds.
    pub fn new(power_on: bool, clock: Clock) -> Self {
        BootPresence { power_on, clock }
    }
}

impl Presence for BootPresence {
    fn confirmed(&self) -> bool {
        self.power_on && (self.clock)() < BOOT_CONFIRM_MS
    }

    fn hint(&self) -> &str {
        "Unplug the box and plug it back in, then try again within 10 minutes"
    }
}

/// The box's button, for the firmware later: `press` confirms until `CONFIRM_WINDOW_MS` later. A
/// clone shares the press, so the button's task keeps one and the home the other.
#[derive(Clone)]
pub struct ButtonPresence {
    /// The uptime of the last press; `NEVER` before the first. A mutex, not an atomic: the
    /// ESP32's Xtensa core has no 64-bit atomics.
    pressed_at: Arc<Mutex<u64>>,
    clock: Clock,
}

const NEVER: u64 = u64::MAX;

impl ButtonPresence {
    pub fn new(clock: Clock) -> Self {
        ButtonPresence {
            pressed_at: Arc::new(Mutex::new(NEVER)),
            clock,
        }
    }

    /// The button was pressed now.
    pub fn press(&self) {
        *crate::devices::lock(&self.pressed_at) = (self.clock)();
    }
}

impl Presence for ButtonPresence {
    fn confirmed(&self) -> bool {
        let at = *crate::devices::lock(&self.pressed_at);
        let now = (self.clock)();
        at != NEVER && now >= at && now - at < CONFIRM_WINDOW_MS
    }

    fn hint(&self) -> &str {
        "Press the button on the box, then try again"
    }
}
