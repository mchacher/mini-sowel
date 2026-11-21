//! The pairing window (spec 017): what the API asks (open for so many seconds, close), and what
//! the radio loop did about it, in one small value the home keeps behind its lock. Pure: it holds
//! no radio and no clock reading of its own, the engine and the API give the time. Times are
//! milliseconds on the monotonic clock of the orders.

use crate::orders::Clock;

/// Longest error text kept, in characters.
const MAX_ERROR_CHARS: usize = 128;
/// Addresses of devices still being paired that the window remembers: the coordinator follows at
/// most 32 devices at a time.
const MAX_BUSY: usize = 32;
/// After a window's time is over, identifications and reporting setups in progress may go on this
/// long, in milliseconds.
pub const CLOSE_GRACE_MS: u64 = 70_000;

/// Where the window is, as the engine sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// Nothing open, nothing being finished.
    Closed,
    /// Joining is open on the radio until `ends_at`.
    Open { ends_at: u64 },
    /// Joining is closed, the setups of the devices that joined are still running, until
    /// `until` at most.
    Finishing { until: u64 },
}

/// The pairing window of a home.
pub struct PairingWindow {
    /// What the API asked and the engine did not take yet: seconds, 0 closes. One at a time, the
    /// last request wins.
    request: Option<u8>,
    phase: Phase,
    /// The radio's last refusal, at most `MAX_ERROR_CHARS` characters.
    error: Option<Box<str>>,
    /// Addresses of the devices being identified or set up, at most `MAX_BUSY`.
    busy: Vec<[u8; 8]>,
    /// The monotonic clock the engine runs on.
    clock: Option<Clock>,
    grace_ms: u64,
}

impl std::fmt::Debug for PairingWindow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PairingWindow")
            .field("request", &self.request)
            .field("phase", &self.phase)
            .field("error", &self.error)
            .field("busy", &self.busy.len())
            .finish_non_exhaustive()
    }
}

impl Default for PairingWindow {
    fn default() -> Self {
        PairingWindow::new()
    }
}

impl PairingWindow {
    pub fn new() -> Self {
        PairingWindow {
            request: None,
            phase: Phase::Closed,
            error: None,
            busy: Vec::new(),
            clock: None,
            grace_ms: CLOSE_GRACE_MS,
        }
    }

    /// The clock the remaining time is read on (`engine::run` sets the orders' one).
    pub fn set_clock(&mut self, clock: Clock) {
        self.clock = Some(clock);
    }

    /// The monotonic time now; 0 before the engine gave its clock.
    pub fn now(&self) -> u64 {
        self.clock.as_ref().map_or(0, |clock| clock())
    }

    /// How long setups in progress may go on after the time is over.
    pub fn grace_ms(&self) -> u64 {
        self.grace_ms
    }

    pub fn set_grace_ms(&mut self, grace_ms: u64) {
        self.grace_ms = grace_ms;
    }

    /// The API's request: open for `time` seconds (1 to 254), or close (0). Replaces a request
    /// not taken yet; clears the last error.
    pub fn request(&mut self, time: u8) {
        self.request = Some(time);
        self.error = None;
    }

