//! The supervisor (spec 036): the record, the boot-loop guard, the safe mode and its retry, the
//! last rung of the ladder, the figures of the watchdog. The adapter is a fake.

use std::sync::Mutex;
use std::sync::atomic::AtomicBool;

use solite_core::radio::READ_SLICE_MS;
use solite_core::radio::serial::ResetReason;
use solite_core::supervisor::{
    DAY_MS, HOUR_MS, LONGEST_STALL_MS, MAX_KEPT, Mode, RECORD_WORDS, Reason, Record,
    SAFE_RETRIES_MS, Status, Supervisor, SupervisorAdapter, TICK_STORE_MS, WATCHDOG_TIMEOUT_MS,
    last_rung,
};

const MINUTE: u64 = 60_000;

#[derive(Default)]
struct Fake {
    words: [u32; RECORD_WORDS],
    stores: usize,
    restarts: usize,
}

impl SupervisorAdapter for Fake {
    fn load(&mut self) -> [u32; RECORD_WORDS] {
        self.words
    }
    fn store(&mut self, words: &[u32; RECORD_WORDS]) {
        self.words = *words;
        self.stores += 1;
    }
    fn feed(&mut self) {}
    fn restart(&mut self) {
        self.restarts += 1;
    }
}

fn nothing(_: &str) {}

/// A first boot at a power-on, with whatever the RTC memory held.
fn power_on(fake: &mut Fake) -> Supervisor {
    Supervisor::boot(fake, ResetReason::PowerOn)
}

/// The box ran `ran_ms`, then the chip reset for `reset` with nothing pending (a panic, a
/// watchdog...), and booted again.
fn crash(fake: &mut Fake, sup: &mut Supervisor, ran_ms: u64, reset: ResetReason) -> Supervisor {
    sup.tick(fake, ran_ms, &mut nothing);
    Supervisor::boot(fake, reset)
}

/// The box ran `ran_ms`, then restarted on purpose for `reason`, and booted again.
fn restart_for(fake: &mut Fake, sup: &mut Supervisor, ran_ms: u64, reason: Reason) -> Supervisor {
    sup.tick(fake, ran_ms, &mut nothing);
    sup.incident(fake, reason, "test", &mut nothing);
    Supervisor::boot(fake, ResetReason::Software)
}

/// Three panics a minute apart from a fresh power-on: the safe mode.
fn into_safe_mode(fake: &mut Fake) -> Supervisor {
    let mut sup = power_on(fake);
    for _ in 0..3 {
        sup = crash(fake, &mut sup, MINUTE, ResetReason::Panic);
    }
    assert_eq!(sup.mode(), Mode::Safe);
    sup
}

fn record(fake: &Fake) -> Record {
    Record::decode(&fake.words).expect("a valid record")
}

// ---- The boot ----

#[test]
fn a_power_on_ignores_the_garbage_in_the_record_and_starts_fresh() {
    let mut fake = Fake {
        words: [0xDEAD_BEEF; RECORD_WORDS],
        ..Fake::default()
    };
    let sup = power_on(&mut fake);
    assert_eq!(sup.mode(), Mode::Normal);
    assert_eq!(sup.status(), Status::default());
    assert!(sup.lines()[0].starts_with("supervisor: boot after power-on, 0 restarts"));
    let stored = record(&fake);
    assert_eq!(stored.boots, 1);
    assert!(stored.restarts.is_empty() && stored.pending.is_none());
    assert_eq!(fake.restarts, 0);
}

#[test]
fn a_power_on_wipes_a_valid_record_too() {
    let mut fake = Fake::default();
    let mut sup = power_on(&mut fake);
    sup = restart_for(&mut fake, &mut sup, MINUTE, Reason::EngineError);
    assert_eq!(sup.status().last_incident, Some("engine-error"));
    let sup = power_on(&mut fake);
    assert_eq!(sup.status(), Status::default());
}

