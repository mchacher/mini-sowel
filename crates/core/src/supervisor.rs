//! The supervisor (spec 036, D58): why the box restarted, kept across the restart in a record of
//! 24 words that the firmware puts in RTC memory (no flash write); the boot-loop guard that turns
//! too many restarts into a safe mode; the last rung of the engine's restart ladder; the figures
//! of the engine's watchdog. The hardware (the RTC words, the watchdog, the restart) is behind
//! `SupervisorAdapter`; nothing here touches it.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Serialize, Serializer};

use crate::devices::lock;
use crate::radio::serial::ResetReason;

/// Words of the record kept across a restart (96 bytes).
pub const RECORD_WORDS: usize = 24;
/// At most this many restart times are kept in the record (bound).
pub const MAX_KEPT: usize = 6;
/// Counted restarts within `HOUR_MS` that enter the safe mode.
pub const RESTARTS_PER_HOUR: usize = 3;
/// Counted restarts within `DAY_MS` that enter the safe mode.
pub const RESTARTS_PER_DAY: usize = 6;
pub const HOUR_MS: u64 = 60 * 60 * 1000;
pub const DAY_MS: u64 = 24 * HOUR_MS;
/// How long the safe mode lasts before its retry: the first entry since the power-on, the second,
/// the third and the later ones (supervisor clock, milliseconds).
pub const SAFE_RETRIES_MS: [u64; 3] = [HOUR_MS, 4 * HOUR_MS, DAY_MS];
/// Counted restarts since the safe mode began that make it the bare safe mode.
pub const BARE_AFTER: usize = 3;
/// The clock goes to the record at most this often, in milliseconds (a crash loses at most this).
pub const TICK_STORE_MS: u64 = 10_000;
/// The engine's task watchdog timeout, in milliseconds (`CONFIG_ESP_TASK_WDT_TIMEOUT_S` = 20).
pub const WATCHDOG_TIMEOUT_MS: u64 = 20_000;
/// The longest time the engine thread blocks without feeding: the flash save of a 98 KB
/// `devices.json` (specs 020, 025), in milliseconds.
pub const LONGEST_STALL_MS: u64 = 3_800;

const MAGIC: u32 = 0x5353_5631; // "SSV1"
const VERSION: u32 = 1;
const NONE_WORD: u32 = u32::MAX;
const POWER_ON: &str = "power-on";

/// Why the box restarted (spec 036 R1): a stable code and a text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    RadioStopped = 1,
    RadioSilent = 2,
    RadioStart = 3,
    EngineError = 4,
    Panic = 5,
    Watchdog = 6,
    BrownOut = 7,
    Unexpected = 8,
    Asked = 9,
    SafeModeRetry = 10,
}

impl Reason {
    /// The stable number kept in the record.
    pub fn code(self) -> u32 {
        self as u32
    }

    /// The reason of a code; `None` for an unknown one (and for 0, which means none).
    pub fn from_code(code: u32) -> Option<Reason> {
        Some(match code {
            1 => Reason::RadioStopped,
            2 => Reason::RadioSilent,
            3 => Reason::RadioStart,
            4 => Reason::EngineError,
            5 => Reason::Panic,
            6 => Reason::Watchdog,
            7 => Reason::BrownOut,
            8 => Reason::Unexpected,
            9 => Reason::Asked,
            10 => Reason::SafeModeRetry,
            _ => return None,
        })
    }

    /// The text of the log and the API.
    pub fn text(self) -> &'static str {
        match self {
            Reason::RadioStopped => "radio-stopped",
            Reason::RadioSilent => "radio-silent",
            Reason::RadioStart => "radio-start",
            Reason::EngineError => "engine-error",
            Reason::Panic => "panic",
            Reason::Watchdog => "watchdog",
            Reason::BrownOut => "brown-out",
            Reason::Unexpected => "unexpected",
            Reason::Asked => "asked",
            Reason::SafeModeRetry => "safe-mode-retry",
        }
    }

    /// True when the restart counts toward the safe mode: a brown-out, a restart asked and the
    /// safe mode's retry do not.
    pub fn counts(self) -> bool {
        !matches!(
            self,
            Reason::BrownOut | Reason::Asked | Reason::SafeModeRetry
        )
    }

    /// The reason a reset gives when nothing of Solite wrote one; `None` for a power-on.
    fn of_reset(reset: ResetReason) -> Option<Reason> {
        match reset {
            ResetReason::PowerOn => None,
            ResetReason::Panic => Some(Reason::Panic),
            ResetReason::Watchdog => Some(Reason::Watchdog),
            ResetReason::BrownOut => Some(Reason::BrownOut),
            ResetReason::Software | ResetReason::Other => Some(Reason::Unexpected),
        }
    }
}

