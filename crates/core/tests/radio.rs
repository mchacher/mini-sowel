//! `Radio::identify` against a scripted radio, with fake time.

use std::cell::Cell;
use std::rc::Rc;

use solite_core::radio::ash::AshFailure;
use solite_core::radio::ezsp::NetworkState;
use solite_core::radio::{Radio, RadioError, RadioTransport, TransportError};
use solite_sim::{Mode, Ncp};

/// A transport wired to the fake radio. Reading advances the fake clock by the timeout when
/// nothing is waiting, so no test really waits.
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

fn radio(ncp: Ncp) -> (Radio<FakeTransport, impl FnMut() -> u64>, Rc<Cell<u64>>) {
    let clock = Rc::new(Cell::new(0u64));
    let transport = FakeTransport {
        ncp,
        to_read: Vec::new(),
        clock: Rc::clone(&clock),
    };
    let reader = Rc::clone(&clock);
    (Radio::new(transport, move || reader.get()), clock)
}

#[test]
fn identify_returns_the_identity_of_the_fake_radio() {
    let mut ncp = Ncp::recording();
    ncp.callback_first = true; // callbacks between command and response are skipped
    ncp.network_state = 2;
    let (mut radio, _) = radio(ncp);
    let identity = radio.identify().unwrap();
    assert_eq!(identity.protocol, 13);
    assert_eq!(identity.stack_type, 2);
    assert_eq!(identity.stack_version, 0x7440);
    assert_eq!(identity.eui64_string(), "0x8C6FB9FFFE123456");
    assert_eq!(identity.network_state, NetworkState::Joined);
}

#[test]
fn identify_works_with_a_radio_at_protocol_19() {
    let mut ncp = Ncp::recording();
    ncp.protocol = 19;
    let (mut radio, _) = radio(ncp);
    assert_eq!(radio.identify().unwrap().protocol, 19);
}

#[test]
fn identify_refuses_a_radio_with_an_unsupported_version() {
    let mut ncp = Ncp::recording();
    ncp.protocol = 12;
    let (mut radio, _) = radio(ncp);
    let error = radio.identify().unwrap_err().to_string();
    assert!(
        error.contains("12") && error.contains("13 to 19"),
        "{error}"
    );
}

#[test]
fn a_radio_that_never_sends_rstack_gives_no_answer_to_reset_within_5_seconds_of_fake_time() {
    let mut ncp = Ncp::recording();
    ncp.mode = Mode::Silent;
    let (mut radio, clock) = radio(ncp);
    let error = radio.identify().unwrap_err();
    assert_eq!(error, RadioError::Ash(AshFailure::NoResetAnswer));
    assert_eq!(error.to_string(), "no answer to reset");
    let elapsed = clock.get();
    assert!((5000..5100).contains(&elapsed), "elapsed {elapsed} ms");
}

#[test]
fn a_command_acknowledged_but_never_answered_gives_no_answer_5_seconds_after_the_ack() {
    let mut ncp = Ncp::recording();
    ncp.mode = Mode::MuteOnCommands;
    let (mut radio, clock) = radio(ncp);
    let error = radio.identify().unwrap_err();
    assert_eq!(error, RadioError::NoResponse);
    assert_eq!(error.to_string(), "no answer to command");
    let elapsed = clock.get();
    assert!((5000..5100).contains(&elapsed), "elapsed {elapsed} ms");
}

#[test]
fn a_command_never_acknowledged_ends_as_a_lost_link() {
    let mut ncp = Ncp::recording();
    ncp.mode = Mode::DeafToCommands;
    let (mut radio, clock) = radio(ncp);
    let error = radio.identify().unwrap_err();
    assert_eq!(error, RadioError::Ash(AshFailure::LinkLost));
    // 6 timeouts of 1.6 s.
    let elapsed = clock.get();
    assert!((9600..9700).contains(&elapsed), "elapsed {elapsed} ms");
}
