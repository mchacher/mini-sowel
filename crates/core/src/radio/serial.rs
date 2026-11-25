//! What the radio over a serial link needs and the hardware cannot decide for it: the speeds to
//! try and the probe that finds the right one, the radio's two pins by role and its hardware
//! reset as a list of steps, the window of pairing at boot, the counters of the serial buffer, and
//! the restart of a wedged radio. No pin number, no clock and no I/O here: the firmware gives the
//! pins, the time and the closures, and this file is tested on the Mac.

use std::collections::VecDeque;

use super::ash::AshFailure;
use super::{Network, RadioError, network_line};

/// The speed the radio is expected to answer at; the probe tries it first.
pub const RADIO_BAUD: u32 = 115_200;
/// The speeds the probe tries, in this order (after `RADIO_BAUD`).
pub const BAUD_RATES: [u32; 3] = [115_200, 460_800, 921_600];
/// Milliseconds the reset line is held active.
pub const RESET_PULSE_MS: u64 = 100;
/// Milliseconds the EFR32's application takes to start after the reset line is released.
pub const BOOT_WAIT_MS: u64 = 1000;
/// Seconds the pairing window stays open at boot.
pub const PAIR_AT_BOOT_SECONDS: u8 = 180;
/// Milliseconds of running after which the boot marker is removed.
pub const STABLE_MS: u64 = 600_000;
/// Milliseconds between two reports of the serial buffer and of the heap.
pub const REPORT_EVERY_MS: u64 = 240_000;
/// Restarts of the radio allowed within `RESTART_WINDOW_MS`.
pub const MAX_RADIO_RESTARTS: usize = 3;
/// The window the restarts are counted in, in milliseconds.
pub const RESTART_WINDOW_MS: u64 = 600_000;

/// The speeds to try, `first` first when it is one of `BAUD_RATES`, then the others in the list's
/// order; no speed twice, and a speed that is not in the list is not tried.
pub fn probe_order(first: u32) -> Vec<u32> {
    let mut order = Vec::with_capacity(BAUD_RATES.len());
    if BAUD_RATES.contains(&first) {
        order.push(first);
    }
    for baud in BAUD_RATES {
        if !order.contains(&baud) {
            order.push(baud);
        }
    }
    order
}

/// Tries each speed of `order` with `try_speed`, which sets the speed, makes a fresh `Radio` and
/// identifies it. No answer to the reset (`NoResetAnswer`) goes to the next speed; any other
/// failure means the radio answered, and stops the probe with that error. Returns the speed that
/// answered with what `try_speed` returned, or `None` when no speed did. The lines go to `sink`.
pub fn probe<R>(
    order: &[u32],
    mut try_speed: impl FnMut(u32) -> Result<R, RadioError>,
    sink: &mut dyn FnMut(&str),
) -> Result<Option<(u32, R)>, RadioError> {
    for &baud in order {
        match try_speed(baud) {
            Ok(found) => {
                sink(&format!("radio: uart {baud} baud"));
                if baud != RADIO_BAUD {
                    sink(&format!(
                        "radio: answered at {baud} baud, RADIO_BAUD is {RADIO_BAUD}"
                    ));
                }
                return Ok(Some((baud, found)));
            }
            Err(RadioError::Ash(AshFailure::NoResetAnswer)) => {
                sink(&format!("radio: no answer at {baud} baud"));
            }
            Err(other) => return Err(other),
        }
    }
    let speeds: Vec<String> = BAUD_RATES.iter().map(u32::to_string).collect();
    sink(&format!(
        "radio: no answer at any baud rate ({})",
        speeds.join(", ")
    ));
    Ok(None)
}

/// The radio's two control lines, by role: the firmware maps each to a pin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RadioPin {
    /// Resets the radio; active low.
    Reset,
    /// Enters the radio's bootloader; active high. Never set active.
    Bootloader,
}

impl RadioPin {
    /// The electrical level (true = high) that makes the line active or inactive.
    pub fn level(self, active: bool) -> bool {
        match self {
            RadioPin::Reset => !active,
            RadioPin::Bootloader => active,
        }
    }
}

