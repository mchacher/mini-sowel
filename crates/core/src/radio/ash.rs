//! ASH (UG101), the frame layer between the host and the radio: framing, CRC, byte stuffing,
//! data randomisation, acknowledgements and retransmission. No I/O, no clock: the caller passes
//! the bytes it received and the time in milliseconds.

use std::collections::VecDeque;
use std::fmt;

const FLAG: u8 = 0x7E;
const ESCAPE: u8 = 0x7D;
const XON: u8 = 0x11;
const XOFF: u8 = 0x13;
const SUBSTITUTE: u8 = 0x18;
const CANCEL: u8 = 0x1A;

const CTRL_ACK: u8 = 0x80;
const CTRL_NAK: u8 = 0xA0;
const CTRL_RST: u8 = 0xC0;
const CTRL_RSTACK: u8 = 0xC1;
const CTRL_ERROR: u8 = 0xC2;

/// ASH version, and the reset code of a software reset (the answer to our RST).
const ASH_VERSION: u8 = 2;
const RESET_SOFTWARE: u8 = 0x0B;

const LFSR_SEED: u8 = 0x42;
const LFSR_POLY: u8 = 0xB8;

/// Longest EZSP payload of a DATA frame, in bytes.
pub const MAX_PAYLOAD: usize = 220;
/// Longest frame before stuffing: control byte, payload, 2 bytes of CRC.
const MAX_FRAME_BYTES: usize = 1 + MAX_PAYLOAD + 2;

/// Milliseconds to wait for an ACK before resending (fixed, not adapted).
pub const ACK_TIMEOUT_MS: u64 = 1600;
/// Timeouts in a row after which the link is declared lost (5 resends, failure on the 6th).
pub const MAX_TIMEOUTS: u32 = 6;
/// Milliseconds to wait for RSTACK after RST.
pub const RESET_TIMEOUT_MS: u64 = 5000;
/// Payloads queued behind the frame in flight (frame numbers are modulo 8).
pub const MAX_PENDING: usize = 8;
/// Delivered payloads not yet read; one past it, the oldest is dropped.
pub const MAX_DELIVERED: usize = 8;

/// An ASH frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AshFrame {
    Data {
        frm: u8,
        ack: u8,
        retransmit: bool,
        payload: Vec<u8>,
    },
    Ack {
        ack: u8,
        not_ready: bool,
    },
    Nak {
        ack: u8,
        not_ready: bool,
    },
    Rst,
    RstAck {
        version: u8,
        code: u8,
    },
    Error {
        version: u8,
        code: u8,
    },
}

/// Something wrong with a frame or with the use of the session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AshError {
    BadCrc,
    TooShort,
    TooLong,
    UnknownControl(u8),
    BadLength,
    Cancelled,
    Substituted,
    DeliveredOverflow,
    NotConnected,
    PayloadTooLong,
    QueueFull,
}

impl fmt::Display for AshError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AshError::BadCrc => write!(f, "bad CRC"),
            AshError::TooShort => write!(f, "frame too short"),
            AshError::TooLong => write!(f, "frame too long"),
            AshError::UnknownControl(c) => write!(f, "unknown control byte 0x{c:02X}"),
            AshError::BadLength => write!(f, "wrong frame length"),
            AshError::Cancelled => write!(f, "frame cancelled"),
            AshError::Substituted => write!(f, "substitute byte in frame"),
            AshError::DeliveredOverflow => write!(f, "received payload dropped, queue full"),
            AshError::NotConnected => write!(f, "not connected"),
            AshError::PayloadTooLong => write!(f, "payload too long"),
            AshError::QueueFull => write!(f, "send queue full"),
        }
    }
}

impl std::error::Error for AshError {}

