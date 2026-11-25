//! The supervisor on the radio image (spec 036, D58): the RTC record that survives a software
//! restart, the adapter the core's `Supervisor` needs, the engine thread's task watchdog, and
//! the restart. The decisions (cause, mode, boot-loop guard, retry) are the core's, tested on
//! the Mac; this file gives them the hardware.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use esp_idf_svc::sys::{esp_restart, esp_task_wdt_add, esp_task_wdt_delete, esp_task_wdt_reset};
use solite_core::devices::lock;
use solite_core::home::Home;
use solite_core::supervisor::{
    Mode, RECORD_WORDS, Reason, Status, Supervisor, SupervisorAdapter, last_rung,
};

use crate::radio_engine::{STOP, reset_reason};

/// The record, in RTC slow memory that no boot clears (`.rtc_noinit`): it survives a software
/// restart, a panic and a watchdog reset, and holds garbage after a power-on (the core tells
/// the two apart). Plain loads and stores only: the atomic read-modify-write instructions
/// (`S32C1I`) do not work on RTC memory.
#[allow(unsafe_code)]
#[unsafe(link_section = ".rtc_noinit")]
static RECORD: [AtomicU32; RECORD_WORDS] = [const { AtomicU32::new(0) }; RECORD_WORDS];

/// True once the engine thread is subscribed to the task watchdog: a feed before that, or after
/// the unsubscribe, is not made (the watchdog would log an error).
static SUBSCRIBED: AtomicBool = AtomicBool::new(false);

/// The supervisor and the instant of its boot, set once by `boot`.
static SUPERVISOR: OnceLock<Mutex<Supervisor>> = OnceLock::new();
static BOOTED: OnceLock<Instant> = OnceLock::new();
/// The home the API serves, once `watch_home` has run: its copy of the status is refreshed by
/// the tick, so the hour and day counts follow the clock.
static HOME: OnceLock<Arc<Mutex<Home>>> = OnceLock::new();
/// The tick refreshes the home's status this often, in seconds.
const STATUS_REFRESH_S: u64 = 60;
/// The tick's run time at the last refresh, in seconds.
static REFRESHED_S: AtomicU32 = AtomicU32::new(0);

/// What the core's supervisor gets: the RTC words, the watchdog's feed, the restart.
pub struct Adapter;

impl SupervisorAdapter for Adapter {
    fn load(&mut self) -> [u32; RECORD_WORDS] {
        let mut words = [0; RECORD_WORDS];
        for (word, cell) in words.iter_mut().zip(RECORD.iter()) {
            *word = cell.load(Ordering::Relaxed);
        }
        words
    }

    fn store(&mut self, words: &[u32; RECORD_WORDS]) {
        for (cell, word) in RECORD.iter().zip(words.iter()) {
            cell.store(*word, Ordering::Relaxed);
        }
    }

    fn feed(&mut self) {
        feed();
    }

    #[allow(unsafe_code)]
    fn restart(&mut self) {
        // The line just logged gets out of the serial port first.
        std::thread::sleep(Duration::from_secs(1));
        // SAFETY: takes no argument; restarts the chip, never returns.
        unsafe { esp_restart() }
    }
}

/// Reads the record and the reset reason, logs the boot lines, and returns the mode. The first
/// call of a run; the console, the OTA thread and the engine use the supervisor after it.
pub fn boot() -> Mode {
    let supervisor = Supervisor::boot(&mut Adapter, reset_reason());
    for line in supervisor.lines() {
        if supervisor.mode() == Mode::Normal {
            log::info!("{line}");
        } else {
            log::warn!("{line}");
        }
    }
    let mode = supervisor.mode();
    let _ = BOOTED.set(Instant::now());
    let _ = SUPERVISOR.set(Mutex::new(supervisor));
    mode
}

/// The status the API shows, or the default one before `boot`.
pub fn status() -> Status {
    SUPERVISOR
        .get()
        .map(|supervisor| lock(supervisor).status())
        .unwrap_or_default()
}