    /// The engine takes the request, if any.
    pub fn take_request(&mut self) -> Option<u8> {
        self.request.take()
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    /// The radio accepted the opening.
    pub fn opened(&mut self, ends_at: u64) {
        self.phase = Phase::Open { ends_at };
    }

    /// The radio refused the opening: closed, and why (cut to `MAX_ERROR_CHARS` characters).
    pub fn refused(&mut self, reason: &str) {
        self.phase = Phase::Closed;
        self.error = Some(reason.chars().take(MAX_ERROR_CHARS).collect());
    }

    /// The time is over (or a close was asked): joining is closed, the setups may finish until
    /// `until`.
    pub fn ended(&mut self, until: u64) {
        self.phase = Phase::Finishing { until };
    }

    /// Nothing more to follow.
    pub fn finished(&mut self) {
        self.phase = Phase::Closed;
    }

    /// What the API reports at `now`: whether joining is open, the whole seconds left (rounded
    /// up), and the last refusal. A request the engine did not take yet is reported as asked.
    pub fn state(&self, now: u64) -> (bool, u32, Option<&str>) {
        let error = self.error.as_deref();
        if let Some(time) = self.request {
            return (time > 0, u32::from(time), error);
        }
        match self.phase {
            Phase::Open { ends_at } if ends_at > now => {
                let remaining = (ends_at - now).div_ceil(1000);
                (true, u32::try_from(remaining).unwrap_or(u32::MAX), error)
            }
            _ => (false, 0, error),
        }
    }

    /// The engine copies the addresses of the devices still being identified or set up (the
    /// first `MAX_BUSY`).
    pub fn set_busy(&mut self, addresses: &[[u8; 8]]) {
        self.busy.clear();
        self.busy.extend(addresses.iter().take(MAX_BUSY).copied());
    }

    /// True when the device with this address is being identified or set up.
    pub fn is_busy(&self, eui64: &[u8; 8]) -> bool {
        self.busy.contains(eui64)
    }

    /// True when the device whose `sourceDeviceId` is this text (`0x` and 16 hexadecimal digits)
    /// is being identified or set up.
    pub fn is_busy_source(&self, source: &str) -> bool {
        let Some(digits) = source.strip_prefix("0x") else {
            return false;
        };
        self.busy.iter().any(|address| {
            digits.len() == 16
                && address
                    .iter()
                    .zip(digits.as_bytes().chunks(2))
                    .all(|(byte, pair)| {
                        std::str::from_utf8(pair)
                            .ok()
                            .and_then(|text| u8::from_str_radix(text, 16).ok())
                            == Some(*byte)
                    })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_window_is_closed() {
        let window = PairingWindow::new();
        assert_eq!(window.state(0), (false, 0, None));
        assert_eq!(window.phase(), Phase::Closed);
    }

    #[test]
    fn a_request_not_taken_yet_is_reported_as_asked() {
        let mut window = PairingWindow::new();
        window.request(60);
        assert_eq!(window.state(5_000), (true, 60, None));
        window.request(0);
        assert_eq!(window.state(0), (false, 0, None));
    }

    #[test]
    fn the_last_request_wins() {
        let mut window = PairingWindow::new();
        window.request(10);
        window.request(20);
        assert_eq!(window.take_request(), Some(20));
        assert_eq!(window.take_request(), None);
    }

    #[test]
    fn remaining_is_rounded_up_and_ends_at_zero() {
        let mut window = PairingWindow::new();
        window.opened(60_000);
        assert_eq!(window.state(10_200), (true, 50, None));
        assert_eq!(window.state(59_999), (true, 1, None));
        assert_eq!(window.state(60_000), (false, 0, None));
        assert_eq!(window.state(90_000), (false, 0, None));
    }

    #[test]
    fn finishing_reports_closed() {
        let mut window = PairingWindow::new();
        window.opened(1_000);
        window.ended(71_000);
        assert_eq!(window.state(500), (false, 0, None));
        assert_eq!(window.phase(), Phase::Finishing { until: 71_000 });
    }

    #[test]
    fn a_refusal_closes_and_the_next_request_clears_it() {
        let mut window = PairingWindow::new();
        window.opened(60_000);
        window.refused("the radio refused permitJoining (status 0x70)");
        assert_eq!(
            window.state(0),
            (
                false,
                0,
                Some("the radio refused permitJoining (status 0x70)")
            )
        );
        window.request(30);
        assert_eq!(window.state(0), (true, 30, None));
    }

    #[test]
    fn a_long_multibyte_error_is_cut_on_a_character() {
        let mut window = PairingWindow::new();
        window.refused(&"é".repeat(300));
        let (_, _, error) = window.state(0);
        assert_eq!(error.map(|e| e.chars().count()), Some(MAX_ERROR_CHARS));
    }

    #[test]
    fn busy_addresses_are_bounded_and_matched_by_text() {
        let mut window = PairingWindow::new();
        let addresses: Vec<[u8; 8]> = (0..40u8).map(|i| [i, 0, 0, 0, 0, 0, 0, 0xAB]).collect();
        window.set_busy(&addresses);
        assert!(window.is_busy(&addresses[31]));
        assert!(!window.is_busy(&addresses[32]));
        assert!(window.is_busy_source("0x05000000000000ab"));
        assert!(!window.is_busy_source("0x05000000000000ac"));
        assert!(!window.is_busy_source("05000000000000ab"));
        assert!(!window.is_busy_source("0x05"));
        window.set_busy(&[]);
        assert!(!window.is_busy_source("0x05000000000000ab"));
    }
}