/// CRC-CCITT (polynomial 0x1021, initial value 0xFFFF).
pub fn crc16(data: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for &byte in data {
        crc ^= u16::from(byte) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    crc
}

/// XORs the payload with the pseudo-random sequence (the same call randomises and
/// derandomises).
fn randomize(data: &mut [u8]) {
    let mut rand = LFSR_SEED;
    for byte in data {
        *byte ^= rand;
        rand = if rand & 1 == 0 {
            rand >> 1
        } else {
            (rand >> 1) ^ LFSR_POLY
        };
    }
}

fn is_reserved(byte: u8) -> bool {
    matches!(byte, FLAG | ESCAPE | XON | XOFF | SUBSTITUTE | CANCEL)
}

/// Encodes a frame as it goes on the wire: control byte, payload, CRC, stuffing, flag.
/// RST is preceded by a cancel byte, as UG101 sends it.
pub fn encode(frame: &AshFrame) -> Vec<u8> {
    let mut raw: Vec<u8> = Vec::new();
    match frame {
        AshFrame::Data {
            frm,
            ack,
            retransmit,
            payload,
        } => {
            raw.push(((frm & 7) << 4) | (u8::from(*retransmit) << 3) | (ack & 7));
            let start = raw.len();
            raw.extend_from_slice(payload);
            randomize(&mut raw[start..]);
        }
        AshFrame::Ack { ack, not_ready } => {
            raw.push(CTRL_ACK | (u8::from(*not_ready) << 3) | (ack & 7));
        }
        AshFrame::Nak { ack, not_ready } => {
            raw.push(CTRL_NAK | (u8::from(*not_ready) << 3) | (ack & 7));
        }
        AshFrame::Rst => raw.push(CTRL_RST),
        AshFrame::RstAck { version, code } => {
            raw.extend_from_slice(&[CTRL_RSTACK, *version, *code])
        }
        AshFrame::Error { version, code } => raw.extend_from_slice(&[CTRL_ERROR, *version, *code]),
    }
    let crc = crc16(&raw);
    raw.extend_from_slice(&crc.to_be_bytes());

    let mut out = Vec::with_capacity(raw.len() + 4);
    if matches!(frame, AshFrame::Rst) {
        out.push(CANCEL);
    }
    for byte in raw {
        if is_reserved(byte) {
            out.push(ESCAPE);
            out.push(byte ^ 0x20);
        } else {
            out.push(byte);
        }
    }
    out.push(FLAG);
    out
}

/// Turns the bytes of one frame (control, payload, CRC; unstuffed) into a frame.
fn parse(raw: &[u8]) -> Result<AshFrame, AshError> {
    if raw.len() < 3 {
        return Err(AshError::TooShort);
    }
    let (body, crc_bytes) = raw.split_at(raw.len() - 2);
    if crc16(body) != u16::from_be_bytes([crc_bytes[0], crc_bytes[1]]) {
        return Err(AshError::BadCrc);
    }
    let control = body[0];
    let rest = &body[1..];
    if control & 0x80 == 0 {
        if rest.is_empty() {
            return Err(AshError::BadLength);
        }
        let mut payload = rest.to_vec();
        randomize(&mut payload);
        return Ok(AshFrame::Data {
            frm: (control >> 4) & 7,
            ack: control & 7,
            retransmit: control & 0x08 != 0,
            payload,
        });
    }
    let expect_len = |n: usize| {
        if rest.len() == n {
            Ok(())
        } else {
            Err(AshError::BadLength)
        }
    };
    match control {
        CTRL_RSTACK => {
            expect_len(2)?;
            Ok(AshFrame::RstAck {
                version: rest[0],
                code: rest[1],
            })
        }
        CTRL_ERROR => {
            expect_len(2)?;
            Ok(AshFrame::Error {
                version: rest[0],
                code: rest[1],
            })
        }
        CTRL_RST => {
            expect_len(0)?;
            Ok(AshFrame::Rst)
        }
        c if c & 0xE0 == CTRL_ACK => {
            expect_len(0)?;
            Ok(AshFrame::Ack {
                ack: c & 7,
                not_ready: c & 0x08 != 0,
            })
        }
        c if c & 0xE0 == CTRL_NAK => {
            expect_len(0)?;
            Ok(AshFrame::Nak {
                ack: c & 7,
                not_ready: c & 0x08 != 0,
            })
        }
        c => Err(AshError::UnknownControl(c)),
    }
}

/// What the decoder found in the bytes it was given.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeEvent {
    Frame(AshFrame),
    Error(AshError),
}

