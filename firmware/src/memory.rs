//! Reads the free memory from ESP-IDF's heap functions.

use esp_idf_svc::sys::{
    MALLOC_CAP_INTERNAL, MALLOC_CAP_SPIRAM, heap_caps_get_free_size,
    heap_caps_get_largest_free_block, heap_caps_get_minimum_free_size, heap_caps_get_total_size,
};
use solite_core::{MemoryProbe, MemoryReading, MemoryStats, PsramStats};

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

/// The heap as the recipe installs read it (spec 042 R4.4): what is free now, the largest block of
/// the internal RAM, and the lowest each memory has been since boot.
#[allow(dead_code)]
pub struct EspProbe;

impl MemoryProbe for EspProbe {
    #[allow(unsafe_code)]
    fn read(&self) -> Option<MemoryReading> {
        // SAFETY: only reads the heap's low-watermark counters for a capability mask; callable
        // from any task after start-up.
        let (internal_min, psram_min) = unsafe {
            (
                heap_caps_get_minimum_free_size(MALLOC_CAP_INTERNAL),
                heap_caps_get_minimum_free_size(MALLOC_CAP_SPIRAM),
            )
        };
        Some(MemoryReading {
            stats: read(),
            internal_min,
            psram_min,
        })
    }
}
