//! The serial link's logic (spec 021): the speed probe, the reset sequence, pairing at boot, the
//! counters of the receive ring, the restart of a wedged radio, and one transport serving several
//! radios in turn.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use solite_core::radio::ash::AshFailure;
use solite_core::radio::serial::{
    BAUD_RATES, BOOT_WAIT_MS, BootPairing, MAX_RADIO_RESTARTS, PAIR_AT_BOOT_SECONDS, PinStep,
    RADIO_BAUD, REPORT_EVERY_MS, RESET_PULSE_MS, RESTART_WINDOW_MS, RadioPin, ResetReason,
    RestartSteps, Restarted, Restarts, STABLE_MS, SerialStats, marker_due, marker_wait_ms,
    pairing_at_boot, probe, probe_order, reset_sequence, restart,
};
use solite_core::radio::{Network, Radio, RadioError, RadioTransport, TransportError};
use solite_sim::Ncp;

fn no_answer() -> RadioError {
    RadioError::Ash(AshFailure::NoResetAnswer)
}

type Probed = (
    Result<Option<(u32, u32)>, RadioError>,
    Vec<String>,
    Vec<u32>,
);

/// Runs the probe with a radio that answers only at `answers`, and returns what it logged.
fn probe_with(order: &[u32], answers: &[u32], failure: Option<(u32, RadioError)>) -> Probed {
    let mut tried = Vec::new();
    let mut lines = Vec::new();
    let result = probe(
        order,
        |baud| {
            tried.push(baud);
            if let Some((at, error)) = &failure
                && *at == baud
            {
                return Err(error.clone());
            }
            if answers.contains(&baud) {
                Ok(baud * 2)
            } else {
                Err(no_answer())
            }
        },
        &mut |line| lines.push(line.to_string()),
    );
    (result, lines, tried)
}

#[test]
fn probe_order_puts_the_fixed_speed_first_then_the_others_in_order() {
    assert_eq!(probe_order(115_200), [115_200, 460_800, 921_600]);
    assert_eq!(probe_order(921_600), [921_600, 115_200, 460_800]);
}

#[test]
fn probe_order_of_a_speed_not_in_the_list_is_the_list_without_duplicate() {
    assert_eq!(probe_order(9_600), [115_200, 460_800, 921_600]);
}

#[test]
fn the_probe_goes_on_to_the_next_speed_and_stops_at_the_one_that_answers() {
    let (result, lines, tried) = probe_with(&probe_order(RADIO_BAUD), &[460_800], None);
    assert_eq!(result.unwrap(), Some((460_800, 921_600)));
    assert_eq!(tried, [115_200, 460_800], "the third is never tried");
    assert_eq!(lines[0], "radio: no answer at 115200 baud");
    assert_eq!(lines[1], "radio: uart 460800 baud");
}

#[test]
fn the_probe_with_no_answer_at_any_speed_says_so_and_returns_none() {
    let (result, lines, _) = probe_with(&probe_order(RADIO_BAUD), &[], None);
    assert_eq!(result.unwrap(), None);
    assert_eq!(
        lines,
        [
            "radio: no answer at 115200 baud",
            "radio: no answer at 460800 baud",
            "radio: no answer at 921600 baud",
            "radio: no answer at any baud rate (115200, 460800, 921600)",
        ]
    );
}

#[test]
fn a_failure_other_than_no_reset_answer_stops_the_probe_at_that_speed() {
    let (result, lines, tried) = probe_with(
        &probe_order(RADIO_BAUD),
        &[921_600],
        Some((
            115_200,
            RadioError::Transport(TransportError::Io("ESP_FAIL".into())),
        )),
    );
    assert_eq!(
        result.unwrap_err(),
        RadioError::Transport(TransportError::Io("ESP_FAIL".into()))
    );
    assert_eq!(tried, [115_200]);
    assert!(lines.is_empty());
}

#[test]
fn a_speed_other_than_the_fixed_one_gets_a_warning_naming_both() {
    let (result, lines, _) = probe_with(&probe_order(RADIO_BAUD), &[921_600], None);
    assert_eq!(result.unwrap().map(|(baud, _)| baud), Some(921_600));
    assert!(lines.contains(&"radio: uart 921600 baud".to_string()));
    assert_eq!(
        lines.last().unwrap(),
        "radio: answered at 921600 baud, RADIO_BAUD is 115200"
    );
    // At the fixed speed: no warning.
    let (_, lines, _) = probe_with(&probe_order(RADIO_BAUD), &[115_200], None);
    assert_eq!(lines, ["radio: uart 115200 baud"]);
}

