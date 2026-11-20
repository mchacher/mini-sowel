//! EZSP (UG100), the commands the host sends to the radio. No I/O: frames in, frames out.

use std::fmt;

/// Frame id of `version`.
pub const ID_VERSION: u16 = 0x0000;
/// Frame id of `networkState`.
pub const ID_NETWORK_STATE: u16 = 0x0018;
/// Frame id of `getEui64`.
pub const ID_GET_EUI64: u16 = 0x0026;

/// Oldest EZSP protocol version supported (EmberZNet 7.2).
pub const MIN_PROTOCOL_VERSION: u8 = 13;
/// Newest EZSP protocol version supported, and the one asked for first.
pub const MAX_PROTOCOL_VERSION: u8 = 19;
/// The only stack type accepted: mesh.
pub const STACK_TYPE_MESH: u8 = 2;

const FC_RESPONSE: u8 = 0x80;
const FC_OVERFLOW: u8 = 0x01;
const FC_TRUNCATED: u8 = 0x02;
const FC_ASYNC_CALLBACK: u8 = 0x10;
/// High byte of the frame control in the extended format (frame format version 1).
const FC_HIGH_EXTENDED: u8 = 0x01;

/// Something wrong in an EZSP frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EzspError {
    TooShort,
    NotAResponse,
    Truncated,
    Overflow,
    NoCommandPending,
    WrongSequence { expected: u8, got: u8 },
    WrongFrameId { expected: u16, got: u16 },
    BadLength { expected: usize, got: usize },
    UnsupportedVersion { found: u8 },
    UnsupportedStackType(u8),
}

impl fmt::Display for EzspError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EzspError::TooShort => write!(f, "EZSP frame too short"),
            EzspError::NotAResponse => write!(f, "EZSP frame is not a response"),
            EzspError::Truncated => write!(f, "EZSP response truncated by the radio"),
            EzspError::Overflow => write!(f, "EZSP response overflowed in the radio"),
            EzspError::NoCommandPending => write!(f, "EZSP response without a command"),
            EzspError::WrongSequence { expected, got } => {
                write!(f, "EZSP response with sequence {got}, expected {expected}")
            }
            EzspError::WrongFrameId { expected, got } => {
                write!(
                    f,
                    "EZSP response with frame id 0x{got:04X}, expected 0x{expected:04X}"
                )
            }
            EzspError::BadLength { expected, got } => {
                write!(f, "EZSP response of {got} bytes, expected {expected}")
            }
            EzspError::UnsupportedVersion { found } => write!(
                f,
                "EZSP protocol version {found} is not supported (supported: {MIN_PROTOCOL_VERSION} to {MAX_PROTOCOL_VERSION})"
            ),
            EzspError::UnsupportedStackType(t) => {
                write!(
                    f,
                    "stack type {t} is not supported (expected {STACK_TYPE_MESH})"
                )
            }
        }
    }
}

impl std::error::Error for EzspError {}

/// State of the radio's network, as `networkState` reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkState {
    NoNetwork,
    Joining,
    Joined,
    JoinedNoParent,
    Leaving,
    Unknown(u8),
}

impl NetworkState {
    pub fn from_u8(value: u8) -> Self {
        match value {
            0 => NetworkState::NoNetwork,
            1 => NetworkState::Joining,
            2 => NetworkState::Joined,
            3 => NetworkState::JoinedNoParent,
            6 => NetworkState::Leaving,
            other => NetworkState::Unknown(other),
        }
    }
}

impl fmt::Display for NetworkState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NetworkState::NoNetwork => write!(f, "no network"),
            NetworkState::Joining => write!(f, "joining"),
            NetworkState::Joined => write!(f, "joined"),
            NetworkState::JoinedNoParent => write!(f, "joined with no parent"),
            NetworkState::Leaving => write!(f, "leaving"),
            NetworkState::Unknown(n) => write!(f, "unknown ({n})"),
        }
    }
}

/// The answer to `version`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Version {
    pub protocol: u8,
    pub stack_type: u8,
    pub stack_version: u16,
}

/// Writes a stack version as `major.minor.patch.build` (one nibble each, as UG100).
pub fn format_stack_version(version: u16) -> String {
    format!(
        "{}.{}.{}.{}",
        (version >> 12) & 0xF,
        (version >> 8) & 0xF,
        (version >> 4) & 0xF,
        version & 0xF
    )
}

/// Builds a command frame. Legacy format: sequence, frame control, frame id (1 byte each).
/// Extended format: sequence, frame control (2 bytes), frame id (2 bytes), little-endian.
pub fn encode_command(seq: u8, frame_id: u16, params: &[u8], extended: bool) -> Vec<u8> {
    let mut frame = vec![seq, 0x00];
    if extended {
        frame.push(FC_HIGH_EXTENDED);
        frame.extend_from_slice(&frame_id.to_le_bytes());
    } else {
        // The legacy format only has room for a one-byte id.
        frame.push(frame_id.to_le_bytes()[0]);
    }
    frame.extend_from_slice(params);
    frame
}