/// Gives the home the supervisor's status (the API's `GET /system/supervisor` and the safe-mode
/// refusals read it).
pub fn set_supervision(home: &mut Home) {
    home.set_supervision(status());
}

/// Keeps the served home's status current: the tick refreshes it every `STATUS_REFRESH_S`.
pub fn watch_home(home: &Arc<Mutex<Home>>) {
    let _ = HOME.set(Arc::clone(home));
}

/// Writes the supervisor's current status into the watched home. The supervisor's lock is
/// released before the home's is taken (the order elsewhere is home, then supervisor).
fn refresh_home() {
    let Some(home) = HOME.get() else {
        return;
    };
    let current = status();
    lock(home).set_supervision(current);
}

/// Called about once a second by the OTA thread: moves the supervisor's clock and, in safe mode,
/// restarts the box when the retry is due. Does nothing before `boot`.
pub fn tick() {
    let (Some(supervisor), Some(booted)) = (SUPERVISOR.get(), BOOTED.get()) else {
        return;
    };
    let run_ms = u64::try_from(booted.elapsed().as_millis()).unwrap_or(u64::MAX);
    lock(supervisor).tick(&mut Adapter, run_ms, &mut |line| log::warn!("{line}"));
    let run_s = u32::try_from(run_ms / 1000).unwrap_or(u32::MAX);
    if run_s.saturating_sub(REFRESHED_S.load(Ordering::Relaxed)) >= STATUS_REFRESH_S as u32 {
        REFRESHED_S.store(run_s, Ordering::Relaxed);
        refresh_home();
    }
}

/// Writes `asked` as the reason of the restart that follows (the console, an update, a restore,
/// the rollback); the caller restarts. Without a booted supervisor it does nothing.
pub fn asked_restart() {
    if let Some(supervisor) = SUPERVISOR.get() {
        lock(supervisor).asked(&mut Adapter);
    }
}

/// Takes back an `asked` whose restart did not happen (a refused rollback).
pub fn cancel_asked() {
    if let Some(supervisor) = SUPERVISOR.get() {
        lock(supervisor).cancel_asked(&mut Adapter);
    }
}

/// The last rung of the engine's restart ladder, and any error that ends the engine: `save` runs
/// once, the reason is written, the box restarts. Returns when a stop was asked meanwhile (the
/// asked restart wins), and on a box whose supervisor was not booted.
pub fn last_rung_restart(
    reason: Reason,
    detail: &str,
    save: &mut dyn FnMut() -> Result<(), String>,
) {
    let Some(supervisor) = SUPERVISOR.get() else {
        return;
    };
    last_rung(
        supervisor,
        &mut Adapter,
        reason,
        detail,
        &STOP,
        save,
        &mut |line| log::error!("{line}"),
    );
}

/// Feeds the engine thread's watchdog, when it is subscribed.
#[allow(unsafe_code)]
pub fn feed() {
    if SUBSCRIBED.load(Ordering::Relaxed) {
        // SAFETY: takes no argument; resets the timer of the calling task's subscription.
        unsafe { esp_task_wdt_reset() };
    }
}

/// Subscribes the calling task (the engine thread) to the task watchdog. An error is logged and
/// the feeds stay off.
#[allow(unsafe_code)]
pub fn subscribe() {
    // SAFETY: a null handle means the calling task; the task is not subscribed yet.
    let code = unsafe { esp_task_wdt_add(std::ptr::null_mut()) };
    if code == 0 {
        SUBSCRIBED.store(true, Ordering::SeqCst);
    } else {
        log::error!("supervisor: cannot subscribe the engine to the watchdog (error {code})");
    }
}

/// Takes the calling task off the watchdog, before its thread ends.
#[allow(unsafe_code)]
pub fn unsubscribe() {
    if SUBSCRIBED.swap(false, Ordering::SeqCst) {
        // SAFETY: a null handle means the calling task, which is subscribed.
        let code = unsafe { esp_task_wdt_delete(std::ptr::null_mut()) };
        if code != 0 {
            log::error!(
                "supervisor: cannot unsubscribe the engine from the watchdog (error {code})"
            );
        }
    }
}