#[test]
fn the_reset_sequence_releases_the_bootloader_before_the_pulse_and_the_reset_last() {
    let steps = reset_sequence();
    let position = |wanted: PinStep| steps.iter().position(|s| *s == wanted).unwrap();
    assert!(
        position(PinStep::Set(RadioPin::Bootloader, false))
            < position(PinStep::Set(RadioPin::Reset, true))
    );
    assert!(
        position(PinStep::Set(RadioPin::Reset, true))
            < position(PinStep::Set(RadioPin::Reset, false))
    );
    let last_set = steps
        .iter()
        .rposition(|s| matches!(s, PinStep::Set(..)))
        .unwrap();
    assert_eq!(steps[last_set], PinStep::Set(RadioPin::Reset, false));
    assert!(matches!(steps[last_set + 1], PinStep::Wait(_)));
}

#[test]
fn the_reset_sequence_never_sets_the_bootloader_active() {
    assert!(!reset_sequence().contains(&PinStep::Set(RadioPin::Bootloader, true)));
}

#[test]
fn the_reset_sequence_holds_the_pulse_and_waits_for_the_application() {
    let steps = reset_sequence();
    let pulse = steps
        .iter()
        .position(|s| *s == PinStep::Set(RadioPin::Reset, true));
    assert_eq!(steps[pulse.unwrap() + 1], PinStep::Wait(RESET_PULSE_MS));
    assert_eq!((RESET_PULSE_MS, BOOT_WAIT_MS), (100, 1000));
    assert_eq!(*steps.last().unwrap(), PinStep::Wait(BOOT_WAIT_MS));
}

#[test]
fn the_reset_line_is_active_low_and_the_bootloader_line_active_high() {
    assert!(!RadioPin::Reset.level(true));
    assert!(RadioPin::Reset.level(false));
    assert!(RadioPin::Bootloader.level(true));
    assert!(!RadioPin::Bootloader.level(false));
}

#[test]
fn the_serial_stats_keep_the_largest_sample() {
    let mut stats = SerialStats::new(4096, 0);
    for sample in [10, 300, 20] {
        stats.observe(sample);
    }
    let line = stats.report(REPORT_EVERY_MS, 115_200).unwrap();
    assert_eq!(
        line,
        "serial: 115200 baud, rx buffer 4096 B, highest 300 B, full 0 times"
    );
}

#[test]
fn the_serial_stats_count_full_once_per_reach_and_again_after_a_drop() {
    let mut stats = SerialStats::new(100, 0);
    for sample in [50, 100, 100, 100, 99, 100] {
        stats.observe(sample);
    }
    let line = stats.report(REPORT_EVERY_MS, 460_800).unwrap();
    assert!(line.ends_with("highest 100 B, full 2 times"), "{line}");
}

#[test]
fn the_serial_report_is_due_every_240_seconds() {
    let mut stats = SerialStats::new(4096, 0);
    assert!(stats.report(REPORT_EVERY_MS - 1, 115_200).is_none());
    assert!(stats.report(REPORT_EVERY_MS, 115_200).is_some());
    assert!(stats.report(REPORT_EVERY_MS + 1, 115_200).is_none());
    assert!(stats.report(2 * REPORT_EVERY_MS - 1, 115_200).is_none());
    assert!(stats.report(2 * REPORT_EVERY_MS, 115_200).is_some());
    // A late call gives one report, not several.
    assert!(stats.report(10 * REPORT_EVERY_MS, 115_200).is_some());
    assert!(stats.report(10 * REPORT_EVERY_MS + 1, 115_200).is_none());
}

#[test]
fn three_restarts_are_allowed_in_ten_minutes_and_the_fourth_is_refused_with_its_line() {
    let mut restarts = Restarts::new();
    for at in [0, 1000, 2000] {
        restarts.allow(at, "link lost").unwrap();
    }
    assert_eq!(MAX_RADIO_RESTARTS, 3);
    let line = restarts.allow(3000, "link lost").unwrap_err();
    assert_eq!(line, "radio: stopped after 3 restarts in 10 min: link lost");
    // Allowed again once the first is 10 minutes old.
    restarts.allow(RESTART_WINDOW_MS, "link lost").unwrap();
}