/// One step of a hardware reset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinStep {
    /// Makes a line active (`true`) or inactive (`false`).
    Set(RadioPin, bool),
    /// Waits this many milliseconds.
    Wait(u64),
}

/// The hardware reset: bootloader inactive, reset active for `RESET_PULSE_MS`, reset inactive,
/// then `BOOT_WAIT_MS` for the radio's application to start. The bootloader is never active.
pub fn reset_sequence() -> [PinStep; 5] {
    [
        PinStep::Set(RadioPin::Bootloader, false),
        PinStep::Set(RadioPin::Reset, true),
        PinStep::Wait(RESET_PULSE_MS),
        PinStep::Set(RadioPin::Reset, false),
        PinStep::Wait(BOOT_WAIT_MS),
    ]
}

/// Why the ESP32 started, as far as pairing at boot cares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResetReason {
    /// Power, or the USB bridge's reset line.
    PowerOn,
    /// A software restart.
    Software,
    /// A crash.
    Panic,
    /// A watchdog.
    Watchdog,
    /// The supply dipped.
    BrownOut,
    /// Any other reason.
    Other,
}

impl ResetReason {
    fn describe(self) -> &'static str {
        match self {
            ResetReason::PowerOn => "power-on",
            ResetReason::Software => "software restart",
            ResetReason::Panic => "crash",
            ResetReason::Watchdog => "watchdog",
            ResetReason::BrownOut => "brown-out",
            ResetReason::Other => "other reset",
        }
    }
}

/// Whether the pairing window opens at this boot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BootPairing {
    /// Opens for this many seconds.
    Open { seconds: u8 },
    /// Does not open; the reason, for the log.
    NotOpened(String),
}

impl BootPairing {
    /// The line the firmware logs when the window does not open.
    pub fn line(&self) -> Option<String> {
        match self {
            BootPairing::Open { .. } => None,
            BootPairing::NotOpened(reason) => {
                Some(format!("pairing: not opened at boot ({reason})"))
            }
        }
    }
}

/// Pairing opens at a power-on boot with no marker left by a boot within the last `STABLE_MS`;
/// a crash, a watchdog, a brown-out, a software restart or a boot loop opens nothing.
pub fn pairing_at_boot(marker_present: bool, reason: ResetReason) -> BootPairing {
    if reason != ResetReason::PowerOn {
        return BootPairing::NotOpened(format!("reset: {}", reason.describe()));
    }
    if marker_present {
        return BootPairing::NotOpened("another boot within the last 10 min".to_string());
    }
    BootPairing::Open {
        seconds: PAIR_AT_BOOT_SECONDS,
    }
}

/// True when the boot marker is to be removed: the image ran `STABLE_MS`.
pub fn marker_due(running_ms: u64) -> bool {
    running_ms >= STABLE_MS
}

/// Milliseconds still to wait before the boot marker is removed, `running_ms` into the run: the
/// timer of the firmware sleeps this long, whatever the engine does meanwhile.
pub fn marker_wait_ms(running_ms: u64) -> u64 {
    STABLE_MS.saturating_sub(running_ms)
}

/// The counters of the receive ring and the time of the next report. One per UART.
#[derive(Debug, Clone)]
pub struct SerialStats {
    capacity: usize,
    highest: usize,
    full: u32,
    was_full: bool,
    next_report_ms: u64,
}

impl SerialStats {
    /// `capacity` is the ring's size in bytes; the first report is due `REPORT_EVERY_MS` after
    /// `now_ms`.
    pub fn new(capacity: usize, now_ms: u64) -> Self {
        SerialStats {
            capacity,
            highest: 0,
            full: 0,
            was_full: false,
            next_report_ms: now_ms.saturating_add(REPORT_EVERY_MS),
        }
    }

    /// Takes a sample: the bytes waiting in the ring, before a read. "Full" is counted once each
    /// time the ring reaches its size, and again only after it dropped below.
    pub fn observe(&mut self, buffered: usize) {
        self.highest = self.highest.max(buffered);
        let full = buffered >= self.capacity;
        if full && !self.was_full {
            self.full = self.full.saturating_add(1);
        }
        self.was_full = full;
    }