#[test]
fn an_incident_restarts_once_and_the_next_boot_names_it_and_counts_it() {
    let mut fake = Fake::default();
    let mut sup = power_on(&mut fake);
    let mut lines = Vec::new();
    sup.incident(&mut fake, Reason::RadioStopped, "link lost", &mut |l| {
        lines.push(l.to_string());
    });
    assert_eq!(fake.restarts, 1);
    let sup = Supervisor::boot(&mut fake, ResetReason::Software);
    let status = sup.status();
    assert_eq!(status.boot_cause, "radio-stopped");
    assert_eq!(status.last_incident, Some("radio-stopped"));
    assert_eq!(status.restarts_last_hour, 1);
    assert_eq!(status.restarts_last_day, 1);
    assert_eq!(
        sup.lines()[0],
        "supervisor: boot after radio-stopped, 1 restarts in the last hour, 1 in the last day"
    );
}

#[test]
fn each_incident_reason_writes_its_code_restarts_once_and_names_the_detail() {
    for (reason, code, text) in [
        (Reason::RadioStopped, 1, "radio-stopped"),
        (Reason::RadioSilent, 2, "radio-silent"),
        (Reason::RadioStart, 3, "radio-start"),
        (Reason::EngineError, 4, "engine-error"),
    ] {
        let mut fake = Fake::default();
        let mut sup = power_on(&mut fake);
        let mut lines = Vec::new();
        sup.incident(&mut fake, reason, "the detail", &mut |l| {
            lines.push(l.to_string());
        });
        assert_eq!(fake.words[2], code, "{text}");
        assert_eq!(fake.restarts, 1, "{text}");
        assert_eq!(
            lines,
            [format!(
                "supervisor: restarting the box after {text}: the detail"
            )]
        );
    }
}

#[test]
fn with_no_pending_reason_the_reset_reason_gives_the_cause_and_is_counted() {
    for (reset, text) in [
        (ResetReason::Panic, "panic"),
        (ResetReason::Watchdog, "watchdog"),
        (ResetReason::Software, "unexpected"),
        (ResetReason::Other, "unexpected"),
    ] {
        let mut fake = Fake::default();
        power_on(&mut fake);
        let sup = Supervisor::boot(&mut fake, reset);
        let status = sup.status();
        assert_eq!(status.boot_cause, text);
        assert_eq!(status.last_incident, Some(text));
        assert_eq!(status.restarts_last_hour, 1, "{text}");
    }
}

#[test]
fn a_brown_out_is_shown_as_the_last_incident_and_never_counted() {
    let mut fake = Fake::default();
    let mut sup = power_on(&mut fake);
    for _ in 0..3 {
        sup = crash(&mut fake, &mut sup, MINUTE, ResetReason::BrownOut);
    }
    let status = sup.status();
    assert_eq!(status.boot_cause, "brown-out");
    assert_eq!(status.last_incident, Some("brown-out"));
    assert_eq!(
        (status.restarts_last_hour, status.restarts_last_day),
        (0, 0)
    );
    assert_eq!(status.mode, Mode::Normal);
}

// ---- The boot-loop guard ----

#[test]
fn two_counted_restarts_in_an_hour_are_normal_and_the_third_enters_the_safe_mode() {
    let mut fake = Fake::default();
    let mut sup = power_on(&mut fake);
    for _ in 0..2 {
        sup = crash(&mut fake, &mut sup, MINUTE, ResetReason::Panic);
    }
    assert_eq!(sup.mode(), Mode::Normal);
    assert_eq!(sup.lines().len(), 1);
    sup = crash(&mut fake, &mut sup, MINUTE, ResetReason::Panic);
    assert_eq!(sup.mode(), Mode::Safe);
    assert_eq!(
        sup.lines(),
        [
            "supervisor: boot after panic, 3 restarts in the last hour, 3 in the last day, safe mode",
            "supervisor: safe mode: the engine stays stopped, the API and updates stay up; retried in 1 h"
        ]
    );
}