/// How the box runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    /// The engine is stopped; the home is loaded and served.
    Safe,
    /// The engine is stopped and the home's files are not even read.
    SafeBare,
}

impl Mode {
    pub fn text(self) -> &'static str {
        match self {
            Mode::Normal => "normal",
            Mode::Safe => "safe",
            Mode::SafeBare => "safe-bare",
        }
    }
}

impl Serialize for Mode {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.text())
    }
}

/// What `GET /system/supervisor` answers (spec 036 R9).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub mode: Mode,
    /// A reason's text, or `power-on`.
    pub boot_cause: &'static str,
    /// The last reason but `asked` and `safe-mode-retry` since the power-on.
    pub last_incident: Option<&'static str>,
    pub restarts_last_hour: usize,
    pub restarts_last_day: usize,
}

impl Default for Status {
    fn default() -> Self {
        Status {
            mode: Mode::Normal,
            boot_cause: POWER_ON,
            last_incident: None,
            restarts_last_hour: 0,
            restarts_last_day: 0,
        }
    }
}

/// What is kept across a restart.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Record {
    /// Milliseconds the box ran since the last power-on, across restarts (the supervisor clock).
    pub clock_ms: u64,
    /// The clock when the safe mode began; `None` outside it.
    pub safe_since: Option<u64>,
    /// The reason written before a restart, read at the next boot.
    pub pending: Option<Reason>,
    /// The last incident since the power-on.
    pub last: Option<Reason>,
    /// The clock at the last counted restarts, oldest first; bound `MAX_KEPT`.
    pub restarts: VecDeque<u64>,
    /// Boots since the power-on.
    pub boots: u32,
    /// Safe-mode entries since the power-on.
    pub safe_entries: u32,
}

fn fnv1a(words: &[u32]) -> u32 {
    let mut hash: u32 = 0x811c_9dc5;
    for word in words {
        for byte in word.to_le_bytes() {
            hash ^= u32::from(byte);
            hash = hash.wrapping_mul(0x0100_0193);
        }
    }
    hash
}

/// A `u64` as (low, high) words.
fn split(value: u64) -> (u32, u32) {
    (value as u32, (value >> 32) as u32)
}

fn join(low: u32, high: u32) -> u64 {
    u64::from(low) | (u64::from(high) << 32)
}

impl Record {
    /// The words of the record, the checksum last.
    pub fn encode(&self) -> [u32; RECORD_WORDS] {
        let mut words = [0u32; RECORD_WORDS];
        words[0] = MAGIC;
        words[1] = VERSION;
        words[2] = self.pending.map_or(0, Reason::code);
        words[3] = self.last.map_or(0, Reason::code);
        (words[4], words[5]) = split(self.clock_ms);
        (words[6], words[7]) = self.safe_since.map_or((NONE_WORD, NONE_WORD), split);
        // The newest `MAX_KEPT` times, whatever the caller put in.
        let skip = self.restarts.len().saturating_sub(MAX_KEPT);
        let mut kept = 0;
        for (i, time) in self.restarts.iter().skip(skip).enumerate() {
            (words[9 + 2 * i], words[10 + 2 * i]) = split(*time);
            kept += 1;
        }
        words[8] = kept;
        words[21] = self.boots;
        words[22] = self.safe_entries;
        words[23] = fnv1a(&words[..23]);
        words
    }

    /// The record of these words; `None` when the magic, the version, the checksum, a reason code
    /// or the count is wrong (the caller starts a fresh record).
    pub fn decode(words: &[u32; RECORD_WORDS]) -> Option<Record> {
        if words[0] != MAGIC || words[1] != VERSION || words[23] != fnv1a(&words[..23]) {
            return None;
        }
        let optional = |code: u32| match code {
            0 => Some(None),
            code => Reason::from_code(code).map(Some),
        };
        let pending = optional(words[2])?;
        let last = optional(words[3])?;
        let count = usize::try_from(words[8]).ok().filter(|n| *n <= MAX_KEPT)?;
        let safe_since = if words[6] == NONE_WORD && words[7] == NONE_WORD {
            None
        } else {
            Some(join(words[6], words[7]))
        };
        Some(Record {
            clock_ms: join(words[4], words[5]),
            safe_since,
            pending,
            last,
            restarts: (0..count)
                .map(|i| join(words[9 + 2 * i], words[10 + 2 * i]))
                .collect(),
            boots: words[21],
            safe_entries: words[22],
        })
    }
}

