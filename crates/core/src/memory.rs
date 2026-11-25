//! The free memory of the device, and the line that reports it in the logs.

/// Free memory, in bytes, as the platform reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryStats {
    /// Free internal RAM.
    pub internal_free: usize,
    /// Largest block of internal RAM that can still be allocated in one piece.
    pub internal_largest: usize,
    /// Free PSRAM (external RAM) and its total size; `None` when the device has no PSRAM.
    pub psram: Option<PsramStats>,
}

/// The PSRAM sizes, in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PsramStats {
    pub free: usize,
    pub total: usize,
}

/// Bytes to KiB, rounded down.
fn kib(bytes: usize) -> usize {
    bytes / 1024
}

/// The line written after the banner: `memory: internal 245 KB free (largest block 110 KB), psram
/// 2039 KB free of 2048 KB`, or `... psram none` without PSRAM. Sizes in KiB, rounded down.
pub fn memory_line(stats: &MemoryStats) -> String {
    let psram = match stats.psram {
        Some(p) => format!("{} KB free of {} KB", kib(p.free), kib(p.total)),
        None => "none".to_string(),
    };
    format!(
        "memory: internal {} KB free (largest block {} KB), psram {}",
        kib(stats.internal_free),
        kib(stats.internal_largest),
        psram
    )
}

/// What the engine holds and what it cost, for the line the firmware logs after the scenario of
/// the QEMU image (spec 016 R3.5). Sizes in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EngineStats {
    pub devices: usize,
    /// The devices that got at least one measure into the engine while the scenario played.
    pub reported: usize,
    /// The zones made by the user (the root zone is not counted).
    pub zones: usize,
    pub equipments: usize,
    pub recipes: usize,
    /// Heap used by the engine: free before it started minus free after the scenario (internal
    /// RAM and PSRAM together).
    pub heap_used: usize,
    /// Free before it started minus the lowest free seen while it ran.
    pub heap_peak: usize,
    pub internal_free: usize,
    /// The lowest internal RAM free seen while the engine ran.
    pub internal_min: usize,
    pub psram_free: usize,
    pub psram_min: usize,
}

/// The line logged after the scenario: `engine: 50 devices (50 reported), 10 zones, 50 equipments, 3 recipes;
/// heap 160 KB (peak 190 KB); internal 120 KB free (min 90 KB); psram 1700 KB free (min 1650
/// KB)`. Sizes in KiB, rounded down.
pub fn engine_line(stats: &EngineStats) -> String {
    format!(
        "engine: {} devices ({} reported), {} zones, {} equipments, {} recipes; heap {} KB (peak {} KB); internal {} KB free (min {} KB); psram {} KB free (min {} KB)",
        stats.devices,
        stats.reported,
        stats.zones,
        stats.equipments,
        stats.recipes,
        kib(stats.heap_used),
        kib(stats.heap_peak),
        kib(stats.internal_free),
        kib(stats.internal_min),
        kib(stats.psram_free),
        kib(stats.psram_min),
    )
}

/// What the radio image reports every few minutes: the free memory with its minimums since boot,
/// and the engine thread's stack. Sizes in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeapStats {
    pub internal_free: usize,
    /// The lowest internal RAM free since boot.
    pub internal_min: usize,
    /// The largest free block of internal RAM (fragmentation indicator).
    pub internal_largest: usize,
    pub psram_free: usize,
    /// The lowest PSRAM free since boot.
    pub psram_min: usize,
    /// The largest free block of PSRAM; 0 when there is none.
    pub psram_largest: usize,
    /// Stack bytes never used by the engine thread (its high-water mark).
    pub stack_unused: usize,
    /// The engine thread's whole stack.
    pub stack_total: usize,
    /// The HTTP workers' stack, `(unused, total)` in bytes: the smallest high-water mark of the
    /// workers. `None` when the image serves no HTTP (spec 022).
    pub http_stack: Option<(usize, usize)>,
}

/// The line the radio image logs every few minutes: `heap: internal 120 KB free (min 90 KB),
/// largest block 60 KB, psram 1700 KB free (min 1650 KB), largest block 1500 KB, engine stack 20 KB
/// unused of 32 KB`, followed by `, http stack 6 KB unused of 10 KB` when the image serves HTTP. Sizes in KiB, rounded down.
pub fn heap_line(stats: &HeapStats) -> String {
    let http = stats
        .http_stack
        .map_or_else(String::new, |(unused, total)| {
            format!(
                ", http stack {} KB unused of {} KB",
                kib(unused),
                kib(total)
            )
        });
    format!(
        "heap: internal {} KB free (min {} KB), largest block {} KB, psram {} KB free (min {} KB), largest block {} KB, engine stack {} KB unused of {} KB{http}",
        kib(stats.internal_free),
        kib(stats.internal_min),
        kib(stats.internal_largest),
        kib(stats.psram_free),
        kib(stats.psram_min),
        kib(stats.psram_largest),
        kib(stats.stack_unused),
        kib(stats.stack_total),
    )
}