#[test]
fn three_restarts_spread_over_three_hours_do_not_enter_the_safe_mode() {
    let mut fake = Fake::default();
    let mut sup = power_on(&mut fake);
    for _ in 0..3 {
        sup = crash(&mut fake, &mut sup, 61 * MINUTE, ResetReason::Panic);
    }
    let status = sup.status();
    assert_eq!(status.mode, Mode::Normal);
    assert_eq!(status.restarts_last_hour, 1);
    assert_eq!(status.restarts_last_day, 3);
}

#[test]
fn six_counted_restarts_in_a_day_enter_the_safe_mode_and_five_do_not() {
    let mut fake = Fake::default();
    let mut sup = power_on(&mut fake);
    for n in 1..=6 {
        sup = crash(&mut fake, &mut sup, 2 * HOUR_MS, ResetReason::Watchdog);
        if n < 6 {
            assert_eq!(sup.mode(), Mode::Normal, "restart {n}");
        }
    }
    assert_eq!(sup.mode(), Mode::Safe);
    assert_eq!(sup.status().restarts_last_day, 6);
}

#[test]
fn restart_times_older_than_a_day_are_dropped_and_at_most_six_are_kept() {
    let mut fake = Fake::default();
    let mut sup = power_on(&mut fake);
    sup = crash(&mut fake, &mut sup, MINUTE, ResetReason::Panic);
    sup = crash(&mut fake, &mut sup, DAY_MS + MINUTE, ResetReason::Panic);
    // The first is a day and more old: only the second counts.
    assert_eq!(sup.status().restarts_last_day, 1);
    assert_eq!(record(&fake).restarts.len(), 1);
    // A crash loop: the record never holds more than its bound, the newest ones.
    for _ in 0..10 {
        sup = crash(&mut fake, &mut sup, 1_000, ResetReason::Panic);
    }
    let kept = record(&fake).restarts;
    assert_eq!(kept.len(), MAX_KEPT);
    assert!(kept.iter().zip(kept.iter().skip(1)).all(|(a, b)| a < b));
    assert_eq!(*kept.back().unwrap(), sup.clock_ms());
}

// ---- The safe mode ----

#[test]
fn a_crash_in_the_safe_mode_keeps_it_and_three_make_it_the_bare_safe_mode() {
    let mut fake = Fake::default();
    let mut sup = into_safe_mode(&mut fake);
    for crashes in 1..=2 {
        sup = crash(&mut fake, &mut sup, 1_000, ResetReason::Panic);
        assert_eq!(sup.mode(), Mode::Safe, "{crashes} crashes");
    }
    sup = crash(&mut fake, &mut sup, 1_000, ResetReason::Panic);
    assert_eq!(sup.mode(), Mode::SafeBare);
    assert!(sup.lines()[0].ends_with(", bare safe mode"));
    // It stays bare.
    sup = crash(&mut fake, &mut sup, 1_000, ResetReason::Watchdog);
    assert_eq!(sup.mode(), Mode::SafeBare);
}

#[test]
fn the_first_retry_comes_after_an_hour_with_one_restart_that_is_not_counted() {
    let mut fake = Fake::default();
    let mut sup = into_safe_mode(&mut fake);
    let restarts = fake.restarts;
    assert!(!sup.tick(&mut fake, HOUR_MS - 1, &mut nothing));
    assert_eq!(fake.restarts, restarts, "no action a millisecond early");
    let mut lines = Vec::new();
    assert!(sup.tick(&mut fake, HOUR_MS, &mut |l| lines.push(l.to_string())));
    assert_eq!(fake.restarts, restarts + 1);
    assert_eq!(fake.words[2], Reason::SafeModeRetry.code());
    assert_eq!(lines.len(), 1);
    let sup = Supervisor::boot(&mut fake, ResetReason::Software);
    let status = sup.status();
    assert_eq!(sup.mode(), Mode::Normal);
    assert_eq!(status.boot_cause, "safe-mode-retry");
    assert_eq!(
        (status.restarts_last_hour, status.restarts_last_day),
        (0, 0)
    );
    // The incident that led there is still the last one.
    assert_eq!(status.last_incident, Some("panic"));
}

