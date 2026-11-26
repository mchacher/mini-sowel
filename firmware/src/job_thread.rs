//! The threads a heavy request runs on (D59, D60): started for the job, their stack in internal
//! RAM, pinned to core 0 with the API workers, ended with the job. The worker that took the request
//! lends it and waits. An API worker's stack is 10 KB in PSRAM: a job that touches the flash (the
//! backup's) or that needs more stack than a worker has left (the update's signature check) runs
//! here. Each thread logs `<name>: thread stack <n> of <m> bytes never used` at its end.

use std::ffi::CStr;

use esp_idf_svc::hal::cpu::Core;
use esp_idf_svc::hal::task::thread::{MallocCap, ThreadSpawnConfiguration};
use esp_idf_svc::sys::uxTaskGetStackHighWaterMark2;

/// The stack of the thread a backup's export or restore runs on, taken for the job only (spec
/// 035, D59): the worker's size, for the same code.
#[cfg(feature = "radio")]
pub const BACKUP_STACK_BYTES: usize = 10 * 1024;

/// The stack of the thread an update's installation runs on, taken for the job only (spec 032,
/// D60). The core's whole installation (the header's Ed25519 signature check, which builds two
/// lookup tables of 1280 bytes on the stack, and the streamed SHA-256) uses 9.8 KB of an Xtensa
/// stack, measured in QEMU on 2026-11-25 (a bare thread uses 0.76 KB of it); the box adds the
/// request's frames (the body reader, the answer), which never nest under the signature check.
/// 16 KB leaves about 6 KB (5940 bytes unused in the QEMU run's check, `update_check.rs`), which
/// fails below 2 KB.
pub const UPDATE_STACK_BYTES: usize = 16 * 1024;

/// The stack of the thread a release check or pull runs on (`update-check`, `update-pull`; spec
/// 039 R8, D68). By reasoning, to be read on the box: ESP-IDF's `esp-tls` runs mbedTLS in the
/// calling task, and ESP-IDF's HTTPS examples give such a task 8 KB, the handshake (an ECDHE key
/// exchange, a chain check against the bundle) being its deepest part; the installation's Ed25519
/// check takes 9.2 KB (D60) and runs after the handshake returned, never nested in it; 20 KB
/// covers the larger of the two plus the request's frames and a margin. QEMU measures the pull's
/// core part on this size (`update_check.rs`); the handshake's share is read on the box from the
/// `thread stack` line.
pub const FETCH_STACK_BYTES: usize = 20 * 1024;

/// The calling task's unused stack, in bytes.
#[allow(unsafe_code)]
pub fn stack_unused() -> usize {
    // SAFETY: a null handle means the calling task; only reads its high-water mark.
    unsafe { uxTaskGetStackHighWaterMark2(std::ptr::null_mut()) as usize }
}

/// Runs `job` on a thread named `name` whose stack (`stack_bytes`) is in internal RAM, on core 0,
/// and waits for it. `Err` when the thread cannot start (no internal RAM block of that size):
/// `job` did not run. `Ok(None)` when `job` panicked.
pub fn run<T: Send>(
    name: &'static CStr,
    stack_bytes: usize,
    job: impl FnOnce() -> T + Send,
) -> Result<Option<T>, String> {
    let label = name.to_str().unwrap_or("job");
    let configured = ThreadSpawnConfiguration {
        name: Some(name),
        stack_size: stack_bytes,
        pin_to_core: Some(Core::Core0),
        stack_alloc_caps: MallocCap::Internal | MallocCap::Cap8bit,
        ..Default::default()
    }
    .set();
    if let Err(e) = configured {
        return Err(format!("{label}: cannot configure its job thread: {e}"));
    }
    let result = std::thread::scope(|scope| {
        let spawned = std::thread::Builder::new()
            .name(label.into())
            .stack_size(stack_bytes)
            .spawn_scoped(scope, move || {
                let result = job();
                log::info!(
                    "{label}: thread stack {} of {stack_bytes} bytes never used",
                    stack_unused()
                );
                result
            });
        match spawned {
            Ok(handle) => Ok(handle.join().ok()),
            Err(e) => Err(format!("{label}: cannot start its job thread: {e}")),
        }
    });
    // Threads this caller starts later get the default configuration again.
    let _ = ThreadSpawnConfiguration::default().set();
    result
}

/// Starts `job` on a thread named `name` (stack in internal RAM, core 0) and does not wait for it
/// (the automatic check, spec 039 R6): it logs the same `thread stack` line at its end. `Err`
/// when the thread cannot start: `job` did not run (and is dropped, with what it holds).
#[cfg(feature = "radio")]
pub fn spawn(
    name: &'static CStr,
    stack_bytes: usize,
    job: impl FnOnce() + Send + 'static,
) -> Result<(), String> {
    let label = name.to_str().unwrap_or("job");
    let configured = ThreadSpawnConfiguration {
        name: Some(name),
        stack_size: stack_bytes,
        pin_to_core: Some(Core::Core0),
        stack_alloc_caps: MallocCap::Internal | MallocCap::Cap8bit,
        ..Default::default()
    }
    .set();
    if let Err(e) = configured {
        return Err(format!("{label}: cannot configure its job thread: {e}"));
    }
    let spawned = std::thread::Builder::new()
        .name(label.into())
        .stack_size(stack_bytes)
        .spawn(move || {
            job();
            log::info!(
                "{label}: thread stack {} of {stack_bytes} bytes never used",
                stack_unused()
            );
        });
    // Threads this caller starts later get the default configuration again.
    let _ = ThreadSpawnConfiguration::default().set();
    spawned
        .map(|_| ())
        .map_err(|e| format!("{label}: cannot start its job thread: {e}"))
}