/// Turns a stream of bytes, in any chunking, into frames.
#[derive(Debug, Default)]
pub struct Decoder {
    /// Unstuffed bytes of the frame in progress; never more than `MAX_FRAME_BYTES` (223).
    buf: Vec<u8>,
    escaped: bool,
    /// First problem seen in the frame in progress, reported at its flag byte.
    bad: Option<AshError>,
}

impl Decoder {
    pub fn new() -> Self {
        Self::default()
    }

    fn in_progress(&self) -> bool {
        !self.buf.is_empty() || self.escaped || self.bad.is_some()
    }

    fn clear(&mut self) {
        self.buf.clear();
        self.escaped = false;
        self.bad = None;
    }

    /// Feeds bytes; returns the frames and errors completed by them, in order.
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<DecodeEvent> {
        let mut events = Vec::new();
        for &byte in bytes {
            match byte {
                // Flow-control bytes are discarded wherever they appear.
                XON | XOFF => {}
                CANCEL => {
                    if self.in_progress() {
                        events.push(DecodeEvent::Error(AshError::Cancelled));
                    }
                    self.clear();
                }
                SUBSTITUTE => {
                    self.escaped = false;
                    self.bad.get_or_insert(AshError::Substituted);
                }
                FLAG => {
                    if self.in_progress() {
                        let result = match self.bad.take() {
                            Some(error) => Err(error),
                            None => parse(&self.buf),
                        };
                        events.push(match result {
                            Ok(frame) => DecodeEvent::Frame(frame),
                            Err(error) => DecodeEvent::Error(error),
                        });
                    }
                    self.clear();
                }
                ESCAPE => self.escaped = true,
                other => {
                    let value = if self.escaped { other ^ 0x20 } else { other };
                    self.escaped = false;
                    if self.buf.len() < MAX_FRAME_BYTES {
                        self.buf.push(value);
                    } else {
                        self.bad.get_or_insert(AshError::TooLong);
                    }
                }
            }
        }
        events
    }
}

/// Why the link stopped working.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AshFailure {
    /// No RSTACK within 5 seconds of RST.
    NoResetAnswer,
    /// 6 ACK timeouts in a row.
    LinkLost,
    /// The radio sent an ERROR frame.
    ErrorFrame { code: u8 },
    /// The radio sent RSTACK while connected.
    UnexpectedReset { code: u8 },
}

impl fmt::Display for AshFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AshFailure::NoResetAnswer => write!(f, "no answer to reset"),
            AshFailure::LinkLost => write!(f, "link lost"),
            AshFailure::ErrorFrame { code } | AshFailure::UnexpectedReset { code } => {
                write!(f, "error 0x{code:02X}")
            }
        }
    }
}

/// What the session tells its caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Connected,
    Failed(AshFailure),
    /// A problem that does not stop the link (bad frame, dropped payload).
    Error(AshError),
}

/// What the session produced: bytes to write to the radio, and events.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Output {
    pub write: Vec<u8>,
    pub events: Vec<Event>,
}

impl Output {
    pub fn merge(&mut self, other: Output) {
        self.write.extend(other.write);
        self.events.extend(other.events);
    }
}

#[derive(Debug)]
enum State {
    Idle,
    Resetting { since: u64 },
    Connected,
    Failed,
}

#[derive(Debug)]
struct InFlight {
    frm: u8,
    payload: Vec<u8>,
    sent_at: u64,
    timeouts: u32,
}

