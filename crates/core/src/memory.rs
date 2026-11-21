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
