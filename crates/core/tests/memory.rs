use mini_sowel_core::{MemoryStats, PsramStats, memory_line};

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