/// What an incoming EZSP frame is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Parsed {
    /// An asynchronous callback: not the answer to a command, to be ignored for now.
    Callback,
    /// The answer to the pending command, with its parameters.
    Response(Vec<u8>),
}

/// The parameters of a `version` response.
pub fn parse_version(params: &[u8]) -> Result<Version, EzspError> {
    if params.len() != 4 {
        return Err(EzspError::BadLength {
            expected: 4,
            got: params.len(),
        });
    }
    Ok(Version {
        protocol: params[0],
        stack_type: params[1],
        stack_version: u16::from_le_bytes([params[2], params[3]]),
    })
}

/// The parameters of a `networkState` response.
pub fn parse_network_state(params: &[u8]) -> Result<NetworkState, EzspError> {
    match params {
        [state] => Ok(NetworkState::from_u8(*state)),
        _ => Err(EzspError::BadLength {
            expected: 1,
            got: params.len(),
        }),
    }
}

/// The parameters of a `getEui64` response: the address, most significant byte first (the
/// radio sends it little-endian).
pub fn parse_eui64(params: &[u8]) -> Result<[u8; 8], EzspError> {
    let mut eui: [u8; 8] = params.try_into().map_err(|_| EzspError::BadLength {
        expected: 8,
        got: params.len(),
    })?;
    eui.reverse();
    Ok(eui)
}

/// The EZSP side of the conversation: sequence numbers, frame format, version negotiation.
#[derive(Debug, Default)]
pub struct Ezsp {
    seq: u8,
    extended: bool,
    /// Sequence and frame id of the last command, which the next response must match.
    pending: Option<(u8, u16)>,
    version: Option<u8>,
}

impl Ezsp {
    pub fn new() -> Self {
        Self::default()
    }

    /// The protocol version agreed with the radio, once negotiated.
    pub fn protocol_version(&self) -> Option<u8> {
        self.version
    }

    /// Builds the next command frame.
    pub fn command(&mut self, frame_id: u16, params: &[u8]) -> Vec<u8> {
        let seq = self.seq;
        self.seq = self.seq.wrapping_add(1);
        self.pending = Some((seq, frame_id));
        encode_command(seq, frame_id, params, self.extended)
    }

    /// The first `version` command: legacy format, the newest version we know.
    pub fn version_command(&mut self) -> Vec<u8> {
        self.command(ID_VERSION, &[MAX_PROTOCOL_VERSION])
    }

    /// Checks a frame against the pending command.
    pub fn parse_response(&self, frame: &[u8]) -> Result<Parsed, EzspError> {
        if frame.len() < 3 {
            return Err(EzspError::TooShort);
        }
        let control = frame[1];
        if control & FC_ASYNC_CALLBACK != 0 {
            return Ok(Parsed::Callback);
        }
        if control & FC_RESPONSE == 0 {
            return Err(EzspError::NotAResponse);
        }
        if control & FC_TRUNCATED != 0 {
            return Err(EzspError::Truncated);
        }
        if control & FC_OVERFLOW != 0 {
            return Err(EzspError::Overflow);
        }
        let (expected_seq, expected_id) = self.pending.ok_or(EzspError::NoCommandPending)?;
        if frame[0] != expected_seq {
            return Err(EzspError::WrongSequence {
                expected: expected_seq,
                got: frame[0],
            });
        }
        // The format is read from the response itself: in the extended format the byte after
        // the frame control's low byte is its high byte, whose low bits are the format version.
        let extended = frame[2] & 0x03 == FC_HIGH_EXTENDED;
        if extended && frame.len() < 5 {
            return Err(EzspError::TooShort);
        }
        let (id, params) = if extended {
            (u16::from_le_bytes([frame[3], frame[4]]), &frame[5..])
        } else {
            (u16::from(frame[2]), &frame[3..])
        };
        if id != expected_id {
            return Err(EzspError::WrongFrameId {
                expected: expected_id,
                got: id,
            });
        }
        Ok(Parsed::Response(params.to_vec()))
    }

    /// Handles the answer to the first `version`: checks stack type and version range, switches
    /// to the extended format and returns the second `version` command.
    pub fn on_first_version(&mut self, params: &[u8]) -> Result<Vec<u8>, EzspError> {
        let version = parse_version(params)?;
        if version.stack_type != STACK_TYPE_MESH {
            return Err(EzspError::UnsupportedStackType(version.stack_type));
        }
        if !(MIN_PROTOCOL_VERSION..=MAX_PROTOCOL_VERSION).contains(&version.protocol) {
            return Err(EzspError::UnsupportedVersion {
                found: version.protocol,
            });
        }
        self.extended = true;
        Ok(self.command(ID_VERSION, &[version.protocol]))
    }

    /// Handles the answer to the second `version`: the session's version is settled.
    pub fn on_second_version(&mut self, params: &[u8]) -> Result<Version, EzspError> {
        let version = parse_version(params)?;
        if !(MIN_PROTOCOL_VERSION..=MAX_PROTOCOL_VERSION).contains(&version.protocol) {
            return Err(EzspError::UnsupportedVersion {
                found: version.protocol,
            });
        }
        self.version = Some(version.protocol);
        Ok(version)
    }
}
