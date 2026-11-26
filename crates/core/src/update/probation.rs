//! The probation of a newly booted image (R5): healthy for a minute confirms it, five minutes
//! without a confirmation roll it back. Pure: the caller gives the monotonic time and the health
//! (the engine, the API and the PSRAM, spec 043).

use super::{CONFIRM_AFTER_MS, ROLLBACK_AFTER_MS};

/// What the probation asks for at one tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Wait,
    Confirm,
    RollBack,
}

/// What the probation reads at one tick: all three must hold for the program to be healthy. The
/// Wi-Fi link is not one of them (D54); the PSRAM is (D79): the memory budget does not hold without
/// it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Health {
    /// The engine's loop is running.
    pub engine: bool,
    /// The API server is listening.
    pub api: bool,
    /// The PSRAM is there (the host, which has none to expect, says true).
    pub psram: bool,
}

impl Health {
    pub fn healthy(self) -> bool {
        self.reason().is_none()
    }

    /// Why the program is not healthy, the first of the three that fails (`None`: healthy).
    pub fn reason(self) -> Option<&'static str> {
        if !self.engine {
            Some("engine stopped")
        } else if !self.api {
            Some("API not listening")
        } else if !self.psram {
            Some("no PSRAM")
        } else {
            None
        }
    }
}

/// The state of one probation: when it started and since when the program has been healthy
/// without a break.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Probation {
    start_ms: u64,
    healthy_since: Option<u64>,
    /// The reason last given by `reason_changed`, so each change is said once.
    reported: Option<&'static str>,
}

impl Probation {
    pub fn new(start_ms: u64) -> Self {
        Probation {
            start_ms,
            healthy_since: None,
            reported: None,
        }
    }

    /// The reason of an unhealthy tick when it differs from the one last reported (to be logged
    /// once, not each second); healthy ticks forget it, so a new break is reported again.
    pub fn reason_changed(&mut self, health: Health) -> Option<&'static str> {
        let reason = health.reason();
        if reason == self.reported {
            return None;
        }
        self.reported = reason;
        reason
    }

    /// The verdict at `now_ms` (monotonic). The deadline wins over a confirmation due at the same
    /// tick; the verdict stays the same until the caller drops the probation, so a `confirm` or a
    /// `roll_back` that failed is asked again at the next tick.
    pub fn tick(&mut self, now_ms: u64, health: Health) -> Verdict {
        if now_ms.saturating_sub(self.start_ms) >= ROLLBACK_AFTER_MS {
            return Verdict::RollBack;
        }
        if !health.healthy() {
            self.healthy_since = None;
            return Verdict::Wait;
        }
        let since = *self.healthy_since.get_or_insert(now_ms);
        if now_ms.saturating_sub(since) >= CONFIRM_AFTER_MS {
            Verdict::Confirm
        } else {
            Verdict::Wait
        }
    }
}