#[test]
fn the_retries_come_after_one_hour_then_four_then_twenty_four_hours_for_every_later_one() {
    assert_eq!(SAFE_RETRIES_MS, [HOUR_MS, 4 * HOUR_MS, 24 * HOUR_MS]);
    let mut fake = Fake::default();
    let mut sup = into_safe_mode(&mut fake);
    for (entry, wait) in [1, 4, 24, 24, 24].into_iter().enumerate() {
        let wait_ms = wait * HOUR_MS;
        let before = fake.restarts;
        assert!(
            !sup.tick(&mut fake, wait_ms - 1, &mut nothing),
            "entry {entry}"
        );
        assert!(sup.tick(&mut fake, wait_ms, &mut nothing), "entry {entry}");
        assert_eq!(fake.restarts, before + 1);
        sup = Supervisor::boot(&mut fake, ResetReason::Software);
        assert_eq!(sup.mode(), Mode::Normal);
        // A fault that persists: three more restarts bring the safe mode back.
        for _ in 0..3 {
            sup = crash(&mut fake, &mut sup, MINUTE, ResetReason::Panic);
        }
        assert_eq!(sup.mode(), Mode::Safe, "entry {entry}");
        let wait_next = [4, 24, 24, 24, 24][entry];
        assert!(
            sup.lines()[1].ends_with(&format!("retried in {wait_next} h")),
            "{:?}",
            sup.lines()
        );
    }
}

#[test]
fn a_restart_asked_in_the_safe_mode_ends_it_and_clears_the_restart_times() {
    let mut fake = Fake::default();
    let mut sup = into_safe_mode(&mut fake);
    sup.asked(&mut fake);
    assert_eq!(fake.words[2], Reason::Asked.code());
    let sup = Supervisor::boot(&mut fake, ResetReason::Software);
    let status = sup.status();
    assert_eq!(sup.mode(), Mode::Normal);
    assert_eq!(status.boot_cause, "asked");
    assert_eq!(
        (status.restarts_last_hour, status.restarts_last_day),
        (0, 0)
    );
    // An asked restart is not an incident: the last one stays.
    assert_eq!(status.last_incident, Some("panic"));
}

#[test]
fn a_power_on_after_the_safe_mode_starts_normal_and_fresh() {
    let mut fake = Fake::default();
    into_safe_mode(&mut fake);
    let sup = power_on(&mut fake);
    assert_eq!(sup.mode(), Mode::Normal);
    assert_eq!(sup.status(), Status::default());
    assert_eq!(record(&fake).safe_entries, 0);
}

#[test]
fn the_tick_stores_the_clock_at_most_every_ten_seconds_and_the_next_boot_goes_on_from_it() {
    let mut fake = Fake::default();
    let mut sup = power_on(&mut fake);
    let boot_stores = fake.stores;
    for second in 1..=35 {
        sup.tick(&mut fake, second * 1_000, &mut nothing);
    }
    assert_eq!(fake.stores - boot_stores, 3, "at 10, 20 and 30 s");
    assert_eq!(record(&fake).clock_ms, 3 * TICK_STORE_MS);
    // A crash: the clock goes on from what was stored (it lost the last 5 s).
    let sup = Supervisor::boot(&mut fake, ResetReason::Panic);
    assert_eq!(sup.clock_ms(), 30_000);
    assert_eq!(record(&fake).restarts, [30_000]);
}

// ---- The record ----

#[test]
fn the_record_round_trips_with_every_field_set() {
    let record = Record {
        clock_ms: 0x0123_4567_89AB,
        safe_since: Some(0x0000_0001_0000_0002),
        pending: Some(Reason::SafeModeRetry),
        last: Some(Reason::Watchdog),
        restarts: (1..=6).map(|n| n * 0x1_0000_0001).collect(),
        boots: 4_000_000_000,
        safe_entries: 7,
    };
    assert_eq!(Record::decode(&record.encode()), Some(record));
    let fresh = Record::default();
    assert_eq!(Record::decode(&fresh.encode()), Some(fresh));
}

