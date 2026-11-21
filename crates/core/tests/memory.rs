use solite_core::{EngineStats, MemoryStats, PsramStats, engine_line, memory_line};

#[test]
fn line_shows_internal_and_psram_in_kib() {
    let stats = MemoryStats {
        internal_free: 245 * 1024,
        internal_largest: 110 * 1024,
        psram: Some(PsramStats {
            free: 2039 * 1024,
            total: 2048 * 1024,
        }),
    };
    assert_eq!(
        memory_line(&stats),
        "memory: internal 245 KB free (largest block 110 KB), psram 2039 KB free of 2048 KB"
    );
}

#[test]
fn line_says_none_without_psram() {
    let stats = MemoryStats {
        internal_free: 245 * 1024,
        internal_largest: 110 * 1024,
        psram: None,
    };
    assert_eq!(
        memory_line(&stats),
        "memory: internal 245 KB free (largest block 110 KB), psram none"
    );
}

#[test]
fn sizes_are_rounded_down() {
    let stats = MemoryStats {
        internal_free: 1023,
        internal_largest: 2047,
        psram: Some(PsramStats {
            free: 1024 + 1023,
            total: 3 * 1024 - 1,
        }),
    };
    assert_eq!(
        memory_line(&stats),
        "memory: internal 0 KB free (largest block 1 KB), psram 1 KB free of 2 KB"
    );
}

#[test]
fn zeros_do_not_panic() {
    let stats = MemoryStats {
        internal_free: 0,
        internal_largest: 0,
        psram: Some(PsramStats { free: 0, total: 0 }),
    };
    assert_eq!(
        memory_line(&stats),
        "memory: internal 0 KB free (largest block 0 KB), psram 0 KB free of 0 KB"
    );
}

#[test]
fn the_engine_line_shows_counts_and_sizes_in_kib_rounded_down() {
    let stats = EngineStats {
        devices: 50,
        zones: 10,
        equipments: 50,
        recipes: 3,
        heap_used: 160 * 1024 + 1023,
        heap_peak: 190 * 1024,
        internal_free: 120 * 1024,
        internal_min: 90 * 1024,
        psram_free: 1700 * 1024,
        psram_min: 1650 * 1024,
    };
    assert_eq!(
        engine_line(&stats),
        "engine: 50 devices, 10 zones, 50 equipments, 3 recipes; heap 160 KB (peak 190 KB); internal 120 KB free (min 90 KB); psram 1700 KB free (min 1650 KB)"
    );
}