/// What the firmware gives the supervisor (the Mac's tests use a fake).
pub trait SupervisorAdapter {
    /// The words kept across a restart (garbage after a power-on).
    fn load(&mut self) -> [u32; RECORD_WORDS];
    /// Writes them: RTC memory, never the flash.
    fn store(&mut self, words: &[u32; RECORD_WORDS]);
    /// Feeds the engine thread's watchdog.
    fn feed(&mut self);
    /// Restarts the whole box; never returns on the box.
    fn restart(&mut self);
}

/// The boot-loop guard and the record, for one run of the box.
#[derive(Debug)]
pub struct Supervisor {
    record: Record,
    /// The cause of this boot: `None` for a power-on.
    cause: Option<Reason>,
    mode: Mode,
    /// The clock at this boot, the clock now, and the clock last written to the record.
    base_ms: u64,
    clock_ms: u64,
    stored_ms: u64,
    /// The lines of the boot: two at most.
    lines: Vec<String>,
}

fn hours_text(ms: u64) -> String {
    format!("{} h", ms / HOUR_MS)
}

impl Supervisor {
    /// Reads the record and the reset reason, decides the boot cause and the mode, writes the
    /// record back (the cause is consumed). A power-on starts a fresh record.
    pub fn boot(adapter: &mut dyn SupervisorAdapter, reset: ResetReason) -> Supervisor {
        let words = adapter.load();
        let (mut record, cause) = if reset == ResetReason::PowerOn {
            (Record::default(), None)
        } else {
            let mut record = Record::decode(&words).unwrap_or_default();
            let cause = record.pending.take().or(Reason::of_reset(reset));
            (record, cause)
        };
        record.boots = record.boots.saturating_add(1);
        if let Some(reason) = cause {
            match reason {
                Reason::Asked | Reason::SafeModeRetry => {
                    record.restarts.clear();
                    record.safe_since = None;
                }
                _ => {
                    record.last = Some(reason);
                    if reason.counts() {
                        // Strictly increasing times: two boots with the same stored clock stay
                        // two restarts, and "since the safe mode began" is exact.
                        if let Some(last) = record.restarts.back() {
                            record.clock_ms = record.clock_ms.max(last + 1);
                        }
                        if record.restarts.len() == MAX_KEPT {
                            record.restarts.pop_front();
                        }
                        record.restarts.push_back(record.clock_ms);
                    }
                }
            }
        }
        let clock = record.clock_ms;
        while let Some(&oldest) = record.restarts.front() {
            if clock.saturating_sub(oldest) >= DAY_MS {
                record.restarts.pop_front();
            } else {
                break;
            }
        }
        let mut entered = false;
        let mode = match record.safe_since {
            Some(since) => {
                let since_safe = record.restarts.iter().filter(|t| **t > since).count();
                if since_safe >= BARE_AFTER {
                    Mode::SafeBare
                } else {
                    Mode::Safe
                }
            }
            None => {
                let (hour, day) = counts_at(&record.restarts, clock);
                if hour >= RESTARTS_PER_HOUR || day >= RESTARTS_PER_DAY {
                    record.safe_since = Some(clock);
                    record.safe_entries = record.safe_entries.saturating_add(1);
                    entered = true;
                    Mode::Safe
                } else {
                    Mode::Normal
                }
            }
        };
        let (hour, day) = counts_at(&record.restarts, clock);
        let mut line = format!(
            "supervisor: boot after {}, {hour} restarts in the last hour, {day} in the last day",
            cause.map_or(POWER_ON, Reason::text)
        );
        match mode {
            Mode::Normal => {}
            Mode::Safe => line.push_str(", safe mode"),
            Mode::SafeBare => line.push_str(", bare safe mode"),
        }
        let mut lines = vec![line];
        if entered {
            lines.push(format!(
                "supervisor: safe mode: the engine stays stopped, the API and updates stay up; \
                 retried in {}",
                hours_text(retry_after(record.safe_entries))
            ));
        }
        let supervisor = Supervisor {
            cause,
            mode,
            base_ms: clock,
            clock_ms: clock,
            stored_ms: clock,
            lines,
            record,
        };
        adapter.store(&supervisor.record.encode());
        supervisor
    }

    /// The lines the boot logs.
    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// What the API shows, the counts taken at the current clock.
    pub fn status(&self) -> Status {
        let (hour, day) = counts_at(&self.record.restarts, self.clock_ms);
        Status {
            mode: self.mode,
            boot_cause: self.cause.map_or(POWER_ON, Reason::text),
            last_incident: self.record.last.map(Reason::text),
            restarts_last_hour: hour,
            restarts_last_day: day,
        }
    }

