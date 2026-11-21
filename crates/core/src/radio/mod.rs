//! The radio of the Dongle Max (EFR32MG24, EmberZNet NCP): ASH frames, EZSP commands, and the
//! `Radio` that drives both over a transport. No clock and no I/O of its own: the caller gives
//! the transport and the time.

pub mod ash;
mod commands;
mod coordinator;
pub mod ezsp;
pub mod measures;
pub mod transport;
pub mod zcl;
pub mod zdo;

use std::collections::VecDeque;
use std::fmt;

use ash::{AshError, AshFailure, Event, Output, Session};
pub use commands::{
    CommandFailure, CommandHandle, CommandTarget, Commands, MAX_COMMANDS, RouteUpdate,
};
pub use coordinator::{
    Activity, Enrollment, EnrollmentOutcome, MeasureEvent, Network, NetworkKeys, NetworkSettings,
    PairingEvent, Sender, cut_short_line, network_line,
};
use ezsp::{Callback, Ezsp, EzspError, NetworkState, Parsed};
pub use transport::{RadioTransport, TransportError};

/// How long one read waits, in milliseconds.
const READ_SLICE_MS: u64 = 50;
/// Milliseconds to wait for the answer to an EZSP command once it is acknowledged.
const RESPONSE_TIMEOUT_MS: u64 = 5000;
/// Callbacks kept while a command waits for its response: the oldest is dropped past this.
const MAX_QUEUED_CALLBACKS: usize = 16;
/// Bytes given to the ASH session at a time: a burst of frames read at once is handed over in
/// small parts, so that the session's own bound on unread payloads is never reached.
const FEED_CHUNK: usize = 16;
/// Payloads taken from the session and not yet read; the oldest is dropped past this.
const MAX_INBOX: usize = 64;

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
    /// A command was refused by the radio: its name and the status it answered.
    Refused {
        command: &'static str,
        status: u32,
    },
    /// The network did not come up in time.
    NetworkDidNotStart,
    /// The radio holds a network it is only a member of (router or end device).
    NotCoordinator,
    /// The radio was not identified first.
    NotIdentified,
    /// The radio has no network and none is to be formed (listening only resumes one).
    NoNetwork,
    /// An argument out of range.
    InvalidArgument(&'static str),
    /// The caller's random source failed.
    Random(String),
    /// The caller's `on_formed` callback failed: its message, shown as it is.
    OnFormed(String),
}

impl fmt::Display for RadioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RadioError::Transport(e) => write!(f, "{e}"),
            RadioError::Ash(e) => write!(f, "{e}"),
            RadioError::Send(e) => write!(f, "{e}"),
            RadioError::Ezsp(e) => write!(f, "{e}"),
            RadioError::NoResponse => write!(f, "no answer to command"),
            RadioError::Refused { command, status } => {
                write!(f, "the radio refused {command} (status 0x{status:X})")
            }
            RadioError::NetworkDidNotStart => write!(f, "network did not start"),
            RadioError::NotCoordinator => write!(f, "the radio is not a coordinator"),
            RadioError::NotIdentified => write!(f, "the radio was not identified"),
            RadioError::NoNetwork => write!(f, "no network, pair a device first"),
            RadioError::InvalidArgument(what) => write!(f, "{what}"),
            RadioError::Random(reason) => write!(f, "no random source: {reason}"),
            RadioError::OnFormed(message) => write!(f, "{message}"),
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
    /// Callbacks received while a command waited for its response; at most
    /// `MAX_QUEUED_CALLBACKS`, the oldest dropped.
    callbacks: VecDeque<Callback>,
    /// EZSP frames received and not yet read; at most `MAX_INBOX`.
    inbox: VecDeque<Vec<u8>>,
    pairing: coordinator::Pairing,
    commands: commands::Commands,
}

impl<T: RadioTransport, C: FnMut() -> u64> Radio<T, C> {
    /// `now` returns the time in milliseconds, from any starting point, never going back.
    pub fn new(transport: T, now: C) -> Self {
        Radio {
            transport,
            now,
            session: Session::new(),
            ezsp: Ezsp::new(),
            callbacks: VecDeque::new(),
            inbox: VecDeque::new(),
            pairing: coordinator::Pairing::default(),
            commands: commands::Commands::default(),
        }
    }

    /// Keeps a callback for later if it is one of the four the coordinator uses; the others
    /// are ignored.
    fn queue_callback(&mut self, callback: Callback) {
        if !matches!(
            callback.id,
            ezsp::ID_STACK_STATUS_HANDLER
                | ezsp::ID_TRUST_CENTER_JOIN_HANDLER
                | ezsp::ID_INCOMING_MESSAGE_HANDLER
                | ezsp::ID_MESSAGE_SENT_HANDLER
        ) {
            return;
        }
        if self.callbacks.len() >= MAX_QUEUED_CALLBACKS {
            self.callbacks.pop_front();
            log::warn!("callback queue full: the oldest callback was dropped");
        }
        self.callbacks.push_back(callback);
    }

    /// Logs the overflow flag if a frame carried it: the radio is short of buffers.
    fn log_overflow(&self) {
        if self.ezsp.take_overflow() {
            log::warn!("the radio reports an overflow (short of buffers)");
        }
    }

    /// Reads the payloads the session delivered outside a command: callbacks are queued, any
    /// other frame is dropped.
    fn drain_payloads(&mut self) {
        while let Some(payload) = self.inbox.pop_front() {
            if let Ok(Parsed::Callback(callback)) = self.ezsp.parse_response(&payload) {
                self.queue_callback(callback);
            }
        }
        self.log_overflow();
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
        let mut out = Output::default();
        for chunk in buf[..n].chunks(FEED_CHUNK) {
            out.merge(self.session.receive(chunk, now));
            while let Some(payload) = self.session.next_payload() {
                if self.inbox.len() >= MAX_INBOX {
                    self.inbox.pop_front();
                    log::warn!("too many unread frames: the oldest was dropped");
                }
                self.inbox.push_back(payload);
            }
        }
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
            while let Some(payload) = self.inbox.pop_front() {
                let parsed = match self.ezsp.parse_response(&payload) {
                    Ok(parsed) => parsed,
                    // A response to an earlier command, arrived after its timeout: ignored.
                    Err(EzspError::WrongSequence { expected, got })
                        if (1..128).contains(&expected.wrapping_sub(got)) =>
                    {
                        log::warn!("a late response (sequence {got}) was ignored");
                        continue;
                    }
                    Err(e) => return Err(e.into()),
                };
                self.log_overflow();
                match parsed {
                    Parsed::Callback(callback) => self.queue_callback(callback),
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
        self.pairing.set_own_eui64(eui64);

        Ok(RadioIdentity {
            protocol: version.protocol,
            stack_type: version.stack_type,
            stack_version: version.stack_version,
            eui64,
            network_state,
        })
    }
}
