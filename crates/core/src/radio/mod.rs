//! The radio of the Dongle Max (EFR32MG24, EmberZNet NCP): ASH frames, EZSP commands, and the
//! `Radio` that drives both over a transport. No clock and no I/O of its own: the caller gives
//! the transport and the time.

pub mod ash;
pub mod ezsp;
pub mod transport;

use std::fmt;

use ash::{AshError, AshFailure, Event, Output, Session};
use ezsp::{Ezsp, EzspError, NetworkState, Parsed};
pub use transport::{RadioTransport, TransportError};

/// How long one read waits, in milliseconds.
const READ_SLICE_MS: u64 = 50;
/// Milliseconds to wait for the answer to an EZSP command once it is acknowledged.
const RESPONSE_TIMEOUT_MS: u64 = 5000;

/// What the radio is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RadioIdentity {
    pub protocol: u8,
    pub stack_type: u8,
    pub stack_version: u16,
    /// The radio's 64-bit address, most significant byte first.
    pub eui64: [u8; 8],
    pub network_state: NetworkState,
}

impl RadioIdentity {
    /// The address as `0x` followed by 16 hexadecimal digits.
    pub fn eui64_string(&self) -> String {
        let mut text = String::from("0x");
        for byte in self.eui64 {
            text.push_str(&format!("{byte:02X}"));
        }
        text
    }
}

/// The lines the host prints once the radio is identified.
pub fn identity_lines(address: &str, identity: &RadioIdentity) -> [String; 4] {
    [
        format!("radio: {address}"),
        format!(
            "ezsp: protocol {}, stack type {}, EmberZNet {}",
            identity.protocol,
            identity.stack_type,
            ezsp::format_stack_version(identity.stack_version)
        ),
        format!("radio address: {}", identity.eui64_string()),
        format!("network: {}", identity.network_state),
    ]
}

/// Why talking to the radio failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RadioError {
    Transport(TransportError),
    Ash(AshFailure),
    Send(AshError),
    Ezsp(EzspError),
    /// The command was acknowledged but never answered.
    NoResponse,
}

impl fmt::Display for RadioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RadioError::Transport(e) => write!(f, "{e}"),
            RadioError::Ash(e) => write!(f, "{e}"),
            RadioError::Send(e) => write!(f, "{e}"),
            RadioError::Ezsp(e) => write!(f, "{e}"),
            RadioError::NoResponse => write!(f, "no answer to command"),
        }
    }
}

impl std::error::Error for RadioError {}

impl From<TransportError> for RadioError {
    fn from(e: TransportError) -> Self {
        RadioError::Transport(e)
    }
}

impl From<EzspError> for RadioError {
    fn from(e: EzspError) -> Self {
        RadioError::Ezsp(e)
    }
}

/// Drives ASH and EZSP over a transport. Calls block, in steps of `READ_SLICE_MS`.
pub struct Radio<T: RadioTransport, C: FnMut() -> u64> {
    transport: T,
    now: C,
    session: Session,
    ezsp: Ezsp,
}

impl<T: RadioTransport, C: FnMut() -> u64> Radio<T, C> {
    /// `now` returns the time in milliseconds, from any starting point, never going back.
    pub fn new(transport: T, now: C) -> Self {
        Radio {
            transport,
            now,
            session: Session::new(),
            ezsp: Ezsp::new(),
        }
    }

    fn write(&mut self, out: &Output) -> Result<(), RadioError> {
        if !out.write.is_empty() {
            self.transport.write(&out.write)?;
        }
        Ok(())
    }

    /// Reads for a moment, feeds the session, writes what it answers. `Ok(true)` when the
    /// session became connected during this step.
    fn step(&mut self) -> Result<bool, RadioError> {
        let mut buf = [0u8; 256];
        let n = self.transport.read(&mut buf, READ_SLICE_MS)?;
        let now = (self.now)();
        let mut out = if n > 0 {
            self.session.receive(&buf[..n], now)
        } else {
            Output::default()
        };
        out.merge(self.session.poll(now));
        self.write(&out)?;
        let mut connected = false;
        for event in out.events {
            match event {
                Event::Connected => connected = true,
                Event::Failed(failure) => return Err(RadioError::Ash(failure)),
                // A bad frame or a dropped payload does not stop the link: the session
                // recovers by itself (NAK, resend).
                Event::Error(_) => {}
            }
        }
        Ok(connected)
    }

    fn connect(&mut self) -> Result<(), RadioError> {
        let now = (self.now)();
        let rst = self.session.reset(now);
        self.transport.write(&rst)?;
        while !self.step()? {}
        Ok(())
    }

    /// Sends one EZSP command frame and returns the parameters of its response.
    fn exchange(&mut self, command: Vec<u8>) -> Result<Vec<u8>, RadioError> {
        let sent = (self.now)();
        let bytes = self.session.send(command, sent).map_err(RadioError::Send)?;
        // The answer is awaited from the moment the command is acknowledged: a command that
        // is never acknowledged ends as a lost link (the session's 6th timeout) instead.
        let mut acknowledged_at: Option<u64> = None;
        self.transport.write(&bytes)?;
        loop {
            while let Some(payload) = self.session.next_payload() {
                match self.ezsp.parse_response(&payload)? {
                    Parsed::Callback => {}
                    Parsed::Response(params) => return Ok(params),
                }
            }
            let now = (self.now)();
            if !self.session.has_unacknowledged() {
                let since = *acknowledged_at.get_or_insert(now);
                if now.saturating_sub(since) >= RESPONSE_TIMEOUT_MS {
                    return Err(RadioError::NoResponse);
                }
            }
            self.step()?;
        }
    }

    /// Resets the link, negotiates the EZSP version, and reads the radio's identity.
    pub fn identify(&mut self) -> Result<RadioIdentity, RadioError> {
        self.connect()?;
        let first = self.ezsp.version_command();
        let params = self.exchange(first)?;
        let second = self.ezsp.on_first_version(&params)?;
        let params = self.exchange(second)?;
        let version = self.ezsp.on_second_version(&params)?;

        let command = self.ezsp.command(ezsp::ID_NETWORK_STATE, &[]);
        let network_state = ezsp::parse_network_state(&self.exchange(command)?)?;
        let command = self.ezsp.command(ezsp::ID_GET_EUI64, &[]);
        let eui64 = ezsp::parse_eui64(&self.exchange(command)?)?;

        Ok(RadioIdentity {
            protocol: version.protocol,
            stack_type: version.stack_type,
            stack_version: version.stack_version,
            eui64,
            network_state,
        })
    }
}