    /// The supervisor clock now, in milliseconds.
    pub fn clock_ms(&self) -> u64 {
        self.clock_ms
    }

    /// Called about once a second with the milliseconds this boot has run: moves the clock,
    /// writes it to the record at most every `TICK_STORE_MS`, and in safe mode restarts the box
    /// once when the retry is due (`safe-mode-retry`; its line goes to `sink` first). Returns
    /// true when it asked the restart.
    pub fn tick(
        &mut self,
        adapter: &mut dyn SupervisorAdapter,
        run_ms: u64,
        sink: &mut dyn FnMut(&str),
    ) -> bool {
        self.clock_ms = self.clock_ms.max(self.base_ms.saturating_add(run_ms));
        if let Some(since) = self.record.safe_since
            && self.clock_ms.saturating_sub(since) >= retry_after(self.record.safe_entries)
        {
            sink("supervisor: restarting the box to retry the engine after the safe mode");
            self.record.pending = Some(Reason::SafeModeRetry);
            self.store_now(adapter);
            adapter.restart();
            return true;
        }
        if self.clock_ms.saturating_sub(self.stored_ms) >= TICK_STORE_MS {
            self.store_now(adapter);
        }
        false
    }

    /// Writes `reason` as pending, with the clock, then restarts the box; the line goes to `sink`
    /// first (the restart never returns on the box).
    pub fn incident(
        &mut self,
        adapter: &mut dyn SupervisorAdapter,
        reason: Reason,
        detail: &str,
        sink: &mut dyn FnMut(&str),
    ) {
        sink(&format!(
            "supervisor: restarting the box after {}: {detail}",
            reason.text()
        ));
        self.record.pending = Some(reason);
        self.store_now(adapter);
        adapter.restart();
    }

    /// Writes `asked` as pending, with the clock: the caller restarts the box (the console's
    /// `restart`, an update, a restore, a rollback).
    pub fn asked(&mut self, adapter: &mut dyn SupervisorAdapter) {
        self.record.pending = Some(Reason::Asked);
        self.store_now(adapter);
    }

    /// Takes back an `asked` written for a restart that did not happen (a rollback refused): a
    /// later crash is then counted as such.
    pub fn cancel_asked(&mut self, adapter: &mut dyn SupervisorAdapter) {
        if self.record.pending == Some(Reason::Asked) {
            self.record.pending = None;
            self.store_now(adapter);
        }
    }

    fn store_now(&mut self, adapter: &mut dyn SupervisorAdapter) {
        self.record.clock_ms = self.clock_ms;
        self.stored_ms = self.clock_ms;
        adapter.store(&self.record.encode());
    }
}

/// Counted restarts within the last hour and the last day at `clock`.
fn counts_at(restarts: &VecDeque<u64>, clock: u64) -> (usize, usize) {
    let within = |window: u64| {
        restarts
            .iter()
            .filter(|t| clock.saturating_sub(**t) < window)
            .count()
    };
    (within(HOUR_MS), within(DAY_MS))
}

/// How long the safe mode lasts before its retry, at its `entries`-th entry since the power-on.
fn retry_after(entries: u32) -> u64 {
    let index = usize::try_from(entries.max(1) - 1)
        .unwrap_or(usize::MAX)
        .min(SAFE_RETRIES_MS.len() - 1);
    SAFE_RETRIES_MS[index]
}

/// The last rung of the engine's restart ladder (spec 036 R2). When a stop was asked meanwhile
/// (`stop`) it does nothing and returns `false`: the asked restart wins. Otherwise `save` runs
/// once (an error goes to `sink` and does not stop the restart), then the reason is written, the
/// line goes to `sink` and the adapter restarts the box; returns `true`.
pub fn last_rung(
    supervisor: &Mutex<Supervisor>,
    adapter: &mut dyn SupervisorAdapter,
    reason: Reason,
    detail: &str,
    stop: &AtomicBool,
    save: &mut dyn FnMut() -> Result<(), String>,
    sink: &mut dyn FnMut(&str),
) -> bool {
    if stop.load(Ordering::SeqCst) {
        return false;
    }
    if let Err(error) = save() {
        sink(&error);
    }
    let mut guard = lock(supervisor);
    // A stop asked while the save ran wins too: its restart writes `asked`.
    if stop.load(Ordering::SeqCst) {
        return false;
    }
    guard.incident(adapter, reason, detail, sink);
    true
}