#[test]
fn pairing_opens_at_a_first_boot_and_at_a_normal_reboot() {
    // First boot and a reboot after the marker was removed look the same: no marker, power-on.
    assert_eq!(
        pairing_at_boot(false, ResetReason::PowerOn),
        BootPairing::Open {
            seconds: PAIR_AT_BOOT_SECONDS
        }
    );
    assert_eq!(PAIR_AT_BOOT_SECONDS, 180);
}

#[test]
fn pairing_does_not_open_when_a_marker_is_there() {
    let decision = pairing_at_boot(true, ResetReason::PowerOn);
    let line = decision.line().unwrap();
    assert!(line.starts_with("pairing: not opened at boot ("), "{line}");
    assert!(line.contains("10 min"), "{line}");
}

#[test]
fn pairing_does_not_open_after_a_crash_a_watchdog_or_a_brown_out() {
    for (reason, word) in [
        (ResetReason::Panic, "crash"),
        (ResetReason::Watchdog, "watchdog"),
        (ResetReason::BrownOut, "brown-out"),
        (ResetReason::Software, "software"),
    ] {
        let decision = pairing_at_boot(false, reason);
        let line = decision.line().unwrap();
        assert!(line.contains(word), "{line}");
    }
    assert!(
        pairing_at_boot(false, ResetReason::PowerOn)
            .line()
            .is_none()
    );
}

#[test]
fn the_marker_is_due_after_ten_minutes_of_running_and_not_before() {
    assert!(!marker_due(STABLE_MS - 1));
    assert!(marker_due(STABLE_MS));
}

#[test]
fn the_marker_timer_waits_what_is_left_of_ten_minutes_and_never_underflows() {
    assert_eq!(marker_wait_ms(0), STABLE_MS);
    assert_eq!(marker_wait_ms(STABLE_MS - 1), 1);
    assert_eq!(marker_wait_ms(STABLE_MS), 0);
    assert_eq!(marker_wait_ms(STABLE_MS + 5), 0);
    assert!(marker_due(STABLE_MS - marker_wait_ms(STABLE_MS - 1) + 1));
}

/// The closures of a restart, recording the order they run in.
struct Recorded {
    calls: Rc<RefCell<Vec<&'static str>>>,
    listened: Rc<Cell<usize>>,
}

fn network() -> Network {
    Network {
        pan_id: 0x1A2B,
        extended_pan_id: [1; 8],
        channel: 15,
        formed: false,
        from_backup: false,
        keys: None,
    }
}

fn run_restart(
    restarts: &mut Restarts,
    now: u64,
    probe_answer: Result<Option<u32>, RadioError>,
    listen_answer: Result<Network, RadioError>,
) -> (Result<Restarted, RadioError>, Recorded, Vec<String>) {
    let recorded = Recorded {
        calls: Rc::new(RefCell::new(Vec::new())),
        listened: Rc::new(Cell::new(0)),
    };
    let (c1, c2, c3, c4, c5) = (
        Rc::clone(&recorded.calls),
        Rc::clone(&recorded.calls),
        Rc::clone(&recorded.calls),
        Rc::clone(&recorded.calls),
        Rc::clone(&recorded.calls),
    );
    let listened = Rc::clone(&recorded.listened);
    let mut save = move || {
        c1.borrow_mut().push("save");
        Ok(())
    };
    let mut finish = move || c2.borrow_mut().push("finished");
    let mut reset = move || c3.borrow_mut().push("reset");
    let mut probe_step = move || {
        c4.borrow_mut().push("probe");
        probe_answer.clone()
    };
    let mut listen = move || {
        c5.borrow_mut().push("listen");
        listened.set(listened.get() + 1);
        listen_answer.clone()
    };
    let mut steps = RestartSteps {
        save: &mut save,
        finish_pairing: &mut finish,
        reset: &mut reset,
        probe: &mut probe_step,
        listen: &mut listen,
    };
    let mut lines = Vec::new();
    let result = restart(restarts, now, "link lost", &mut steps, &mut |l| {
        lines.push(l.to_string());
    });
    (result, recorded, lines)
}