/// The ASH state machine. One frame in flight at a time (window of 1).
#[derive(Debug)]
pub struct Session {
    state: State,
    decoder: Decoder,
    /// Number of the next DATA frame we send.
    tx_frm: u8,
    /// Number of the next DATA frame we expect from the radio (the ack number we send).
    rx_expected: u8,
    in_flight: Option<InFlight>,
    /// Payloads waiting behind the frame in flight; at most `MAX_PENDING` (8).
    pending: VecDeque<Vec<u8>>,
    /// Payloads received and not yet read; at most `MAX_DELIVERED` (8).
    delivered: VecDeque<Vec<u8>>,
    /// UG101's reject condition: a NAK was sent and no frame in sequence has come since.
    rejecting: bool,
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

impl Session {
    pub fn new() -> Self {
        Session {
            state: State::Idle,
            decoder: Decoder::new(),
            tx_frm: 0,
            rx_expected: 0,
            in_flight: None,
            pending: VecDeque::new(),
            delivered: VecDeque::new(),
            rejecting: false,
        }
    }

    pub fn is_connected(&self) -> bool {
        matches!(self.state, State::Connected)
    }

    /// Starts (or restarts) the link: returns the RST bytes to write.
    pub fn reset(&mut self, now: u64) -> Vec<u8> {
        *self = Session::new();
        self.state = State::Resetting { since: now };
        encode(&AshFrame::Rst)
    }

    fn transmit(
        &mut self,
        frm: u8,
        payload: Vec<u8>,
        retransmit: bool,
        now: u64,
        timeouts: u32,
    ) -> Vec<u8> {
        let bytes = encode(&AshFrame::Data {
            frm,
            ack: self.rx_expected,
            retransmit,
            payload: payload.clone(),
        });
        self.in_flight = Some(InFlight {
            frm,
            payload,
            sent_at: now,
            timeouts,
        });
        bytes
    }

    /// Sends a payload: returns the bytes to write now (empty when it is queued behind the
    /// frame in flight).
    pub fn send(&mut self, payload: Vec<u8>, now: u64) -> Result<Vec<u8>, AshError> {
        if !self.is_connected() {
            return Err(AshError::NotConnected);
        }
        if payload.len() > MAX_PAYLOAD {
            return Err(AshError::PayloadTooLong);
        }
        if self.in_flight.is_some() {
            if self.pending.len() >= MAX_PENDING {
                return Err(AshError::QueueFull);
            }
            self.pending.push_back(payload);
            return Ok(Vec::new());
        }
        let frm = self.tx_frm;
        self.tx_frm = (self.tx_frm + 1) % 8;
        Ok(self.transmit(frm, payload, false, now, 0))
    }

    /// The oldest payload received and not yet read.
    pub fn next_payload(&mut self) -> Option<Vec<u8>> {
        self.delivered.pop_front()
    }

    fn fail(&mut self, failure: AshFailure, out: &mut Output) {
        self.state = State::Failed;
        self.in_flight = None;
        self.pending.clear();
        out.events.push(Event::Failed(failure));
    }

    /// The peer acknowledges every frame before `ack`.
    fn on_ack(&mut self, ack: u8) {
        if let Some(flight) = &self.in_flight
            && ack == (flight.frm + 1) % 8
        {
            self.in_flight = None;
        }
    }

    /// Sends the next queued payload when nothing is in flight.
    fn pump(&mut self, now: u64, out: &mut Output) {
        if self.in_flight.is_none()
            && self.is_connected()
            && let Some(payload) = self.pending.pop_front()
        {
            let frm = self.tx_frm;
            self.tx_frm = (self.tx_frm + 1) % 8;
            let bytes = self.transmit(frm, payload, false, now, 0);
            out.write.extend(bytes);
        }
    }

