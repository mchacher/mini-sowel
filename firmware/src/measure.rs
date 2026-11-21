//! The engine's memory measure under QEMU: ESP-IDF's local-minimum monitor of the free heap
//! (spec 016 R3.5).

use esp_idf_svc::sys::{
    MALLOC_CAP_INTERNAL, MALLOC_CAP_SPIRAM, heap_caps_get_minimum_free_size,
    heap_caps_monitor_local_minimum_free_size_start,
    heap_caps_monitor_local_minimum_free_size_stop,
};

use crate::memory;

/// The free heap, internal RAM and PSRAM together, now.
pub fn free_total() -> usize {
    let stats = memory::read();
    stats.internal_free + stats.psram.map_or(0, |p| p.free)
}

/// Starts the monitor of the local minimum of the free heap: from now on the minimums below are
/// the lowest seen since this call, not since boot. `false` when it was already running.
#[allow(unsafe_code)]
pub fn start_local_minimum() -> bool {
    // SAFETY: takes no argument and only switches the heap's low-watermark accounting; it returns
    // an error code (not UB) when the monitor is already running.
    unsafe { heap_caps_monitor_local_minimum_free_size_start() == 0 }
}

/// The lowest free internal RAM and the lowest free PSRAM, in bytes, since `start_local_minimum`
/// (each region's low watermark, so the sum of the two is a worst case).
#[allow(unsafe_code)]
pub fn local_minimums() -> (usize, usize) {
    // SAFETY: reads the heap's low-watermark counters for a capability mask; no precondition.
    unsafe {
        (
            heap_caps_get_minimum_free_size(MALLOC_CAP_INTERNAL),
            heap_caps_get_minimum_free_size(MALLOC_CAP_SPIRAM),
        )
    }
}

/// Stops the monitor started by `start_local_minimum`.
#[allow(unsafe_code)]
pub fn stop_local_minimum() {
    // SAFETY: takes no argument; returns an error code when the monitor is not running, which
    // is ignored (nothing to stop).
    let _ = unsafe { heap_caps_monitor_local_minimum_free_size_stop() };
}

/// The lowest free memory seen over the phases of a run (spec 016 R3.5). The monitor's figures
/// are the sum of the low watermarks of each region: read once over a whole run, the internal
/// RAM's low (at one moment) and the PSRAM's low (at another) would be added as if they came
/// together. Read phase by phase (the monitor is restarted at each `mark`), the sum is tight
/// within a phase, and the lowest sum of the phases is an upper bound of the true lowest free
/// heap.
pub struct Lows {
    /// The lowest sum of the internal RAM's and the PSRAM's free minimums of a phase.
    pub total: usize,
    pub internal: usize,
    pub psram: usize,
    phases: usize,
}

impl Lows {
    pub fn new() -> Self {
        Lows {
            total: usize::MAX,
            internal: usize::MAX,
            psram: usize::MAX,
            phases: 0,
        }
    }

    /// Ends a phase: folds its minimums in, and starts the monitor again for the next one.
    pub fn mark(&mut self) {
        let (internal, psram) = local_minimums();
        self.phases += 1;
        log::info!(
            "memory phase {}: lowest free {} KB",
            self.phases,
            (internal + psram) / 1024
        );
        self.total = self.total.min(internal + psram);
        self.internal = self.internal.min(internal);
        self.psram = self.psram.min(psram);
        stop_local_minimum();
        start_local_minimum();
    }
}
