//! The probation of a newly booted image (R5): healthy for a minute confirms it, five minutes
//! without a confirmation roll it back. Pure: the caller gives the monotonic time and the health.

use super::{CONFIRM_AFTER_MS, ROLLBACK_AFTER_MS};

/// What the probation asks for at one tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Wait,
    Confirm,
    RollBack,
}

/// The state of one probation: when it started and since when the program has been healthy
/// without a break.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Probation {
    start_ms: u64,
    healthy_since: Option<u64>,
}

impl Probation {
    pub fn new(start_ms: u64) -> Self {
        Probation {
            start_ms,
            healthy_since: None,
        }
    }

    /// The verdict at `now_ms` (monotonic). The deadline wins over a confirmation due at the same
    /// tick; the verdict stays the same until the caller drops the probation, so a `confirm` or a
    /// `roll_back` that failed is asked again at the next tick.
    pub fn tick(&mut self, now_ms: u64, healthy: bool) -> Verdict {
        if now_ms.saturating_sub(self.start_ms) >= ROLLBACK_AFTER_MS {
            return Verdict::RollBack;
        }
        if !healthy {
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