    fn on_frame(&mut self, frame: AshFrame, now: u64, out: &mut Output) {
        match (&self.state, frame) {
            (State::Idle | State::Failed, _) => {}
            // While resetting, the only frame that counts is the RSTACK of our own reset
            // (version 2, software reset); a power-on RSTACK or an ERROR is stale.
            (
                State::Resetting { .. },
                AshFrame::RstAck {
                    version: ASH_VERSION,
                    code: RESET_SOFTWARE,
                },
            ) => {
                self.state = State::Connected;
                out.events.push(Event::Connected);
            }
            (State::Resetting { .. }, _) => {}
            (State::Connected, AshFrame::Error { code, .. }) => {
                self.fail(AshFailure::ErrorFrame { code }, out);
            }
            (State::Connected, AshFrame::RstAck { code, .. }) => {
                self.fail(AshFailure::UnexpectedReset { code }, out);
            }
            (State::Connected, AshFrame::Ack { ack, .. }) => self.on_ack(ack),
            (State::Connected, AshFrame::Nak { ack, .. }) => {
                // The radio expects frame `ack` and did not get it: send it again.
                if let Some(flight) = self.in_flight.take() {
                    if flight.frm == ack {
                        let bytes =
                            self.transmit(flight.frm, flight.payload, true, now, flight.timeouts);
                        out.write.extend(bytes);
                    } else {
                        self.in_flight = Some(flight);
                    }
                }
            }
            (State::Connected, AshFrame::Rst) => {
                out.events
                    .push(Event::Error(AshError::UnknownControl(CTRL_RST)));
            }
            (
                State::Connected,
                AshFrame::Data {
                    frm,
                    ack,
                    retransmit,
                    payload,
                },
            ) => {
                self.on_ack(ack);
                if frm == self.rx_expected {
                    self.rejecting = false;
                    self.rx_expected = (self.rx_expected + 1) % 8;
                    if self.delivered.len() >= MAX_DELIVERED {
                        self.delivered.pop_front();
                        out.events.push(Event::Error(AshError::DeliveredOverflow));
                    }
                    self.delivered.push_back(payload);
                    out.write.extend(encode(&AshFrame::Ack {
                        ack: self.rx_expected,
                        not_ready: false,
                    }));
                } else if retransmit && frm == (self.rx_expected + 7) % 8 {
                    // Our ACK was lost: acknowledge again, deliver nothing.
                    out.write.extend(encode(&AshFrame::Ack {
                        ack: self.rx_expected,
                        not_ready: false,
                    }));
                } else {
                    self.reject(out);
                }
            }
        }
    }

    /// Sends one NAK, then stays silent until a frame arrives in sequence.
    fn reject(&mut self, out: &mut Output) {
        if !self.rejecting {
            self.rejecting = true;
            out.write.extend(encode(&AshFrame::Nak {
                ack: self.rx_expected,
                not_ready: false,
            }));
        }
    }

    /// True while a sent payload is not acknowledged (in flight or queued).
    pub fn has_unacknowledged(&self) -> bool {
        self.in_flight.is_some() || !self.pending.is_empty()
    }

    /// Feeds the bytes read from the radio.
    pub fn receive(&mut self, bytes: &[u8], now: u64) -> Output {
        let mut out = Output::default();
        for event in self.decoder.feed(bytes) {
            match event {
                DecodeEvent::Error(error) => {
                    // A frame lost to noise is rejected like one out of sequence.
                    if self.is_connected()
                        && matches!(
                            error,
                            AshError::BadCrc | AshError::TooLong | AshError::Substituted
                        )
                    {
                        self.reject(&mut out);
                    }
                    out.events.push(Event::Error(error));
                }
                DecodeEvent::Frame(frame) => self.on_frame(frame, now, &mut out),
            }
        }
        self.pump(now, &mut out);
        out
    }

    /// Handles the passing of time: resends, link failure, no answer to reset.
    pub fn poll(&mut self, now: u64) -> Output {
        let mut out = Output::default();
        match self.state {
            State::Resetting { since } => {
                if now.saturating_sub(since) >= RESET_TIMEOUT_MS {
                    self.fail(AshFailure::NoResetAnswer, &mut out);
                }
            }
            State::Connected => {
                if let Some(flight) = self.in_flight.take() {
                    if now.saturating_sub(flight.sent_at) >= ACK_TIMEOUT_MS {
                        let timeouts = flight.timeouts + 1;
                        if timeouts >= MAX_TIMEOUTS {
                            self.fail(AshFailure::LinkLost, &mut out);
                        } else {
                            let bytes =
                                self.transmit(flight.frm, flight.payload, true, now, timeouts);
                            out.write.extend(bytes);
                        }
                    } else {
                        self.in_flight = Some(flight);
                    }
                }
            }
            State::Idle | State::Failed => {}
        }
        out
    }
}