/// The checksum the record carries (FNV-1a over words 0 to 22, little-endian bytes).
fn seal(words: &mut [u32; RECORD_WORDS]) {
    let mut hash: u32 = 0x811c_9dc5;
    for word in &words[..23] {
        for byte in word.to_le_bytes() {
            hash ^= u32::from(byte);
            hash = hash.wrapping_mul(0x0100_0193);
        }
    }
    words[23] = hash;
}

#[test]
fn a_corrupt_record_is_ignored_and_the_boot_starts_a_fresh_one() {
    let good = Record {
        pending: Some(Reason::EngineError),
        ..Record::default()
    }
    .encode();
    assert!(Record::decode(&good).is_some());
    let mut cases: Vec<(&str, [u32; RECORD_WORDS])> = Vec::new();
    let mut words = good;
    words[0] ^= 1;
    cases.push(("magic", words));
    let mut words = good;
    words[1] = 2;
    seal(&mut words);
    cases.push(("version", words));
    let mut words = good;
    words[23] ^= 1;
    cases.push(("checksum", words));
    let mut words = good;
    words[21] ^= 1;
    cases.push(("a bit flipped under the checksum", words));
    let mut words = good;
    words[2] = 99;
    seal(&mut words);
    cases.push(("pending code", words));
    let mut words = good;
    words[3] = 11;
    seal(&mut words);
    cases.push(("last code", words));
    let mut words = good;
    words[8] = 7;
    seal(&mut words);
    cases.push(("count", words));
    for (name, words) in cases {
        assert_eq!(Record::decode(&words), None, "{name}");
        // At a boot that is no power-on: the cause comes from the reset reason alone.
        let mut fake = Fake {
            words,
            ..Fake::default()
        };
        let sup = Supervisor::boot(&mut fake, ResetReason::Panic);
        assert_eq!(sup.status().boot_cause, "panic", "{name}");
        assert_eq!(sup.status().restarts_last_hour, 1, "{name}");
        assert_eq!(record(&fake).boots, 1, "{name}");
    }
}

#[test]
fn the_status_serializes_in_camel_case_with_null_for_no_incident() {
    let json = serde_json::to_value(Status::default()).unwrap();
    assert_eq!(
        json,
        serde_json::json!({
            "mode": "normal",
            "bootCause": "power-on",
            "lastIncident": null,
            "restartsLastHour": 0,
            "restartsLastDay": 0
        })
    );
    let mut fake = Fake::default();
    let sup = into_safe_mode(&mut fake);
    let json = serde_json::to_value(sup.status()).unwrap();
    assert_eq!(
        json,
        serde_json::json!({
            "mode": "safe",
            "bootCause": "panic",
            "lastIncident": "panic",
            "restartsLastHour": 3,
            "restartsLastDay": 3
        })
    );
    for (mode, text) in [
        (Mode::Normal, "normal"),
        (Mode::Safe, "safe"),
        (Mode::SafeBare, "safe-bare"),
    ] {
        assert_eq!(serde_json::to_value(mode).unwrap(), text);
    }
}

#[test]
fn the_reasons_have_stable_codes_and_only_the_faults_count() {
    let all = [
        (Reason::RadioStopped, 1, "radio-stopped", true),
        (Reason::RadioSilent, 2, "radio-silent", true),
        (Reason::RadioStart, 3, "radio-start", true),
        (Reason::EngineError, 4, "engine-error", true),
        (Reason::Panic, 5, "panic", true),
        (Reason::Watchdog, 6, "watchdog", true),
        (Reason::BrownOut, 7, "brown-out", false),
        (Reason::Unexpected, 8, "unexpected", true),
        (Reason::Asked, 9, "asked", false),
        (Reason::SafeModeRetry, 10, "safe-mode-retry", false),
    ];
    for (reason, code, text, counts) in all {
        assert_eq!(reason.code(), code);
        assert_eq!(Reason::from_code(code), Some(reason));
        assert_eq!(reason.text(), text);
        assert_eq!(reason.counts(), counts, "{text}");
    }
    assert_eq!(Reason::from_code(0), None);
    assert_eq!(Reason::from_code(11), None);
}