    /// The `serial:` line when a report is due at `now_ms`, and the next one scheduled
    /// `REPORT_EVERY_MS` after the one due (never several for a late call).
    pub fn report(&mut self, now_ms: u64, baud: u32) -> Option<String> {
        if now_ms < self.next_report_ms {
            return None;
        }
        while self.next_report_ms <= now_ms {
            self.next_report_ms = self.next_report_ms.saturating_add(REPORT_EVERY_MS);
        }
        Some(format!(
            "serial: {baud} baud, rx buffer {} B, highest {} B, full {} times",
            self.capacity, self.highest, self.full
        ))
    }
}

/// The times of the last restarts of the radio, at most `MAX_RADIO_RESTARTS` kept.
#[derive(Debug, Default)]
pub struct Restarts {
    times: VecDeque<u64>,
}

impl Restarts {
    pub fn new() -> Self {
        Restarts::default()
    }

    /// Records a restart at `now_ms`, or refuses it with the line to log when
    /// `MAX_RADIO_RESTARTS` happened within `RESTART_WINDOW_MS`.
    pub fn allow(&mut self, now_ms: u64, last_error: &str) -> Result<(), String> {
        while let Some(&oldest) = self.times.front() {
            if now_ms.saturating_sub(oldest) >= RESTART_WINDOW_MS {
                self.times.pop_front();
            } else {
                break;
            }
        }
        if self.times.len() >= MAX_RADIO_RESTARTS {
            return Err(format!(
                "radio: stopped after {MAX_RADIO_RESTARTS} restarts in {} min: {last_error}",
                RESTART_WINDOW_MS / 60_000
            ));
        }
        self.times.push_back(now_ms);
        Ok(())
    }
}

/// What a restart does, as closures the firmware gives.
pub struct RestartSteps<'a> {
    /// Feeds the engine's watchdog; called before each of the steps below.
    pub feed: &'a mut dyn FnMut(),
    /// Saves what is due; an error is logged and does not stop the restart.
    pub save: &'a mut dyn FnMut() -> Result<(), String>,
    /// Ends an open pairing phase (the home's `finished`).
    pub finish_pairing: &'a mut dyn FnMut(),
    /// Runs the hardware reset sequence.
    pub reset: &'a mut dyn FnMut(),
    /// Probes the serial speed and identifies the radio; `None` when no speed answers.
    pub probe: &'a mut dyn FnMut() -> Result<Option<u32>, RadioError>,
    /// Resumes the radio's network: never forms one.
    pub listen: &'a mut dyn FnMut() -> Result<Network, RadioError>,
}

/// How a restart ended.
#[derive(Debug)]
pub enum Restarted {
    /// The radio answers and its network is resumed.
    Resumed(Network),
    /// The 4th restart in the window was refused: the caller restarts the box (`radio-stopped`).
    /// The line was logged.
    Stopped,
    /// The radio answers at no speed: the caller restarts the box (`radio-silent`).
    Silent,
}

/// Restarts a wedged radio: save what is due, end an open pairing phase, reset, probe, resume
/// the network (never form one, never open a window). `last_error` is what wedged it. An error
/// of the probe or of the resume is returned: the caller counts it as a radio error again.
pub fn restart(
    restarts: &mut Restarts,
    now_ms: u64,
    last_error: &str,
    steps: &mut RestartSteps,
    sink: &mut dyn FnMut(&str),
) -> Result<Restarted, RadioError> {
    if let Err(line) = restarts.allow(now_ms, last_error) {
        sink(&line);
        return Ok(Restarted::Stopped);
    }
    sink(&format!("radio: restarting after: {last_error}"));
    (steps.feed)();
    if let Err(error) = (steps.save)() {
        sink(&error);
    }
    (steps.feed)();
    (steps.finish_pairing)();
    (steps.feed)();
    (steps.reset)();
    (steps.feed)();
    if (steps.probe)()?.is_none() {
        return Ok(Restarted::Silent);
    }
    (steps.feed)();
    let network = (steps.listen)()?;
    sink(&network_line(&network, false));
    Ok(Restarted::Resumed(network))
}