#[test]
fn a_restart_saves_ends_the_pairing_resets_probes_then_listens_in_that_order() {
    let mut restarts = Restarts::new();
    let (result, recorded, lines) = run_restart(&mut restarts, 0, Ok(Some(115_200)), Ok(network()));
    assert!(matches!(result.unwrap(), Restarted::Resumed(_)));
    assert_eq!(
        *recorded.calls.borrow(),
        ["save", "finished", "reset", "probe", "listen"]
    );
    assert_eq!(lines[0], "radio: restarting after: link lost");
}

#[test]
fn a_restart_only_resumes_and_a_radio_with_no_network_ends_with_its_error() {
    let mut restarts = Restarts::new();
    let (result, recorded, _) = run_restart(
        &mut restarts,
        0,
        Ok(Some(115_200)),
        Err(RadioError::NoNetwork),
    );
    // The only network step is `listen`, called once; a radio with no network is an error, not a
    // forming.
    assert_eq!(recorded.listened.get(), 1);
    assert_eq!(result.unwrap_err(), RadioError::NoNetwork);
}

#[test]
fn a_restart_asks_for_no_pairing_window() {
    // The steps hold no way to open one: the pairing phase is only ended.
    let mut restarts = Restarts::new();
    let (_, recorded, _) = run_restart(&mut restarts, 0, Ok(Some(115_200)), Ok(network()));
    assert_eq!(
        recorded
            .calls
            .borrow()
            .iter()
            .filter(|c| **c == "finished")
            .count(),
        1
    );
    assert!(!recorded.calls.borrow().contains(&"open"));
}

#[test]
fn a_restart_where_the_probe_finds_nothing_stops_without_listening() {
    let mut restarts = Restarts::new();
    let (result, recorded, _) = run_restart(&mut restarts, 0, Ok(None), Ok(network()));
    assert!(matches!(result.unwrap(), Restarted::Stopped));
    assert_eq!(recorded.listened.get(), 0);
}

#[test]
fn a_fourth_restart_in_ten_minutes_is_refused_and_touches_nothing() {
    let mut restarts = Restarts::new();
    for at in 0..3 {
        run_restart(&mut restarts, at, Ok(Some(115_200)), Ok(network()))
            .0
            .unwrap();
    }
    let (result, recorded, lines) =
        run_restart(&mut restarts, 10, Ok(Some(115_200)), Ok(network()));
    assert!(matches!(result.unwrap(), Restarted::Stopped));
    assert!(recorded.calls.borrow().is_empty());
    assert_eq!(
        lines,
        ["radio: stopped after 3 restarts in 10 min: link lost"]
    );
}

/// A transport wired to the fake radio, as in `radio.rs`.
struct FakeTransport {
    ncp: Ncp,
    to_read: Vec<u8>,
    clock: Rc<Cell<u64>>,
}

impl RadioTransport for FakeTransport {
    fn read(&mut self, buf: &mut [u8], timeout_ms: u64) -> Result<usize, TransportError> {
        if self.to_read.is_empty() {
            self.clock.set(self.clock.get() + timeout_ms);
            return Ok(0);
        }
        let n = buf.len().min(self.to_read.len());
        buf[..n].copy_from_slice(&self.to_read[..n]);
        self.to_read.drain(..n);
        Ok(n)
    }

    fn write(&mut self, bytes: &[u8]) -> Result<(), TransportError> {
        let answer = self.ncp.on_bytes(bytes);
        self.to_read.extend(answer);
        Ok(())
    }
}

#[test]
fn one_transport_serves_two_radios_in_turn() {
    let clock = Rc::new(Cell::new(0u64));
    let mut transport = FakeTransport {
        ncp: Ncp::recording(),
        to_read: Vec::new(),
        clock: Rc::clone(&clock),
    };
    for _ in 0..2 {
        let reader = Rc::clone(&clock);
        let mut radio = Radio::new(&mut transport, move || reader.get());
        let identity = radio.identify().unwrap();
        assert_eq!(identity.eui64_string(), "0x8C6FB9FFFE123456");
    }
    assert_eq!(BAUD_RATES.len(), 3);
}
