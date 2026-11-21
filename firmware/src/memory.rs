//! Reads the free memory from ESP-IDF's heap functions.

use esp_idf_svc::sys::{
    MALLOC_CAP_INTERNAL, MALLOC_CAP_SPIRAM, heap_caps_get_free_size,
    heap_caps_get_largest_free_block, heap_caps_get_total_size,
};
use solite_core::{MemoryStats, PsramStats};

/// The free internal RAM and PSRAM, now.
#[allow(unsafe_code)]
pub fn read() -> MemoryStats {
    // SAFETY: these ESP-IDF functions only read the heap's counters. They take a capability mask
    // and have no other precondition; they are safe to call from any task after start-up.
    let (internal_free, internal_largest, psram_free, psram_total) = unsafe {
        (
            heap_caps_get_free_size(MALLOC_CAP_INTERNAL),
            heap_caps_get_largest_free_block(MALLOC_CAP_INTERNAL),
            heap_caps_get_free_size(MALLOC_CAP_SPIRAM),
            heap_caps_get_total_size(MALLOC_CAP_SPIRAM),
        )
    };
    MemoryStats {
        internal_free,
        internal_largest,
        psram: (psram_total > 0).then_some(PsramStats {
            free: psram_free,
            total: psram_total,
        }),
    }
}
