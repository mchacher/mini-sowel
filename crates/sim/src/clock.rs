//! Time for the simulation. The simulator never reads a clock of its own: its caller gives it
//! one (real time in the host, a fake clock that only moves when it is made to wait in the
//! tests).

use std::cell::Cell;
use std::rc::Rc;
use std::thread;
use std::time::{Duration, Instant};

/// A clock the simulator can read and wait on.
pub trait Clock {
    /// Milliseconds since the clock's own origin.
    fn now_ms(&self) -> u64;
    /// Lets `ms` milliseconds pass: sleeps for a real clock, moves a fake one forward.
    fn wait_ms(&self, ms: u64);
}

/// Real time.
pub struct RealClock {
    start: Instant,
}

impl RealClock {
    pub fn new() -> Self {
        RealClock {
            start: Instant::now(),
        }
    }
}

impl Default for RealClock {
    fn default() -> Self {
        RealClock::new()
    }
}

impl Clock for RealClock {
    fn now_ms(&self) -> u64 {
        u64::try_from(self.start.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    fn wait_ms(&self, ms: u64) {
        thread::sleep(Duration::from_millis(ms));
    }
}

/// A fake clock: it moves only when something waits. The tests keep a handle on `time`.
#[derive(Clone, Default)]
pub struct FakeClock {
    pub time: Rc<Cell<u64>>,
}

impl Clock for FakeClock {
    fn now_ms(&self) -> u64 {
        self.time.get()
    }

    fn wait_ms(&self, ms: u64) {
        self.time.set(self.time.get().saturating_add(ms));
    }
}

/// Scenario time against clock time: `at` times pass `speed` times faster than the clock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScenarioClock {
    /// 1 to 100.
    pub speed: u32,
}

impl ScenarioClock {
    /// Scenario time reached after `clock_ms` of clock time since the start.
    pub fn scenario_ms(&self, clock_ms: u64) -> u64 {
        clock_ms.saturating_mul(u64::from(self.speed))
    }

    /// Clock time since the start at which the scenario time `at_ms` is reached (rounded up).
    pub fn clock_ms(&self, at_ms: u64) -> u64 {
        let speed = u64::from(self.speed.max(1));
        at_ms.div_ceil(speed)
    }
}