// ---- The last rung ----

fn rung(
    fake: &mut Fake,
    stop: bool,
    save: &mut dyn FnMut() -> Result<(), String>,
) -> (bool, Vec<String>) {
    let supervisor = Mutex::new(power_on(fake));
    fake.stores = 0;
    let stop = AtomicBool::new(stop);
    let mut lines = Vec::new();
    let asked = last_rung(
        &supervisor,
        fake,
        Reason::EngineError,
        "storage: boom",
        &stop,
        save,
        &mut |l| lines.push(l.to_string()),
    );
    (asked, lines)
}

#[test]
fn the_last_rung_saves_once_writes_the_reason_and_restarts() {
    let mut fake = Fake::default();
    let mut saves = 0;
    let (asked, lines) = rung(&mut fake, false, &mut || {
        saves += 1;
        Ok(())
    });
    assert!(asked);
    assert_eq!(saves, 1);
    assert_eq!(fake.words[2], Reason::EngineError.code());
    assert_eq!(fake.restarts, 1);
    assert_eq!(
        lines,
        ["supervisor: restarting the box after engine-error: storage: boom"]
    );
}

#[test]
fn the_last_rung_still_restarts_when_the_save_fails() {
    let mut fake = Fake::default();
    let (asked, lines) = rung(&mut fake, false, &mut || Err("zones: disk full".into()));
    assert!(asked);
    assert_eq!(fake.restarts, 1);
    assert_eq!(fake.words[2], Reason::EngineError.code());
    assert_eq!(lines[0], "zones: disk full");
    assert!(lines[1].starts_with("supervisor: restarting the box after engine-error"));
}

#[test]
fn the_last_rung_does_nothing_when_a_stop_was_asked() {
    let mut fake = Fake::default();
    let mut saves = 0;
    let (asked, lines) = rung(&mut fake, true, &mut || {
        saves += 1;
        Ok(())
    });
    assert!(!asked);
    assert_eq!((saves, fake.restarts, fake.stores), (0, 0, 0));
    assert_eq!(fake.words[2], 0, "no reason written");
    assert!(lines.is_empty());
}

#[test]
fn the_last_rung_does_nothing_when_a_stop_is_asked_during_its_save() {
    let mut fake = Fake::default();
    let supervisor = Mutex::new(power_on(&mut fake));
    let stop = AtomicBool::new(false);
    let asked = last_rung(
        &supervisor,
        &mut fake,
        Reason::EngineError,
        "storage: boom",
        &stop,
        &mut || {
            stop.store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        },
        &mut nothing,
    );
    assert!(!asked);
    assert_eq!(fake.restarts, 0);
    assert_eq!(
        fake.words[2], 0,
        "no reason written: the asked restart writes its own"
    );
}

#[test]
fn a_refused_rollback_takes_back_its_asked_so_a_later_crash_counts() {
    let mut fake = Fake::default();
    let mut sup = power_on(&mut fake);
    sup.asked(&mut fake);
    sup.cancel_asked(&mut fake);
    assert_eq!(fake.words[2], 0);
    let next = crash(&mut fake, &mut sup, MINUTE, ResetReason::Panic);
    assert_eq!(next.status().boot_cause, "panic");
    assert_eq!(next.status().restarts_last_hour, 1);
}

// ---- The figures ----

#[test]
fn the_watchdog_timeout_is_over_five_times_the_longest_stall_and_over_a_read_slice() {
    const { assert!(WATCHDOG_TIMEOUT_MS >= 5 * LONGEST_STALL_MS) };
    const { assert!(WATCHDOG_TIMEOUT_MS > READ_SLICE_MS) };
    assert_eq!((WATCHDOG_TIMEOUT_MS, LONGEST_STALL_MS), (20_000, 3_800));
}
