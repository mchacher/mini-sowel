use solite_core::{
    EngineStats, HeapStats, MemoryStats, PsramStats, ScriptPhase, engine_line, heap_line,
    memory_line, script_phase_line, scripts_lines,
};

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
        reported: 48,
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
        "engine: 50 devices (48 reported), 10 zones, 50 equipments, 3 recipes; heap 160 KB (peak 190 KB); internal 120 KB free (min 90 KB); psram 1700 KB free (min 1650 KB)"
    );
}

#[test]
fn heap_line_shows_the_minimums_and_the_stack_in_kib() {
    let stats = HeapStats {
        internal_free: 120 * 1024 + 500,
        internal_min: 90 * 1024,
        internal_largest: 60 * 1024 + 100,
        psram_free: 1700 * 1024,
        psram_min: 1650 * 1024,
        psram_largest: 1500 * 1024,
        stack_unused: 20 * 1024,
        stack_total: 32 * 1024,
        http_stack: None,
    };
    assert_eq!(
        heap_line(&stats),
        "heap: internal 120 KB free (min 90 KB), largest block 60 KB, psram 1700 KB free (min 1650 KB), largest block 1500 KB, engine stack 20 KB unused of 32 KB"
    );
}

#[test]
fn heap_line_adds_the_http_workers_stack_when_there_is_one() {
    let stats = HeapStats {
        internal_free: 120 * 1024,
        internal_min: 90 * 1024,
        internal_largest: 60 * 1024 + 100,
        psram_free: 1700 * 1024,
        psram_min: 1650 * 1024,
        psram_largest: 1500 * 1024,
        stack_unused: 27 * 1024,
        stack_total: 32 * 1024,
        http_stack: Some((6 * 1024, 10 * 1024)),
    };
    assert_eq!(
        heap_line(&stats),
        "heap: internal 120 KB free (min 90 KB), largest block 60 KB, psram 1700 KB free (min 1650 KB), largest block 1500 KB, engine stack 27 KB unused of 32 KB, http stack 6 KB unused of 10 KB"
    );
}

#[test]
fn heap_line_prints_zero_when_there_is_no_psram() {
    let stats = HeapStats {
        internal_free: 120 * 1024,
        internal_min: 90 * 1024,
        internal_largest: 60 * 1024,
        psram_free: 0,
        psram_min: 0,
        psram_largest: 0,
        stack_unused: 20 * 1024,
        stack_total: 32 * 1024,
        http_stack: None,
    };
    assert!(heap_line(&stats).contains("psram 0 KB free (min 0 KB), largest block 0 KB, engine"));
}

#[test]
fn heap_line_keeps_the_prefix_the_scripts_parse() {
    let stats = HeapStats {
        internal_free: 120 * 1024,
        internal_min: 90 * 1024,
        internal_largest: 60 * 1024,
        psram_free: 1700 * 1024,
        psram_min: 1650 * 1024,
        psram_largest: 1500 * 1024,
        stack_unused: 20 * 1024,
        stack_total: 32 * 1024,
        http_stack: None,
    };
    let line = heap_line(&stats);
    assert!(line.starts_with("heap: internal 120 KB free (min 90 KB)"));
    // The `sed` of scripts/lib/firmware.sh reads the first `(min <m> KB)`.
    let start = line.find("(min ").unwrap() + 5;
    let minimum: String = line[start..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    assert_eq!(minimum, "90");
}

#[test]
fn the_scripts_line_shows_calls_time_and_heap_per_script() {
    let stats = solite_recipe_sdk::RunStats {
        instances: 3,
        calls: 4,
        total_us: 1000,
        max_us: 700,
        held: 90 * 1024 + 5,
        peak: 100 * 1024,
    };
    let idle = solite_recipe_sdk::RunStats::default();
    assert_eq!(
        scripts_lines(&[("motion-light".to_string(), stats), ("b".to_string(), idle)]),
        vec![
            "scripts: motion-light 3 instances, 4 calls, average 250 µs, max 700 µs, heap 90 KB (peak 100 KB)",
            "scripts: b 0 instances, 0 calls, average 0 µs, max 0 µs, heap 0 KB (peak 0 KB)",
        ]
    );
}

#[test]
fn the_script_phase_line_is_the_one_qemu_reads() {
    let phase = ScriptPhase {
        source: 15 * 1024 + 100,
        psram_used: 60 * 1024,
        psram_peak: 70 * 1024,
        heap_peak: 480 * 1024 + 5,
        internal_used: 1024,
        internal_min: 120 * 1024,
        engine_stack: (6000, 32768),
        script_stack: (3000, 24576),
    };
    assert_eq!(
        script_phase_line("motion-light", &phase),
        "recipes: script motion-light 15 KB source; psram +60 KB (peak +70 KB); internal +1 KB (min 120 KB free); engine stack 6000 of 32768 unused; script thread stack 3000 of 24576 unused; heap peak 480 KB"
    );
}
