//! The requests in progress (spec 022, moved from the host's `serve.rs`): at most `MAX_WORKERS`
//! at once, the next one is told to come back (503 `Busy`). A slot is released when its guard
//! drops, a panic unwinding included.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Requests answered at the same time; one more is told to come back (503).
pub const MAX_WORKERS: usize = 4;

/// A counter of the requests in progress. At most `max` slots exist at once (a number, not a
/// collection: nothing grows).
#[derive(Debug, Clone)]
pub struct Slots {
    taken: Arc<AtomicUsize>,
    max: usize,
}

/// A slot taken; released when dropped.
#[derive(Debug)]
pub struct SlotGuard {
    taken: Arc<AtomicUsize>,
}

impl Slots {
    pub fn new(max: usize) -> Slots {
        Slots {
            taken: Arc::new(AtomicUsize::new(0)),
            max,
        }
    }

    /// Takes a slot, or `None` when `max` are taken.
    pub fn try_take(&self) -> Option<SlotGuard> {
        if self.taken.fetch_add(1, Ordering::SeqCst) >= self.max {
            self.taken.fetch_sub(1, Ordering::SeqCst);
            return None;
        }
        Some(SlotGuard {
            taken: Arc::clone(&self.taken),
        })
    }
}

impl Drop for SlotGuard {
    fn drop(&mut self) {
        self.taken.fetch_sub(1, Ordering::SeqCst);
    }
}
